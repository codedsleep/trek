//! The window's side of opening things: which items, which folder.

use super::MainWindow;
use crate::{dialogs, open::Opener, view::FileItem};
use gtk::{gio, prelude::*};
use tallboy_fs::terminal;

impl MainWindow {
    pub(super) fn opener(&self) -> Opener {
        Opener::new(&self.0.app, Some(self))
    }

    /// Opens activated items.
    pub fn open_items(&self, items: Vec<FileItem>) {
        self.opener().open(items);
    }

    /// Edit ▸ Open With for the selection: the dialog is for the type of the
    /// first selected file, and opens every selected file of that type.
    pub(super) fn open_selection_with(&self) {
        let Some(tab) = self.active_tab() else { return };
        let selected = tab.targets();
        let Some(first) = selected.iter().find(|item| !item.is_dir()) else {
            return;
        };
        let content_type = first.meta().content_type.to_string();
        let files: Vec<gio::File> = selected
            .iter()
            .filter(|item| *item.meta().content_type == *content_type)
            .map(|item| gio::File::for_uri(&item.meta().uri))
            .collect();
        self.opener().open_with(files, &content_type);
    }

    /// An application picked from the Open With submenu opens every
    /// selected file.
    pub(super) fn open_selection_with_app(&self, desktop_id: &str) {
        let Some(tab) = self.active_tab() else { return };
        let files: Vec<gio::File> = tab
            .targets()
            .iter()
            .filter(|item| !item.is_dir())
            .map(|item| gio::File::for_uri(&item.meta().uri))
            .collect();
        let app = gio::AppInfo::all()
            .into_iter()
            .find(|app| app.id().as_deref() == Some(desktop_id));
        if let (Some(app), false) = (app, files.is_empty()) {
            self.opener().open_with_app(files, app);
        }
    }

    pub(super) fn current_folder(&self) -> Option<gio::File> {
        self.active_tab()
            .map(|tab| gio::File::for_uri(tab.location().to_uri()))
    }

    /// Tools ▸ Run a Command in Current Folder…
    pub(super) fn run_command(&self) {
        let Some(directory) = self.current_folder() else {
            return;
        };
        let app = self.0.app.clone();
        dialogs::run::show(self.0.window.upcast_ref(), move |request| {
            let terminal = if request.in_terminal {
                match crate::open::resolve_terminal(&app) {
                    Ok(terminal) => Some(terminal),
                    Err(e) => return Some(dialogs::error::terminal(&e)),
                }
            } else {
                None
            };
            match terminal::run(
                &request.command,
                &directory,
                terminal.as_ref(),
                request.keep_open,
            ) {
                Ok(child) => {
                    crate::open::reap(child);
                    None
                }
                Err(e) => Some(dialogs::error::terminal(&e)),
            }
        });
    }
}
