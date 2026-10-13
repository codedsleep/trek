//! Directory-only projections for lazy trees and location completion.
use crate::{
    folder::{Folder, FolderError, FolderEvent, Options},
    meta::FileMeta,
};
use gio::prelude::*;
/// Lazily constructed monitored directory children. Drop closes the monitor.
pub struct Children {
    folder: Folder,
    show_hidden: bool,
}
impl Children {
    /// Construct only when a tree node expands or location completion is requested.
    pub fn new(
        file: &gio::File,
        show_hidden: bool,
        mount: Option<gio::MountOperation>,
    ) -> Result<Self, FolderError> {
        Ok(Self {
            folder: Folder::new(file, Options::default(), mount)?,
            show_hidden,
        })
    }
    /// Next directory-only batch/change. Ordinary file changes are ignored or removed.
    pub async fn recv(&self) -> Result<FolderEvent, async_channel::RecvError> {
        loop {
            let event = self.folder.events.recv().await?;
            let visible = |m: &FileMeta| {
                m.file_type == gio::FileType::Directory && (self.show_hidden || !m.hidden)
            };
            let event = match event {
                FolderEvent::Batch(mut items) => {
                    items.retain(visible);
                    FolderEvent::Batch(items)
                }
                FolderEvent::Added(m) if !visible(&m) => continue,
                FolderEvent::Changed(m) if !visible(&m) => FolderEvent::Removed(m.uri),
                FolderEvent::Renamed { from, to } if !visible(&to) => FolderEvent::Removed(from),
                e => e,
            };
            return Ok(event);
        }
    }
    /// Stop loading/monitoring immediately.
    pub fn cancel(&self) {
        self.folder.cancel();
    }
}
/// Probe until the first visible directory; does not enumerate the remaining children.
pub async fn has_subdirectories(file: &gio::File, show_hidden: bool) -> Result<bool, glib::Error> {
    let e = file
        .enumerate_children_future(
            "standard::type,standard::is-hidden",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await?;
    let result = loop {
        let items = e.next_files_future(50, glib::Priority::DEFAULT).await?;
        if items.is_empty() {
            break false;
        }
        if items
            .iter()
            .any(|i| i.file_type() == gio::FileType::Directory && (show_hidden || !i.is_hidden()))
        {
            break true;
        }
    };
    e.close_future(glib::Priority::DEFAULT).await?;
    Ok(result)
}
/// Directory-name completion, sorting matching display names with GLib collation.
pub async fn prefix(
    file: &gio::File,
    text: &str,
    show_hidden: bool,
) -> Result<Vec<FileMeta>, FolderError> {
    let children = Children::new(file, show_hidden, None)?;
    let mut result = vec![];
    loop {
        match children
            .recv()
            .await
            .map_err(|_| glib::Error::new(gio::IOErrorEnum::Cancelled, "Completion cancelled"))?
        {
            FolderEvent::Batch(items) => result.extend(
                items
                    .into_iter()
                    .filter(|i| i.display_name.starts_with(text)),
            ),
            FolderEvent::Finished => break,
            FolderEvent::Error(e) => return Err(e),
            _ => {}
        }
    }
    result.sort_by(|a, b| crate::meta::compare(a, b, &Default::default()));
    Ok(result)
}
