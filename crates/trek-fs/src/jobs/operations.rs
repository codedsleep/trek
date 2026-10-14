//! Delete, attribute and trash operations, all guarded at the write boundary.
use super::{engine::Runner, *};
use trek_core::perms::AttrChange;
impl Runner {
    pub(super) fn delete(
        &mut self,
        file: &gio::File,
        parent_fs: Option<&str>,
    ) -> Result<(), JobError> {
        self.checkpoint()?;
        crate::guard::check(file, &self.policy)?;
        let i = info(file, &self.control.cancel)?;
        let id = i.attribute_string("id::filesystem");
        // The trash backend removes a trashed folder as a whole and refuses
        // to modify anything inside one, so the job must not descend there.
        let directory = i.file_type() == gio::FileType::Directory
            && !i.is_symlink()
            && !file.has_uri_scheme("trash");
        if mount_boundary(&i, parent_fs) {
            match self.ask(Question::MountPoint {
                uri: file.uri().into(),
            })? {
                Answer::EnterMount => {}
                Answer::Skip { .. } => {
                    self.summary.skipped += 1;
                    return Ok(());
                }
                Answer::Cancel => return Err(JobError::Cancelled),
                Answer::Overwrite { .. }
                | Answer::Rename(_)
                | Answer::Retry
                | Answer::DeleteInstead { .. } => return Err(JobError::Invalid),
            }
        }
        let skipped = self.summary.skipped;
        if directory {
            for child in children(file, &self.control.cancel)? {
                self.delete(&child, id.as_deref())?;
            }
        }
        if self.summary.skipped != skipped {
            return Ok(());
        }
        crate::guard::check(file, &self.policy)?;
        file.delete(Some(&self.control.cancel))?;
        self.completed(i.size().max(0) as u64);
        Ok(())
    }
    pub(super) fn attrs(
        &mut self,
        file: &gio::File,
        change: AttrChange,
        recursive: bool,
    ) -> Result<(), JobError> {
        self.checkpoint()?;
        crate::guard::check(file, &self.policy)?;
        let i = info(file, &self.control.cancel)?;
        if i.is_symlink() {
            self.summary.skipped += 1;
            return Ok(());
        }
        if recursive && i.file_type() == gio::FileType::Directory {
            for child in children(file, &self.control.cancel)? {
                self.attrs(&child, change, true)?;
            }
        }
        let flags = gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS;
        if let Some(uid) = change.uid {
            file.set_attribute_uint32("unix::uid", uid, flags, Some(&self.control.cancel))?;
        }
        if let Some(gid) = change.gid {
            file.set_attribute_uint32("unix::gid", gid, flags, Some(&self.control.cancel))?;
        }
        if change.mode_mask != 0 {
            file.set_attribute_uint32(
                "unix::mode",
                change.apply_mode(i.attribute_uint32("unix::mode")),
                flags,
                Some(&self.control.cancel),
            )?;
        }
        self.completed(i.size().max(0) as u64);
        Ok(())
    }
    pub(super) fn trash(&mut self, file: &gio::File) -> Result<(), JobError> {
        self.checkpoint()?;
        crate::guard::check(file, &self.policy)?;
        crate::guard::check(&gio::File::for_uri("trash:///"), &self.policy)?;
        let result = if crate::trash::offer_delete(
            self.options.no_usb_trash,
            crate::trash::is_removable(file),
        ) {
            Err(glib::Error::new(
                gio::IOErrorEnum::NotSupported,
                "Trash disabled on removable media",
            ))
        } else {
            file.trash(Some(&self.control.cancel))
        };
        match result {
            Ok(()) => {
                self.completed(0);
                Ok(())
            }
            Err(e) if e.matches(gio::IOErrorEnum::NotSupported) => {
                match self.ask(Question::TrashUnsupported {
                    uri: file.uri().into(),
                })? {
                    Answer::DeleteInstead { .. } => self.delete(file, None),
                    Answer::Skip { .. } => {
                        self.summary.skipped += 1;
                        Ok(())
                    }
                    Answer::Cancel => Err(JobError::Cancelled),
                    Answer::Overwrite { .. }
                    | Answer::Rename(_)
                    | Answer::Retry
                    | Answer::EnterMount => Err(JobError::Invalid),
                }
            }
            Err(e) => Err(e.into()),
        }
    }
    pub(super) fn restore(&mut self, file: &gio::File) -> Result<(), JobError> {
        crate::guard::check(file, &self.policy)?;
        let i = info(file, &self.control.cancel)?;
        let original = i
            .attribute_file_path("trash::orig-path")
            .ok_or(JobError::Invalid)?;
        let dest = gio::File::for_path(original);
        crate::guard::check(&dest, &self.policy)?;
        self.transfer_to(file, &dest, true, false)
    }
}

fn mount_boundary(info: &gio::FileInfo, parent_fs: Option<&str>) -> bool {
    info.file_type() == gio::FileType::Directory
        && !info.is_symlink()
        && (info.boolean("unix::is-mountpoint")
            || parent_fs
                .zip(info.attribute_string("id::filesystem").as_deref())
                .is_some_and(|(a, b)| a != b))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_delete_mount_boundary() {
        let i = gio::FileInfo::new();
        i.set_file_type(gio::FileType::Directory);
        i.set_is_symlink(false);
        i.set_attribute_string("id::filesystem", "child");
        assert!(mount_boundary(&i, Some("parent")));
        assert!(!mount_boundary(&i, Some("child")));
        i.set_attribute_boolean("unix::is-mountpoint", true);
        assert!(mount_boundary(&i, Some("child")));
        i.set_is_symlink(true);
        assert!(!mount_boundary(&i, Some("parent")));
    }
}
