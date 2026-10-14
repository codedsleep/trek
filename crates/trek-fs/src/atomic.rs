//! Atomic private-file persistence used by configuration and caches.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
/// Replace a file without following its final symlink; failed writes leave the old file intact.
/// The temporary file is private (0600), in the destination directory, and synced before rename.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("No parent directory"))?;
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("No file name"))?;
    let mut temp = parent.join(name);
    temp.set_file_name(format!(
        ".trek-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err()
        && let Err(e) = fs::remove_file(&temp)
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("Temporary file cleanup: {e}");
    }
    result
}
