//! Read-only prepare and size/count walks; never follow symlinks.
use super::{engine::Runner, *};
impl Runner {
    pub(super) fn count_prepare(
        &mut self,
        file: &gio::File,
        totals: &mut (u64, u64),
    ) -> Result<(), JobError> {
        self.checkpoint()?;
        let before = *totals;
        let i = info(file, &self.control.cancel)?;
        totals.0 += 1;
        totals.1 += if i.file_type() == gio::FileType::Directory && !i.is_symlink() {
            0
        } else {
            i.size().max(0) as u64
        };
        if i.file_type() == gio::FileType::Directory && !i.is_symlink() {
            for child in children(file, &self.control.cancel)? {
                self.count_prepare(&child, totals)?;
            }
        }
        self.prepared.insert(
            file.uri().into(),
            (totals.0 - before.0, totals.1 - before.1),
        );
        Ok(())
    }
    pub(super) fn count_execute(&mut self, file: &gio::File) -> Result<(), JobError> {
        self.checkpoint()?;
        let i = info(file, &self.control.cancel)?;
        self.summary.allocated_bytes += i.attribute_uint64("standard::allocated-size");
        self.completed(
            if i.file_type() == gio::FileType::Directory && !i.is_symlink() {
                0
            } else {
                i.size().max(0) as u64
            },
        );
        if i.file_type() == gio::FileType::Directory && !i.is_symlink() {
            for child in children(file, &self.control.cancel)? {
                self.count_execute(&child)?;
            }
        }
        Ok(())
    }
}
