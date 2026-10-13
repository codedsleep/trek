//! One tab: a location, its history, its folder and the view showing it.

mod events;
mod menus;
mod nav;
mod status;
mod typeahead;
mod views;

use crate::{
    app::App,
    debug,
    settings::HandlerId,
    view::{FileItem, FolderModel, View},
};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::{Rc, Weak},
    time::Instant,
};
use tallboy_core::{
    config::{Columns, ViewMode},
    history::History,
    location::Location,
};
use tallboy_fs::folder::{Folder, FsInfo};

pub use status::Status;

/// What changed in a tab; listeners refresh the matching chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The tab shows another folder: title, path bar, history buttons.
    Location,
    /// Item counts, free space, the filter or a view setting changed.
    Status,
    /// The selection changed.
    Selection,
    /// The load state changed.
    Load,
}

/// Where the tab is with loading its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Loading,
    Loaded,
    Failed(String),
}

type Listener = Rc<dyn Fn(Change)>;
type Opener = Rc<dyn Fn(Vec<FileItem>)>;
type Dropper = Rc<dyn Fn(crate::view::dnd::DropRequest)>;

/// View state to restore once a folder has loaded.
#[derive(Debug, Default)]
struct Restore {
    cursor: Option<String>,
    scroll: Option<f64>,
}

struct Inner {
    app: App,
    root: gtk::Overlay,
    scroller: gtk::ScrolledWindow,
    empty: gtk::Label,
    error: gtk::Box,
    error_text: gtk::Label,
    model: FolderModel,
    view: RefCell<Option<View>>,
    /// The grid's column-fitting handler and the adjustment it watches.
    view_fit: RefCell<Option<(gtk::Adjustment, gtk::glib::SignalHandlerId)>>,
    mode: Cell<ViewMode>,
    columns: RefCell<Columns>,
    /// "Preserve This Folder's Settings" is on for the current folder.
    preserve: Cell<bool>,
    location: RefCell<Location>,
    history: RefCell<History>,
    folder: RefCell<Option<debug::Counted<Folder>>>,
    generation: Cell<u64>,
    load: RefCell<LoadState>,
    fs_info: RefCell<FsInfo>,
    restore: RefCell<Restore>,
    /// A file expected to appear soon (just created or renamed): select it.
    awaited: RefCell<Option<String>>,
    /// Whether the folder itself accepts new files; assumed until known.
    writable: Cell<bool>,
    /// `id::filesystem` of the folder itself, for the drop default "auto".
    filesystem: RefCell<Option<String>>,
    listeners: RefCell<Vec<Listener>>,
    opener: RefCell<Option<Opener>>,
    dropper: RefCell<Option<Dropper>>,
    settings_handlers: RefCell<Vec<HandlerId>>,
    /// While a folder loads, the view stays at its start until the user
    /// scrolls, clicks or types; otherwise GTK keeps the first item it showed
    /// in place and the view drifts as sorted items arrive above it.
    pin_start: Cell<bool>,
    pin_queued: Cell<bool>,
    /// The view should have the keyboard once it has something to focus.
    wants_focus: Cell<bool>,
    type_ahead: typeahead::TypeAhead,
    load_started: Cell<Option<Instant>>,
    stalls: RefCell<Option<debug::StallWatch>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.app
            .settings()
            .disconnect(&self.settings_handlers.borrow());
        debug::count(&debug::LIVE_TABS, -1);
    }
}

/// A tab page. Clones share the same tab.
#[derive(Clone)]
pub struct TabPage(Rc<Inner>);

/// A weak tab reference for signal closures.
#[derive(Clone)]
pub struct WeakTab(Weak<Inner>);

impl WeakTab {
    pub fn upgrade(&self) -> Option<TabPage> {
        self.0.upgrade().map(TabPage)
    }
}

/// Builds a `Location` from a URI produced by GIO.
pub fn location_from_uri(uri: &str) -> Option<Location> {
    Location::parse(uri, Path::new("/"), Path::new("/")).ok()
}

impl TabPage {
    /// Creates a tab and starts loading `location`.
    pub fn new(app: &App, location: Location) -> Self {
        let settings = app.settings();
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        let empty = gtk::Label::new(Some("Folder is empty"));
        empty.add_css_class("dim-label");
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        // The placeholder must not swallow clicks meant for the view.
        empty.set_can_target(false);
        empty.set_visible(false);
        let error_text = gtk::Label::new(None);
        error_text.set_wrap(true);
        error_text.set_justify(gtk::Justification::Center);
        let retry = gtk::Button::with_label("Retry");
        retry.set_halign(gtk::Align::Center);
        let error = gtk::Box::new(gtk::Orientation::Vertical, 12);
        error.set_halign(gtk::Align::Center);
        error.set_valign(gtk::Align::Center);
        error.append(&error_text);
        error.append(&retry);
        error.set_visible(false);
        let type_ahead = typeahead::TypeAhead::new();
        let root = gtk::Overlay::new();
        root.set_child(Some(&scroller));
        root.add_overlay(&empty);
        root.add_overlay(&error);
        root.add_overlay(type_ahead.widget());

        let tab = Self(Rc::new(Inner {
            app: app.clone(),
            root,
            scroller,
            empty,
            error,
            error_text,
            model: FolderModel::new(settings.sort(), settings.show_hidden()),
            view: RefCell::default(),
            view_fit: RefCell::default(),
            mode: Cell::new(settings.view_mode()),
            columns: RefCell::new(settings.columns()),
            preserve: Cell::new(false),
            location: RefCell::new(location.clone()),
            history: RefCell::default(),
            folder: RefCell::default(),
            generation: Cell::new(0),
            load: RefCell::new(LoadState::Loading),
            fs_info: RefCell::default(),
            restore: RefCell::default(),
            awaited: RefCell::default(),
            writable: Cell::new(true),
            filesystem: RefCell::default(),
            listeners: RefCell::default(),
            opener: RefCell::default(),
            dropper: RefCell::default(),
            settings_handlers: RefCell::default(),
            pin_start: Cell::new(true),
            pin_queued: Cell::new(false),
            wants_focus: Cell::new(false),
            type_ahead,
            load_started: Cell::new(None),
            stalls: RefCell::default(),
        }));
        debug::count(&debug::LIVE_TABS, 1);

        let weak = tab.downgrade();
        retry.connect_clicked(move |_| {
            if let Some(tab) = weak.upgrade() {
                tab.reload();
            }
        });
        let selection = tab.0.model.selection();
        let weak = tab.downgrade();
        selection.connect_selection_changed(move |_, _, _| {
            if let Some(tab) = weak.upgrade() {
                tab.emit(Change::Selection);
            }
        });
        let weak = tab.downgrade();
        selection.connect_items_changed(move |_, _, _, _| {
            if let Some(tab) = weak.upgrade() {
                tab.keep_at_start();
                tab.forget_type_ahead_names();
                tab.update_placeholders();
                tab.emit(Change::Status);
            }
        });
        tab.watch_user_input();
        tab.watch_settings();
        tab.accept_drops_on_background();
        tab.watch_background_menu();
        tab.connect_type_ahead();
        tab.0.history.borrow_mut().navigate(location);
        tab.load_folder_settings();
        tab.rebuild_view();
        tab.start_load();
        tab
    }

    pub fn downgrade(&self) -> WeakTab {
        WeakTab(Rc::downgrade(&self.0))
    }

    /// The widget to put in a notebook page.
    pub fn widget(&self) -> &gtk::Overlay {
        &self.0.root
    }

    /// Registers a listener. It must capture its owner weakly.
    pub fn connect_changed(&self, listener: impl Fn(Change) + 'static) {
        self.0.listeners.borrow_mut().push(Rc::new(listener));
    }

    /// Sets who opens activated files. It must capture its owner weakly.
    pub fn connect_open(&self, opener: impl Fn(Vec<FileItem>) + 'static) {
        self.0.opener.replace(Some(Rc::new(opener)));
    }

    /// Sets who handles files dropped on this tab. Weak capture, as above.
    pub fn connect_drop(&self, dropper: impl Fn(crate::view::dnd::DropRequest) + 'static) {
        self.0.dropper.replace(Some(Rc::new(dropper)));
    }

    /// What cells and the background need for drag and drop.
    pub fn drop_hooks(&self) -> crate::view::dnd::Hooks {
        let weak = self.downgrade();
        let settings = self.0.app.settings().clone();
        crate::view::dnd::Hooks {
            drop: Rc::new(move |request| {
                let dropper = weak
                    .upgrade()
                    .and_then(|tab| tab.0.dropper.borrow().clone());
                if let Some(dropper) = dropper {
                    dropper(request);
                }
            }),
            default_action: Rc::new(move || settings.drop_default_action()),
            menu: {
                let weak = self.downgrade();
                Rc::new(move |widget, x, y| {
                    if let Some(tab) = weak.upgrade() {
                        tab.show_context_menu(Some((widget, x, y)));
                    }
                })
            },
            thumbs: self
                .0
                .app
                .settings()
                .show_thumbnail()
                .then(|| self.0.app.thumbs())
                .flatten()
                .map(|thumbs| {
                    crate::view::thumbs::Loader::new(thumbs, self.0.app.settings().config())
                }),
        }
    }

    /// A drop on the view, outside any folder item, goes to this folder.
    fn accept_drops_on_background(&self) {
        let weak = self.downgrade();
        let target = crate::view::dnd::drop_target(
            move || {
                let tab = weak.upgrade()?;
                Some((tab.location(), tab.filesystem_id()))
            },
            &self.drop_hooks(),
        );
        self.0.scroller.add_controller(target);
    }

    fn emit(&self, change: Change) {
        let listeners = self.0.listeners.borrow().clone();
        for listener in listeners {
            listener(change);
        }
    }

    /// The folder this tab shows.
    pub fn location(&self) -> Location {
        self.0.location.borrow().clone()
    }

    /// The folder's display name, used for the tab label and window title.
    pub fn title(&self) -> String {
        let location = self.0.location.borrow();
        location
            .breadcrumbs()
            .last()
            .map_or_else(|| location.display(), |crumb| crumb.label.clone())
    }

    /// Whether this tab shows the trash can, where files cannot be cut,
    /// renamed, created or trashed again (§8.16).
    pub fn in_trash(&self) -> bool {
        self.0.location.borrow().to_uri().starts_with("trash://")
    }

    /// The filesystem this folder is on, once it has been asked.
    pub fn filesystem_id(&self) -> Option<String> {
        self.0.filesystem.borrow().clone()
    }

    /// Whether files can be created in or pasted into this folder.
    pub fn folder_writable(&self) -> bool {
        self.0.writable.get()
    }

    /// Selects and scrolls to a file once the folder monitor reports it,
    /// or at once when it is already listed.
    pub fn select_when_listed(&self, uri: &str) {
        if !self.select_uri(uri) {
            self.0.awaited.replace(Some(uri.to_owned()));
        }
    }

    pub(super) fn select_uri(&self, uri: &str) -> bool {
        let Some(position) = self.0.model.position_of(uri) else {
            return false;
        };
        self.0.pin_start.set(false);
        if let Some(view) = self.0.view.borrow().as_ref() {
            view.scroll_to(
                position,
                gtk::ListScrollFlags::SELECT | gtk::ListScrollFlags::FOCUS,
            );
        }
        true
    }

    /// The current load state.
    pub fn load_state(&self) -> LoadState {
        self.0.load.borrow().clone()
    }

    /// The tab's model, for commands that act on the selection.
    pub fn model(&self) -> &FolderModel {
        &self.0.model
    }

    /// Gives keyboard focus to the folder view. An empty or loading view
    /// has nothing to focus yet, so the wish is kept until items arrive.
    pub fn grab_focus(&self) {
        self.0.wants_focus.set(true);
        if let Some(view) = self.0.view.borrow().as_ref() {
            view.grab_focus();
        }
    }

    /// Whether the keyboard focus is inside this tab's view.
    pub(super) fn has_focus(&self) -> bool {
        self.0
            .scroller
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN)
    }

    /// Opens the item at a view position. When that item is part of the
    /// selection the whole selection is opened, as upstream does.
    pub fn activate(&self, position: u32) {
        let Some(item) = self.0.model.item_at(position) else {
            return;
        };
        let items = if self.0.model.selection().is_selected(position) {
            self.0.model.selected()
        } else {
            vec![item]
        };
        self.open(items);
    }

    /// Hands items to the window to be opened.
    pub fn open(&self, items: Vec<FileItem>) {
        let opener = self.0.opener.borrow().clone();
        if let Some(opener) = opener
            && !items.is_empty()
        {
            opener(items);
        }
    }

    pub fn select_all(&self) {
        self.0.model.selection().select_all();
    }

    pub fn invert_selection(&self) {
        let selection = self.0.model.selection();
        let count = selection.n_items();
        if count == 0 {
            return;
        }
        let inverted = gtk::Bitset::new_range(0, count);
        inverted.subtract(&selection.selection());
        let all = gtk::Bitset::new_range(0, count);
        selection.set_selection(&inverted, &all);
    }

    fn update_placeholders(&self) {
        let state = self.0.load.borrow().clone();
        let failed = matches!(state, LoadState::Failed(_));
        if let LoadState::Failed(message) = &state {
            self.0.error_text.set_text(message);
        }
        self.0.error.set_visible(failed);
        self.0
            .empty
            .set_visible(state == LoadState::Loaded && self.0.model.visible_count() == 0);
    }

    /// One line describing the tab, for agent checks (`win.debug-dump`).
    pub fn debug_state(&self) -> String {
        let model = &self.0.model;
        let names: Vec<String> = model
            .selected()
            .iter()
            .take(8)
            .map(|item| item.meta().display_name.clone())
            .collect();
        format!(
            "location={} load={:?} mode={} sort={} columns={:?} preserve={} filter={:?} items={} visible={} hidden={} selected={} names={:?} scroll={:.0}",
            self.0.location.borrow().to_uri(),
            self.0.load.borrow(),
            self.0.mode.get(),
            model.sort(),
            self.0.columns.borrow().to_string(),
            self.0.preserve.get(),
            model.glob().unwrap_or_default(),
            model.total_count(),
            model.visible_count(),
            model.hidden_count(),
            model.selected_count(),
            names,
            self.scroll_adjustment().value(),
        )
    }
}
