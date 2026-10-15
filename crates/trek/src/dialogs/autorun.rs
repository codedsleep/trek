//! The autorun dialog (PROMPT §8.17): what to do with a removable medium.

use super::base::{Dialog, Role, caption};
use gtk::{gio, prelude::*};
use std::rc::Rc;

/// What the user picked.
pub struct Choice {
    /// The application, or `None` for "Open in File Manager".
    pub app: Option<gio::AppInfo>,
    /// "Always perform this action for this type of media".
    pub remember: bool,
}

/// Shows the dialog for a medium of `content_type`, or of no particular
/// type. `may_remember` is false for media that must always ask
/// (DISCOVERY D15).
pub fn show(
    parent: Option<&gtk::Window>,
    medium: &str,
    content_type: Option<&str>,
    apps: Vec<gio::AppInfo>,
    may_remember: bool,
    chosen: impl Fn(Choice) + 'static,
) {
    let dialog = Dialog::new(parent, "Removable medium is inserted");
    let description = content_type.map_or_else(
        || "Removable Disk".to_owned(),
        |content_type| gio::content_type_get_description(content_type).to_string(),
    );
    dialog.content().append(&caption(&format!(
        "Removable medium is inserted: {medium}\nType of medium: {description}\n\nPlease select the action you want to perform:"
    )));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Browse);
    list.set_activate_on_single_click(false);
    let row = |icon: Option<gio::Icon>, name: &str| {
        let image = gtk::Image::new();
        image.set_pixel_size(24);
        match icon {
            Some(icon) => image.set_from_gicon(&icon),
            None => image.set_icon_name(Some("system-file-manager")),
        }
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&image);
        row.append(&label);
        row
    };
    for app in &apps {
        list.append(&row(app.icon(), &app.display_name()));
    }
    list.append(&row(None, "Open in File Manager"));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_min_content_height(160);
    scroller.set_min_content_width(360);
    scroller.set_has_frame(true);
    scroller.set_child(Some(&list));
    dialog.content().append(&scroller);
    let remember =
        gtk::CheckButton::with_mnemonic("_Always perform this action for this type of media");
    remember.set_visible(may_remember);
    dialog.content().append(&remember);

    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let apps = Rc::new(apps);
    let window = dialog.window().downgrade();
    let picker = list.clone();
    ok.connect_clicked(move |_| {
        let index = picker
            .selected_row()
            .map_or(apps.len(), |row| row.index() as usize);
        if let Some(window) = window.upgrade() {
            window.close();
        }
        chosen(Choice {
            app: apps.get(index).cloned(),
            remember: may_remember && remember.is_active(),
        });
    });
    let accept = ok.downgrade();
    list.connect_row_activated(move |_, _| {
        if let Some(ok) = accept.upgrade() {
            ok.emit_clicked();
        }
    });
    // "Open in File Manager" is the safe default.
    if let Some(last) = list.row_at_index(list.observe_children().n_items() as i32 - 1) {
        list.select_row(Some(&last));
    }
    dialog.present();
}
