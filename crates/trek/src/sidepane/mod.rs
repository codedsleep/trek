//! The side pane (PROMPT §8.14): Places or the Directory Tree, chosen from
//! the button at its top.

pub mod dirtree;
pub mod places;

use crate::{app::App, view::dnd::Hooks};
use dirtree::DirTree;
use gtk::{gio, prelude::*};
use places::{Places, Request};
use trek_core::{config::PaneMode, location::Location};

const PLACES: &str = "places";
const TREE: &str = "dirtree";

pub struct SidePane {
    root: gtk::Box,
    mode: gtk::MenuButton,
    stack: gtk::Stack,
    places: Places,
    tree: DirTree,
}

impl SidePane {
    /// `request` handles what Places asks for; `open` a folder picked in the
    /// tree. Both must capture the window weakly.
    pub fn new(
        app: &App,
        hooks: Hooks,
        request: impl Fn(Request) + 'static,
        open: impl Fn(Location) + 'static,
    ) -> Self {
        let places = Places::new(app, hooks, request);
        let tree = DirTree::new(app, open);
        let stack = gtk::Stack::new();
        stack.add_named(places.widget(), Some(PLACES));
        stack.add_named(tree.widget(), Some(TREE));
        stack.set_vexpand(true);

        // The mode switch: a button that names the mode and lists both.
        let menu = gio::Menu::new();
        menu.append(Some("Places"), Some("win.sidepane-mode::places"));
        menu.append(Some("Directory Tree"), Some("win.sidepane-mode::dirtree"));
        let mode = gtk::MenuButton::new();
        mode.set_menu_model(Some(&menu));
        mode.add_css_class("flat");
        mode.set_always_show_arrow(true);
        mode.set_focus_on_click(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("side-pane");
        root.append(&mode);
        root.append(&stack);
        Self {
            root,
            mode,
            stack,
            places,
            tree,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn set_mode(&self, mode: PaneMode) {
        let (page, label) = match mode {
            PaneMode::Places => (PLACES, "Places"),
            PaneMode::Tree => (TREE, "Directory Tree"),
        };
        self.stack.set_visible_child_name(page);
        self.mode.set_label(label);
    }

    /// Follows the active tab: highlights its place, expands the tree to it.
    pub fn set_location(&self, location: &Location, show_hidden: bool) {
        self.places.set_location(location);
        if self.stack.visible_child_name().as_deref() == Some(TREE) {
            self.tree.follow(location, show_hidden);
        }
    }

    /// Gives the keyboard to the list that is showing (F6), not to the mode
    /// button above it.
    pub fn grab_focus(&self) {
        if let Some(shown) = self.stack.visible_child() {
            shown.child_focus(gtk::DirectionType::TabForward);
        }
    }

    pub fn debug_state(&self) -> String {
        format!("{} {}", self.places.debug_state(), self.tree.debug_state())
    }
}
