//! Navigation: history, parent, reload, and the view state that goes with them.

use super::{Change, Restore, TabPage, location_from_uri};
use gtk::{gio, prelude::*};
use tallboy_core::location::Location;

impl TabPage {
    /// Shows `location`, recording it in the history.
    pub fn navigate(&self, location: Location) {
        self.remember_view_state();
        self.0.history.borrow_mut().navigate(location.clone());
        self.0.restore.replace(Restore::default());
        self.show(location);
    }

    /// Goes to what the fuzzy finder found: into a folder, or to the folder
    /// of a file with the cursor on the file.
    pub fn jump_to(&self, path: &std::path::Path, enter: bool) {
        let file = gio::File::for_path(path);
        let target = if enter {
            Some(file.clone())
        } else {
            file.parent()
        };
        let Some(folder) = target.and_then(|folder| location_from_uri(&folder.uri())) else {
            return;
        };
        if folder.to_uri() != self.0.location.borrow().to_uri() {
            self.navigate(folder);
        } else if !enter {
            // Found in the folder shown, where the `f` filter may hide it.
            self.set_needle(None);
        }
        if !enter {
            self.select_when_listed(&file.uri());
        }
    }

    /// Loads the current folder again, keeping scroll position and selection.
    pub fn reload(&self) {
        let cursor = self.cursor_uri();
        let scroll = self.scroll_adjustment().value();
        self.0.restore.replace(Restore {
            cursor,
            scroll: Some(scroll),
        });
        self.start_load();
    }

    /// Goes to the parent folder and selects the folder just left (§8.4).
    pub fn go_up(&self) {
        let current = gio::File::for_uri(self.0.location.borrow().to_uri());
        let Some(parent) = current.parent() else {
            return;
        };
        let Some(location) = location_from_uri(&parent.uri()) else {
            return;
        };
        self.navigate(location);
        self.0.restore.replace(Restore {
            cursor: Some(current.uri().into()),
            scroll: None,
        });
    }

    /// Whether Parent Folder can do anything here.
    pub fn can_go_up(&self) -> bool {
        gio::File::for_uri(self.0.location.borrow().to_uri())
            .parent()
            .is_some()
    }

    pub fn can_go_back(&self) -> bool {
        self.0.history.borrow().can_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.0.history.borrow().can_forward()
    }

    /// Steps back (`forward == false`) or forward in the history.
    pub fn go_history(&self, forward: bool) {
        self.remember_view_state();
        let entry = {
            let mut history = self.0.history.borrow_mut();
            if forward {
                history.forward().cloned()
            } else {
                history.back().cloned()
            }
        };
        if let Some(entry) = entry {
            self.0.restore.replace(Restore {
                cursor: entry.cursor,
                scroll: Some(entry.scroll),
            });
            self.show(entry.location);
        }
    }

    /// The history as menu labels, oldest first, and the current index.
    pub fn history_labels(&self) -> (Vec<String>, usize) {
        let history = self.0.history.borrow();
        let labels = history
            .entries()
            .iter()
            .map(|entry| entry.location.display())
            .collect();
        (labels, history.index())
    }

    /// Jumps to a history entry chosen from the menu.
    pub fn go_to_history(&self, index: usize) {
        if index == self.0.history.borrow().index() {
            return;
        }
        self.remember_view_state();
        let entry = self.0.history.borrow_mut().go_to(index).cloned();
        if let Some(entry) = entry {
            self.0.restore.replace(Restore {
                cursor: entry.cursor,
                scroll: Some(entry.scroll),
            });
            self.show(entry.location);
        }
    }

    fn show(&self, location: Location) {
        self.end_visual();
        // A filter belongs to the folder it was typed in.
        self.0.model.set_needle(None);
        self.0.location.replace(location);
        // A folder may carry its own view mode, sort order and columns.
        self.load_folder_settings();
        self.start_load();
        self.emit(Change::Location);
    }

    fn remember_view_state(&self) {
        let cursor = self.cursor_uri();
        let scroll = self.scroll_adjustment().value();
        if let Some(entry) = self.0.history.borrow_mut().current_mut() {
            entry.cursor = cursor;
            entry.scroll = scroll;
        }
    }

    fn cursor_uri(&self) -> Option<String> {
        self.cursor_item().map(|item| item.meta().uri.clone())
    }
}
