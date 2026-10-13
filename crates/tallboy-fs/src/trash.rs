//! Trash state and removable-media policy helpers.
use gio::prelude::*;
/// Whether the location belongs to removable media; unknown mounts are not classified removable.
pub fn is_removable(file: &gio::File) -> bool {
    file.find_enclosing_mount(None::<&gio::Cancellable>)
        .ok()
        .and_then(|m| m.drive())
        .is_some_and(|d| d.is_removable())
}
/// Whether no_usb_trash requires an explicit permanent-delete offer.
pub fn offer_delete(no_usb_trash: bool, removable: bool) -> bool {
    no_usb_trash && removable
}
/// Query isolated GVFS trash state asynchronously. Errors are surfaced, never treated as empty.
pub async fn is_empty() -> Result<bool, glib::Error> {
    let f = gio::File::for_uri("trash:///");
    let e = f
        .enumerate_children_future(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            glib::Priority::DEFAULT,
        )
        .await?;
    let items = e.next_files_future(1, glib::Priority::DEFAULT).await?;
    e.close_future(glib::Priority::DEFAULT).await?;
    Ok(items.is_empty())
}
