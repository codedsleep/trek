//! Bookmarks in the UI (PROMPT §8.15): the Bookmarks menu section and the
//! Add and Edit dialogs. Parsing, saving and watching the shared
//! `gtk-3.0/bookmarks` file is `fs::bookmarks`.

use crate::{
    app::App,
    dialogs::base::{self, Dialog, Role, caption},
};
use gtk::{gio, prelude::*};
use std::rc::Rc;
use trek_core::location::Location;
use trek_fs::bookmarks::{BookmarkError, Bookmarks};

/// What a bookmark is called: its label, or the folder's name.
pub fn label(location: &Location, label: Option<&str>) -> String {
    label.map_or_else(
        || {
            location
                .breadcrumbs()
                .last()
                .map_or_else(|| location.display(), |crumb| crumb.label.clone())
        },
        String::from,
    )
}

/// (label, location) of every bookmark, in file order.
pub fn entries(app: &App) -> Vec<(String, Location)> {
    app.bookmarks()
        .map(|store| {
            store
                .snapshot()
                .entries()
                .iter()
                .map(|b| (label(&b.location, b.label.as_deref()), b.location.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Refills the Bookmarks menu section shared by every window.
pub fn fill_menu(app: &App, menu: &gio::Menu) {
    menu.remove_all();
    for (label, location) in entries(app) {
        let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
        item.set_action_and_target_value(
            Some("win.open-bookmark"),
            Some(&location.to_uri().to_variant()),
        );
        menu.append_item(&item);
    }
}

fn edit(app: &App, change: impl FnOnce(&mut Bookmarks) -> Result<(), BookmarkError>) -> bool {
    let Some(store) = app.bookmarks() else {
        return false;
    };
    match store.edit(change) {
        Ok(()) => {
            app.bookmarks_changed();
            true
        }
        Err(e) => {
            log::warn!("Bookmarks: {e}");
            false
        }
    }
}

pub fn add(app: &App, location: Location, label: Option<String>) -> bool {
    edit(app, |bookmarks| bookmarks.add(location, label))
}

pub fn rename(app: &App, index: usize, label: Option<String>) -> bool {
    edit(app, |bookmarks| bookmarks.rename(index, label))
}

pub fn remove(app: &App, index: usize) -> bool {
    edit(app, |bookmarks| bookmarks.remove(index))
}

pub fn move_to(app: &App, from: usize, to: usize) -> bool {
    edit(app, |bookmarks| bookmarks.move_to(from, to))
}

/// Bookmarks ▸ Add to Bookmarks…: asks for the label, default the folder name.
pub fn add_dialog(app: &App, parent: &gtk::Window, location: Location) {
    let app = app.clone();
    let default = label(&location, None);
    base::prompt(
        Some(parent),
        "Add to Bookmarks",
        &format!(
            "Add following folder to bookmarks:\n\"{}\"\nEnter a name for the bookmark item:",
            location.display()
        ),
        &default,
        None,
        move |name| {
            let name = name.trim();
            if name.is_empty() || name.contains('\n') {
                return Some("Enter a name for the bookmark.".into());
            }
            if add(&app, location.clone(), Some(name.to_owned())) {
                None
            } else {
                Some("The bookmark could not be saved.".into())
            }
        },
    );
}

/// Asks for a new label for one bookmark.
pub fn rename_dialog(app: &App, parent: &gtk::Window, index: usize) {
    let Some((current, _)) = entries(app).into_iter().nth(index) else {
        return;
    };
    let app = app.clone();
    base::prompt(
        Some(parent),
        "Rename Bookmark Item",
        "Enter a new name:",
        &current,
        None,
        move |name| {
            let name = name.trim();
            if name.is_empty() || name.contains('\n') {
                return Some("Enter a name for the bookmark.".into());
            }
            if rename(&app, index, Some(name.to_owned())) {
                None
            } else {
                Some("The bookmark could not be saved.".into())
            }
        },
    );
}

/// Bookmarks ▸ Edit Bookmarks…: a list with rename, remove, move up and down.
pub fn edit_dialog(app: &App, parent: &gtk::Window) {
    let dialog = Dialog::new(Some(parent), "Edit Bookmarks");
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Browse);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_min_content_height(240);
    scroller.set_min_content_width(380);
    scroller.set_has_frame(true);
    scroller.set_child(Some(&list));
    dialog.content().append(&caption(
        "Use drag and drop in the side pane, or the buttons below, to reorder the bookmarks.",
    ));
    dialog.content().append(&scroller);

    let fill: Rc<dyn Fn(Option<usize>)> = {
        let (app, list) = (app.clone(), list.clone());
        Rc::new(move |select| {
            list.remove_all();
            for (label, location) in entries(&app) {
                let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
                let name = gtk::Label::new(Some(&label));
                name.set_xalign(0.0);
                let path = gtk::Label::new(Some(&location.display()));
                path.set_xalign(0.0);
                path.add_css_class("dim-label");
                path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                row.append(&name);
                row.append(&path);
                list.append(&row);
            }
            if let Some(row) = select.and_then(|index| list.row_at_index(index as i32)) {
                list.select_row(Some(&row));
            }
        })
    };
    fill(Some(0));

    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let selected = {
        let list = list.clone();
        move || list.selected_row().map(|row| row.index() as usize)
    };
    for (text, step) in [("Move _Up", -1i64), ("Move _Down", 1)] {
        let button = gtk::Button::with_mnemonic(text);
        let (app, fill, selected) = (app.clone(), fill.clone(), selected.clone());
        button.connect_clicked(move |_| {
            let Some(from) = selected() else { return };
            let to = from as i64 + step;
            if to >= 0 && (to as usize) < entries(&app).len() && move_to(&app, from, to as usize) {
                fill(Some(to as usize));
            }
        });
        tools.append(&button);
    }
    let rename_button = gtk::Button::with_mnemonic("_Rename...");
    {
        let (app, selected, window) = (app.clone(), selected.clone(), dialog.window().clone());
        rename_button.connect_clicked(move |_| {
            if let Some(index) = selected() {
                rename_dialog(&app, &window, index);
            }
        });
    }
    tools.append(&rename_button);
    let remove_button = gtk::Button::with_mnemonic("Re_move");
    {
        let (app, fill, selected) = (app.clone(), fill.clone(), selected.clone());
        remove_button.connect_clicked(move |_| {
            if let Some(index) = selected()
                && remove(&app, index)
            {
                fill(Some(index.saturating_sub(1)));
            }
        });
    }
    tools.append(&remove_button);
    dialog.content().append(&tools);

    // A rename made from this dialog, or an edit by another program, shows
    // up here too.
    let listener = app.places_changed().connect({
        let (fill, selected) = (fill.clone(), selected.clone());
        move || fill(selected())
    });
    let app = app.clone();
    dialog.window().connect_close_request(move |_| {
        app.places_changed().disconnect(listener);
        gtk::glib::Propagation::Proceed
    });
    let close = dialog.button("_Close", Role::Default);
    let window = dialog.window().downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
    dialog.present();
}
