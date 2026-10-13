//! Safe replacement across file types, with rollback before publication succeeds.
use super::*;
impl Runner {
    pub(super) fn replace_directory(
        &mut self,
        src: &gio::File,
        dest: &gio::File,
        moving: bool,
    ) -> Result<(), JobError> {
        let temp = self.reserve_temp(dest)?;
        temp.delete(Some(&self.control.cancel))?;
        let result = (|| {
            crate::guard::check(&temp, &self.policy)?;
            temp.make_directory(Some(&self.control.cancel))?;
            let skipped = self.summary.skipped;
            self.transfer_to(src, &temp, false, false)?;
            if self.summary.skipped != skipped {
                return Ok(());
            }
            if self.publish(src, &temp, dest, true)? && moving {
                let before = (self.summary.done_files, self.summary.done_bytes);
                let result = self.delete(src, None);
                self.summary.done_files = before.0;
                self.summary.done_bytes = before.1;
                result?;
            }
            Ok(())
        })();
        self.cleanup_tree(&temp);
        result
    }
    pub(super) fn replace_mismatch(
        &mut self,
        temp: &gio::File,
        dest: &gio::File,
    ) -> Result<bool, JobError> {
        let backup = self.reserve_temp(dest)?;
        backup.delete(Some(&self.control.cancel))?;
        crate::guard::check(dest, &self.policy)?;
        dest.move_(
            &backup,
            gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE | gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
            Some(&self.control.cancel),
            None,
        )?;
        // The entry may have changed since the conflict question. Inspect
        // the entry actually moved aside before publishing or deleting it:
        // an overwrite answer for a file never authorizes deleting a folder.
        let publish = (|| -> Result<(), JobError> {
            let previous = info(&backup, &self.control.cancel)?;
            let replacement = info(temp, &self.control.cancel)?;
            let folder =
                |i: &gio::FileInfo| i.file_type() == gio::FileType::Directory && !i.is_symlink();
            if folder(&previous) && !folder(&replacement) {
                return Err(JobError::ReplaceFolder);
            }
            temp.move_(
                dest,
                gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE | gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                Some(&self.control.cancel),
                None,
            )?;
            Ok(())
        })();
        match publish {
            Ok(()) => {
                let before = (self.summary.done_files, self.summary.done_bytes);
                if let Err(e) = self.delete(&backup, None) {
                    self.summary.warnings.push(format!(
                        "Replacement succeeded; old entry remains at {}: {e}",
                        backup.uri()
                    ));
                }
                self.summary.done_files = before.0;
                self.summary.done_bytes = before.1;
                Ok(true)
            }
            Err(e) => {
                if let Err(rollback) = backup.move_(
                    dest,
                    gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE
                        | gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                    None::<&gio::Cancellable>,
                    None,
                ) {
                    self.summary.warnings.push(format!(
                        "Previous destination retained at {}: {rollback}",
                        backup.uri()
                    ));
                }
                Err(e)
            }
        }
    }
    pub(super) fn cleanup_tree(&mut self, temp: &gio::File) {
        let result = self.remove_temporary(temp);
        if let Err(e) = result {
            self.summary
                .warnings
                .push(format!("Temporary directory {}: {e}", temp.uri()));
        }
    }
    fn remove_temporary(&self, file: &gio::File) -> Result<(), JobError> {
        crate::guard::check(file, &self.policy)?;
        let cancel = gio::Cancellable::new();
        let i = match info(file, &cancel) {
            Ok(i) => i,
            Err(JobError::Gio(e)) if e.matches(gio::IOErrorEnum::NotFound) => return Ok(()),
            Err(e) => return Err(e),
        };
        if i.file_type() == gio::FileType::Directory && !i.is_symlink() {
            file.set_attribute_uint32(
                "unix::mode",
                i.attribute_uint32("unix::mode") | 0o700,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                Some(&cancel),
            )?;
            for child in children(file, &cancel)? {
                self.remove_temporary(&child)?;
            }
        }
        file.delete(Some(&cancel))?;
        Ok(())
    }
}
