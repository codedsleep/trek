//! Async mount operations with caller-supplied credential/progress prompts.
use gio::prelude::*;
use tallboy_core::location::Location;
/// A volume operation failed; original GIO error (including Busy) is retained.
#[derive(Debug, thiserror::Error)]
#[error("Volume operation: {0}")]
pub struct Error(#[from] pub glib::Error);
/// Mount without changing mount policy or installing a GTK dependency.
pub async fn mount(volume: &gio::Volume, operation: &gio::MountOperation) -> Result<(), Error> {
    volume
        .mount_future(gio::MountMountFlags::NONE, Some(operation))
        .await?;
    Ok(())
}
/// Unmount safely, never FORCE. Busy failures propagate to the UI.
pub async fn unmount(mount: &gio::Mount, operation: &gio::MountOperation) -> Result<(), Error> {
    mount
        .unmount_with_operation_future(gio::MountUnmountFlags::NONE, Some(operation))
        .await?;
    Ok(())
}
/// Eject a mounted device without forcing an unmount.
pub async fn eject(mount: &gio::Mount, operation: &gio::MountOperation) -> Result<(), Error> {
    mount
        .eject_with_operation_future(gio::MountUnmountFlags::NONE, Some(operation))
        .await?;
    Ok(())
}
/// Eject an unmounted removable volume.
pub async fn eject_volume(
    volume: &gio::Volume,
    operation: &gio::MountOperation,
) -> Result<(), Error> {
    volume
        .eject_with_operation_future(gio::MountUnmountFlags::NONE, Some(operation))
        .await?;
    Ok(())
}
/// Indices of open locations inside a mount root, including the root itself.
pub fn locations_inside(root: &gio::File, locations: &[Location]) -> Vec<usize> {
    locations
        .iter()
        .enumerate()
        .filter_map(|(index, l)| {
            let file = gio::File::for_uri(l.to_uri());
            (file.equal(root) || file.has_prefix(root)).then_some(index)
        })
        .collect()
}
