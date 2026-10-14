//! Cancellable folder enumeration on a worker thread and coalesced monitoring.
mod load;
mod monitor;
use crate::meta::{FileMeta, TypeFacts};
use gio::prelude::*;
use std::{
    rc::Rc,
    sync::{Arc, OnceLock},
};
/// A folder-loading error, ready for the application's error mapper.
#[derive(Debug, Clone, thiserror::Error)]
pub enum FolderError {
    /// Backend query error.
    #[error("{0}")]
    Io(#[from] glib::Error),
    /// Location denied or malformed.
    #[error("{0}")]
    Location(#[from] trek_core::location::LocationError),
}
/// Filesystem capacity and identity inherited by listed children.
#[derive(Debug, Clone, Default)]
pub struct FsInfo {
    /// Available bytes, if provided.
    pub free: Option<u64>,
    /// Total bytes.
    pub total: Option<u64>,
    /// Filesystem type.
    pub filesystem_type: Option<String>,
    /// Identity for drop decisions.
    pub id: Option<String>,
}
/// Changes delivered to the UI; batches should be inserted with one splice.
#[derive(Debug, Clone)]
pub enum FolderEvent {
    /// Loading begins.
    Started,
    /// About 200 items at once.
    Batch(Vec<FileMeta>),
    /// Initial loading is complete.
    Finished,
    /// User-facing error.
    Error(FolderError),
    /// Item added.
    Added(FileMeta),
    /// Metadata changed.
    Changed(FileMeta),
    /// Item removed, by URI.
    Removed(String),
    /// Renamed item, retaining old URI.
    Renamed {
        /// Old URI.
        from: String,
        /// New metadata.
        to: FileMeta,
    },
    /// Capacity information.
    FsInfo(FsInfo),
    /// Files listed by name whose content has now been read, by URI. Sent
    /// after `Finished`, also for a file whose type stayed the same.
    Typed(Vec<(String, TypeFacts)>),
    /// Folder disappeared or was unmounted.
    RootGone,
}
/// Listing preferences captured for one load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Treat backup names as hidden.
    pub backup_as_hidden: bool,
    /// Every listing goes by name first. Without this the files a name
    /// does not settle are read once the listing is complete and reported
    /// as `FolderEvent::Typed`; with it nothing is read until a file is used.
    pub defer_content_test: bool,
    /// Number of entries per batch (clamped to 1–1000).
    pub batch_size: i32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            backup_as_hidden: true,
            defer_content_test: false,
            batch_size: 200,
        }
    }
}
struct Inner {
    file: gio::File,
    options: Options,
    cancel: gio::Cancellable,
    tx: async_channel::Sender<FolderEvent>,
    /// The folder's filesystem type, set by the listing and read by the
    /// monitor for files that appear later.
    filesystem: Arc<OnceLock<Option<String>>>,
}
impl Inner {
    fn send(&self, event: FolderEvent) {
        if !self.cancel.is_cancelled() {
            let _ = self.tx.try_send(event);
        }
    }
}
/// A listed and watched folder. Construct on an acquired GLib main context;
/// drop cancels load and monitor, without holding references to any GTK widget.
pub struct Folder {
    inner: Rc<Inner>,
    monitor: Option<gio::FileMonitor>,
    /// Receive events asynchronously on the main context.
    pub events: async_channel::Receiver<FolderEvent>,
}
impl Folder {
    /// Start a load and monitoring. Mount operations are supplied by the caller.
    /// Locations pass through core validation, including the admin scheme block.
    pub fn new(
        file: &gio::File,
        options: Options,
        mount_operation: Option<gio::MountOperation>,
    ) -> Result<Self, FolderError> {
        trek_core::location::Location::parse(
            &file.uri(),
            std::path::Path::new("/"),
            std::path::Path::new("/"),
        )?;
        let (tx, events) = async_channel::unbounded();
        let inner = Rc::new(Inner {
            file: file.clone(),
            options,
            cancel: gio::Cancellable::new(),
            tx,
            filesystem: Default::default(),
        });
        let monitor = monitor::start(&inner);
        let task = inner.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let cancel = task.cancel.clone();
            let _ = gio::CancellableFuture::new(load::run(task, mount_operation), cancel).await;
        });
        Ok(Self {
            inner,
            monitor,
            events,
        })
    }
    /// Cancel outstanding I/O and discard queued events. No further events are sent.
    pub fn cancel(&self) {
        self.inner.cancel.cancel();
        if let Some(m) = &self.monitor {
            m.cancel();
        }
        self.events.close();
        while self.events.try_recv().is_ok() {}
    }
    /// Whether changes in the folder are reported. Without a monitor the
    /// listing is only as fresh as its load.
    pub fn is_watched(&self) -> bool {
        self.monitor.is_some()
    }
    /// The underlying folder URI.
    pub fn uri(&self) -> String {
        self.inner.file.uri().into()
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        self.cancel();
    }
}
/// Find the nearest existing ancestor asynchronously; None lets the UI use Home.
pub async fn nearest_existing_ancestor(file: &gio::File) -> Option<gio::File> {
    let mut parent = file.parent();
    while let Some(p) = parent {
        if p.query_info_future(
            "standard::type",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await
        .is_ok()
        {
            return Some(p);
        }
        parent = p.parent();
    }
    None
}
