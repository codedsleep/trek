//! The devices button in the toolbar: removable media, disk images and
//! other mounts, which PCManFM lists in its side pane. The side pane is
//! hidden by default here, so this button is where a plugged-in drive
//! shows up. It is visible only while there is a device, and it stands
//! out after a medium was mounted until its list is opened.
//!
//! In the list: Enter or a click opens the device (mounting it first),
//! `e` or the button at the end of a row ejects or unmounts it.

use crate::sidepane::places::{Request, Target};
use gtk::{gio, pango, prelude::*};
use std::{cell::RefCell, rc::Rc};
use trek_fs::volumes::Entry;

struct Inner {
    button: gtk::MenuButton,
    name: gtk::Label,
    list: gtk::ListBox,
    entries: RefCell<Vec<Entry>>,
    emit: Box<dyn Fn(Request)>,
}

#[derive(Clone)]
pub struct Devices(Rc<Inner>);

/// Where a mounted entry lives, as a path when it has one.
pub fn place(entry: &Entry) -> Option<String> {
    let root = gio::File::for_uri(entry.root.as_deref()?);
    Some(
        root.path()
            .map_or_else(|| root.uri().to_string(), |path| path.display().to_string()),
    )
}

/// Ejects removable media and unmounts anything else.
fn put_away(entry: &Entry) -> Request {
    if entry.can_eject {
        Request::Eject(entry.id.clone())
    } else {
        Request::Unmount(entry.id.clone())
    }
}

impl Devices {
    pub fn new(emit: impl Fn(Request) + 'static) -> Self {
        let icon = gtk::Image::from_icon_name("drive-removable-media-symbolic");
        let name = gtk::Label::new(None);
        name.set_ellipsize(pango::EllipsizeMode::End);
        name.set_max_width_chars(20);
        let face = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        face.append(&icon);
        face.append(&name);
        let button = gtk::MenuButton::new();
        button.set_child(Some(&face));
        button.set_tooltip_text(Some("Devices (g m)"));
        button.add_css_class("flat");
        button.add_css_class("devices");
        button.set_focus_on_click(false);
        button.set_visible(false);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Browse);
        list.add_css_class("devices-list");
        let heading = gtk::Label::new(Some("Devices"));
        heading.set_xalign(0.0);
        heading.add_css_class("heading");
        let hint = gtk::Label::new(Some("Enter open  ·  e eject"));
        hint.set_xalign(0.0);
        hint.add_css_class("dim-label");
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.append(&heading);
        column.append(&list);
        column.append(&hint);
        let popover = gtk::Popover::new();
        popover.set_child(Some(&column));
        button.set_popover(Some(&popover));

        let this = Self(Rc::new(Inner {
            button,
            name,
            list,
            entries: RefCell::default(),
            emit: Box::new(emit),
        }));
        this.connect();
        this
    }

    fn connect(&self) {
        let inner = &self.0;
        let weak = Rc::downgrade(&self.0);
        inner.list.connect_row_activated(move |_, row| {
            let Some(inner) = weak.upgrade() else { return };
            let entry = inner.entries.borrow().get(row.index() as usize).cloned();
            if let Some(entry) = entry {
                inner.button.popdown();
                (inner.emit)(Request::OpenVolume(entry.id, Target::Default));
            }
        });

        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&self.0);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(inner) = weak.upgrade() else {
                return gtk::glib::Propagation::Proceed;
            };
            let entry = inner
                .list
                .selected_row()
                .and_then(|row| inner.entries.borrow().get(row.index() as usize).cloned());
            match (key, entry) {
                (gtk::gdk::Key::e, Some(entry)) if entry.mounted && modifiers.is_empty() => {
                    (inner.emit)(put_away(&entry));
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        inner.list.add_controller(keys);

        // Opening the list is noticing the device; the keyboard goes to it.
        let weak = Rc::downgrade(&self.0);
        if let Some(popover) = inner.button.popover() {
            popover.connect_show(move |_| {
                let Some(inner) = weak.upgrade() else { return };
                inner.button.remove_css_class("attention");
                let row = inner
                    .list
                    .selected_row()
                    .or_else(|| inner.list.row_at_index(0));
                if let Some(row) = row {
                    inner.list.select_row(Some(&row));
                    row.grab_focus();
                }
            });
        }
    }

    pub fn widget(&self) -> &gtk::MenuButton {
        &self.0.button
    }

    /// Shows the devices that are not fixed disks.
    pub fn set(&self, entries: Vec<Entry>) {
        let inner = &self.0;
        let entries: Vec<Entry> = entries.into_iter().filter(|e| !e.internal).collect();
        inner.list.remove_all();
        for entry in &entries {
            inner.list.append(&self.row(entry));
        }
        inner.name.set_text(&match entries.as_slice() {
            [one] => one.name.clone(),
            many => format!("{} devices", many.len()),
        });
        inner.button.set_visible(!entries.is_empty());
        if entries.is_empty() {
            inner.button.popdown();
            inner.button.remove_css_class("attention");
        }
        inner.entries.replace(entries);
    }

    /// Makes the button stand out until its list is opened.
    pub fn attention(&self) {
        self.0.button.add_css_class("attention");
    }

    /// Opens the list (`g m`).
    pub fn popup(&self) -> bool {
        if self.0.button.is_visible() {
            self.0.button.popup();
            true
        } else {
            false
        }
    }

    fn row(&self, entry: &Entry) -> gtk::Box {
        let image = gtk::Image::new();
        image.set_pixel_size(24);
        match entry
            .icon
            .as_deref()
            .and_then(|icon| gio::Icon::for_string(icon).ok())
        {
            Some(icon) => image.set_from_gicon(&icon),
            None => image.set_icon_name(Some("drive-removable-media")),
        }
        let name = gtk::Label::new(Some(&entry.name));
        name.set_xalign(0.0);
        name.add_css_class("device-name");
        let state = gtk::Label::new(Some(&match place(entry) {
            Some(place) if entry.mounted => place,
            _ => "Not mounted — Enter mounts and opens it".to_owned(),
        }));
        state.set_xalign(0.0);
        state.set_ellipsize(pango::EllipsizeMode::Middle);
        state.set_max_width_chars(40);
        state.add_css_class("dim-label");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_hexpand(true);
        text.append(&name);
        text.append(&state);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("device-row");
        row.append(&image);
        row.append(&text);
        if entry.mounted {
            let eject = gtk::Button::from_icon_name("media-eject-symbolic");
            eject.add_css_class("flat");
            eject.set_valign(gtk::Align::Center);
            eject.set_focus_on_click(false);
            eject.set_tooltip_text(Some(if entry.can_eject { "Eject" } else { "Unmount" }));
            let weak = Rc::downgrade(&self.0);
            let request = entry.clone();
            eject.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    (inner.emit)(put_away(&request));
                }
            });
            row.append(&eject);
        }
        row
    }
}
