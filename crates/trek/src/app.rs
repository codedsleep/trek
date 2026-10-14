//! The application object: single instance, profiles, windows, lifetime.

use crate::{
    actions, cli, debug, listeners::Listeners, menubar, settings::Settings, window::MainWindow,
};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashSet,
    ffi::OsString,
    rc::Rc,
};
use trek_core::{invocation, location::Location};
use trek_fs::{bookmarks, dir_settings::DirectoryStore, volumes};

const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

struct Inner {
    gtk: gtk::Application,
    settings: Settings,
    /// The bookmarks shared with GTK file choosers; `None` if unreadable.
    bookmarks: Option<bookmarks::Store>,
    /// The Bookmarks menu's live section, shared by every window.
    bookmarks_menu: gio::Menu,
    volumes: volumes::Model,
    /// The whole volume inventory as last seen, to tell what changed.
    inventory: RefCell<Vec<volumes::Entry>>,
    /// Volumes this process mounted because they were inserted.
    automounted: RefCell<HashSet<String>>,
    /// Bookmarks or volumes changed: side panes rebuild.
    places_changed: Listeners,
    /// The thumbnail workers and texture cache; `None` if they cannot start.
    thumbs: Option<Rc<crate::view::thumbs::Thumbs>>,
    /// Per-folder view settings; `None` when the file could not be read.
    dir_settings: Option<DirectoryStore>,
    windows: RefCell<Vec<MainWindow>>,
    /// Held while daemon mode keeps the process alive without a window.
    daemon: RefCell<Option<gio::ApplicationHoldGuard>>,
    seen_first_invocation: Cell<bool>,
    smoke: bool,
    exit_code: Cell<u8>,
    /// The key bindings of the file view.
    keymap: trek_core::keymap::Keymap,
    /// Accelerators are off while a text entry has the keyboard.
    accels_suspended: Cell<bool>,
}

/// Shared application state. Clones are cheap.
#[derive(Clone)]
pub struct App(Rc<Inner>);

/// A handle that does not keep the application alive.
pub struct WeakApp(std::rc::Weak<Inner>);

impl WeakApp {
    pub fn upgrade(&self) -> Option<App> {
        self.0.upgrade().map(App)
    }
}

impl App {
    fn start(gtk_app: &gtk::Application, profile: &str) -> Result<Self, String> {
        let settings = Settings::load(profile).map_err(|e| e.to_string())?;
        let dir_settings = DirectoryStore::load(&settings.dir_settings_path())
            .map_err(|e| log::warn!("Per-folder settings are unavailable: {e}"))
            .ok();
        let bookmarks = bookmarks::Store::open(&glib::user_config_dir())
            .map_err(|e| log::warn!("Bookmarks are unavailable: {e}"))
            .ok();
        let volumes = volumes::Model::new(volumes::Filters {
            places_unmounted: settings.places_unmounted(),
            show_internal_volumes: settings.show_internal_volumes(),
        });
        let app = Self(Rc::new(Inner {
            gtk: gtk_app.clone(),
            settings,
            bookmarks,
            bookmarks_menu: gio::Menu::new(),
            inventory: RefCell::new(volumes.inventory()),
            volumes,
            automounted: RefCell::default(),
            places_changed: Listeners::default(),
            thumbs: crate::view::thumbs::Thumbs::new(),
            dir_settings,
            windows: RefCell::default(),
            daemon: RefCell::default(),
            seen_first_invocation: Cell::new(false),
            smoke: std::env::var_os("TREK_SMOKE").is_some_and(|v| v == "1"),
            exit_code: Cell::new(0),
            keymap: trek_core::keymap::Keymap::yazi(),
            accels_suspended: Cell::new(false),
        }));
        gtk::Window::set_default_icon_name(crate::dialogs::help::ICON);
        actions::install_accels(gtk_app, true);
        gtk_app.set_menubar(Some(&menubar::build(&app.0.bookmarks_menu)));
        app.load_css();
        app.watch_places();
        crate::filemanager1::start(&app);
        for signal in [SIGHUP, SIGINT, SIGTERM] {
            let app = app.clone();
            glib_unix::unix_signal_add_local(signal, move || {
                app.quit();
                glib::ControlFlow::Break
            });
        }
        Ok(app)
    }

    fn load_css(&self) {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        crate::theme::follow_desktop(&display);
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("../../../data/style.css"));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    /// Follows the bookmarks file and the volume monitor for as long as the
    /// application lives.
    fn watch_places(&self) {
        crate::bookmarks::fill_menu(self, &self.0.bookmarks_menu);
        if let Some(store) = &self.0.bookmarks {
            let changes = store.changes.clone();
            let weak = Rc::downgrade(&self.0);
            glib::spawn_future_local(async move {
                while changes.recv().await.is_ok() {
                    let Some(app) = weak.upgrade().map(App) else {
                        break;
                    };
                    app.bookmarks_changed();
                }
            });
        }
        let changes = self.0.volumes.changes.clone();
        let weak = Rc::downgrade(&self.0);
        glib::spawn_future_local(async move {
            while changes.recv().await.is_ok() {
                let Some(app) = weak.upgrade().map(App) else {
                    break;
                };
                let now = app.0.volumes.inventory();
                let before = app.0.inventory.replace(now.clone());
                crate::volumes::changed(&app, &before, &now);
                app.0.places_changed.emit();
            }
        });
        // Places visibility is a filter inside the model.
        let weak = Rc::downgrade(&self.0);
        self.0.settings.connect_changed_any(
            &[
                crate::settings::Key::PlacesUnmounted,
                crate::settings::Key::ShowInternalVolumes,
            ],
            move || {
                if let Some(app) = weak.upgrade().map(App) {
                    app.0.volumes.set_filters(crate::volumes::filters(&app));
                    app.0.places_changed.emit();
                }
            },
        );
        crate::volumes::startup(self);
    }

    /// The key bindings of the file view.
    pub fn keymap(&self) -> &trek_core::keymap::Keymap {
        &self.0.keymap
    }

    /// Switches the window accelerators off while a text entry has the
    /// keyboard, and back on afterwards.
    pub fn suspend_accels(&self, suspended: bool) {
        if self.0.accels_suspended.replace(suspended) != suspended {
            actions::install_accels(&self.0.gtk, !suspended);
        }
    }

    /// The bookmark store, when the bookmarks file could be opened.
    pub fn bookmarks(&self) -> Option<&bookmarks::Store> {
        self.0.bookmarks.as_ref()
    }

    /// Called after any bookmark edit, ours or another program's.
    pub fn bookmarks_changed(&self) {
        crate::bookmarks::fill_menu(self, &self.0.bookmarks_menu);
        self.0.places_changed.emit();
    }

    pub fn thumbs(&self) -> Option<Rc<crate::view::thumbs::Thumbs>> {
        self.0.thumbs.clone()
    }

    pub fn volumes(&self) -> &volumes::Model {
        &self.0.volumes
    }

    /// Emitted when bookmarks or volumes changed.
    pub fn places_changed(&self) -> &Listeners {
        &self.0.places_changed
    }

    pub fn note_automount(&self, id: &str) {
        self.0.automounted.borrow_mut().insert(id.to_owned());
    }

    pub fn take_automount(&self, id: &str) -> bool {
        self.0.automounted.borrow_mut().remove(id)
    }

    /// A mount went away: tabs inside it close or go Home (§8.17).
    pub fn mount_gone(&self, root: &gio::File) {
        let windows = self.0.windows.borrow().clone();
        for window in windows {
            window.mount_gone(root);
        }
    }

    pub fn gtk(&self) -> &gtk::Application {
        &self.0.gtk
    }

    pub fn settings(&self) -> &Settings {
        &self.0.settings
    }

    /// The per-folder settings store.
    pub fn dir_settings(&self) -> Option<&DirectoryStore> {
        self.0.dir_settings.as_ref()
    }

    /// Whether this is a `TREK_SMOKE=1` run.
    pub fn smoke(&self) -> bool {
        self.0.smoke
    }

    /// The Home folder as a location.
    pub fn home_location(&self) -> Option<Location> {
        Location::from_path(&self.0.settings.home()).ok()
    }

    /// Saves settings and ends the main loop.
    pub fn quit(&self) {
        self.0.settings.save_now();
        self.0.gtk.quit();
    }

    /// Ends a smoke run with `code`.
    pub fn finish_smoke(&self, code: u8) {
        self.0.exit_code.set(code);
        self.quit();
    }

    /// Opens a new window on `location`.
    pub fn new_window(&self, location: Location) -> MainWindow {
        let window = MainWindow::new(self, location);
        self.0.windows.borrow_mut().push(window.clone());
        window.present();
        window
    }

    /// Forgets a closed window, releasing its tabs and folders.
    pub fn forget_window(&self, window: &gtk::ApplicationWindow) {
        self.0.windows.borrow_mut().retain(|w| w.widget() != window);
        // The leak check (SCOPE 9.4): with tracing on, report what is still
        // alive once the window's widgets have been released.
        if debug::tracing() {
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), || {
                debug::note(&format!("after a window closed: {}", debug::live()));
            });
        }
    }

    /// The window that last had focus.
    pub fn active_window(&self) -> Option<MainWindow> {
        let active = self.0.gtk.active_window();
        let windows = self.0.windows.borrow();
        windows
            .iter()
            .find(|w| active.as_ref() == Some(w.widget().upcast_ref()))
            .or_else(|| windows.last())
            .cloned()
    }

    /// A weak handle, for tasks that outlive a borrow of the application.
    pub fn downgrade(&self) -> WeakApp {
        WeakApp(Rc::downgrade(&self.0))
    }

    /// Shows a folder for another program (FileManager1): a tab in the
    /// active window or a new window, brought forward, with `select`, the
    /// URI of one of its files, selected once it is listed.
    pub fn show_folder(
        &self,
        location: Location,
        select: Option<&str>,
        startup_id: &str,
    ) -> MainWindow {
        let window = match self.active_window() {
            Some(window) => {
                window.add_tab(location);
                window
            }
            None => self.new_window(location),
        };
        if let Some(uri) = select {
            window.select_when_listed(uri);
        }
        if !startup_id.is_empty() {
            window.widget().set_startup_id(startup_id);
        }
        window.present();
        window
    }

    /// Opens a folder as a tab in the active window, or in a new window.
    pub fn open_folder(&self, location: Location, new_window: bool) {
        match self.active_window().filter(|_| !new_window) {
            Some(window) => {
                window.add_tab(location);
                window.present();
            }
            None => {
                self.new_window(location);
            }
        }
    }

    fn command_line(&self, command_line: &gio::ApplicationCommandLine) -> glib::ExitCode {
        let first = !self.0.seen_first_invocation.replace(true);
        let home = self.0.settings.home();
        let cwd = command_line.cwd().unwrap_or_else(|| home.clone());
        let arguments = command_line.arguments();
        let arguments = arguments.get(1..).unwrap_or_default();
        match invocation::parse_os(arguments, &cwd, &home, first) {
            Ok(invocation) => {
                if invocation.daemon && first {
                    self.0.daemon.replace(Some(self.0.gtk.hold()));
                }
                let mut requests = invocation.requests;
                let mut status = 0;
                if requests.contains(&invocation::Request::Import) {
                    requests.retain(|request| *request != invocation::Request::Import);
                    status = cli::import(self, command_line);
                    // GLib resets a first instance's status to zero as soon
                    // as its main loop turns once, so it is kept here too.
                    if first {
                        self.0.exit_code.set(status);
                    }
                }
                if !requests.is_empty() {
                    cli::execute(self, requests);
                }
                glib::ExitCode::from(status)
            }
            Err(e) => {
                command_line.printerr_literal(&format!("trek: {}\n", e.message));
                glib::ExitCode::from(e.exit_code.clamp(1, 255) as u8)
            }
        }
    }
}

/// Opens an empty window and exits after its first frame: the D6 baseline.
fn baseline() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.github.codedsleep.Trek.baseline")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_default_size(640, 480);
        let app = app.clone();
        window.connect_map(move |window| {
            let app = app.clone();
            debug::after_next_paint(window, move || {
                debug::trace("first frame");
                app.quit();
            });
        });
        window.present();
    });
    app.run_with_args::<&str>(&[])
}

/// Entry point after logging is set up.
pub fn run(args: &[OsString]) -> glib::ExitCode {
    if std::env::var_os("TREK_BASELINE").is_some_and(|v| v == "1") {
        return baseline();
    }
    if let Err(message) = cli::check_smoke_isolation() {
        eprintln!("trek: {message}");
        return glib::ExitCode::from(3);
    }
    let profile = match cli::prescan_profile(args) {
        Ok(profile) => profile,
        Err(message) => {
            eprintln!("trek: {message}");
            return glib::ExitCode::from(2);
        }
    };
    let gtk_app = gtk::Application::builder()
        .application_id(invocation::application_id(&profile))
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let state: Rc<OnceCell<App>> = Rc::default();
    gtk_app.connect_startup({
        let state = state.clone();
        move |gtk_app| match App::start(gtk_app, &profile) {
            Ok(app) => {
                let _ = state.set(app);
            }
            Err(message) => eprintln!("trek: {message}"),
        }
    });
    gtk_app.connect_command_line({
        let state = state.clone();
        move |_, command_line| match state.get() {
            Some(app) => app.command_line(command_line),
            None => glib::ExitCode::FAILURE,
        }
    });
    gtk_app.connect_shutdown({
        let state = state.clone();
        move |_| {
            if let Some(app) = state.get() {
                app.0.settings.save_now();
                // Break the app → window → tab → app reference cycle.
                app.0.windows.borrow_mut().clear();
            }
        }
    });
    let code = gtk_app.run();
    match state.get().map(|app| app.0.exit_code.get()) {
        Some(code) if code != 0 => glib::ExitCode::from(code),
        _ => code,
    }
}
