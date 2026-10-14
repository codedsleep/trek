//! Plain volume inventory and GIO change notifications.
pub mod autorun;
mod operations;
use gio::prelude::*;
pub use operations::*;
use std::{cell::Cell, path::PathBuf, rc::Rc};
/// Widget-free volume entry, suitable for Places rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Stable id within one monitor lifetime.
    pub id: String,
    /// Visible name.
    pub name: String,
    /// Serialized GIO icon.
    pub icon: Option<String>,
    /// Current mount root URI.
    pub root: Option<String>,
    /// Native device identifier (for distinguishing internal disks).
    pub device: Option<PathBuf>,
    /// Currently mounted.
    pub mounted: bool,
    /// Removable drive/media.
    pub removable: bool,
    /// Internal fixed disk.
    pub internal: bool,
    /// Eject is supported.
    pub can_eject: bool,
    /// Mount is supported.
    pub can_mount: bool,
    /// Backend recommends automounting.
    pub should_automount: bool,
}
/// Places visibility preferences.
#[derive(Debug, Clone, Copy)]
pub struct Filters {
    /// Include unmounted entries.
    pub places_unmounted: bool,
    /// Include internal fixed disks.
    pub show_internal_volumes: bool,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            places_unmounted: true,
            show_internal_volumes: false,
        }
    }
}
/// Whether this volume should be visible in Places.
pub fn visible(entry: &Entry, filters: Filters) -> bool {
    (!entry.internal || filters.show_internal_volumes)
        && (entry.mounted || filters.places_unmounted)
}
fn id(v: &gio::Volume) -> String {
    v.uuid()
        .or_else(|| v.identifier("unix-device"))
        .map_or_else(|| format!("volume:{:p}", v.as_ptr()), |s| s.into())
}
fn volume_entry(v: &gio::Volume) -> Entry {
    let drive = v.drive();
    let removable = drive
        .as_ref()
        .is_some_and(|d| d.is_removable() || d.is_media_removable());
    let device = v
        .identifier("unix-device")
        .map(|s| PathBuf::from(s.as_str()));
    let mount = v.get_mount();
    Entry {
        id: id(v),
        name: v.name().into(),
        icon: v.icon().to_string().map(|s| s.into()),
        root: mount.as_ref().map(|m| m.root().uri().into()),
        device: device.clone(),
        mounted: mount.is_some(),
        removable,
        internal: device.is_some() && !removable,
        can_eject: v.can_eject() || mount.as_ref().is_some_and(|m| m.can_eject()),
        can_mount: v.can_mount(),
        should_automount: v.should_automount(),
    }
}
fn entries(m: &gio::VolumeMonitor, filters: Filters) -> Vec<Entry> {
    let mut entries: Vec<_> = m
        .volumes()
        .iter()
        .map(volume_entry)
        .filter(|e| visible(e, filters))
        .collect();
    for mount in m.mounts() {
        if mount.volume().is_none() && !mount.is_shadowed() {
            entries.push(Entry {
                id: mount.root().uri().into(),
                name: mount.name().into(),
                icon: mount.icon().to_string().map(|s| s.into()),
                root: Some(mount.root().uri().into()),
                device: None,
                mounted: true,
                removable: mount
                    .drive()
                    .is_some_and(|d| d.is_removable() || d.is_media_removable()),
                internal: false,
                can_eject: mount.can_eject(),
                can_mount: false,
                should_automount: false,
            });
        }
    }
    entries.sort_by(|a, b| {
        glib::FilenameCollationKey::from(&a.name).cmp(&glib::FilenameCollationKey::from(&b.name))
    });
    entries
}
struct Inner {
    monitor: gio::VolumeMonitor,
    filters: Cell<Filters>,
    tx: async_channel::Sender<Vec<Entry>>,
}
/// Owns signal subscriptions, disconnecting them on drop. Construct on the main context.
pub struct Model {
    inner: Rc<Inner>,
    signals: Vec<glib::SignalHandlerId>,
    /// Full filtered snapshots after inventory changes.
    pub changes: async_channel::Receiver<Vec<Entry>>,
}
impl Model {
    /// Subscribe to the system GIO volume inventory.
    pub fn new(filters: Filters) -> Self {
        let monitor = gio::VolumeMonitor::get();
        let (tx, changes) = async_channel::unbounded();
        let inner = Rc::new(Inner {
            monitor,
            filters: Cell::new(filters),
            tx,
        });
        let mut signals = vec![];
        macro_rules! connect {
            ($method:ident) => {{
                let weak = Rc::downgrade(&inner);
                signals.push(inner.monitor.$method(move |monitor, _| {
                    if let Some(inner) = weak.upgrade() {
                        let _ = inner.tx.try_send(entries(monitor, inner.filters.get()));
                    }
                }));
            }};
        }
        connect!(connect_volume_added);
        connect!(connect_volume_changed);
        connect!(connect_volume_removed);
        connect!(connect_mount_added);
        connect!(connect_mount_changed);
        connect!(connect_mount_removed);
        connect!(connect_drive_changed);
        connect!(connect_drive_connected);
        connect!(connect_drive_disconnected);
        Self {
            inner,
            signals,
            changes,
        }
    }
    /// Current plain entries.
    pub fn snapshot(&self) -> Vec<Entry> {
        entries(&self.inner.monitor, self.inner.filters.get())
    }
    /// Unfiltered inventory for automount decisions, independent of Places preferences.
    pub fn inventory(&self) -> Vec<Entry> {
        entries(
            &self.inner.monitor,
            Filters {
                places_unmounted: true,
                show_internal_volumes: true,
            },
        )
    }
    /// Update visibility and publish a fresh snapshot.
    pub fn set_filters(&self, filters: Filters) {
        self.inner.filters.set(filters);
        let _ = self.inner.tx.try_send(self.snapshot());
    }
    /// Resolve a current volume id into the GIO operation object.
    pub fn volume(&self, key: &str) -> Option<gio::Volume> {
        self.inner
            .monitor
            .volumes()
            .into_iter()
            .find(|v| id(v) == key)
    }
    /// Resolve a current entry to its mounted object.
    pub fn mounted(&self, key: &str) -> Option<gio::Mount> {
        self.volume(key).and_then(|v| v.get_mount()).or_else(|| {
            self.inner
                .monitor
                .mounts()
                .into_iter()
                .find(|m| m.root().uri() == key)
        })
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        for id in self.signals.drain(..) {
            self.inner.monitor.disconnect(id);
        }
        self.inner.tx.close();
    }
}
