//! The tab's view: building it for a mode, the settings that shape it, and
//! "Preserve This Folder's Settings" (PROMPT §8.6, §8.7).

use super::{Change, TabPage};
use crate::{
    debug,
    settings::{self, Key},
    view::{Style, View, dnd, grid, keys, list},
};
use gtk::{glib, prelude::*};
use std::time::Instant;
use tallboy_core::{
    config::{Columns, ViewMode},
    sort::SortSpec,
};
use tallboy_fs::dir_settings::FolderSettings;

/// A position in a view that survives rebuilding it in another mode.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// This item should be in view.
    Item(u32),
    /// The view was scrolled this far through its range, from 0 to 1.
    Fraction(f64),
}

impl TabPage {
    /// The tab's view mode.
    pub fn mode(&self) -> ViewMode {
        self.0.mode.get()
    }

    /// Switches the view mode. Model and selection are shared by every
    /// mode, so they survive the switch.
    pub fn set_mode(&self, mode: ViewMode) {
        if self.0.mode.get() != mode {
            // Read the place while the old mode's scroll axis still applies.
            let place = self.place();
            self.0.mode.set(mode);
            self.rebuild_view_at(place);
            self.grab_focus();
            self.persist();
        }
    }

    pub fn set_show_hidden(&self, show: bool) {
        self.0.model.set_show_hidden(show);
        self.persist();
    }

    /// Changes the sort order from the menu or a column header.
    pub fn set_sort(&self, sort: SortSpec) {
        self.apply_sort(sort, true);
    }

    fn apply_sort(&self, sort: SortSpec, move_header_arrow: bool) {
        if self.0.model.sort() == sort {
            return;
        }
        let started = Instant::now();
        self.0.model.set_sort(sort.clone());
        if move_header_arrow && let Some(View::List(list)) = self.0.view.borrow().as_ref() {
            list.show_sort(&sort);
        }
        self.persist();
        self.emit(Change::Status);
        if debug::tracing() {
            self.trace_resort(started);
        }
    }

    fn trace_resort(&self, started: Instant) {
        let weak = self.downgrade();
        let stalls = std::cell::Cell::new(Some(debug::StallWatch::start()));
        glib::idle_add_local(move || {
            let Some(tab) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if tab.0.model.sorted().pending() > 0 {
                return glib::ControlFlow::Continue;
            }
            tallboy_fs::trace::elapsed("re-sort", started);
            if let Some(stalls) = stalls.take() {
                debug::note(&format!(
                    "longest stall while re-sorting: {:.1} ms",
                    stalls.finish().as_secs_f64() * 1000.0
                ));
            }
            glib::ControlFlow::Break
        });
    }

    // ---- per-folder settings ----

    /// Whether this folder keeps its own view settings.
    pub fn preserves_settings(&self) -> bool {
        self.0.preserve.get()
    }

    /// Turns "Preserve This Folder's Settings" on or off. Turning it off
    /// deletes the folder's group; the view stays as it is.
    pub fn set_preserve(&self, on: bool) {
        let Some(store) = self.0.app.dir_settings() else {
            return;
        };
        self.0.preserve.set(on);
        let result = if on {
            store.set(&self.folder_group(), &self.folder_settings())
        } else {
            store.remove(&self.folder_group())
        };
        if let Err(e) = result {
            log::warn!("Folder settings: {e}");
        }
    }

    /// The `dir-settings.conf` group: a native path, or the URI.
    fn folder_group(&self) -> String {
        let location = self.0.location.borrow();
        location
            .path()
            .and_then(|path| path.to_str().map(String::from))
            .unwrap_or_else(|| location.to_uri().to_owned())
    }

    fn folder_settings(&self) -> FolderSettings {
        FolderSettings {
            view_mode: self.0.mode.get(),
            sort: self.0.model.sort(),
            show_hidden: self.0.model.show_hidden(),
            columns: self.0.columns.borrow().clone(),
        }
    }

    /// Applies the folder's saved view settings, or the global ones.
    pub(super) fn load_folder_settings(&self) {
        let app_settings = self.0.app.settings();
        let saved = self.0.app.dir_settings().and_then(|store| {
            store
                .get(&self.folder_group(), &app_settings.config())
                .unwrap_or_else(|e| {
                    log::warn!("Folder settings: {e}");
                    None
                })
        });
        self.0.preserve.set(saved.is_some());
        let wanted = saved.unwrap_or_else(|| FolderSettings::from(&app_settings.config()));
        self.0.model.set_sort(wanted.sort.clone());
        self.0.model.set_show_hidden(wanted.show_hidden);
        let columns_changed = self.0.columns.replace(wanted.columns.clone()) != wanted.columns;
        let mode_changed = self.0.mode.replace(wanted.view_mode) != wanted.view_mode;
        let has_view = self.0.view.borrow().is_some();
        if has_view && (mode_changed || (columns_changed && wanted.view_mode == ViewMode::List)) {
            // Another folder: there is no place to return to.
            self.rebuild_view_at(None);
        } else if let Some(View::List(list)) = self.0.view.borrow().as_ref() {
            list.show_sort(&wanted.sort);
        }
    }

    /// Saves the view settings: to this folder when it preserves its own,
    /// otherwise as the global defaults (§8.7).
    fn persist(&self) {
        let current = self.folder_settings();
        if self.0.preserve.get() {
            if let Some(store) = self.0.app.dir_settings()
                && let Err(e) = store.set(&self.folder_group(), &current)
            {
                log::warn!("Folder settings: {e}");
            }
            return;
        }
        let app_settings = self.0.app.settings();
        settings::set::view_mode(app_settings, current.view_mode);
        settings::set::sort(app_settings, current.sort);
        settings::set::show_hidden(app_settings, current.show_hidden);
        settings::set::columns(app_settings, current.columns);
    }

    // ---- building the view ----

    fn style(&self) -> Style {
        let settings = self.0.app.settings();
        let mode = self.0.mode.get();
        Style {
            mode,
            icon_size: match mode {
                ViewMode::Icon => settings.big_icon_size(),
                ViewMode::Thumbnail => settings.thumbnail_size(),
                ViewMode::Compact | ViewMode::List => settings.small_icon_size(),
            },
            full_names: settings.show_full_names(),
            shadow_hidden: settings.shadow_hidden(),
            single_click: settings.single_click(),
            hover_delay_ms: settings.auto_selection_delay(),
            si_unit: settings.si_unit(),
        }
    }

    /// The adjustment along the direction the view scrolls in.
    pub(super) fn scroll_adjustment(&self) -> gtk::Adjustment {
        if self.0.mode.get() == ViewMode::Compact {
            self.0.scroller.hadjustment()
        } else {
            self.0.scroller.vadjustment()
        }
    }

    /// Where the view is, so that a rebuilt view can go back there: the
    /// first selected item, or else how far down the view is scrolled.
    fn place(&self) -> Option<Place> {
        self.0.view.borrow().as_ref()?;
        if self.0.model.visible_count() == 0 {
            return None;
        }
        let selection = self.0.model.selection().selection();
        if !selection.is_empty() {
            return Some(Place::Item(selection.minimum()));
        }
        let adjustment = self.scroll_adjustment();
        let range = adjustment.upper() - adjustment.page_size();
        if range <= 0.0 || adjustment.value() <= 0.0 {
            return None;
        }
        Some(Place::Fraction(
            (adjustment.value() / range).clamp(0.0, 1.0),
        ))
    }

    /// Builds the view for the current mode and settings, keeping the place.
    pub(super) fn rebuild_view(&self) {
        self.rebuild_view_at(self.place());
    }

    fn rebuild_view_at(&self, place: Option<Place>) {
        let inner = &self.0;
        if let Some((adjustment, handler)) = inner.view_fit.take() {
            adjustment.disconnect(handler);
        }
        let style = self.style();
        let weak = self.downgrade();
        let activate = move |position: u32| {
            if let Some(tab) = weak.upgrade() {
                tab.activate(position);
            }
        };
        let (view, policy) = match style.mode {
            ViewMode::List => {
                let weak = self.downgrade();
                let list = list::ListView::new(
                    &inner.model,
                    style,
                    &inner.columns.borrow(),
                    self.drop_hooks(),
                    move |event| {
                        if let Some(tab) = weak.upgrade() {
                            tab.list_event(event);
                        }
                    },
                );
                list.widget()
                    .connect_activate(move |_, position| activate(position));
                list.show_sort(&inner.model.sort());
                (
                    View::List(list),
                    (gtk::PolicyType::Automatic, gtk::PolicyType::Automatic),
                )
            }
            ViewMode::Icon | ViewMode::Thumbnail | ViewMode::Compact => {
                let compact = style.mode == ViewMode::Compact;
                let across = if compact {
                    inner.scroller.vadjustment()
                } else {
                    inner.scroller.hadjustment()
                };
                let (grid, fit) = grid::build(&inner.model, style, &across, self.drop_hooks());
                inner.view_fit.replace(Some((across, fit)));
                grid.connect_activate(move |_, position| activate(position));
                if compact {
                    dnd::wheel_scrolls_sideways(&grid, &inner.scroller.hadjustment());
                }
                let policy = if compact {
                    (gtk::PolicyType::Automatic, gtk::PolicyType::Never)
                } else {
                    (gtk::PolicyType::Never, gtk::PolicyType::Automatic)
                };
                (View::Grid(grid), policy)
            }
        };
        let widget = view.widget();
        keys::install(&widget);
        self.watch_type_ahead_keys(&widget);
        inner.scroller.set_policy(policy.0, policy.1);
        inner.scroller.set_child(Some(&widget));
        inner.view.replace(Some(view));

        if let Some(place) = place {
            inner.pin_start.set(false);
            self.return_to(place);
        }
    }

    /// Puts a freshly built view back where the old one was. A list widget
    /// ignores `scroll_to` and has no scroll range until it has laid its
    /// items out once, so this waits for the first frame and acts at the
    /// start of the second.
    fn return_to(&self, place: Place) {
        let weak = self.downgrade();
        let frames = std::cell::Cell::new(0u8);
        self.0.scroller.add_tick_callback(move |_, _| {
            if frames.replace(frames.get() + 1) == 0 {
                return glib::ControlFlow::Continue;
            }
            let Some(tab) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            match place {
                Place::Item(position) => {
                    if let Some(view) = tab.0.view.borrow().as_ref()
                        && position < tab.0.model.visible_count() as u32
                    {
                        view.scroll_to(
                            position,
                            if tab.has_focus() {
                                gtk::ListScrollFlags::FOCUS
                            } else {
                                gtk::ListScrollFlags::NONE
                            },
                        );
                    }
                }
                Place::Fraction(fraction) => {
                    let adjustment = tab.scroll_adjustment();
                    let range = (adjustment.upper() - adjustment.page_size()).max(0.0);
                    adjustment.set_value(fraction * range);
                }
            }
            glib::ControlFlow::Break
        });
    }

    fn list_event(&self, event: list::ListEvent) {
        match event {
            list::ListEvent::Sort(column, descending) => {
                let mut sort = self.0.model.sort();
                sort.column = column;
                sort.descending = descending;
                // The header already shows it; do not echo back.
                self.apply_sort(sort, false);
            }
            list::ListEvent::Columns(columns) => self.set_columns(columns),
        }
    }

    fn set_columns(&self, columns: Columns) {
        if self.0.columns.replace(columns.clone()) != columns {
            self.persist();
        }
    }

    /// Settings that change how cells look or react rebuild the view.
    pub(super) fn watch_settings(&self) {
        let weak = self.downgrade();
        let handlers = self.0.app.settings().connect_changed_any(
            &[
                Key::BigIconSize,
                Key::SmallIconSize,
                Key::ThumbnailSize,
                Key::ShowFullNames,
                Key::ShadowHidden,
                Key::SingleClick,
                Key::AutoSelectionDelay,
                Key::SiUnit,
                Key::ShowThumbnail,
                Key::ThumbnailLocal,
                Key::ThumbnailMax,
            ],
            move || {
                if let Some(tab) = weak.upgrade() {
                    tab.rebuild_view();
                    tab.emit(Change::Status);
                }
            },
        );
        // Whether `name~` is hidden is decided when a file is listed.
        let weak = self.downgrade();
        let reload = self
            .0
            .app
            .settings()
            .connect_changed(Key::BackupAsHidden, move || {
                if let Some(tab) = weak.upgrade() {
                    tab.reload();
                }
            });
        self.0
            .settings_handlers
            .replace(handlers.into_iter().chain([reload]).collect());
    }

    // ---- staying at the start while loading ----

    /// Any scroll, key or button press hands the scroll position back to the
    /// user for the rest of this load.
    pub(super) fn watch_user_input(&self) {
        let scroller = &self.0.scroller;
        let unpin = {
            let weak = self.downgrade();
            move || {
                if let Some(tab) = weak.upgrade() {
                    tab.0.pin_start.set(false);
                }
            }
        };
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let on_scroll = unpin.clone();
        scroll.connect_scroll(move |_, _, _| {
            on_scroll();
            glib::Propagation::Proceed
        });
        scroller.add_controller(scroll);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let on_key = unpin.clone();
        keys.connect_key_pressed(move |_, _, _, _| {
            on_key();
            glib::Propagation::Proceed
        });
        scroller.add_controller(keys);
        let press = gtk::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk::PropagationPhase::Capture);
        press.connect_pressed(move |_, _, _, _| unpin());
        scroller.add_controller(press);
    }

    /// Re-anchors the view on its first item before the next frame.
    ///
    /// This runs from the model's `items-changed`, where the view has not yet
    /// processed the change itself; calling into the view there crashes GTK.
    /// So the work is queued, at a priority that still precedes the redraw.
    pub(super) fn keep_at_start(&self) {
        if !self.0.pin_start.get() || self.0.pin_queued.replace(true) {
            return;
        }
        let weak = self.downgrade();
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            if let Some(tab) = weak.upgrade() {
                tab.0.pin_queued.set(false);
                if tab.0.pin_start.get()
                    && tab.0.model.visible_count() > 0
                    && let Some(view) = tab.0.view.borrow().as_ref()
                {
                    view.scroll_to(0, gtk::ListScrollFlags::NONE);
                }
            }
            glib::ControlFlow::Break
        });
    }
}
