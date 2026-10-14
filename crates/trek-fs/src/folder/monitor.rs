//! Debounced monitoring, using weak ownership throughout.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::Duration,
};
#[derive(Clone)]
enum Pending {
    Add(gio::File),
    Change(gio::File),
    Remove(String),
    Rename(String, gio::File),
    RootGone,
}
pub(super) fn start(inner: &Rc<Inner>) -> Option<gio::FileMonitor> {
    let monitor = match inner
        .file
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, Some(&inner.cancel))
    {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Folder monitor: {e}");
            return None;
        }
    };
    let pending = Rc::new(RefCell::new(BTreeMap::<String, Pending>::new()));
    let scheduled = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(inner);
    monitor.connect_changed(move |_, file, other, event| {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        if inner.cancel.is_cancelled() {
            return;
        }
        let uri = file.uri().to_string();
        let next = if file.equal(&inner.file) {
            // An event about the watched folder itself. Only its loss
            // matters; a change to its own attributes (a chmod, or the trash
            // can's item count) must not be mistaken for a child changing.
            matches!(
                event,
                gio::FileMonitorEvent::Deleted | gio::FileMonitorEvent::Unmounted
            )
            .then_some(Pending::RootGone)
        } else {
            match event {
                gio::FileMonitorEvent::Created | gio::FileMonitorEvent::MovedIn => {
                    Some(Pending::Add(file.clone()))
                }
                gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::AttributeChanged => {
                    Some(Pending::Change(file.clone()))
                }
                gio::FileMonitorEvent::Deleted | gio::FileMonitorEvent::MovedOut => {
                    Some(Pending::Remove(uri.clone()))
                }
                gio::FileMonitorEvent::Renamed | gio::FileMonitorEvent::Moved => {
                    other.map(|to| Pending::Rename(uri.clone(), to.clone()))
                }
                _ => None,
            }
        };
        let Some(next) = next else {
            return;
        };
        {
            let mut p = pending.borrow_mut();
            let combined = match (p.get(&uri), &next) {
                (Some(Pending::Add(_)), Pending::Change(_)) => p.get(&uri).cloned().unwrap_or(next),
                _ => next,
            };
            p.insert(uri, combined);
        }
        if !scheduled.replace(true) {
            let weak = weak.clone();
            let pending = pending.clone();
            let scheduled = scheduled.clone();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                glib::timeout_future(Duration::from_millis(200)).await;
                scheduled.set(false);
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let changes = std::mem::take(&mut *pending.borrow_mut());
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let cancel = inner.cancel.clone();
                    let _ = gio::CancellableFuture::new(
                        async {
                            for change in changes.into_values() {
                                match change {
                                    Pending::RootGone => inner.send(FolderEvent::RootGone),
                                    Pending::Remove(uri) => inner.send(FolderEvent::Removed(uri)),
                                    Pending::Add(ref file)
                                    | Pending::Change(ref file)
                                    | Pending::Rename(_, ref file) => {
                                        match file
                                            .query_info_future(
                                                &crate::meta::attributes(
                                                    inner.options.defer_content_test,
                                                ),
                                                gio::FileQueryInfoFlags::NONE,
                                                glib::Priority::DEFAULT,
                                            )
                                            .await
                                        {
                                            Ok(info) => {
                                                let fs = inner.filesystem.get().cloned().flatten();
                                                let meta = FileMeta::from_info(
                                                    file,
                                                    &info,
                                                    inner.options.backup_as_hidden,
                                                    fs.as_deref(),
                                                );
                                                let event = match change {
                                                    Pending::Add(_) => FolderEvent::Added(meta),
                                                    Pending::Change(_) => {
                                                        FolderEvent::Changed(meta)
                                                    }
                                                    Pending::Rename(from, _) => {
                                                        FolderEvent::Renamed { from, to: meta }
                                                    }
                                                    Pending::Remove(_) | Pending::RootGone => {
                                                        continue;
                                                    }
                                                };
                                                inner.send(event);
                                            }
                                            Err(e) if e.matches(gio::IOErrorEnum::NotFound) => {
                                                inner.send(FolderEvent::Removed(
                                                    file.uri().to_string(),
                                                ))
                                            }
                                            Err(e) => inner.send(FolderEvent::Error(e.into())),
                                        }
                                    }
                                }
                            }
                        },
                        cancel,
                    )
                    .await;
                });
            });
        }
    });
    Some(monitor)
}
