//! Stage file contents before publishing, preserving the previous destination.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
impl Runner {
    pub(super) fn reserve_temp(&self, dest: &gio::File) -> Result<gio::File, JobError> {
        let parent = dest.parent().ok_or(JobError::Invalid)?;
        loop {
            let temp = parent.child(format!(
                ".tallboy-copy-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            crate::guard::check(&temp, &self.policy)?;
            match temp.create(gio::FileCreateFlags::PRIVATE, Some(&self.control.cancel)) {
                Ok(stream) => {
                    if let Err(error) = stream.close(None::<&gio::Cancellable>) {
                        let _ = temp.delete(None::<&gio::Cancellable>);
                        return Err(error.into());
                    }
                    return Ok(temp);
                }
                Err(e) if e.matches(gio::IOErrorEnum::Exists) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub(super) fn cleanup(&mut self, temp: &gio::File) {
        if let Err(e) = crate::guard::check(temp, &self.policy) {
            self.summary.warnings.push(e.to_string());
            return;
        }
        if let Err(e) = temp.delete(None::<&gio::Cancellable>)
            && !e.matches(gio::IOErrorEnum::NotFound)
        {
            let message = format!("Partial-file cleanup {}: {e}", temp.uri());
            log::warn!("{message}");
            self.summary.warnings.push(message);
        }
    }
    pub(super) fn publish(
        &mut self,
        src: &gio::File,
        temp: &gio::File,
        dest: &gio::File,
        overwrite: bool,
    ) -> Result<bool, JobError> {
        let mut dest = dest.clone();
        let mut overwrite = overwrite;
        loop {
            self.checkpoint()?;
            crate::guard::check(&dest, &self.policy)?;
            let mut flags =
                gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE | gio::FileCopyFlags::NOFOLLOW_SYMLINKS;
            if overwrite {
                let source = info(temp, &self.control.cancel)?;
                if info(&dest, &self.control.cancel)
                    .is_ok_and(|i| i.file_type() != source.file_type())
                {
                    return self.replace_mismatch(temp, &dest);
                }
                flags |= gio::FileCopyFlags::OVERWRITE;
            }
            match temp.move_(&dest, flags, Some(&self.control.cancel), None) {
                Ok(()) => return Ok(true),
                Err(e) if e.matches(gio::IOErrorEnum::Exists) => {
                    match self.conflict(src, &dest)? {
                        Some((d, o)) => {
                            dest = d;
                            overwrite = o;
                        }
                        None => return Ok(false),
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub(super) fn copy_file(
        &mut self,
        src: &gio::File,
        dest: &gio::File,
        overwrite: bool,
    ) -> Result<bool, JobError> {
        let temp = self.reserve_temp(dest)?;
        temp.delete(None::<&gio::Cancellable>)?;
        let mut owns_temp = false;
        let result = (|| {
            let cancel = self.control.cancel.clone();
            let control = self.control.clone();
            let warning = copy_with_metadata_fallback(|metadata| {
                crate::guard::check(&temp, &self.policy).map_err(|e| {
                    glib::Error::new(gio::IOErrorEnum::PermissionDenied, &e.to_string())
                })?;
                if !metadata && owns_temp {
                    self.cleanup(&temp);
                    owns_temp = false;
                }
                let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS;
                if metadata {
                    flags |= gio::FileCopyFlags::ALL_METADATA;
                }
                let mut progress = |bytes: i64, _total: i64| {
                    if control.checkpoint().is_err() {
                        cancel.cancel();
                    }
                    self.progress(bytes.max(0) as u64);
                };
                let result = src.copy(&temp, flags, Some(&cancel), Some(&mut progress));
                owns_temp = !result
                    .as_ref()
                    .is_err_and(|e| e.matches(gio::IOErrorEnum::Exists));
                result
            })?;
            if let Some(error) = warning {
                self.summary.warnings.push(format!(
                    "{}: metadata could not be preserved: {error}",
                    src.uri()
                ));
            }
            let size = info(&temp, &self.control.cancel)?.size().max(0) as u64;
            let published = self.publish(src, &temp, dest, overwrite)?;
            if published {
                self.completed(size);
            }
            Ok(published)
        })();
        if owns_temp {
            self.cleanup(&temp);
        }
        result
    }
}

// A metadata failure is recoverable only if a second content copy succeeds.
// Both attempts target the private temporary file, never the user's destination.
fn copy_with_metadata_fallback(
    mut attempt: impl FnMut(bool) -> Result<(), glib::Error>,
) -> Result<Option<glib::Error>, glib::Error> {
    match attempt(true) {
        Ok(()) => Ok(None),
        Err(error)
            if error.matches(gio::IOErrorEnum::Cancelled)
                || error.matches(gio::IOErrorEnum::Exists) =>
        {
            Err(error)
        }
        Err(error) => {
            attempt(false)?;
            Ok(Some(error))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_failure_is_warning_only_after_successful_content_copy() {
        let mut calls = vec![];
        let warning = copy_with_metadata_fallback(|metadata| {
            calls.push(metadata);
            if metadata {
                Err(glib::Error::new(
                    gio::IOErrorEnum::PermissionDenied,
                    "metadata denied",
                ))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert!(warning.is_some());
        assert_eq!(calls, vec![true, false]);
        let error = copy_with_metadata_fallback(|_| {
            Err(glib::Error::new(
                gio::IOErrorEnum::PermissionDenied,
                "contents denied",
            ))
        });
        assert!(error.is_err());
        let mut n = 0;
        let error = copy_with_metadata_fallback(|_| {
            n += 1;
            Err(glib::Error::new(gio::IOErrorEnum::Cancelled, "cancelled"))
        });
        assert!(error.is_err());
        assert_eq!(n, 1);
    }
}
