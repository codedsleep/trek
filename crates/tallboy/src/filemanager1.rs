//! The `org.freedesktop.FileManager1` service (PROMPT §8.27): what other
//! programs call for "Show in folder". The name is asked for without
//! replacing or queueing behind another file manager, and only by the
//! default profile, so a development or test profile never takes it from the
//! file manager the desktop already uses.

use crate::{app::App, dialogs};
use gtk::{gio, glib, prelude::*};
use tallboy_core::location::Location;
use tallboy_fs::{
    filemanager1::{self, Event},
    meta::{self, FileMeta},
};

/// Set to `1` to offer the service from another profile: agent checks run in
/// a private session bus under `--profile agent`.
const FORCE: &str = "TALLBOY_FILEMANAGER1";

/// Registers the service and answers its calls for as long as the
/// application runs.
pub fn start(app: &App) {
    let forced = std::env::var_os(FORCE).is_some_and(|value| value == "1");
    if app.settings().profile() != "default" && !forced {
        return;
    }
    let Some(connection) = app.gtk().dbus_connection() else {
        return;
    };
    let weak = app.downgrade();
    glib::spawn_future_local(async move {
        let service = match filemanager1::register(&connection).await {
            Ok(service) => service,
            // Another file manager has the name: nothing to do, not an error.
            Err(filemanager1::Error::AlreadyOwned) => {
                log::info!("org.freedesktop.FileManager1 belongs to another file manager");
                return;
            }
            Err(e) => {
                log::warn!("The FileManager1 service could not be started: {e}");
                return;
            }
        };
        while let Ok(event) = service.events.recv().await {
            let Some(app) = weak.upgrade() else { break };
            handle(&app, event).await;
        }
    });
}

fn parent_of(location: &Location) -> Option<(Location, String)> {
    let file = gio::File::for_uri(location.to_uri());
    let folder = Location::parse(&file.parent()?.uri(), "/".as_ref(), "/".as_ref()).ok()?;
    Some((folder, file.uri().into()))
}

async fn handle(app: &App, event: Event) {
    match event {
        Event::ShowFolders {
            locations,
            startup_id,
        } => {
            for location in locations {
                app.show_folder(location, None, &startup_id);
            }
        }
        Event::ShowItems {
            locations,
            startup_id,
        } => {
            // One tab per folder; the first item asked for in it is selected.
            let mut seen: Vec<Location> = Vec::new();
            for location in &locations {
                let Some((folder, item)) = parent_of(location) else {
                    // The root of a filesystem has no folder to be shown in.
                    app.show_folder(location.clone(), None, &startup_id);
                    continue;
                };
                if !seen.contains(&folder) {
                    seen.push(folder.clone());
                    app.show_folder(folder, Some(&item), &startup_id);
                }
            }
        }
        Event::ShowItemProperties {
            locations,
            startup_id,
        } => {
            let mut files = Vec::new();
            for location in &locations {
                let file = gio::File::for_uri(location.to_uri());
                let info = file
                    .query_info_future(
                        &meta::attributes(app.settings().defer_content_test()),
                        gio::FileQueryInfoFlags::NONE,
                        glib::Priority::DEFAULT,
                    )
                    .await;
                match info {
                    Ok(info) => files.push(FileMeta::from_info(
                        &file,
                        &info,
                        app.settings().backup_as_hidden(),
                        None,
                    )),
                    Err(e) => log::warn!("Properties of {}: {e}", location.to_uri()),
                }
            }
            let Some((folder, _)) = locations.first().and_then(parent_of) else {
                return;
            };
            if files.is_empty() {
                return;
            }
            // Properties needs a window to belong to.
            let window = match app.active_window() {
                Some(window) => window,
                None => app.show_folder(folder, None, &startup_id),
            };
            dialogs::properties::show(app, window.widget().upcast_ref(), files, |_| {});
        }
    }
}
