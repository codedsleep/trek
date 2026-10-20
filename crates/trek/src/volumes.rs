//! Volumes in the UI (PROMPT §8.17): mount, unmount and eject with their
//! prompts, automount, and what happens to tabs when a mount goes. An
//! inserted medium is only mounted and shown in the toolbar's devices
//! button; nothing opens by itself. The inventory and the policies are
//! `fs::volumes`.

use crate::{
    app::App,
    dialogs::{self, base},
    tab::location_from_uri,
};
use gtk::{gio, glib, prelude::*};
use trek_core::location::Location;
use trek_fs::volumes::{self, Entry, Filters, automount};

/// Places visibility from the settings.
pub fn filters(app: &App) -> Filters {
    let settings = app.settings();
    Filters {
        places_unmounted: settings.places_unmounted(),
        show_internal_volumes: settings.show_internal_volumes(),
    }
}

fn operation(parent: Option<&gtk::Window>) -> gio::MountOperation {
    gtk::MountOperation::new(parent).upcast()
}

/// A failed volume operation as a dialog. A prompt the user cancelled has
/// already been "handled" and says nothing more.
fn report(parent: Option<&gtk::Window>, title: &str, error: &volumes::Error) {
    if error.0.matches(gio::IOErrorEnum::FailedHandled) {
        return;
    }
    let detail = if error.0.matches(gio::IOErrorEnum::Busy) {
        format!(
            "The volume is in use. Close the files and folders on it and try again.\n\n{}",
            error.0.message()
        )
    } else {
        dialogs::error::io(&error.0)
    };
    base::alert(parent, title, &detail);
}

fn root_of(app: &App, id: &str) -> Option<Location> {
    app.volumes()
        .inventory()
        .into_iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.root)
        .and_then(|root| location_from_uri(&root))
}

/// Mounts a volume; `then` gets its root once it is mounted.
pub fn mount(
    app: &App,
    parent: Option<&gtk::Window>,
    id: &str,
    then: impl FnOnce(Location) + 'static,
) {
    if let Some(root) = root_of(app, id) {
        then(root);
        return;
    }
    let Some(volume) = app.volumes().volume(id) else {
        return;
    };
    let (app, parent, id) = (app.clone(), parent.cloned(), id.to_owned());
    glib::spawn_future_local(async move {
        match volumes::mount(&volume, &operation(parent.as_ref())).await {
            Ok(()) => {
                // The mount's root: from the volume itself, in case the
                // inventory has not caught up yet.
                let root = volume
                    .get_mount()
                    .and_then(|mount| location_from_uri(&mount.root().uri()))
                    .or_else(|| root_of(&app, &id));
                if let Some(root) = root {
                    then(root);
                }
            }
            Err(e) => report(parent.as_ref(), "The volume could not be mounted", &e),
        }
    });
}

/// Unmounts a volume. Never forced: a busy volume reports the error.
pub fn unmount(app: &App, parent: Option<&gtk::Window>, id: &str) {
    let Some(mount) = app.volumes().mounted(id) else {
        return;
    };
    let parent = parent.cloned();
    glib::spawn_future_local(async move {
        if let Err(e) = volumes::unmount(&mount, &operation(parent.as_ref())).await {
            report(parent.as_ref(), "The volume could not be unmounted", &e);
        }
    });
}

/// Ejects removable media, mounted or not.
pub fn eject(app: &App, parent: Option<&gtk::Window>, id: &str) {
    let mounted = app.volumes().mounted(id);
    let volume = app.volumes().volume(id);
    let name = app
        .volumes()
        .inventory()
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.name);
    let (app, parent) = (app.clone(), parent.cloned());
    glib::spawn_future_local(async move {
        let operation = operation(parent.as_ref());
        let result = match (mounted, volume) {
            (Some(mount), _) => volumes::eject(&mount, &operation).await,
            (None, Some(volume)) => volumes::eject_volume(&volume, &operation).await,
            (None, None) => return,
        };
        match result {
            Ok(()) => {
                if let Some(name) = name {
                    app.volume_event(&format!("“{name}” ejected — safe to unplug"), false);
                }
            }
            Err(e) => report(parent.as_ref(), "The medium could not be ejected", &e),
        }
    });
}

/// `mount_on_startup`: mount every volume that wants automounting.
pub fn startup(app: &App) {
    let inventory = app.volumes().inventory();
    for id in automount::plan(&inventory, &app.settings().config(), true) {
        mount(app, None, &id, |_| {});
    }
}

/// Reacts to a change of the inventory: automounts newly inserted media,
/// tells windows about lost mounts, and says in the status line what
/// happened to drives that are not fixed disks.
pub fn changed(app: &App, before: &[Entry], after: &[Entry]) {
    let config = app.settings().config();
    let old = |id: &str| before.iter().find(|entry| entry.id == id);

    let inserted: Vec<Entry> = after
        .iter()
        .filter(|entry| old(&entry.id).is_none())
        .cloned()
        .collect();
    let plan = automount::plan(&inserted, &config, false);
    for entry in inserted
        .iter()
        .filter(|entry| !entry.internal && !entry.mounted)
    {
        if plan.contains(&entry.id) {
            app.volume_event(&format!("“{}” connected — mounting…", entry.name), false);
        } else {
            announce_unmounted(app, entry);
        }
    }
    for id in plan {
        mount(app, None, &id, |_| {});
    }

    for entry in after.iter().filter(|entry| entry.mounted) {
        let newly = old(&entry.id).is_none_or(|was| !was.mounted);
        if newly && !entry.internal {
            let place = crate::window::device_place(entry).unwrap_or_default();
            app.volume_event(
                &format!("“{}” mounted at {place} — g m to open it", entry.name),
                true,
            );
        }
    }

    for gone in before.iter().filter(|entry| entry.mounted) {
        let now = after.iter().find(|entry| entry.id == gone.id);
        if now.is_some_and(|entry| entry.mounted) {
            continue;
        }
        if let Some(root) = &gone.root {
            app.mount_gone(&gio::File::for_uri(root));
        }
        if now.is_some() && !gone.internal {
            app.volume_event(&format!("“{}” unmounted", gone.name), false);
        }
    }
}

/// Points out an inserted volume that was left unmounted. A device being
/// torn down shows up for a moment as a nameless "Volume"; only one that is
/// still there a second later is worth a word.
fn announce_unmounted(app: &App, entry: &Entry) {
    let (weak, id, name) = (app.downgrade(), entry.id.clone(), entry.name.clone());
    glib::timeout_add_local_once(std::time::Duration::from_secs(1), move || {
        let Some(app) = weak.upgrade() else { return };
        let waiting = app
            .volumes()
            .inventory()
            .iter()
            .any(|entry| entry.id == id && !entry.mounted);
        if waiting {
            app.volume_event(&format!("“{name}” connected — g m to mount it"), false);
        }
    });
}
