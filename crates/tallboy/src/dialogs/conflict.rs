//! The file-conflict dialog (PROMPT §8.10): both files with icon, size and
//! modification time; Overwrite, Skip, Rename, Cancel; apply to all.

use super::base::{Dialog, Role, caption};
use gtk::{gio, glib, pango, prelude::*};
use std::{cell::Cell, rc::Rc};
use tallboy_core::{format, naming};
use tallboy_fs::{jobs::Answer, meta::FileMeta};

type Responder = Box<dyn FnOnce(Answer)>;

fn describe(meta: &FileMeta, si: bool) -> gtk::Box {
    let icon = gtk::Image::new();
    icon.set_pixel_size(48);
    if let Some(gicon) = meta
        .icon
        .as_deref()
        .and_then(|s| gio::Icon::for_string(s).ok())
    {
        icon.set_from_gicon(&gicon);
    }
    let name = gtk::Label::new(Some(&meta.display_name));
    name.set_xalign(0.0);
    name.set_ellipsize(pango::EllipsizeMode::Middle);
    name.set_max_width_chars(48);
    let modified = glib::DateTime::from_unix_local(meta.mtime)
        .and_then(|time| time.format("%x %R"))
        .map(String::from)
        .unwrap_or_default();
    let is_folder = meta.file_type == gio::FileType::Directory;
    let details = gtk::Label::new(Some(&if is_folder {
        format!("Type: {}\nModified: {modified}", meta.description)
    } else {
        format!(
            "Type: {}\nSize: {}\nModified: {modified}",
            meta.description,
            format::size(meta.size, si)
        )
    }));
    details.set_xalign(0.0);
    details.add_css_class("dim-label");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.append(&name);
    text.append(&details);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&icon);
    row.append(&text);
    row
}

/// Asks what to do about `source` meeting `existing`. `answer` is called
/// exactly once; closing the dialog cancels the job.
pub fn show(
    parent: Option<&gtk::Window>,
    source: &FileMeta,
    existing: &FileMeta,
    suggested_name: &str,
    can_overwrite: bool,
    si: bool,
    answer: impl FnOnce(Answer) + 'static,
) {
    let dialog = Dialog::new(parent, "Confirm to replace files");
    let content = dialog.content();
    content.append(&caption(
        "There is already a file with the same name in this location.\nDo you want to replace the existing file",
    ));
    content.append(&describe(existing, si));
    content.append(&caption("with the following file?"));
    content.append(&describe(source, si));
    if !can_overwrite {
        content.append(&caption(
            "A folder cannot be replaced by a file. Rename the file or skip it.",
        ));
    }
    let name = gtk::Entry::new();
    name.set_text(suggested_name);
    name.set_activates_default(true);
    let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    name_row.append(&gtk::Label::new(Some("File name:")));
    name.set_hexpand(true);
    name_row.append(&name);
    content.append(&name_row);
    let apply_all = gtk::CheckButton::with_mnemonic("_Apply this option to all existing files");
    content.append(&apply_all);

    let answer: Rc<Cell<Option<Responder>>> = Rc::new(Cell::new(Some(Box::new(answer))));
    let reply = {
        let answer = answer.clone();
        let window = dialog.window().downgrade();
        move |choice: Answer| {
            if let Some(answer) = answer.take() {
                answer(choice);
            }
            if let Some(window) = window.upgrade() {
                window.close();
            }
        }
    };
    let cancel = dialog.button("_Cancel", Role::Plain);
    let rename = dialog.button("_Rename", Role::Default);
    let skip = dialog.button("_Skip", Role::Plain);
    let overwrite = dialog.button("_Overwrite", Role::Plain);
    overwrite.set_sensitive(can_overwrite);

    // Rename needs a name that is valid and is not the conflicting one.
    let taken = existing.edit_name.clone();
    let valid = move |text: &str| naming::validate(text).is_ok() && text != taken;
    rename.set_sensitive(valid(&name.text()));
    let rename_button = rename.downgrade();
    let check = valid.clone();
    name.connect_changed(move |entry| {
        if let Some(button) = rename_button.upgrade() {
            button.set_sensitive(check(&entry.text()));
        }
    });

    let on = reply.clone();
    cancel.connect_clicked(move |_| on(Answer::Cancel));
    let on = reply.clone();
    let entry = name.clone();
    rename.connect_clicked(move |_| on(Answer::Rename(entry.text().to_string())));
    let on = reply.clone();
    let all = apply_all.clone();
    skip.connect_clicked(move |_| {
        on(Answer::Skip {
            apply_to_all: all.is_active(),
        })
    });
    let on = reply.clone();
    let all = apply_all.clone();
    overwrite.connect_clicked(move |_| {
        on(Answer::Overwrite {
            apply_to_all: all.is_active(),
        })
    });
    // Esc and the window's close button cancel the job.
    dialog.window().connect_close_request(move |_| {
        if let Some(answer) = answer.take() {
            answer(Answer::Cancel);
        }
        glib::Propagation::Proceed
    });
    dialog.present();
    name.grab_focus();
}
