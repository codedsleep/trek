//! The Places list (PROMPT §8.14): fixed places, volumes and bookmarks.

use crate::{
    app::App,
    bookmarks,
    tab::location_from_uri,
    view::dnd::{self, Hooks},
};
use gtk::{gdk, gio, glib, pango, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use tallboy_core::location::Location;
use tallboy_fs::{trash, volumes::Entry};

/// Where an activated place opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A click: the current tab, or `bm_open_method` for a bookmark.
    Default,
    Tab,
    Window,
}

/// What the Places list asks its window to do.
pub enum Request {
    Open(Location, Target, bool),
    /// Mount if needed, then open.
    OpenVolume(String, Target),
    Mount(String),
    Unmount(String),
    Eject(String),
    EmptyTrash,
    RenameBookmark(usize),
}

#[derive(Clone)]
enum Place {
    Fixed(Location),
    Trash(Location),
    Volume(Entry),
    Bookmark(usize, Location),
}

impl Place {
    /// The folder this place stands for, when it has one right now.
    fn location(&self) -> Option<Location> {
        match self {
            Place::Fixed(location) | Place::Trash(location) | Place::Bookmark(_, location) => {
                Some(location.clone())
            }
            Place::Volume(entry) => entry.root.as_deref().and_then(location_from_uri),
        }
    }

    /// Rows are grouped; a separator is drawn where the group changes.
    fn section(&self) -> u8 {
        match self {
            Place::Fixed(_) | Place::Trash(_) => 0,
            Place::Volume(_) => 1,
            Place::Bookmark(..) => 2,
        }
    }
}

struct Inner {
    app: App,
    list: gtk::ListBox,
    scroller: gtk::ScrolledWindow,
    rows: RefCell<Vec<Place>>,
    current: RefCell<Option<Location>>,
    trash_full: Cell<bool>,
    trash_monitor: RefCell<Option<gio::FileMonitor>>,
    hooks: Hooks,
    emit: Box<dyn Fn(Request)>,
    listener: Cell<u64>,
    settings_handlers: RefCell<Vec<crate::settings::HandlerId>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.app.places_changed().disconnect(self.listener.get());
        self.app
            .settings()
            .disconnect(&self.settings_handlers.borrow());
        if let Some(monitor) = self.trash_monitor.take() {
            monitor.cancel();
        }
    }
}

/// The Places list. Clones share one list.
#[derive(Clone)]
pub struct Places(Rc<Inner>);

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Places {
    pub fn new(app: &App, hooks: Hooks, emit: impl Fn(Request) + 'static) -> Self {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        list.add_css_class("navigation-sidebar");
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));
        let places = Self(Rc::new(Inner {
            app: app.clone(),
            list,
            scroller,
            rows: RefCell::default(),
            current: RefCell::default(),
            trash_full: Cell::new(false),
            trash_monitor: RefCell::default(),
            hooks,
            emit: Box::new(emit),
            listener: Cell::new(0),
            settings_handlers: RefCell::default(),
        }));
        places.connect();
        places.rebuild();
        places.watch_trash();
        places
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.0.scroller
    }

    fn connect(&self) {
        let inner = &self.0;
        let weak = self.weak();
        inner.list.connect_row_activated(move |_, row| {
            if let Some(places) = weak.upgrade().map(Places) {
                places.activate(row.index() as usize, Target::Default);
            }
        });
        // Separators between the three sections.
        let weak = self.weak();
        inner.list.set_header_func(move |row, before| {
            let Some(inner) = weak.upgrade() else { return };
            let rows = inner.rows.borrow();
            let section = |r: &gtk::ListBoxRow| rows.get(r.index() as usize).map(Place::section);
            let boundary = before.is_some_and(|before| section(before) != section(row));
            row.set_header(
                boundary
                    .then(|| gtk::Separator::new(gtk::Orientation::Horizontal))
                    .as_ref(),
            );
        });
        // Bookmarks and volumes change under us; so do the [places] keys.
        let weak = self.weak();
        inner
            .listener
            .set(inner.app.places_changed().connect(move || {
                if let Some(places) = weak.upgrade().map(Places) {
                    places.rebuild();
                }
            }));
        use crate::settings::Key;
        let weak = self.weak();
        let handlers = inner.app.settings().connect_changed_any(
            &[
                Key::PlacesHome,
                Key::PlacesDesktop,
                Key::PlacesRoot,
                Key::PlacesComputer,
                Key::PlacesTrash,
                Key::PlacesNetwork,
                Key::PaneIconSize,
                Key::HomePath,
            ],
            move || {
                if let Some(places) = weak.upgrade().map(Places) {
                    places.rebuild();
                }
            },
        );
        inner.settings_handlers.replace(handlers);
        // A folder dropped on the empty part of the list becomes a bookmark.
        let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = self.weak();
        target.connect_drop(move |_, value, _, _| {
            let (Some(inner), Ok(files)) = (weak.upgrade(), value.get::<gdk::FileList>()) else {
                return false;
            };
            for file in files.files() {
                let app = inner.app.clone();
                glib::spawn_future_local(async move {
                    let is_folder = file
                        .query_info_future(
                            "standard::type",
                            gio::FileQueryInfoFlags::NONE,
                            glib::Priority::DEFAULT,
                        )
                        .await
                        .is_ok_and(|info| info.file_type() == gio::FileType::Directory);
                    if is_folder && let Some(location) = location_from_uri(&file.uri()) {
                        bookmarks::add(&app, location, None);
                    }
                });
            }
            true
        });
        inner.scroller.add_controller(target);
    }

    /// The trash can's icon follows whether it holds anything.
    fn watch_trash(&self) {
        let refresh = {
            let weak = self.weak();
            move || {
                let weak = weak.clone();
                glib::spawn_future_local(async move {
                    let full = trash::is_empty().await.is_ok_and(|empty| !empty);
                    if let Some(places) = weak.upgrade().map(Places)
                        && places.0.trash_full.replace(full) != full
                    {
                        places.rebuild();
                    }
                });
            }
        };
        refresh();
        let monitor = gio::File::for_uri("trash:///")
            .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE);
        if let Ok(monitor) = monitor {
            monitor.connect_changed(move |_, _, _, _| refresh());
            self.0.trash_monitor.replace(Some(monitor));
        }
    }

    fn collect(&self) -> Vec<(Place, String, gio::Icon)> {
        let app = &self.0.app;
        let settings = app.settings();
        let mut rows = Vec::new();
        let mut fixed = |show: bool, label: &str, icon: &str, location: Option<Location>| {
            if let (true, Some(location)) = (show, location) {
                rows.push((Place::Fixed(location), label.to_owned(), themed(icon)));
            }
        };
        fixed(
            settings.places_home(),
            "Home Folder",
            "user-home",
            app.home_location(),
        );
        fixed(
            settings.places_desktop(),
            "Desktop",
            "user-desktop",
            glib::user_special_dir(glib::UserDirectory::Desktop)
                .and_then(|path| Location::from_path(&path).ok()),
        );
        if settings.places_trash()
            && let Some(trash) = location_from_uri("trash:///")
        {
            let icon = if self.0.trash_full.get() {
                "user-trash-full"
            } else {
                "user-trash"
            };
            rows.push((Place::Trash(trash), "Trash Can".into(), themed(icon)));
        }
        let mut fixed = |show: bool, label: &str, icon: &str, uri: &str| {
            if let (true, Some(location)) = (show, location_from_uri(uri)) {
                rows.push((Place::Fixed(location), label.to_owned(), themed(icon)));
            }
        };
        fixed(
            settings.places_root(),
            "Filesystem Root",
            "drive-harddisk",
            "file:///",
        );
        // "Applications" belongs to a later version (DISCOVERY D4).
        fixed(
            settings.places_computer(),
            "Devices",
            "computer",
            "computer:///",
        );
        fixed(
            settings.places_network(),
            "Network",
            "network-workgroup",
            "network:///",
        );
        for entry in app.volumes().snapshot() {
            let icon = entry
                .icon
                .as_deref()
                .and_then(|icon| gio::Icon::for_string(icon).ok())
                .unwrap_or_else(|| themed("drive-harddisk"));
            let name = entry.name.clone();
            rows.push((Place::Volume(entry), name, icon));
        }
        for (index, (label, location)) in bookmarks::entries(app).into_iter().enumerate() {
            rows.push((Place::Bookmark(index, location), label, themed("folder")));
        }
        rows
    }

    /// Rebuilds every row from the settings, the volumes and the bookmarks.
    pub fn rebuild(&self) {
        let inner = &self.0;
        let rows = self.collect();
        inner.list.remove_all();
        inner
            .rows
            .replace(rows.iter().map(|(place, _, _)| place.clone()).collect());
        let size = inner.app.settings().pane_icon_size() as i32;
        for (index, (place, label, icon)) in rows.iter().enumerate() {
            inner
                .list
                .append(&self.row(index, place, label, icon, size));
        }
        inner.list.invalidate_headers();
        self.highlight();
    }

    fn row(
        &self,
        index: usize,
        place: &Place,
        label: &str,
        icon: &gio::Icon,
        size: i32,
    ) -> gtk::Box {
        let image = gtk::Image::from_gicon(icon);
        image.set_pixel_size(size);
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_hexpand(true);
        text.set_ellipsize(pango::EllipsizeMode::End);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&image);
        row.append(&text);

        if let Place::Volume(entry) = place
            && entry.mounted
        {
            let eject = gtk::Button::from_icon_name("media-eject-symbolic");
            eject.add_css_class("flat");
            eject.set_focus_on_click(false);
            eject.set_tooltip_text(Some(if entry.can_eject { "Eject" } else { "Unmount" }));
            let weak = self.weak();
            let (id, can_eject) = (entry.id.clone(), entry.can_eject);
            eject.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    (inner.emit)(if can_eject {
                        Request::Eject(id.clone())
                    } else {
                        Request::Unmount(id.clone())
                    });
                }
            });
            row.append(&eject);
        }

        // Right-click: the menu for this kind of place.
        let menu = gtk::GestureClick::new();
        menu.set_button(gdk::BUTTON_SECONDARY);
        let weak = self.weak();
        menu.connect_pressed(move |gesture, _, x, y| {
            if let (Some(places), Some(widget)) = (weak.upgrade().map(Places), gesture.widget()) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                places.show_menu(index, &widget, x, y);
            }
        });
        row.add_controller(menu);

        // Files dropped on a place go into its folder.
        if !matches!(place, Place::Trash(_)) {
            let weak = self.weak();
            row.add_controller(dnd::drop_target(
                move || {
                    let inner = weak.upgrade()?;
                    let location = inner.rows.borrow().get(index)?.location()?;
                    Some((location, None))
                },
                &self.0.hooks,
            ));
        }

        // Bookmarks are reordered by dragging one onto another.
        if let Place::Bookmark(own, _) = place {
            let own = *own as u32;
            let source = gtk::DragSource::new();
            source.set_actions(gdk::DragAction::MOVE);
            source.connect_prepare(move |_, _, _| {
                Some(gdk::ContentProvider::for_value(&own.to_value()))
            });
            row.add_controller(source);
            let target = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
            let weak = self.weak();
            target.connect_drop(
                move |_, value, _, _| match (weak.upgrade(), value.get::<u32>()) {
                    (Some(inner), Ok(from)) if from != own => {
                        bookmarks::move_to(&inner.app, from as usize, own as usize)
                    }
                    _ => false,
                },
            );
            row.add_controller(target);
        }
        row
    }

    fn activate(&self, index: usize, target: Target) {
        let Some(place) = self.0.rows.borrow().get(index).cloned() else {
            return;
        };
        let request = match place {
            Place::Fixed(location) | Place::Trash(location) => {
                Request::Open(location, target, false)
            }
            Place::Bookmark(_, location) => Request::Open(location, target, true),
            Place::Volume(entry) => Request::OpenVolume(entry.id, target),
        };
        (self.0.emit)(request);
    }

    fn show_menu(&self, index: usize, row: &gtk::Widget, x: f64, y: f64) {
        let Some(place) = self.0.rows.borrow().get(index).cloned() else {
            return;
        };
        let group = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        let add = |section: &gio::Menu, label: &str, name: &str, run: Box<dyn Fn(&Places)>| {
            let action = gio::SimpleAction::new(name, None);
            let weak = self.weak();
            action.connect_activate(move |_, _| {
                if let Some(places) = weak.upgrade().map(Places) {
                    run(&places);
                }
            });
            group.add_action(&action);
            section.append(Some(label), Some(&format!("place.{name}")));
        };
        let open = gio::Menu::new();
        add(
            &open,
            "Open in New _Tab",
            "tab",
            Box::new(move |p| p.activate(index, Target::Tab)),
        );
        add(
            &open,
            "Open in New _Window",
            "window",
            Box::new(move |p| p.activate(index, Target::Window)),
        );
        menu.append_section(None, &open);
        let more = gio::Menu::new();
        match &place {
            Place::Fixed(_) => {}
            Place::Trash(_) => add(
                &more,
                "_Empty Trash Can",
                "empty",
                Box::new(|p| (p.0.emit)(Request::EmptyTrash)),
            ),
            Place::Volume(entry) => {
                let id = entry.id.clone();
                if entry.mounted {
                    add(
                        &more,
                        "_Unmount Volume",
                        "unmount",
                        Box::new(move |p| (p.0.emit)(Request::Unmount(id.clone()))),
                    );
                } else if entry.can_mount {
                    add(
                        &more,
                        "_Mount Volume",
                        "mount",
                        Box::new(move |p| (p.0.emit)(Request::Mount(id.clone()))),
                    );
                }
                if entry.can_eject {
                    let id = entry.id.clone();
                    add(
                        &more,
                        "_Eject Removable Media",
                        "eject",
                        Box::new(move |p| (p.0.emit)(Request::Eject(id.clone()))),
                    );
                }
            }
            Place::Bookmark(own, _) => {
                let own = *own;
                let last = bookmarks::entries(&self.0.app).len().saturating_sub(1);
                add(
                    &more,
                    "_Rename Bookmark Item",
                    "rename",
                    Box::new(move |p| (p.0.emit)(Request::RenameBookmark(own))),
                );
                add(
                    &more,
                    "Re_move from Bookmarks",
                    "remove",
                    Box::new(move |p| {
                        bookmarks::remove(&p.0.app, own);
                    }),
                );
                if own > 0 {
                    add(
                        &more,
                        "Move Bookmark _Up",
                        "up",
                        Box::new(move |p| {
                            bookmarks::move_to(&p.0.app, own, own - 1);
                        }),
                    );
                }
                if own < last {
                    add(
                        &more,
                        "Move Bookmark _Down",
                        "down",
                        Box::new(move |p| {
                            bookmarks::move_to(&p.0.app, own, own + 1);
                        }),
                    );
                }
            }
        }
        menu.append_section(None, &more);
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.insert_action_group("place", Some(&group));
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);
        popover.set_parent(row);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }

    /// Marks the row for the folder the active tab shows, if there is one.
    pub fn set_location(&self, location: &Location) {
        self.0.current.replace(Some(location.clone()));
        self.highlight();
    }

    fn highlight(&self) {
        let inner = &self.0;
        let current = inner.current.borrow();
        let index = current.as_ref().and_then(|current| {
            inner
                .rows
                .borrow()
                .iter()
                .position(|place| place.location().as_ref() == Some(current))
        });
        match index.and_then(|index| inner.list.row_at_index(index as i32)) {
            Some(row) => inner.list.select_row(Some(&row)),
            None => inner.list.unselect_all(),
        }
    }

    /// Row labels in order, for agent checks.
    pub fn debug_state(&self) -> String {
        let labels: Vec<String> = self
            .collect()
            .into_iter()
            .map(|(_, label, _)| label)
            .collect();
        let selected = self.0.list.selected_row().map_or(-1, |row| row.index());
        format!("places={labels:?} place_selected={selected}")
    }
}
