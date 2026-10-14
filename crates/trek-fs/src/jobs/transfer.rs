//! Transfer traversal, collision decisions and native-move preference.
mod file_copy;
mod replacement;
use super::{engine::Runner, *};
use trek_core::naming::{unique_name, validate};
impl Runner {
    pub(super) fn transfer_item(
        &mut self,
        src: &gio::File,
        parent: &gio::File,
        moving: bool,
        linking: bool,
    ) -> Result<(), JobError> {
        self.checkpoint()?;
        let name = src.basename().ok_or(JobError::Invalid)?;
        let mut dest = parent.child(&name);
        if moving && src.parent().is_some_and(|p| p.equal(parent)) {
            self.summary.skipped += 1;
            return Ok(());
        }
        if src.equal(&dest) {
            if let Some(name) = name.to_str() {
                let unique = unique_name(name, |n| {
                    parent.child(n).query_exists(Some(&self.control.cancel))
                });
                dest = parent.child(unique);
            } else {
                let mut number = 1u64;
                loop {
                    let mut candidate = name.as_os_str().to_os_string();
                    candidate.push(if number == 1 {
                        " (copy)".into()
                    } else {
                        format!(" (copy {number})")
                    });
                    if !parent
                        .child(&candidate)
                        .query_exists(Some(&self.control.cancel))
                    {
                        dest = parent.child(candidate);
                        break;
                    }
                    number += 1;
                }
            }
        }
        self.transfer_to(src, &dest, moving, linking)
    }
    pub(super) fn conflict(
        &mut self,
        src: &gio::File,
        dest: &gio::File,
    ) -> Result<Option<(gio::File, bool)>, JobError> {
        let mut dest = dest.clone();
        loop {
            self.checkpoint()?;
            crate::guard::check(&dest, &self.policy)?;
            let existing = match info(&dest, &self.control.cancel) {
                Ok(existing) => existing,
                Err(JobError::Gio(e)) if e.matches(gio::IOErrorEnum::NotFound) => {
                    return Ok(Some((dest, false)));
                }
                Err(e) => return Err(e),
            };
            let s = info(src, &self.control.cancel)?;
            let is_folder =
                |i: &gio::FileInfo| i.file_type() == gio::FileType::Directory && !i.is_symlink();
            // Two folders merge and never reach this point. A folder may
            // replace a file (one file is lost, as with any overwrite), but
            // a file must never replace a folder and everything in it.
            let can_overwrite = !is_folder(&existing) || is_folder(&s);
            let parent = dest.parent().ok_or(JobError::Invalid)?;
            let src_meta =
                crate::meta::FileMeta::query(src, false, true, Some(&self.control.cancel))?;
            let dest_meta =
                crate::meta::FileMeta::query(&dest, false, true, Some(&self.control.cancel))?;
            // GVFS need not supply standard::edit-name. The metadata
            // boundary supplies its display-name fallback for that case.
            let suggested = unique_name(&src_meta.edit_name, |n| {
                parent.child(n).query_exists(Some(&self.control.cancel))
            });
            match self.ask(Question::Conflict {
                src: Box::new(src_meta),
                dest: Box::new(dest_meta),
                suggested_name: suggested,
                can_overwrite,
            })? {
                Answer::Overwrite { .. } => return Ok(Some((dest, true))),
                Answer::Skip { .. } => {
                    self.summary.skipped += 1;
                    return Ok(None);
                }
                Answer::Rename(name) => {
                    validate(&name)?;
                    dest = parent.child(name);
                    if dest.equal(src) {
                        return Err(JobError::IntoSelf);
                    }
                }
                Answer::Cancel => return Err(JobError::Cancelled),
                Answer::Retry | Answer::DeleteInstead { .. } | Answer::EnterMount => {
                    return Err(JobError::Invalid);
                }
            }
        }
    }
    pub(super) fn transfer_to(
        &mut self,
        src: &gio::File,
        dest: &gio::File,
        moving: bool,
        linking: bool,
    ) -> Result<(), JobError> {
        self.checkpoint()?;
        crate::guard::check(src, &self.policy)?;
        crate::guard::check(dest, &self.policy)?;
        self.send(JobEvent::Current {
            src: src.uri().into(),
            dest: Some(dest.uri().into()),
        });
        let s = info(src, &self.control.cancel)?;
        let directory = s.file_type() == gio::FileType::Directory && !s.is_symlink() && !linking;
        let existing = match info(dest, &self.control.cancel) {
            Ok(i) => Some(i),
            Err(JobError::Gio(e)) if e.matches(gio::IOErrorEnum::NotFound) => None,
            Err(e) => return Err(e),
        };
        let merging = directory
            && existing
                .as_ref()
                .is_some_and(|i| i.file_type() == gio::FileType::Directory && !i.is_symlink());
        let (dest, overwrite) = if existing.is_some() && !merging {
            match self.conflict(src, dest)? {
                Some(pair) => pair,
                None => return Ok(()),
            }
        } else {
            (dest.clone(), false)
        };
        let mismatch = overwrite
            && info(&dest, &self.control.cancel).is_ok_and(|i| i.file_type() != s.file_type());
        if directory && mismatch {
            return self.replace_directory(src, &dest, moving);
        }
        if moving && !merging && !mismatch {
            let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS
                | gio::FileCopyFlags::ALL_METADATA
                | gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE;
            if overwrite {
                flags |= gio::FileCopyFlags::OVERWRITE;
            }
            match src.move_(&dest, flags, Some(&self.control.cancel), None) {
                Ok(()) => {
                    let totals = self
                        .prepared
                        .get(src.uri().as_str())
                        .copied()
                        .unwrap_or((1, s.size().max(0) as u64));
                    self.summary.done_files += totals.0;
                    self.summary.done_bytes += totals.1;
                    self.progress(0);
                    return Ok(());
                }
                Err(e)
                    if e.matches(gio::IOErrorEnum::NotSupported)
                        | e.matches(gio::IOErrorEnum::WouldRecurse)
                        | e.matches(gio::IOErrorEnum::WouldMerge) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if directory {
            if !merging {
                match dest.make_directory(Some(&self.control.cancel)) {
                    Ok(()) => {}
                    Err(e) if e.matches(gio::IOErrorEnum::Exists) => {
                        return self.transfer_to(src, &dest, moving, linking);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            let skipped = self.summary.skipped;
            for child in children(src, &self.control.cancel)? {
                let child_name = child.basename().ok_or(JobError::Invalid)?;
                self.transfer_to(&child, &dest.child(child_name), moving, linking)?;
            }
            // GLib applies every attribute it can and then reports the first
            // one it could not; for a folder that is always the read-only
            // `standard::size` ("not supported"), with mode and times already
            // set. Only a real failure is worth a warning.
            if let Err(e) = src.copy_attributes(
                &dest,
                gio::FileCopyFlags::ALL_METADATA | gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                Some(&self.control.cancel),
            ) && !e.matches(gio::IOErrorEnum::NotSupported)
            {
                self.summary
                    .warnings
                    .push(format!("{}: metadata: {e}", dest.uri()));
            }
            self.completed(0);
            if moving && self.summary.skipped == skipped {
                crate::guard::check(src, &self.policy)?;
                src.delete(Some(&self.control.cancel))?;
            }
            Ok(())
        } else if linking {
            self.copy_link(src, &dest, overwrite)
        } else {
            let copied = self.copy_file(src, &dest, overwrite)?;
            if copied && moving {
                crate::guard::check(src, &self.policy)?;
                src.delete(Some(&self.control.cancel))?;
            }
            Ok(())
        }
    }
    fn copy_link(
        &mut self,
        src: &gio::File,
        dest: &gio::File,
        overwrite: bool,
    ) -> Result<(), JobError> {
        let target = symlink_target(src, dest)?;
        let temp = self.reserve_temp(dest)?;
        temp.delete(Some(&self.control.cancel))?;
        let result = (|| {
            crate::guard::check(&temp, &self.policy)?;
            temp.make_symbolic_link(target, Some(&self.control.cancel))?;
            if self.publish(src, &temp, dest, overwrite)? {
                self.completed(0);
            }
            Ok(())
        })();
        self.cleanup(&temp);
        result
    }
}

// A symbolic-link target is a path in the destination's backend, never a URI
// or a path from a different server. GIO relative_path preserves escaped bytes.
fn symlink_target(src: &gio::File, dest: &gio::File) -> Result<std::path::PathBuf, JobError> {
    let mut root = src.clone();
    while let Some(parent) = root.parent() {
        root = parent;
    }
    if dest.equal(&root) || dest.has_prefix(&root) {
        if src.equal(&root) {
            return Ok(std::path::PathBuf::from("/"));
        }
        if let Some(path) = root.relative_path(src) {
            return Ok(std::path::Path::new("/").join(path));
        }
    }
    Err(glib::Error::new(
        gio::IOErrorEnum::NotSupported,
        "Symbolic links require source and destination on the same filesystem backend/server",
    )
    .into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    #[test]
    fn link_targets_preserve_bytes_and_server_identity() {
        for scheme in ["file:///", "sftp://host/", "smb://host/share/"] {
            let src = gio::File::for_uri(&format!("{scheme}folder/a%FFb"));
            let dest = gio::File::for_uri(&format!("{scheme}other/link"));
            let path = symlink_target(&src, &dest).unwrap();
            assert!(path.as_os_str().as_bytes().ends_with(b"folder/a\xffb"));
            assert!(path.is_absolute());
        }
        let src = gio::File::for_uri("sftp://host/a");
        for uri in [
            "sftp://different-host/link",
            "file:///tmp/link",
            "smb://host/link",
        ] {
            let error = symlink_target(&src, &gio::File::for_uri(uri)).unwrap_err();
            assert!(matches!(error, JobError::Gio(e) if e.matches(gio::IOErrorEnum::NotSupported)));
        }
    }
}
