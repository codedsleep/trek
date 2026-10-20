//! The devices button in the toolbar: removable media, disk images and
//! other mounts, which PCManFM lists in its side pane. The side pane is
//! hidden by default here, so this button is where a plugged-in drive
//! shows up, by name; its free space is in the list. It is visible only
//! while there is a device, and it stands out after a medium was mounted
//! until its list is opened.
//!
//! In the list: Enter or a click opens the device (mounting it first),
//! `e` or the button at the end of a row ejects or unmounts it.

use crate::{
    settings::Settings,
    sidepane::places::{Request, Target},
};
use gtk::{gio, glib, pango, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use trek_core::format;
use trek_fs::volumes::Entry;

struct Inner {
    button: gtk::MenuButton,
    name: gtk::Label,
    list: gtk::ListBox,
    entries: RefCell<Vec<Entry>>,
    /// Each row's second line, in the order of `entries`.
    states: RefCell<Vec<gtk::Label>>,
    /// Bumped by every `set`, so a late free-space answer for an older
    /// list is dropped.
    generation: Cell<u64>,
    settings: Settings,
    emit: Box<dyn Fn(Request)>,
}

/// Free and total bytes of a mounted entry's filesystem.
async fn space(root: &str) -> Option<(u64, Option<u64>)> {
    let info = gio::File::for_uri(root)
        .query_filesystem_info_future("filesystem::free,filesystem::size", glib::Priority::LOW)
        .await
        .ok()?;
    let free = info
        .has_attribute("filesystem::free")
        .then(|| info.attribute_uint64("filesystem::free"))?;
    let size = info
        .has_attribute("filesystem::size")
        .then(|| info.attribute_uint64("filesystem::size"));
    Some((free, size))
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

/// The button's tooltip for a single device, with its space once known.
fn tooltip(entry: &Entry, space: Option<&str>) -> String {
    let mut lines = vec![entry.name.clone()];
    match place(entry) {
        Some(place) if entry.mounted => {
            lines.extend(space.map(str::to_owned));
            lines.push(format!("Mounted at {place}"));
            lines.push("Click or g m to open or eject it".to_owned());
        }
        _ => {
            lines.push("Not mounted".to_owned());
            lines.push("Click or g m to mount and open it".to_owned());
        }
    }
    lines.join("\n")
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
    pub fn new(settings: Settings, emit: impl Fn(Request) + 'static) -> Self {
        let icon = gtk::Image::from_icon_name("drive-removable-media-symbolic");
        let name = gtk::Label::new(None);
        name.set_ellipsize(pango::EllipsizeMode::End);
        name.set_max_width_chars(32);
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
            states: RefCell::default(),
            generation: Cell::new(0),
            settings,
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
                // Files may have been added since the list was built.
                Devices(inner.clone()).measure();
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
        inner.generation.set(inner.generation.get() + 1);
        inner.list.remove_all();
        let mut states = Vec::with_capacity(entries.len());
        for entry in &entries {
            let (row, state) = self.row(entry);
            inner.list.append(&row);
            states.push(state);
        }
        inner.states.replace(states);
        match entries.as_slice() {
            [one] => {
                inner.name.set_text(&one.name);
                inner.button.set_tooltip_text(Some(&tooltip(one, None)));
            }
            many => {
                inner.name.set_text(&format!("{} devices", many.len()));
                inner
                    .button
                    .set_tooltip_text(Some("Devices — click or g m to open or eject"));
            }
        }
        inner.button.set_visible(!entries.is_empty());
        if entries.is_empty() {
            inner.button.popdown();
            inner.button.remove_css_class("attention");
        }
        inner.entries.replace(entries);
        self.measure();
    }

    /// Looks up the free space of every mounted entry and adds it to its
    /// row, and to the button's tooltip when it is the only device.
    fn measure(&self) {
        let inner = &self.0;
        let generation = inner.generation.get();
        let entries = inner.entries.borrow().clone();
        let only = entries.len() == 1;
        for (index, entry) in entries.into_iter().enumerate() {
            let (true, Some(root), Some(place)) =
                (entry.mounted, entry.root.clone(), place(&entry))
            else {
                continue;
            };
            let weak = Rc::downgrade(&self.0);
            glib::spawn_future_local(async move {
                let space = space(&root).await;
                let Some(inner) = weak.upgrade() else { return };
                let Some((free, size)) = space else { return };
                if inner.generation.get() != generation {
                    return;
                }
                let si = inner.settings.si_unit();
                let free_text = format!("{} free", format::size(free, si));
                if let Some(state) = inner.states.borrow().get(index) {
                    state.set_text(&format!("{free_text} · {place}"));
                }
                if only {
                    let detail = match size {
                        Some(size) => format!("{free_text} of {}", format::size(size, si)),
                        None => free_text,
                    };
                    inner
                        .button
                        .set_tooltip_text(Some(&tooltip(&entry, Some(&detail))));
                }
            });
        }
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

    fn row(&self, entry: &Entry) -> (gtk::Box, gtk::Label) {
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
        (row, state)
    }
}
