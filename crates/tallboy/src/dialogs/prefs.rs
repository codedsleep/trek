//! The Preferences dialog (PROMPT §8.25): five pages, every control bound to
//! `Settings` in both directions and applied at once. There is no OK or
//! Cancel: a change is in force as soon as it is made, as upstream.
//!
//! Controls for what v1 leaves out (templates, archivers, the Applications
//! place, the desktop) are not shown (SCOPE D4).

use crate::{
    app::App,
    settings::{HandlerId, Key, Settings, set},
};
use gtk::{glib, prelude::*};
use std::{cell::RefCell, rc::Rc};
use tallboy_core::config::{BookmarkOpen, DropAction, ViewMode};

/// Page numbers as `--show-pref=N` counts them.
pub const ADVANCED: u32 = 5;

/// Things to apply before the window goes away.
type Flush = Rc<RefCell<Vec<Box<dyn Fn()>>>>;

const ICON_SIZES: [u32; 9] = [16, 24, 32, 48, 64, 96, 128, 192, 256];

/// Builds one page and remembers what must be undone when the dialog closes.
struct Page {
    settings: Settings,
    handlers: Rc<RefCell<Vec<HandlerId>>>,
    /// Text fields apply when they lose the keyboard; closing applies them too.
    flush: Flush,
    root: gtk::Box,
    grid: Option<gtk::Grid>,
    row: i32,
}

impl Page {
    fn new(settings: &Settings, shared: &Shared) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("prefs-page");
        Self {
            settings: settings.clone(),
            handlers: shared.handlers.clone(),
            flush: shared.flush.clone(),
            root,
            grid: None,
            row: 0,
        }
    }

    fn heading(&mut self, text: &str) {
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.0);
        label.add_css_class("heading");
        self.grid = None;
        self.root.append(&label);
    }

    /// A control on a line of its own.
    fn line(&mut self, widget: &impl IsA<gtk::Widget>) {
        self.grid = None;
        self.root.append(widget);
    }

    /// A control with its name in front; consecutive ones line up.
    fn labelled(&mut self, text: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Label {
        let grid = match &self.grid {
            Some(grid) => grid.clone(),
            None => {
                let grid = gtk::Grid::new();
                grid.set_row_spacing(6);
                grid.set_column_spacing(12);
                self.root.append(&grid);
                self.grid = Some(grid.clone());
                self.row = 0;
                grid
            }
        };
        let label = gtk::Label::with_mnemonic(text);
        label.set_xalign(0.0);
        label.set_mnemonic_widget(Some(widget));
        widget.set_halign(gtk::Align::Start);
        grid.attach(&label, 0, self.row, 1, 1);
        grid.attach(widget, 1, self.row, 1, 1);
        self.row += 1;
        label
    }

    fn watch(&self, key: Key, update: impl Fn() + 'static) {
        self.handlers
            .borrow_mut()
            .push(self.settings.connect_changed(key, update));
    }

    fn check(
        &mut self,
        text: &str,
        key: Key,
        get: fn(&Settings) -> bool,
        set: fn(&Settings, bool),
    ) -> gtk::CheckButton {
        let button = gtk::CheckButton::with_mnemonic(text);
        button.set_active(get(&self.settings));
        let settings = self.settings.clone();
        button.connect_toggled(move |button| set(&settings, button.is_active()));
        // The setter reports nothing when the value is already this one, so
        // the two directions cannot feed each other.
        let (weak, settings) = (button.downgrade(), self.settings.clone());
        self.watch(key, move || {
            if let Some(button) = weak.upgrade() {
                button.set_active(get(&settings));
            }
        });
        self.line(&button);
        button
    }

    fn choice<T: Clone + PartialEq + 'static>(
        &mut self,
        text: &str,
        key: Key,
        get: fn(&Settings) -> T,
        set: fn(&Settings, T),
        options: Vec<(String, T)>,
    ) -> gtk::DropDown {
        let names: Vec<&str> = options.iter().map(|(name, _)| name.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&names);
        let values: Rc<Vec<T>> = Rc::new(options.iter().map(|(_, value)| value.clone()).collect());
        let position = |values: &[T], current: &T| {
            values
                .iter()
                .position(|value| value == current)
                .map_or(gtk::INVALID_LIST_POSITION, |index| index as u32)
        };
        dropdown.set_selected(position(&values, &get(&self.settings)));
        let (settings, for_set) = (self.settings.clone(), values.clone());
        dropdown.connect_selected_notify(move |dropdown| {
            if let Some(value) = for_set.get(dropdown.selected() as usize) {
                set(&settings, value.clone());
            }
        });
        let (weak, settings) = (dropdown.downgrade(), self.settings.clone());
        self.watch(key, move || {
            if let Some(dropdown) = weak.upgrade() {
                dropdown.set_selected(position(&values, &get(&settings)));
            }
        });
        self.labelled(text, &dropdown);
        dropdown
    }

    /// A number; `per_unit` is how many stored units one shown unit is.
    #[allow(clippy::too_many_arguments)]
    fn number(
        &mut self,
        text: &str,
        key: Key,
        get: fn(&Settings) -> u32,
        set: fn(&Settings, u32),
        range: (f64, f64, f64),
        digits: u32,
        per_unit: f64,
    ) -> gtk::SpinButton {
        let spin = gtk::SpinButton::with_range(range.0, range.1, range.2);
        spin.set_digits(digits);
        spin.set_value(f64::from(get(&self.settings)) / per_unit);
        let settings = self.settings.clone();
        spin.connect_value_changed(move |spin| {
            set(&settings, (spin.value() * per_unit).round().max(0.0) as u32);
        });
        let (weak, settings) = (spin.downgrade(), self.settings.clone());
        self.watch(key, move || {
            if let Some(spin) = weak.upgrade() {
                spin.set_value(f64::from(get(&settings)) / per_unit);
            }
        });
        self.labelled(text, &spin);
        spin
    }

    fn text(
        &mut self,
        text: &str,
        key: Key,
        get: fn(&Settings) -> String,
        set: fn(&Settings, String),
        placeholder: &str,
    ) -> gtk::Entry {
        let entry = gtk::Entry::new();
        entry.set_width_chars(32);
        entry.set_placeholder_text(Some(placeholder));
        entry.set_text(&get(&self.settings));
        let apply = {
            let (weak, settings) = (entry.downgrade(), self.settings.clone());
            move || {
                if let Some(entry) = weak.upgrade() {
                    set(&settings, entry.text().trim().to_owned());
                }
            }
        };
        let on_enter = apply.clone();
        entry.connect_activate(move |_| on_enter());
        let focus = gtk::EventControllerFocus::new();
        let on_leave = apply.clone();
        focus.connect_leave(move |_| on_leave());
        entry.add_controller(focus);
        self.flush.borrow_mut().push(Box::new(apply));
        let (weak, settings) = (entry.downgrade(), self.settings.clone());
        self.watch(key, move || {
            if let Some(entry) = weak.upgrade() {
                let value = get(&settings);
                if entry.text().trim() != value {
                    entry.set_text(&value);
                }
            }
        });
        self.labelled(text, &entry);
        entry
    }

    /// `dependent` is only usable while `master` is ticked.
    fn follows(&self, dependent: &impl IsA<gtk::Widget>, master: &gtk::CheckButton) {
        master
            .bind_property("active", dependent, "sensitive")
            .sync_create()
            .build();
    }
}

#[derive(Default)]
struct Shared {
    handlers: Rc<RefCell<Vec<HandlerId>>>,
    flush: Flush,
}

fn sizes(from: u32, to: u32) -> Vec<(String, u32)> {
    ICON_SIZES
        .iter()
        .filter(|size| (from..=to).contains(*size))
        .map(|size| (format!("{size} × {size}"), *size))
        .collect()
}

fn named<T: Clone>(options: &[(&str, T)]) -> Vec<(String, T)> {
    options
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn general(settings: &Settings, shared: &Shared) -> gtk::Box {
    let mut page = Page::new(settings, shared);
    page.heading("Behavior");
    let single = page.check(
        "_Open files with single click",
        Key::SingleClick,
        Settings::single_click,
        set::single_click,
    );
    let delay = page.number(
        "Delay of automatic _selection in single click mode (seconds; 0 to disable):",
        Key::AutoSelectionDelay,
        Settings::auto_selection_delay,
        set::auto_selection_delay,
        (0.0, 5.0, 0.1),
        1,
        1000.0,
    );
    page.follows(&delay, &single);
    let trash = page.check(
        "Move deleted files to \"_trash can\" instead of erasing from disk",
        Key::UseTrash,
        Settings::use_trash,
        set::use_trash,
    );
    let usb = page.check(
        "_Erase files on removable media instead of \"trash can\" creation",
        Key::NoUsbTrash,
        Settings::no_usb_trash,
        set::no_usb_trash,
    );
    page.follows(&usb, &trash);
    let confirm = page.check(
        "Confirm before _moving files into \"trash can\"",
        Key::ConfirmTrash,
        Settings::confirm_trash,
        set::confirm_trash,
    );
    page.follows(&confirm, &trash);
    page.check(
        "Con_firm before deleting files",
        Key::ConfirmDel,
        Settings::confirm_del,
        set::confirm_del,
    );
    page.check(
        "Don't ask options on launch e_xecutable file",
        Key::QuickExec,
        Settings::quick_exec,
        set::quick_exec,
    );
    page.choice(
        "Default _drag and drop action:",
        Key::DropDefaultAction,
        Settings::drop_default_action,
        set::drop_default_action,
        named(&[
            ("Auto", DropAction::Auto),
            ("Copy", DropAction::Copy),
            ("Move", DropAction::Move),
            ("Ask", DropAction::Ask),
        ]),
    );
    page.check(
        "Use SI decimal prefixes instead of _IEC binary prefixes",
        Key::SiUnit,
        Settings::si_unit,
        set::si_unit,
    );
    page.check(
        "Treat _backup files as hidden",
        Key::BackupAsHidden,
        Settings::backup_as_hidden,
        set::backup_as_hidden,
    );
    page.check(
        "Always show full file _names",
        Key::ShowFullNames,
        Settings::show_full_names,
        set::show_full_names,
    );
    page.heading("Default View");
    page.choice(
        "Default _view mode:",
        Key::ViewMode,
        Settings::view_mode,
        set::view_mode,
        named(&[
            ("Line View", ViewMode::Lines),
            ("Icon View", ViewMode::Icon),
            ("Thumbnail View", ViewMode::Thumbnail),
            ("Compact View", ViewMode::Compact),
            ("Detailed List View", ViewMode::List),
        ]),
    );
    page.choice(
        "Bookmarks o_pen in:",
        Key::BmOpenMethod,
        Settings::bm_open_method,
        set::bm_open_method,
        named(&[
            ("Current tab", BookmarkOpen::Current),
            ("New tab", BookmarkOpen::Tab),
            ("New window", BookmarkOpen::Window),
        ]),
    );
    page.root
}

fn display(settings: &Settings, shared: &Shared) -> gtk::Box {
    let mut page = Page::new(settings, shared);
    page.heading("Icons");
    page.choice(
        "Size of _big icons:",
        Key::BigIconSize,
        Settings::big_icon_size,
        set::big_icon_size,
        sizes(24, 256),
    );
    page.choice(
        "Size of _small icons:",
        Key::SmallIconSize,
        Settings::small_icon_size,
        set::small_icon_size,
        sizes(16, 48),
    );
    page.choice(
        "Size of _thumbnails:",
        Key::ThumbnailSize,
        Settings::thumbnail_size,
        set::thumbnail_size,
        sizes(64, 256),
    );
    page.choice(
        "Size of side _pane icons:",
        Key::PaneIconSize,
        Settings::pane_icon_size,
        set::pane_icon_size,
        sizes(16, 48),
    );
    page.heading("Thumbnails");
    let show = page.check(
        "S_how thumbnails of files",
        Key::ShowThumbnail,
        Settings::show_thumbnail,
        set::show_thumbnail,
    );
    let local = page.check(
        "_Only show thumbnails for local files",
        Key::ThumbnailLocal,
        Settings::thumbnail_local,
        set::thumbnail_local,
    );
    page.follows(&local, &show);
    let max = page.number(
        "Do not generate thumbnails for image files e_xceeding this size (KB):",
        Key::ThumbnailMax,
        Settings::thumbnail_max,
        set::thumbnail_max,
        (0.0, 1_048_576.0, 512.0),
        0,
        1.0,
    );
    page.follows(&max, &show);
    page.heading("User Interface");
    page.check(
        "_Display hidden files' icons shadowed",
        Key::ShadowHidden,
        Settings::shadow_hidden,
        set::shadow_hidden,
    );
    page.check(
        "Hide 'Close tab' b_uttons",
        Key::HideCloseBtn,
        Settings::hide_close_btn,
        set::hide_close_btn,
    );
    page.number(
        "Maximum tab _label width (characters):",
        Key::MaxTabChars,
        Settings::max_tab_chars,
        set::max_tab_chars,
        (4.0, 256.0, 1.0),
        0,
        1.0,
    );
    page.root
}

fn layout(settings: &Settings, shared: &Shared) -> gtk::Box {
    let mut page = Page::new(settings, shared);
    page.heading("Tabs");
    page.check(
        "Always show the _tab bar",
        Key::AlwaysShowTabs,
        Settings::always_show_tabs,
        set::always_show_tabs,
    );
    page.check(
        "_Activate previous tab on close",
        Key::FocusPrevious,
        Settings::focus_previous,
        set::focus_previous,
    );
    page.heading("Show in Places");
    for (text, key, get, set) in [
        (
            "_Home",
            Key::PlacesHome,
            Settings::places_home as fn(&Settings) -> bool,
            set::places_home as fn(&Settings, bool),
        ),
        (
            "_Desktop",
            Key::PlacesDesktop,
            Settings::places_desktop,
            set::places_desktop,
        ),
        (
            "T_rash can",
            Key::PlacesTrash,
            Settings::places_trash,
            set::places_trash,
        ),
        (
            "_Filesystem root",
            Key::PlacesRoot,
            Settings::places_root,
            set::places_root,
        ),
        (
            "De_vices",
            Key::PlacesComputer,
            Settings::places_computer,
            set::places_computer,
        ),
        (
            "_Network",
            Key::PlacesNetwork,
            Settings::places_network,
            set::places_network,
        ),
        (
            "_Unmounted volumes",
            Key::PlacesUnmounted,
            Settings::places_unmounted,
            set::places_unmounted,
        ),
        (
            "_Internal volumes",
            Key::ShowInternalVolumes,
            Settings::show_internal_volumes,
            set::show_internal_volumes,
        ),
    ] {
        page.check(text, key, get, set);
    }
    page.heading("Window");
    page.check(
        "No e_xpander for folders without children in the directory tree",
        Key::NoChildNonExpandable,
        Settings::no_child_non_expandable,
        set::no_child_non_expandable,
    );
    page.choice(
        "_Path bar mode:",
        Key::PathbarModeButtons,
        Settings::pathbar_mode_buttons,
        set::pathbar_mode_buttons,
        named(&[("Location", false), ("Buttons", true)]),
    );
    page.check(
        "Show _status bar",
        Key::ShowStatusbar,
        Settings::show_statusbar,
        set::show_statusbar,
    );
    page.root
}

fn volumes(settings: &Settings, shared: &Shared) -> gtk::Box {
    let mut page = Page::new(settings, shared);
    page.heading("Auto-mount");
    page.check(
        "_Mount mountable volumes automatically on program startup",
        Key::MountOnStartup,
        Settings::mount_on_startup,
        set::mount_on_startup,
    );
    page.check(
        "Mount _removable media automatically when they are inserted",
        Key::MountRemovable,
        Settings::mount_removable,
        set::mount_removable,
    );
    page.check(
        "_Show available options for removable media when they are inserted",
        Key::Autorun,
        Settings::autorun,
        set::autorun,
    );
    page.heading("Auto-unmount");
    page.check(
        "Close _tab containing removable medium when it is unmounted",
        Key::CloseOnUnmount,
        Settings::close_on_unmount,
        set::close_on_unmount,
    );
    page.check(
        "_Open removable media in a new tab",
        Key::MediaInNewTab,
        Settings::media_in_new_tab,
        set::media_in_new_tab,
    );
    page.heading("Remembered choices");
    let forget = gtk::Button::with_mnemonic("_Forget remembered autorun choices");
    forget.set_halign(gtk::Align::Start);
    forget.set_sensitive(!settings.autorun_choices().is_empty());
    let for_click = settings.clone();
    forget.connect_clicked(move |_| set::autorun_choices(&for_click, Default::default()));
    let (weak, for_watch) = (forget.downgrade(), settings.clone());
    page.watch(Key::AutorunChoices, move || {
        if let Some(forget) = weak.upgrade() {
            forget.set_sensitive(!for_watch.autorun_choices().is_empty());
        }
    });
    page.line(&forget);
    page.root
}

fn advanced(app: &App, shared: &Shared) -> gtk::Box {
    let settings = app.settings();
    let mut page = Page::new(settings, shared);
    page.heading("Programs");
    // What is used while the field is empty: $TERMINAL or the first one
    // installed.
    let fallback = {
        let unset = Settings::terminal(settings).is_empty();
        crate::open::resolve_terminal(app)
            .ok()
            .filter(|_| unset)
            .map_or_else(
                || "for example: foot".to_owned(),
                |terminal| {
                    let program = terminal.argv.first().cloned().unwrap_or_default();
                    format!("automatic: {}", program.to_string_lossy())
                },
            )
    };
    page.text(
        "_Terminal emulator:",
        Key::Terminal,
        Settings::terminal,
        set::terminal,
        &fallback,
    );
    page.heading("Miscellaneous");
    page.check(
        "Show _special permission bits in Properties",
        Key::AdvancedMode,
        Settings::advanced_mode,
        set::advanced_mode,
    );
    page.text(
        "_Home folder override:",
        Key::HomePath,
        Settings::home_path,
        set::home_path,
        &glib::home_dir().to_string_lossy(),
    );
    page.root
}

thread_local! {
    /// The one Preferences window, while it is open.
    static OPEN: RefCell<Option<(glib::WeakRef<gtk::Window>, glib::WeakRef<gtk::Notebook>)>> =
        const { RefCell::new(None) };
}

/// Shows Preferences at `page` (1-based, as `--show-pref=N`), bringing the
/// existing window forward if there is one.
pub fn show(app: &App, parent: Option<&gtk::Window>, page: u32) {
    let index = page.clamp(1, ADVANCED) - 1;
    let existing = OPEN.with_borrow(|open| {
        open.as_ref()
            .and_then(|(window, notebook)| Some((window.upgrade()?, notebook.upgrade()?)))
    });
    if let Some((window, notebook)) = existing {
        notebook.set_current_page(Some(index));
        window.present();
        return;
    }

    let settings = app.settings();
    let shared = Shared::default();
    let notebook = gtk::Notebook::new();
    notebook.set_tab_pos(gtk::PositionType::Left);
    for (title, content) in [
        ("General", general(settings, &shared)),
        ("Display", display(settings, &shared)),
        ("Layout", layout(settings, &shared)),
        ("Volume Management", volumes(settings, &shared)),
        ("Advanced", advanced(app, &shared)),
    ] {
        let tab = gtk::Label::new(Some(title));
        tab.set_xalign(0.0);
        notebook.append_page(&content, Some(&tab));
    }
    notebook.set_current_page(Some(index));

    let window = gtk::Window::builder()
        .title("Preferences")
        .resizable(false)
        .build();
    window.set_transient_for(parent);
    // Keeps the process alive for `--show-pref` with no main window.
    app.gtk().add_window(&window);
    let close = gtk::Button::with_mnemonic("_Close");
    close.set_halign(gtk::Align::End);
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 16);
    outer.add_css_class("dialog-body");
    outer.append(&notebook);
    outer.append(&close);
    window.set_child(Some(&outer));

    let keys = gtk::EventControllerKey::new();
    let weak = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape
            && let Some(window) = weak.upgrade()
        {
            window.close();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);

    let settings = settings.clone();
    window.connect_close_request(move |_| {
        for apply in shared.flush.borrow().iter() {
            apply();
        }
        settings.disconnect(&shared.handlers.take());
        shared.flush.borrow_mut().clear();
        OPEN.set(None);
        glib::Propagation::Proceed
    });
    OPEN.set(Some((window.downgrade(), notebook.downgrade())));
    window.present();
}
