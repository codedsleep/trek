//! Which volumes to mount without being asked.
use super::Entry;
use trek_core::config::Config;
/// Volume ids eligible for startup or newly-added-volume automatic mounting.
/// The backend's automount hint decides, as in PCManFM: udisks gives it to
/// media on hot-plug buses (USB, FireWire, SD) and to disk images the user
/// attached, never to fixed disks. Internal disks are skipped regardless.
pub fn plan(entries: &[Entry], config: &Config, startup: bool) -> Vec<String> {
    entries
        .iter()
        .filter(|e| {
            !e.mounted
                && !e.internal
                && e.should_automount
                && e.can_mount
                && (if startup {
                    config.mount_on_startup
                } else {
                    config.mount_removable
                })
        })
        .map(|e| e.id.clone())
        .collect()
}
