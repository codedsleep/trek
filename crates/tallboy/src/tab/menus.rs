//! Context menus of the folder view (PROMPT §8.13): one for the selection,
//! one for the folder background, one inside the trash can. They are built
//! when shown, from the same window actions the menu bar uses, so what is
//! enabled always agrees.

use super::TabPage;
use crate::{menubar, view::FileItem};
use gtk::{gio, graphene, prelude::*};
use tallboy_fs::apps;

fn item(menu: &gio::Menu, label: &str, action: &str) {
    menu.append(Some(label), Some(action));
}

/// Applications registered for the type, then "Open With…".
fn open_with_menu(content_type: &str) -> gio::Menu {
    let registered = gio::Menu::new();
    for app in apps::all_for_type(content_type) {
        if let Some(id) = app.id() {
            let entry = gio::MenuItem::new(Some(&app.display_name()), None);
            entry.set_action_and_target_value(
                Some("win.open-with-app"),
                Some(&id.to_string().to_variant()),
            );
            registered.append_item(&entry);
        }
    }
    let other = gio::Menu::new();
    item(&other, "Open With...", "win.open-with");
    let menu = gio::Menu::new();
    menu.append_section(None, &registered);
    menu.append_section(None, &other);
    menu
}

fn file_menu(selected: &[FileItem], in_trash: bool, use_trash: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if in_trash {
        // §8.16: only these three apply to trashed files.
        let actions = gio::Menu::new();
        item(&actions, "_Restore", "win.restore");
        item(&actions, "_Remove", "win.delete");
        menu.append_section(None, &actions);
        let properties = gio::Menu::new();
        item(&properties, "Prop_erties", "win.properties");
        menu.append_section(None, &properties);
        return menu;
    }
    let single_folder = matches!(selected, [only] if only.is_dir());
    let open = gio::Menu::new();
    item(&open, "_Open", "win.open");
    if let Some(file) = selected.iter().find(|file| !file.is_dir()) {
        open.append_submenu(
            Some("Open _With..."),
            &open_with_menu(&file.meta().content_type),
        );
    }
    if single_folder {
        item(&open, "Open in New T_ab", "win.open-new-tab");
        item(&open, "Open in New Win_dow", "win.open-new-window");
        item(&open, "Open in Termina_l", "win.open-terminal-here");
    }
    menu.append_section(None, &open);

    let edit = gio::Menu::new();
    item(&edit, "Cu_t", "win.cut");
    item(&edit, "_Copy", "win.copy");
    if single_folder {
        item(&edit, "_Paste", "win.paste-into");
    }
    // One of the two (§8.13): Remove takes the place of Move to Trash when
    // the trash is switched off. Shift+Delete removes either way.
    if use_trash {
        item(&edit, "Move to _Trash", "win.trash");
    } else {
        item(&edit, "_Remove", "win.delete");
    }
    item(&edit, "Copy Pat_h(s)", "win.copy-path");
    menu.append_section(None, &edit);

    let rename = gio::Menu::new();
    item(&rename, "_Rename...", "win.rename");
    menu.append_section(None, &rename);
    let properties = gio::Menu::new();
    item(&properties, "Prop_erties", "win.properties");
    menu.append_section(None, &properties);
    menu
}

fn folder_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let create = gio::Menu::new();
    create.append_submenu(Some("C_reate New..."), &menubar::create_menu());
    item(&create, "_Paste", "win.paste");
    menu.append_section(None, &create);
    let select = gio::Menu::new();
    item(&select, "Select _All", "win.select-all");
    item(&select, "_Invert Selection", "win.invert-selection");
    menu.append_section(None, &select);
    let view = gio::Menu::new();
    view.append_submenu(Some("_Sort Files"), &menubar::sort_menu());
    item(&view, "Show Hidde_n", "win.show-hidden");
    menu.append_section(None, &view);
    let terminal = gio::Menu::new();
    item(&terminal, "Open in Termina_l", "win.terminal");
    menu.append_section(None, &terminal);
    let properties = gio::Menu::new();
    item(&properties, "Folder Prop_erties", "win.folder-properties");
    menu.append_section(None, &properties);
    menu
}

impl TabPage {
    /// Shows the menu for the selection, or for the folder when nothing is
    /// selected. `at` is a point in `widget`'s coordinates; without one (the
    /// Menu key) the menu opens at the focused item.
    pub fn show_context_menu(&self, at: Option<(gtk::Widget, f64, f64)>) {
        let selected = self.0.model.selected();
        let menu = if selected.is_empty() {
            folder_menu()
        } else {
            file_menu(
                &selected,
                self.in_trash(),
                self.0.app.settings().use_trash(),
            )
        };
        let root = &self.0.root;
        let point = at
            .and_then(|(widget, x, y)| {
                widget.compute_point(root, &graphene::Point::new(x as f32, y as f32))
            })
            .or_else(|| {
                // The focused cell, when the keyboard asked for the menu.
                let focus = root
                    .root()
                    .and_downcast::<gtk::Window>()
                    .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
                    .filter(|focus| focus.is_ancestor(root))?;
                let bounds = focus.compute_bounds(root)?;
                Some(graphene::Point::new(
                    bounds.x() + bounds.width() / 2.0,
                    bounds.y() + bounds.height() / 2.0,
                ))
            })
            .unwrap_or_else(|| graphene::Point::new(24.0, 24.0));

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);
        popover.set_parent(root);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            point.x() as i32,
            point.y() as i32,
            1,
            1,
        )));
        // Unparent once the action it triggered has run.
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            gtk::glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }

    /// Right-click on the background: nothing stays selected (§8.13).
    pub(super) fn watch_background_menu(&self) {
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let weak = self.downgrade();
        click.connect_pressed(move |gesture, _, x, y| {
            let (Some(tab), Some(widget)) = (weak.upgrade(), gesture.widget()) else {
                return;
            };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            tab.0.model.selection().unselect_all();
            tab.show_context_menu(Some((widget, x, y)));
        });
        self.0.scroller.add_controller(click);
    }
}
