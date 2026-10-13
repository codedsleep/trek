//! Volumes in the UI (PROMPT §8.17): mount, unmount and eject with their
//! prompts, automount, autorun, and what happens to tabs when a mount goes.
//! The inventory and the policies are `fs::volumes`.

use crate::{
    app::App,
    dialogs::{self, base},
    tab::location_from_uri,
};
use gtk::{gio, glib, prelude::*};
use tallboy_core::{config::AutorunChoice, location::Location};
use tallboy_fs::volumes::{self, Entry, Filters, autorun};

/// The choice stored in `[autorun]` for "Open in File Manager".
const FILE_MANAGER: &str = "tallboy";

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
    let parent = parent.cloned();
    glib::spawn_future_local(async move {
        let operation = operation(parent.as_ref());
        let result = match (mounted, volume) {
            (Some(mount), _) => volumes::eject(&mount, &operation).await,
            (None, Some(volume)) => volumes::eject_volume(&volume, &operation).await,
            (None, None) => return,
        };
        if let Err(e) = result {
            report(parent.as_ref(), "The medium could not be ejected", &e);
        }
    });
}

/// `mount_on_startup`: mount every volume that wants automounting.
pub fn startup(app: &App) {
    let inventory = app.volumes().inventory();
    for id in autorun::automount_plan(&inventory, &app.settings().config(), true) {
        mount(app, None, &id, |_| {});
    }
}

/// Reacts to a change of the inventory: automounts newly inserted media,
/// offers autorun for what it mounted, and tells windows about lost mounts.
pub fn changed(app: &App, before: &[Entry], after: &[Entry]) {
    let config = app.settings().config();
    let old = |id: &str| before.iter().find(|entry| entry.id == id);

    let inserted: Vec<Entry> = after
        .iter()
        .filter(|entry| old(&entry.id).is_none())
        .cloned()
        .collect();
    for id in autorun::automount_plan(&inserted, &config, false) {
        app.note_automount(&id);
        mount(app, None, &id, |_| {});
    }

    for entry in after.iter().filter(|entry| entry.mounted) {
        let newly = old(&entry.id).is_none_or(|was| !was.mounted);
        // Autorun only follows a mount we made for inserted media (§8.17).
        if newly && app.take_automount(&entry.id) && autorun::prompt_enabled(&config) {
            offer_autorun(app, entry.clone());
        }
    }

    for gone in before.iter().filter(|entry| entry.mounted) {
        let still = after
            .iter()
            .any(|entry| entry.id == gone.id && entry.mounted);
        if !still && let Some(root) = &gone.root {
            app.mount_gone(&gio::File::for_uri(root));
        }
    }
}

fn open_in_file_manager(app: &App, root: Location) {
    // `media_in_new_tab`: a tab in the last window rather than a window.
    let new_window = !app.settings().media_in_new_tab();
    app.open_folder(root, new_window);
}

fn perform(app: &App, choice: &str, root: &Location) {
    if choice == FILE_MANAGER {
        open_in_file_manager(app, root.clone());
        return;
    }
    let Some(handler) = gio::AppInfo::all()
        .into_iter()
        .find(|candidate| candidate.id().as_deref() == Some(choice))
    else {
        open_in_file_manager(app, root.clone());
        return;
    };
    let context = gtk::gdk::Display::default().map(|display| display.app_launch_context());
    if let Err(e) = handler.launch_uris(&[root.to_uri()], context.as_ref()) {
        log::warn!("Autorun: {e}");
    }
}

fn offer_autorun(app: &App, entry: Entry) {
    let (Some(mount), Some(root)) = (
        app.volumes().mounted(&entry.id),
        entry.root.as_deref().and_then(location_from_uri),
    ) else {
        return;
    };
    let app = app.clone();
    glib::spawn_future_local(async move {
        let types = autorun::content_types(&mount).await.unwrap_or_default();
        let Some(content_type) = types.into_iter().next() else {
            return;
        };
        let config = app.settings().config();
        if let Some(choice) = autorun::choice(&config, &content_type)
            && choice.remembered
        {
            perform(&app, &choice.app, &root);
            return;
        }
        // Unix software is never remembered: it must ask every time (D15).
        let may_remember = content_type != "x-content/unix-software";
        let parent = app.active_window().map(|w| w.widget().clone().upcast());
        let apps = autorun::applications(&content_type);
        let app_for_choice = app.clone();
        let content = content_type.clone();
        dialogs::autorun::show(
            parent.as_ref(),
            &entry.name,
            &content_type,
            apps,
            may_remember,
            move |choice| {
                let id = choice
                    .app
                    .as_ref()
                    .and_then(|handler| handler.id())
                    .map_or_else(|| FILE_MANAGER.to_owned(), |id| id.to_string());
                if choice.remember {
                    let mut config = app_for_choice.settings().config();
                    autorun::remember(
                        &mut config,
                        &content,
                        AutorunChoice {
                            app: id.clone(),
                            remembered: true,
                        },
                    );
                    crate::settings::set::autorun_choices(
                        app_for_choice.settings(),
                        config.autorun_choices,
                    );
                }
                perform(&app_for_choice, &id, &root);
            },
        );
    });
}
