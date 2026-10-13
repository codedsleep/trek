//! The main window: menu bar, side pane, folder area and status bar.

mod commands;
mod fileops;
mod finder;
mod keys;
mod open;
mod pane;
mod pathbar;
mod places;
mod prompt;
mod statusbar;
mod toolbar;
mod vim;

use crate::sidepane::SidePane;
use crate::{
    actions::{self, Cmd, Value},
    app::App,
    debug,
    settings::{self, HandlerId, Key},
    tab::{Change, LoadState, TabPage},
};
use gtk::{glib, prelude::*};
use pane::{Pane, PaneEvent};
use pathbar::{PathBar, PathEvent};
use statusbar::StatusBar;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use tallboy_core::{enable, location::Location};
use toolbar::Toolbar;

struct Inner {
    app: App,
    window: gtk::ApplicationWindow,
    toolbar: Toolbar,
    path_bar: PathBar,
    paned: gtk::Paned,
    side: SidePane,
    /// What the side pane was last told to show.
    side_shows: RefCell<Option<(Location, bool)>>,
    side_mode: Cell<Option<tallboy_core::config::PaneMode>>,
    /// One pane, or two in dual-pane mode (§8.5).
    panes: RefCell<Vec<Rc<Pane>>>,
    /// Index of the pane that window commands act on.
    active: Cell<usize>,
    /// Holds the panes side by side.
    split: gtk::Paned,
    status: StatusBar,
    /// The continuations of a started key sequence.
    which_key: prompt::WhichKey,
    /// The one-line input over the folder area.
    input: prompt::InputLine,
    /// The yes/no question over the folder area.
    confirm: prompt::Confirm,
    /// The fuzzy finder over the folder area.
    finder: finder::Picker,
    /// The keys of a sequence that is not complete yet.
    pending: RefCell<Vec<tallboy_core::keymap::Key>>,
    /// A text entry has keyboard focus, so clipboard shortcuts belong to it.
    text_focus: Cell<bool>,
    refresh_queued: Cell<bool>,
    status_queued: Cell<bool>,
    settings_handlers: RefCell<Vec<HandlerId>>,
    clipboard_handler: RefCell<Option<(gtk::gdk::Clipboard, glib::SignalHandlerId)>>,
    /// Move to Trash is taken out of the action map inside trash://, which
    /// hides its menu item there; it is kept here to be put back.
    trash_action: RefCell<Option<gtk::gio::Action>>,
}

/// A main window. Clones share the same window.
#[derive(Clone)]
pub struct MainWindow(Rc<Inner>);

#[derive(Clone)]
pub(super) struct WeakWindow(Weak<Inner>);

impl WeakWindow {
    pub(super) fn upgrade(&self) -> Option<MainWindow> {
        self.0.upgrade().map(MainWindow)
    }
}

impl MainWindow {
    /// Builds a window with one tab on `location`.
    pub fn new(app: &App, location: Location) -> Self {
        let settings = app.settings();
        let window = gtk::ApplicationWindow::builder()
            .application(app.gtk())
            .default_width(settings.win_width() as i32)
            .default_height(settings.win_height() as i32)
            .show_menubar(settings.show_menubar())
            .build();
        if settings.maximized() {
            window.maximize();
        }

        // The side pane and the path bar need the window, which does not
        // exist yet; they reach it through this slot.
        let slot: Rc<RefCell<Option<WeakWindow>>> = Rc::default();
        let side = {
            let (drop_slot, request_slot, open_slot) = (slot.clone(), slot.clone(), slot.clone());
            let settings_for_drop = settings.clone();
            let hooks = crate::view::dnd::Hooks {
                drop: Rc::new(move |request| {
                    let window = drop_slot.borrow().as_ref().and_then(WeakWindow::upgrade);
                    if let Some(window) = window {
                        window.handle_drop(request);
                    }
                }),
                default_action: Rc::new(move || settings_for_drop.drop_default_action()),
                menu: Rc::new(|_, _, _| {}),
                thumbs: None,
                pressed: Rc::new(|_, _| {}),
            };
            SidePane::new(
                app,
                hooks,
                move |request| {
                    let window = request_slot.borrow().as_ref().and_then(WeakWindow::upgrade);
                    if let Some(window) = window {
                        window.place_request(request);
                    }
                },
                move |location| {
                    let window = open_slot.borrow().as_ref().and_then(WeakWindow::upgrade);
                    if let Some(tab) = window.and_then(|w| w.active_tab()) {
                        tab.navigate(location);
                    }
                },
            )
        };
        let pane = Rc::new(Pane::new());
        pane.set_active(true);
        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.set_start_child(Some(pane.widget()));
        split.set_shrink_start_child(false);
        split.set_shrink_end_child(false);
        split.add_css_class("panes");
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(side.widget()));
        paned.set_end_child(Some(&split));
        paned.set_resize_start_child(false);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        paned.set_position(settings.splitter_pos() as i32);
        paned.set_vexpand(true);

        let status = StatusBar::new();
        let toolbar = Toolbar::new();
        let path_bar = PathBar::new({
            let slot = slot.clone();
            move |event| {
                let window = slot.borrow().as_ref().and_then(WeakWindow::upgrade);
                if let Some(window) = window {
                    window.path_event(event);
                }
            }
        });
        toolbar.set_path_bar(path_bar.widget());
        toolbar
            .menu_button()
            .set_menu_model(app.gtk().menubar().as_ref());
        // The which-key list, the input line, the question and the fuzzy
        // finder float over the folder area.
        let which_key = prompt::WhichKey::new();
        let input = prompt::InputLine::new();
        let confirm = prompt::Confirm::new();
        let finder = finder::Picker::new();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&paned));
        overlay.add_overlay(which_key.widget());
        overlay.add_overlay(input.widget());
        overlay.add_overlay(confirm.widget());
        overlay.add_overlay(finder.widget());
        overlay.set_vexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(toolbar.widget());
        content.append(&overlay);
        content.append(status.widget());
        window.set_child(Some(&content));

        let this = Self(Rc::new(Inner {
            app: app.clone(),
            window,
            toolbar,
            path_bar,
            paned,
            side,
            side_shows: RefCell::default(),
            side_mode: Cell::new(None),
            panes: RefCell::new(vec![pane]),
            active: Cell::new(0),
            split,
            status,
            which_key,
            input,
            confirm,
            finder,
            pending: RefCell::default(),
            text_focus: Cell::new(false),
            refresh_queued: Cell::new(false),
            status_queued: Cell::new(false),
            settings_handlers: RefCell::default(),
            clipboard_handler: RefCell::default(),
            trash_action: RefCell::default(),
        }));
        slot.replace(Some(this.downgrade()));
        this.connect_signals();
        this.add_tab(location);
        if settings.dual_pane() {
            this.set_dual_pane(true);
        }
        this.refresh();
        this
    }

    pub(super) fn downgrade(&self) -> WeakWindow {
        WeakWindow(Rc::downgrade(&self.0))
    }

    pub fn widget(&self) -> &gtk::ApplicationWindow {
        &self.0.window
    }

    pub fn present(&self) {
        self.0.window.present();
    }

    fn connect_signals(&self) {
        let inner = &self.0;
        let weak = self.downgrade();
        actions::install(
            &inner.window,
            Rc::new(move |cmd: Cmd, value: Value| {
                if let Some(window) = weak.upgrade() {
                    window.run(cmd, value);
                }
            }),
        );

        let weak = self.downgrade();
        inner.window.connect_close_request(move |_| {
            if let Some(window) = weak.upgrade() {
                window.closing();
            }
            glib::Propagation::Proceed
        });

        // Geometry follows the window as the user resizes it (§8.1).
        let weak = self.downgrade();
        let remember = move || {
            if let Some(window) = weak.upgrade() {
                window.remember_geometry();
            }
        };
        let on_width = remember.clone();
        inner
            .window
            .connect_default_width_notify(move |_| on_width());
        let on_height = remember.clone();
        inner
            .window
            .connect_default_height_notify(move |_| on_height());
        inner.window.connect_maximized_notify(move |_| remember());
        let weak = self.downgrade();
        inner.paned.connect_position_notify(move |paned| {
            if let Some(window) = weak.upgrade()
                && window.0.side.widget().is_visible()
            {
                settings::set::splitter_pos(
                    window.0.app.settings(),
                    paned.position().max(0) as u32,
                );
            }
        });

        let weak = self.downgrade();
        inner.window.connect_fullscreened_notify(move |_| {
            if let Some(window) = weak.upgrade() {
                window.queue_refresh();
            }
        });

        // Clipboard shortcuts must reach a focused text entry, not the files
        // (SCOPE 6.6): track whether an editable has the keyboard focus.
        let weak = self.downgrade();
        inner.window.connect_focus_widget_notify(move |window| {
            let Some(this) = weak.upgrade() else { return };
            let editing = gtk::prelude::GtkWindowExt::focus(window)
                .is_some_and(|w| w.is::<gtk::Text>() || w.is::<gtk::TextView>());
            // Every key typed into an entry is the entry's.
            this.0.app.suspend_accels(editing);
            if editing {
                this.clear_pending();
            }
            if this.0.text_focus.replace(editing) != editing {
                this.refresh_enabled();
            }
        });
        self.watch_keys();
        // When the input line or the finder closes the files have the
        // keyboard again.
        let weak = self.downgrade();
        let to_files = move || {
            if let Some(tab) = weak.upgrade().and_then(|window| window.active_tab()) {
                tab.grab_focus();
            }
        };
        inner.input.connect_closed(to_files.clone());
        inner.finder.connect_closed(to_files);

        let first = inner.panes.borrow()[0].clone();
        self.attach_pane(&first);

        // The history menu is built when it is opened.
        let weak = self.downgrade();
        inner
            .toolbar
            .history_button()
            .set_create_popup_func(move |button| {
                let menu = gtk::gio::Menu::new();
                if let Some(tab) = weak.upgrade().and_then(|w| w.active_tab()) {
                    let (labels, _) = tab.history_labels();
                    // Newest first, as upstream lists them.
                    for (index, label) in labels.iter().enumerate().rev() {
                        menu.append(Some(label), Some(&format!("win.history-go::{index}")));
                    }
                }
                button.set_menu_model(Some(&menu));
            });

        let weak = self.downgrade();
        let handlers = inner.app.settings().connect_changed_any(
            &[
                Key::ShowStatusbar,
                Key::SiUnit,
                Key::SidePaneMode,
                Key::Toolbar,
                Key::PathbarModeButtons,
                Key::AlwaysShowTabs,
                Key::HideCloseBtn,
                Key::MaxTabChars,
                Key::ShowMenubar,
                Key::MillerParent,
                Key::MillerPreview,
            ],
            move || {
                if let Some(window) = weak.upgrade() {
                    window.queue_refresh();
                }
            },
        );
        inner.settings_handlers.replace(handlers);

        // Paste follows what is on the clipboard.
        let weak = self.downgrade();
        inner
            .clipboard_handler
            .replace(crate::clipboard::connect_changed(move || {
                if let Some(window) = weak.upgrade() {
                    // What is yanked, and so which files are marked.
                    for tab in window.all_tabs() {
                        tab.model().refresh_yank();
                    }
                    window.queue_refresh();
                }
            }));

        let weak = self.downgrade();
        inner.window.connect_map(move |window| {
            let Some(this) = weak.upgrade() else { return };
            debug::after_next_paint(window, move || {
                debug::trace("first frame");
                this.announce_ready();
            });
        });
    }

    fn announce_ready(&self) {
        thread_local!(static ANNOUNCED: Cell<bool> = const { Cell::new(false) });
        if !ANNOUNCED.replace(true) {
            // The agent driver waits for this line before it starts.
            eprintln!("tallboy ready");
        }
    }

    fn closing(&self) {
        self.remember_geometry();
        let settings = self.0.app.settings();
        settings.disconnect(&self.0.settings_handlers.take());
        if let Some((clipboard, handler)) = self.0.clipboard_handler.take() {
            clipboard.disconnect(handler);
        }
        settings.save_now();
        self.0.app.forget_window(&self.0.window);
    }

    fn remember_geometry(&self) {
        let window = &self.0.window;
        let settings = self.0.app.settings();
        settings::set::maximized(settings, window.is_maximized());
        if window.is_maximized() || window.is_fullscreen() {
            return;
        }
        let (width, height) = window.default_size();
        if width > 0 && height > 0 {
            settings::set::win_width(settings, width as u32);
            settings::set::win_height(settings, height as u32);
        }
    }

    /// Opens `location` in a new tab of the active pane.
    pub fn add_tab(&self, location: Location) {
        let tab = self.new_tab(location);
        self.pane().add(tab.clone(), self.0.app.settings());
        tab.grab_focus();
        self.queue_refresh();
    }

    /// Creates a tab wired to this window; the caller puts it in a pane.
    fn new_tab(&self, location: Location) -> TabPage {
        let tab = TabPage::new(&self.0.app, location);
        let weak = self.downgrade();
        let weak_tab = tab.downgrade();
        tab.connect_changed(move |change| {
            if let (Some(window), Some(tab)) = (weak.upgrade(), weak_tab.upgrade()) {
                window.tab_changed(&tab, change);
            }
        });
        let weak = self.downgrade();
        tab.connect_drop(move |request| {
            if let Some(window) = weak.upgrade() {
                window.handle_drop(request);
            }
        });
        let weak = self.downgrade();
        tab.connect_open(move |items| {
            if let Some(window) = weak.upgrade() {
                window.open_items(items);
            }
        });
        tab
    }

    /// Selects a file in the tab that is showing, once it is listed.
    pub fn select_when_listed(&self, uri: &str) {
        if let Some(tab) = self.active_tab() {
            tab.select_when_listed(uri);
        }
    }

    /// The pane that window commands act on.
    pub(super) fn pane(&self) -> Rc<Pane> {
        let panes = self.0.panes.borrow();
        panes[self.0.active.get().min(panes.len() - 1)].clone()
    }

    fn pane_of(&self, tab: &TabPage) -> Option<Rc<Pane>> {
        self.0
            .panes
            .borrow()
            .iter()
            .find(|pane| pane.holds(tab))
            .cloned()
    }

    /// Every tab of the window, in both panes.
    pub(super) fn all_tabs(&self) -> Vec<TabPage> {
        self.0
            .panes
            .borrow()
            .iter()
            .flat_map(|pane| pane.tabs())
            .collect()
    }

    /// Wires a pane to the window: its tab events, and becoming the active
    /// pane when it is clicked or gets the keyboard (§8.5).
    fn attach_pane(&self, pane: &Rc<Pane>) {
        let weak = self.downgrade();
        let weak_pane = Rc::downgrade(pane);
        pane.connect(move |event| {
            let (Some(window), Some(pane)) = (weak.upgrade(), weak_pane.upgrade()) else {
                return;
            };
            match event {
                PaneEvent::Switched => {
                    pane.note_current();
                    // A question was about the files of the tab it was
                    // asked in; "yes" must not arrive over another one.
                    window.0.confirm.answer(false);
                    // And the finder was searching below its folder.
                    window.0.finder.close();
                    window.clear_pending();
                    window.queue_refresh();
                }
                PaneEvent::Reordered => pane.renumber(),
                PaneEvent::CloseRequested(tab) => window.close_tab(&tab),
            }
        });
        let activate = {
            let weak = self.downgrade();
            let weak_pane = Rc::downgrade(pane);
            move || {
                if let (Some(window), Some(pane)) = (weak.upgrade(), weak_pane.upgrade()) {
                    window.activate_pane(&pane);
                }
            }
        };
        let focus = gtk::EventControllerFocus::new();
        let on_focus = activate.clone();
        focus.connect_enter(move |_| on_focus());
        pane.widget().add_controller(focus);
        let press = gtk::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk::PropagationPhase::Capture);
        press.connect_pressed(move |_, _, _, _| activate());
        pane.widget().add_controller(press);
    }

    fn activate_pane(&self, pane: &Rc<Pane>) {
        let panes = self.0.panes.borrow().clone();
        let Some(index) = panes.iter().position(|other| Rc::ptr_eq(other, pane)) else {
            return;
        };
        if self.0.active.replace(index) == index {
            return;
        }
        for (other_index, other) in panes.iter().enumerate() {
            other.set_active(other_index == index);
        }
        self.queue_refresh();
    }

    /// Dual Pane Mode: a second pane beside the first, opened at the current
    /// folder; turning it off closes the second pane's tabs (§8.5).
    pub(super) fn set_dual_pane(&self, on: bool) {
        let inner = &self.0;
        let count = inner.panes.borrow().len();
        if on && count == 1 {
            let location = self
                .active_tab()
                .map(|tab| tab.location())
                .or_else(|| inner.app.home_location());
            let pane = Rc::new(Pane::new());
            self.attach_pane(&pane);
            inner.split.set_end_child(Some(pane.widget()));
            inner.panes.borrow_mut().push(pane.clone());
            if let Some(location) = location {
                // The new tab goes into the second pane without taking the
                // keyboard from the first.
                let tab = self.new_tab(location);
                pane.add(tab, inner.app.settings());
            }
            // Split the room evenly.
            let width = inner.split.width();
            if width > 0 {
                inner.split.set_position(width / 2);
            }
        } else if !on && count == 2 {
            // Focus leaves the second pane before it is taken out.
            let first = inner.panes.borrow()[0].clone();
            inner.active.set(0);
            first.set_active(true);
            if let Some(tab) = first.current() {
                tab.grab_focus();
            }
            let closed = inner.panes.borrow_mut().remove(1);
            inner.split.set_end_child(gtk::Widget::NONE);
            drop(closed);
        }
        if on {
            inner.split.add_css_class("dual");
        } else {
            inner.split.remove_css_class("dual");
        }
        settings::set::dual_pane(inner.app.settings(), on);
        self.queue_refresh();
    }

    /// F6: keyboard focus from the side pane to pane 1 to pane 2 and round.
    pub(super) fn focus_next_pane(&self) {
        let inner = &self.0;
        let panes = inner.panes.borrow().clone();
        let side_shown = inner.side.widget().is_visible();
        let in_side = inner
            .side
            .widget()
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN);
        let current = if in_side {
            None
        } else {
            Some(inner.active.get())
        };
        let next = match current {
            None => Some(0),
            Some(index) if index + 1 < panes.len() => Some(index + 1),
            Some(_) if side_shown => None,
            Some(_) => Some(0),
        };
        match next {
            Some(index) => {
                self.activate_pane(&panes[index]);
                if let Some(tab) = panes[index].current() {
                    tab.grab_focus();
                }
            }
            None => inner.side.grab_focus(),
        }
    }

    /// Closes a tab; closing the last one closes the window (§8.4).
    pub fn close_tab(&self, tab: &TabPage) {
        let Some(pane) = self.pane_of(tab) else {
            return;
        };
        // A pane about to lose its last tab hands the keyboard to the other
        // pane first, so that focus is never inside a widget being removed.
        if pane.tabs().len() == 1 {
            let other = self
                .0
                .panes
                .borrow()
                .iter()
                .find(|other| !Rc::ptr_eq(other, &pane))
                .cloned();
            if let Some(other) = other {
                self.activate_pane(&other);
                if let Some(tab) = other.current() {
                    tab.grab_focus();
                }
            }
        }
        if pane.close(tab, self.0.app.settings()) == 0 {
            // The last tab of the only pane closes the window; the last tab
            // of one of two panes leaves dual-pane mode with the other.
            let panes = self.0.panes.borrow().clone();
            if panes.len() == 1 {
                self.0.window.close();
                return;
            }
            if Rc::ptr_eq(&panes[0], &pane) {
                // The first pane is the one that stays: it takes over the
                // second pane's tabs, keeping the one that was showing.
                let showing = panes[1].current();
                for moved in panes[1].tabs() {
                    panes[1].close(&moved, self.0.app.settings());
                    panes[0].add(moved, self.0.app.settings());
                }
                if let Some(showing) = showing {
                    panes[0].show(&showing);
                }
            }
            self.set_dual_pane(false);
        }
        if let Some(current) = self.active_tab() {
            current.grab_focus();
        }
        self.queue_refresh();
    }

    fn path_event(&self, event: PathEvent) {
        match event {
            PathEvent::Navigate(location) => {
                if let Some(tab) = self.active_tab() {
                    tab.navigate(location);
                    tab.grab_focus();
                }
            }
            PathEvent::NewTab(location) => self.add_tab(location),
            PathEvent::Dismissed => {
                if let Some(tab) = self.active_tab() {
                    tab.grab_focus();
                }
            }
            PathEvent::Invalid(message) => {
                crate::dialogs::base::alert(
                    Some(self.0.window.upcast_ref()),
                    "Cannot go to this location",
                    &message,
                );
            }
        }
    }

    /// The tab that window commands act on.
    pub fn active_tab(&self) -> Option<TabPage> {
        self.pane().current()
    }

    fn tab_changed(&self, tab: &TabPage, change: Change) {
        if change == Change::Location
            && let Some(pane) = self.pane_of(tab)
        {
            pane.set_label(tab);
        }
        if change == Change::Load && self.0.app.smoke() {
            self.smoke_check(tab);
        }
        // Likewise when the folder under a question changes.
        if change == Change::Location {
            self.0.confirm.answer(false);
            self.0.finder.close();
        }
        match change {
            // Many of these arrive while a key is held; they only move what
            // the status line says and what commands apply to.
            Change::Cursor | Change::Selection => self.queue_status(),
            Change::Location | Change::Status | Change::Load => self.queue_refresh(),
        }
    }

    /// Coalesces status-line updates after cursor and selection changes.
    fn queue_status(&self) {
        if self.0.refresh_queued.get() || self.0.status_queued.replace(true) {
            return;
        }
        let weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(window) = weak.upgrade() {
                window.0.status_queued.set(false);
                if let Some(tab) = window.active_tab() {
                    window.0.status.set(&tab.status());
                }
                window.refresh_enabled();
            }
        });
    }

    /// `TALLBOY_SMOKE=1`: leave once the first folder has loaded and been
    /// painted, with a failing status when it could not be loaded.
    fn smoke_check(&self, tab: &TabPage) {
        let app = self.0.app.clone();
        match tab.load_state() {
            LoadState::Loading => {}
            LoadState::Loaded => {
                let window = self.0.window.clone();
                let tab = tab.downgrade();
                // Wait for the incremental sort, then for one painted frame.
                glib::idle_add_local(move || {
                    if tab
                        .upgrade()
                        .is_some_and(|tab| tab.model().sorted().pending() > 0)
                    {
                        return glib::ControlFlow::Continue;
                    }
                    let quit = {
                        let app = app.clone();
                        Rc::new(move || app.finish_smoke(0))
                    };
                    let on_paint = quit.clone();
                    debug::after_next_paint(&window, move || on_paint());
                    window.queue_draw();
                    // A fully obscured window may never paint again.
                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(500),
                        move || quit(),
                    );
                    glib::ControlFlow::Break
                });
            }
            LoadState::Failed(message) => {
                eprintln!("tallboy smoke: {message}");
                app.finish_smoke(1);
            }
        }
    }

    /// Coalesces chrome updates: many tab events, one refresh.
    pub(super) fn queue_refresh(&self) {
        if self.0.refresh_queued.replace(true) {
            return;
        }
        let weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(window) = weak.upgrade() {
                window.0.refresh_queued.set(false);
                window.refresh();
            }
        });
    }

    /// Brings title, status bar and action states in line with the active tab
    /// and the settings. Action state is derived here and nowhere else.
    fn refresh(&self) {
        let inner = &self.0;
        let settings = inner.app.settings();
        let window = &inner.window;
        let tab = self.active_tab();

        window.set_title(Some(
            &tab.as_ref()
                .map_or_else(|| "tallboy".into(), TabPage::title),
        ));
        inner.toolbar.apply(&settings.toolbar());
        // Two panes leave no room for three columns each.
        let single = inner.panes.borrow().len() == 1;
        for pane in inner.panes.borrow().iter() {
            pane.apply_settings(settings);
            for tab in pane.tabs() {
                tab.set_columns_shown(
                    single && settings.miller_parent(),
                    single && settings.miller_preview(),
                );
            }
        }
        inner
            .path_bar
            .set_buttons_mode(settings.pathbar_mode_buttons());
        if let Some(tab) = &tab {
            inner.path_bar.set_location(
                &tab.location(),
                settings.home(),
                tab.model().show_hidden(),
            );
            let (_, index) = tab.history_labels();
            actions::set_text(window, "history-go", &index.to_string());
        }
        window.set_show_menubar(settings.show_menubar());
        inner.status.widget().set_visible(settings.show_statusbar());
        if let Some(tab) = &tab {
            inner.status.set(&tab.status());
        }
        let side = settings.side_pane_mode();
        inner.side.widget().set_visible(!side.hidden);
        inner.side.set_mode(side.mode);
        if let Some(tab) = &tab {
            // Only when something changed: following a path walks the tree.
            let shows = (tab.location(), tab.model().show_hidden());
            let stale = inner.side_shows.borrow().as_ref() != Some(&shows);
            if stale || inner.side_mode.replace(Some(side.mode)) != Some(side.mode) {
                inner.side.set_location(&shows.0, shows.1);
                inner.side_shows.replace(Some(shows));
            }
        }

        let toolbar = settings.toolbar().0;
        let has = |name: &str| toolbar.iter().any(|t| t == name);
        for (action, on) in [
            ("show-statusbar", settings.show_statusbar()),
            ("show-menubar", settings.show_menubar()),
            ("miller-parent", settings.miller_parent()),
            ("miller-preview", settings.miller_preview()),
            ("sidepane-show", !side.hidden),
            ("dual-pane", inner.panes.borrow().len() == 2),
            ("fullscreen", window.is_fullscreen()),
            ("toolbar-show", !has("hidden")),
            ("toolbar-newwin", has("newwin")),
            ("toolbar-newtab", has("newtab")),
            ("toolbar-nav", has("navigation")),
            ("toolbar-home", has("home")),
        ] {
            actions::set_bool(window, action, on);
        }
        actions::set_text(window, "sidepane-mode", &side.mode.to_string());
        actions::set_text(
            window,
            "pathbar-mode",
            if settings.pathbar_mode_buttons() {
                "buttons"
            } else {
                "location"
            },
        );
        if let Some(tab) = &tab {
            let model = tab.model();
            let sort = model.sort();
            let sort_text = sort.to_string();
            let column = sort_text.split(';').next().unwrap_or("name");
            actions::set_bool(window, "show-hidden", model.show_hidden());
            actions::set_text(window, "sort-by", column);
            actions::set_text(
                window,
                "sort-order",
                if sort.descending {
                    "descending"
                } else {
                    "ascending"
                },
            );
            actions::set_bool(window, "sort-mingle", sort.mingle);
            actions::set_bool(window, "sort-ignore-case", !sort.case_sensitive);
            actions::set_text(window, "view-mode", &tab.mode().to_string());
            actions::set_bool(window, "preserve-settings", tab.preserves_settings());
        }
        self.refresh_enabled();
    }

    /// Enables each file command for the selection, the folder and the
    /// clipboard, as `core::enable` decides (§7.7). Menu bar, context menus
    /// and shortcuts all read these same actions.
    fn refresh_enabled(&self) {
        let window = &self.0.window;
        let tab = self.active_tab();
        let nav = |check: fn(&TabPage) -> bool| tab.as_ref().is_some_and(check);
        actions::set_enabled(window, "back", nav(TabPage::can_go_back));
        actions::set_enabled(window, "forward", nav(TabPage::can_go_forward));
        actions::set_enabled(window, "up", nav(TabPage::can_go_up));
        actions::set_enabled(
            window,
            "preserve-settings",
            self.0.app.dir_settings().is_some(),
        );

        // What a command would act on: the selection, or the cursor's file.
        let selected = tab.as_ref().map(TabPage::targets).unwrap_or_default();
        let every = |test: fn(&tallboy_fs::meta::Access) -> bool| {
            selected.iter().all(|item| test(&item.meta().access))
        };
        let selection = enable::Selection {
            count: selected.len(),
            deletable: every(|access| access.delete),
            trashable: every(|access| access.trash),
            renameable: every(|access| access.rename),
            readable: every(|access| access.read),
        };
        let in_trash = nav(TabPage::in_trash);
        let allowed = enable::actions(
            selection,
            nav(TabPage::folder_writable),
            in_trash,
            crate::clipboard::has_files(),
        );
        // While a text entry has the keyboard, the clipboard keys are its own.
        let files = !self.0.text_focus.get();
        let some = selection.count > 0;
        let single_folder = matches!(selected.as_slice(), [only] if only.is_dir());
        for (action, enabled) in [
            ("open", allowed.open),
            ("open-with", allowed.open),
            ("open-new-tab", allowed.open),
            ("cut", allowed.cut && files),
            ("copy", allowed.copy && files),
            ("paste", allowed.paste && files),
            ("select-all", files),
            ("trash", allowed.trash),
            ("delete", allowed.delete),
            ("rename", allowed.rename),
            ("copy-path", some),
            ("create-link", some && !in_trash),
            ("copy-to", allowed.copy),
            ("move-to", allowed.cut),
            ("create-folder", allowed.create),
            ("create-file", allowed.create),
            ("restore", allowed.restore),
            ("properties", some),
            ("open-new-window", single_folder),
            ("open-terminal-here", single_folder),
            (
                "paste-into",
                single_folder && crate::clipboard::has_files() && !in_trash,
            ),
        ] {
            actions::set_enabled(window, action, enabled);
        }
        self.show_trash_action(allowed.trash_visible);
    }

    /// Adds or removes `win.trash`. Its menu items are hidden while the
    /// action is missing, which is how Move to Trash disappears in trash://.
    fn show_trash_action(&self, visible: bool) {
        let window = &self.0.window;
        let mut stash = self.0.trash_action.borrow_mut();
        match (visible, window.lookup_action("trash")) {
            (false, Some(action)) => {
                window.remove_action("trash");
                *stash = Some(action);
            }
            (true, None) => {
                if let Some(action) = stash.take() {
                    window.add_action(&action);
                }
            }
            _ => {}
        }
    }
}
