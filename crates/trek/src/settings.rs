//! The one settings handle the UI uses: typed getters, typed setters, change
//! notification per key and a debounced save (PROMPT §7.6).

use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};
use trek_core::config::{BookmarkOpen, Columns, Config, DropAction, SidePane, Toolbar, ViewMode};
use trek_core::sort::SortSpec;
use trek_fs::settings_store::{SettingsError, Store};

type Handler = Rc<dyn Fn()>;

struct Inner {
    store: RefCell<Store>,
    handlers: RefCell<Vec<(u64, Key, Handler)>>,
    next_id: Cell<u64>,
    dirty: Cell<bool>,
    pending_save: RefCell<Option<glib::SourceId>>,
}

/// Shared settings handle. Cloning is cheap; every clone sees the same values.
#[derive(Clone)]
pub struct Settings(Rc<Inner>);

/// Identifies a change handler so that its owner can disconnect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerId(u64);

macro_rules! keys {
    ($($variant:ident $field:ident: $ty:ty,)*) => {
        /// One variant per `trek.conf` key that the UI reads or writes.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(dead_code)]
        pub enum Key { $($variant,)* }

        #[allow(dead_code)]
        impl Settings {
            $(
                /// Current value of this key.
                pub fn $field(&self) -> $ty {
                    self.0.store.borrow().config.$field.clone()
                }
            )*
        }

        /// Typed setters, one per key. A setter that does not change the
        /// value does nothing; otherwise it notifies and schedules a save.
        #[allow(dead_code)]
        pub mod set {
            use super::*;
            $(
                /// Sets this key.
                pub fn $field(settings: &Settings, value: $ty) {
                    settings.change(Key::$variant, |c| {
                        if c.$field == value {
                            false
                        } else {
                            c.$field = value;
                            true
                        }
                    });
                }
            )*
        }
    };
}

keys! {
    SingleClick single_click: bool,
    UseTrash use_trash: bool,
    ConfirmDel confirm_del: bool,
    ConfirmTrash confirm_trash: bool,
    NoUsbTrash no_usb_trash: bool,
    QuickExec quick_exec: bool,
    SiUnit si_unit: bool,
    BackupAsHidden backup_as_hidden: bool,
    ShowFullNames show_full_names: bool,
    AdvancedMode advanced_mode: bool,
    DeferContentTest defer_content_test: bool,
    NoChildNonExpandable no_child_non_expandable: bool,
    ShowInternalVolumes show_internal_volumes: bool,
    ThumbnailLocal thumbnail_local: bool,
    MountOnStartup mount_on_startup: bool,
    MountRemovable mount_removable: bool,
    AlwaysShowTabs always_show_tabs: bool,
    HideCloseBtn hide_close_btn: bool,
    Maximized maximized: bool,
    ChangeTabOnDrop change_tab_on_drop: bool,
    CloseOnUnmount close_on_unmount: bool,
    FocusPrevious focus_previous: bool,
    ShowHidden show_hidden: bool,
    ShowStatusbar show_statusbar: bool,
    PathbarModeButtons pathbar_mode_buttons: bool,
    DualPane dual_pane: bool,
    ShowMenubar show_menubar: bool,
    MillerParent miller_parent: bool,
    MillerPreview miller_preview: bool,
    ShowThumbnail show_thumbnail: bool,
    ShadowHidden shadow_hidden: bool,
    PlacesHome places_home: bool,
    PlacesDesktop places_desktop: bool,
    PlacesRoot places_root: bool,
    PlacesComputer places_computer: bool,
    PlacesTrash places_trash: bool,
    PlacesNetwork places_network: bool,
    PlacesUnmounted places_unmounted: bool,
    AutoSelectionDelay auto_selection_delay: u32,
    ThumbnailMax thumbnail_max: u32,
    MaxTabChars max_tab_chars: u32,
    WinWidth win_width: u32,
    WinHeight win_height: u32,
    SplitterPos splitter_pos: u32,
    BigIconSize big_icon_size: u32,
    SmallIconSize small_icon_size: u32,
    PaneIconSize pane_icon_size: u32,
    ThumbnailSize thumbnail_size: u32,
    HomePath home_path: String,
    Terminal terminal: String,
    BmOpenMethod bm_open_method: BookmarkOpen,
    DropDefaultAction drop_default_action: DropAction,
    SidePaneMode side_pane_mode: SidePane,
    ViewMode view_mode: ViewMode,
    Sort sort: SortSpec,
    Columns columns: Columns,
    Toolbar toolbar: Toolbar,
}

impl Settings {
    const SAVE_DELAY: Duration = Duration::from_secs(1);

    /// Loads the profile's settings through the §9 load chain.
    pub fn load(profile: &str) -> Result<Self, SettingsError> {
        Ok(Self(Rc::new(Inner {
            store: RefCell::new(Store::from_environment(profile)?),
            handlers: RefCell::default(),
            next_id: Cell::new(1),
            dirty: Cell::new(false),
            pending_save: RefCell::default(),
        })))
    }

    /// A snapshot of every value, for backend calls that take a `Config`.
    pub fn config(&self) -> Config {
        self.0.store.borrow().config.clone()
    }

    /// Path of the per-folder settings file that sits beside `trek.conf`.
    pub fn dir_settings_path(&self) -> PathBuf {
        self.0
            .store
            .borrow()
            .path()
            .with_file_name("dir-settings.conf")
    }

    /// Home folder: the `home_path` override when set, otherwise `$HOME`.
    pub fn home(&self) -> PathBuf {
        let configured = self.home_path();
        if configured.is_empty() {
            glib::home_dir()
        } else {
            PathBuf::from(configured)
        }
    }

    /// The profile these settings belong to: the folder `trek.conf` is in.
    pub fn profile(&self) -> String {
        self.0
            .store
            .borrow()
            .path()
            .parent()
            .and_then(|folder| folder.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Applies `(group, key, value)` triples read by the importer on top of
    /// the current values, saves at once and tells every observer. Returns
    /// what the value codecs had to say.
    pub fn import(&self, keys: &[(String, String, String)]) -> Vec<String> {
        let mut warnings = Vec::new();
        {
            let config = &mut self.0.store.borrow_mut().config;
            for (group, key, value) in keys {
                config.apply(group, key, value, &mut warnings);
            }
        }
        self.0.dirty.set(true);
        self.save_now();
        let handlers: Vec<Handler> = self
            .0
            .handlers
            .borrow()
            .iter()
            .map(|(_, _, handler)| handler.clone())
            .collect();
        for handler in handlers {
            handler();
        }
        warnings
    }

    /// Calls `handler` after `key` changes. The handler must not hold a
    /// strong reference to whatever owns the returned id.
    pub fn connect_changed(&self, key: Key, handler: impl Fn() + 'static) -> HandlerId {
        let id = self.0.next_id.get();
        self.0.next_id.set(id + 1);
        self.0
            .handlers
            .borrow_mut()
            .push((id, key, Rc::new(handler)));
        HandlerId(id)
    }

    /// Connects one handler to several keys.
    pub fn connect_changed_any(
        &self,
        keys: &[Key],
        handler: impl Fn() + 'static,
    ) -> Vec<HandlerId> {
        let handler: Handler = Rc::new(handler);
        keys.iter()
            .map(|key| {
                let handler = handler.clone();
                self.connect_changed(*key, move || handler())
            })
            .collect()
    }

    /// Removes handlers registered with `connect_changed`.
    pub fn disconnect(&self, ids: &[HandlerId]) {
        self.0
            .handlers
            .borrow_mut()
            .retain(|(id, _, _)| !ids.contains(&HandlerId(*id)));
    }

    fn change(&self, key: Key, apply: impl FnOnce(&mut Config) -> bool) {
        let changed = apply(&mut self.0.store.borrow_mut().config);
        if !changed {
            return;
        }
        self.mark_dirty();
        // Collected first: a handler may connect or disconnect others.
        let handlers: Vec<Handler> = self
            .0
            .handlers
            .borrow()
            .iter()
            .filter(|(_, k, _)| *k == key)
            .map(|(_, _, h)| h.clone())
            .collect();
        for handler in handlers {
            handler();
        }
    }

    fn mark_dirty(&self) {
        self.0.dirty.set(true);
        if self.0.pending_save.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let source = glib::timeout_add_local_once(Self::SAVE_DELAY, move || {
            if let Some(inner) = weak.upgrade() {
                inner.pending_save.replace(None);
                Settings(inner).save_now();
            }
        });
        self.0.pending_save.replace(Some(source));
    }

    /// Writes pending changes immediately. Called on shutdown and on signals.
    pub fn save_now(&self) {
        if let Some(source) = self.0.pending_save.take() {
            source.remove();
        }
        if !self.0.dirty.replace(false) {
            return;
        }
        if let Err(e) = self.0.store.borrow().save() {
            log::warn!("Could not save settings: {e}");
        }
    }
}
