//! The toolbar (PROMPT §8.3): New Window, New Tab, Back, Forward, Up, Home,
//! the path bar, Go. Buttons only name actions; visibility follows the
//! `toolbar` setting.

use gtk::prelude::*;
use tallboy_core::config;

pub struct Toolbar {
    root: gtk::Box,
    new_window: gtk::Button,
    new_tab: gtk::Button,
    navigation: gtk::Box,
    home: gtk::Button,
    history: gtk::MenuButton,
    path_slot: gtk::Box,
    menu: gtk::MenuButton,
}

fn button(icon: &str, tooltip: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_action_name(Some(action));
    button.add_css_class("flat");
    button.set_focus_on_click(false);
    button
}

impl Toolbar {
    pub fn new() -> Self {
        let new_window = button("window-new", "New Window", "win.new-window");
        let new_tab = button("tab-new", "New Tab", "win.new-tab");
        let back = button("go-previous", "Previous Folder", "win.back");
        let forward = button("go-next", "Next Folder", "win.forward");
        let up = button("go-up", "Parent Folder", "win.up");
        let home = button("go-home", "Home Folder", "win.home");
        // Everything the hidden menu bar holds.
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("open-menu-symbolic");
        menu.set_tooltip_text(Some("Menu (F10)"));
        menu.add_css_class("flat");
        menu.set_focus_on_click(false);

        // The history menu, as upstream's arrow between Back and Forward.
        let history = gtk::MenuButton::new();
        history.set_icon_name("pan-down-symbolic");
        history.set_tooltip_text(Some("History"));
        history.add_css_class("flat");
        history.set_focus_on_click(false);
        // §8.3: Back and Forward also open it on right-click and long-press.
        for target in [&back, &forward] {
            let menu = history.downgrade();
            let secondary = gtk::GestureClick::new();
            secondary.set_button(gtk::gdk::BUTTON_SECONDARY);
            secondary.connect_pressed(move |_, _, _, _| {
                if let Some(menu) = menu.upgrade() {
                    menu.popup();
                }
            });
            target.add_controller(secondary);
            let menu = history.downgrade();
            let long_press = gtk::GestureLongPress::new();
            long_press.connect_pressed(move |_, _, _| {
                if let Some(menu) = menu.upgrade() {
                    menu.popup();
                }
            });
            target.add_controller(long_press);
        }

        let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        navigation.append(&back);
        navigation.append(&history);
        navigation.append(&forward);
        navigation.append(&up);
        let path_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        path_slot.set_hexpand(true);

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        root.add_css_class("toolbar");
        for widget in [
            new_window.upcast_ref::<gtk::Widget>(),
            new_tab.upcast_ref(),
            navigation.upcast_ref(),
            home.upcast_ref(),
            path_slot.upcast_ref(),
            menu.upcast_ref(),
        ] {
            root.append(widget);
        }
        Self {
            root,
            new_window,
            new_tab,
            navigation,
            home,
            history,
            path_slot,
            menu,
        }
    }

    /// The button that opens the application menu.
    pub fn menu_button(&self) -> &gtk::MenuButton {
        &self.menu
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// The button whose menu lists the tab's history.
    pub fn history_button(&self) -> &gtk::MenuButton {
        &self.history
    }

    /// Puts the path bar in its place between Home and Go.
    pub fn set_path_bar(&self, path_bar: &impl IsA<gtk::Widget>) {
        self.path_slot.append(path_bar);
    }

    /// Shows the button groups named in the `toolbar` setting.
    pub fn apply(&self, toolbar: &config::Toolbar) {
        let has = |name: &str| toolbar.0.iter().any(|t| t == name);
        self.root.set_visible(!has("hidden"));
        self.new_window.set_visible(has("newwin"));
        self.new_tab.set_visible(has("newtab"));
        self.navigation.set_visible(has("navigation"));
        self.home.set_visible(has("home"));
    }
}
