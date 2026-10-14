//! The window's side of the side pane: opening places and bookmarks, volume
//! commands, and tabs that lose their mount.

use super::MainWindow;
use crate::{
    bookmarks,
    sidepane::places::{Request, Target},
    volumes,
};
use gtk::{gio, prelude::*};
use trek_core::{config::BookmarkOpen, location::Location};

impl MainWindow {
    fn as_window(&self) -> gtk::Window {
        self.0.window.clone().upcast()
    }

    fn open_in(&self, location: Location, target: Target) {
        match target {
            Target::Default => {
                if let Some(tab) = self.active_tab() {
                    tab.navigate(location);
                    tab.grab_focus();
                }
            }
            Target::Tab => self.add_tab(location),
            Target::Window => {
                self.0.app.new_window(location);
            }
        }
    }

    /// A bookmark opens where `bm_open_method` says (§8.14).
    pub(super) fn open_bookmark(&self, location: Location) {
        let target = match self.0.app.settings().bm_open_method() {
            BookmarkOpen::Current => Target::Default,
            BookmarkOpen::Tab => Target::Tab,
            BookmarkOpen::Window => Target::Window,
        };
        self.open_in(location, target);
    }

    pub(super) fn place_request(&self, request: Request) {
        let app = &self.0.app;
        let parent = self.as_window();
        match request {
            Request::Open(location, Target::Default, true) => self.open_bookmark(location),
            Request::Open(location, target, _) => self.open_in(location, target),
            Request::OpenVolume(id, target) => {
                // An unmounted volume is mounted first, then opened.
                let this = self.clone();
                volumes::mount(app, Some(&parent), &id, move |root| {
                    this.open_in(root, target)
                });
            }
            Request::Mount(id) => volumes::mount(app, Some(&parent), &id, |_| {}),
            Request::Unmount(id) => volumes::unmount(app, Some(&parent), &id),
            Request::Eject(id) => volumes::eject(app, Some(&parent), &id),
            Request::EmptyTrash => self.empty_trash(),
            Request::RenameBookmark(index) => bookmarks::rename_dialog(app, &parent, index),
        }
    }

    /// A mount went away. Tabs inside it close when `close_on_unmount` is
    /// set, otherwise they go Home (§8.17).
    pub fn mount_gone(&self, root: &gio::File) {
        let tabs = self.all_tabs();
        let locations: Vec<Location> = tabs.iter().map(|tab| tab.location()).collect();
        let inside = trek_fs::volumes::locations_inside(root, &locations);
        let close = self.0.app.settings().close_on_unmount();
        let home = self.0.app.home_location();
        for index in inside {
            let Some(tab) = tabs.get(index) else { continue };
            // The last tab of a window goes Home rather than taking the
            // window with it.
            if close && self.all_tabs().len() > 1 {
                self.close_tab(tab);
            } else if let Some(home) = &home {
                tab.navigate(home.clone());
            }
        }
    }
}
