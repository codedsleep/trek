//! Showing a folder: taking its listing from where the tab already has it,
//! or reading it, and following what the listing reports.

use super::{Change, LoadState, Restore, TabPage, location_from_uri};
use crate::{
    debug, dialogs,
    view::listing::{Listing, Notice, Phase},
};
use gtk::{gio, glib, prelude::*};
use std::time::Instant;
use tallboy_fs::folder::{Options, nearest_existing_ancestor};

impl TabPage {
    /// Shows the tab's location: from the listing a side column or the
    /// tab's cache holds, or else by reading the folder.
    pub(super) fn start_load(&self) {
        self.load(false);
    }

    /// `afresh` reads the folder from disk whatever the tab has kept.
    pub(super) fn load(&self, afresh: bool) {
        let inner = &self.0;
        let generation = inner.generation.get() + 1;
        inner.generation.set(generation);
        // Replacing the listing destroys the focused cell; remember that the
        // view had the keyboard so that the new items get it back.
        if self.has_focus() {
            inner.wants_focus.set(true);
        }
        self.lift_cursor();
        // A load that restores a scroll position must not be pulled to the start.
        inner.pin_start.set(inner.restore.borrow().scroll.is_none());
        inner.load_started.set(Some(Instant::now()));
        inner.first_batch.set(true);
        if debug::tracing() {
            inner.stalls.replace(Some(debug::StallWatch::start()));
        }

        let uri = inner.location.borrow().to_uri().to_owned();
        let file = gio::File::for_uri(&uri);
        inner.awaited.replace(None);
        inner.writable.set(true);
        inner.filesystem.replace(None);
        self.query_folder_facts(&file, generation);
        let options = self.listing_options();
        let kept = if afresh {
            inner.cache.clear();
            None
        } else {
            self.claim(&uri, options)
        };
        let listing = match kept {
            Some(listing) => Ok(listing),
            None => {
                let parent = inner.root.root().and_downcast::<gtk::Window>();
                let mount = gtk::MountOperation::new(parent.as_ref());
                Listing::load(&file, options, Some(mount.upcast()), &inner.model.shown())
            }
        };
        // The folder shown until now goes to the cache, where the side
        // columns find it: left behind, it is the next preview or parent.
        let keep = |old: Listing| {
            if afresh {
                old.discard();
            } else {
                inner.cache.put(old);
            }
        };
        match listing {
            Ok(listing) => {
                let loaded = listing.is_loaded();
                keep(inner.model.adopt(listing));
                inner
                    .watching
                    .replace(Some(debug::Counted::new((), &debug::LIVE_FOLDERS)));
                if loaded {
                    self.focus_if_wanted();
                    self.finish_load();
                } else {
                    self.set_load(LoadState::Loading);
                    if inner.model.visible_count() > 0 {
                        self.focus_if_wanted();
                    }
                }
            }
            Err(e) => {
                keep(inner.model.clear());
                inner.watching.replace(None);
                self.set_load(LoadState::Failed(dialogs::error::folder(&e)));
            }
        }
    }

    /// The listing of `uri` this tab already has: shown by one of its side
    /// columns, which gives it up, or kept from an earlier visit.
    fn claim(&self, uri: &str, options: Options) -> Option<Listing> {
        let inner = &self.0;
        let shown = inner
            .parent
            .yield_listing(uri)
            .or_else(|| inner.preview.yield_listing(uri));
        match shown {
            Some(listing) if listing.reusable() && listing.options() == options => Some(listing),
            Some(listing) => {
                listing.discard();
                None
            }
            None => inner.cache.take(uri, options),
        }
    }

    fn set_load(&self, state: LoadState) {
        self.0.load.replace(state);
        self.update_placeholders();
        self.emit(Change::Load);
    }

    /// The folder has been read in full, now or when a column read it earlier.
    fn finish_load(&self) {
        self.set_load(LoadState::Loaded);
        self.release_pin_when_sorted();
        self.restore_view_state();
        if debug::tracing() {
            self.trace_when_sorted();
        }
    }

    /// What the listing shown reports, after it has changed its items.
    pub(super) fn heard(&self, notice: Notice<'_>) {
        match notice {
            Notice::Batch => {
                self.focus_if_wanted();
                if self.0.first_batch.replace(false) && debug::tracing() {
                    self.trace_since_load("first batch in the model");
                    let started = self.0.load_started.get();
                    debug::after_next_paint(&self.0.root, move || {
                        if let Some(started) = started {
                            tallboy_fs::trace::elapsed("first batch painted", started);
                        }
                    });
                }
            }
            Notice::Finished => self.finish_load(),
            Notice::Error(e) => {
                let message = dialogs::error::folder(e);
                if self.0.model.phase() == Phase::Failed {
                    self.set_load(LoadState::Failed(message));
                } else {
                    log::warn!("Folder {}: {message}", self.0.location.borrow().to_uri());
                }
            }
            Notice::Listed(uri) => self.select_if_awaited(uri),
            // The status line names the type of the file under the cursor.
            Notice::FsInfo | Notice::Typed => self.emit(Change::Status),
            Notice::RootGone => self.leave_gone_folder(),
            Notice::Replacing(_) => {}
        }
    }

    /// The folder was deleted or unmounted: move to the nearest ancestor
    /// that still exists, or Home.
    fn leave_gone_folder(&self) {
        let generation = self.0.generation.get();
        let file = gio::File::for_uri(self.0.location.borrow().to_uri());
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            let ancestor = nearest_existing_ancestor(&file).await;
            let Some(tab) = weak.upgrade() else { return };
            if tab.0.generation.get() != generation {
                return;
            }
            let target = ancestor
                .and_then(|f| location_from_uri(&f.uri()))
                .or_else(|| tab.0.app.home_location());
            if let Some(target) = target {
                tab.navigate(target);
            }
        });
    }

    /// A file the user just created or renamed has shown up: select it.
    /// Queued, because this runs inside the model's change notification.
    fn select_if_awaited(&self, uri: &str) {
        if self.0.awaited.borrow().as_deref() != Some(uri) {
            return;
        }
        self.0.awaited.replace(None);
        let weak = self.downgrade();
        let uri = uri.to_owned();
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            if let Some(tab) = weak.upgrade() {
                tab.hover_uri(&uri);
            }
            glib::ControlFlow::Break
        });
    }

    /// Asks about the folder itself: whether it is writable, for Paste and
    /// Create New, and which filesystem it is on, for drops, including before
    /// the loader's FsInfo arrives.
    fn query_folder_facts(&self, file: &gio::File, generation: u64) {
        let weak = self.downgrade();
        let file = file.clone();
        glib::spawn_future_local(async move {
            let info = file
                .query_info_future(
                    "access::can-write,id::filesystem",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await;
            let Some(tab) = weak.upgrade() else { return };
            if tab.0.generation.get() != generation {
                return;
            }
            // Unknown counts as writable: the job reports the real error.
            let writable = info.as_ref().map_or(true, |info| {
                !info.has_attribute("access::can-write") || info.boolean("access::can-write")
            });
            tab.0.filesystem.replace(
                info.ok()
                    .and_then(|info| info.attribute_string("id::filesystem"))
                    .map(String::from),
            );
            if tab.0.writable.replace(writable) != writable {
                tab.emit(Change::Status);
            }
        });
    }

    /// Returns the keyboard to the view after a navigation emptied it,
    /// unless the user has meanwhile put it somewhere else.
    fn focus_if_wanted(&self) {
        if !self.0.wants_focus.replace(false) {
            return;
        }
        let elsewhere = self
            .0
            .root
            .root()
            .and_downcast::<gtk::Window>()
            .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
            .is_some_and(|focus| !focus.is_ancestor(&self.0.scroller));
        if !elsewhere && let Some(view) = self.0.view.borrow().as_ref() {
            view.grab_focus();
        }
    }

    /// The pin lasts until the incremental sort has placed every item. A
    /// listing that came sorted is settled here and now, before the frame.
    fn release_pin_when_sorted(&self) {
        if self.0.model.sorted().pending() == 0 {
            self.release_pin();
            return;
        }
        let weak = self.downgrade();
        let generation = self.0.generation.get();
        glib::idle_add_local(move || {
            let Some(tab) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if tab.0.generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            if tab.0.model.sorted().pending() > 0 {
                return glib::ControlFlow::Continue;
            }
            tab.release_pin();
            glib::ControlFlow::Break
        });
    }

    fn release_pin(&self) {
        // A file that was to be selected once listed ("Show in folder")
        // arrived with the listing itself, not as a later change.
        let awaited = self.0.awaited.borrow().clone();
        if let Some(uri) = awaited
            && self.hover_uri(&uri)
        {
            self.0.awaited.replace(None);
            return;
        }
        // The keyboard focus was given to whichever item came first
        // while loading; now that the order is final, put it on the
        // real first item, or a later relayout scrolls to the old one.
        if self.0.pin_start.replace(false) && self.0.model.visible_count() > 0 {
            self.place_cursor(0, true);
        }
    }

    fn restore_view_state(&self) {
        let Restore { cursor, scroll } = self.0.restore.take();
        let position = cursor.and_then(|uri| self.0.model.position_of(&uri));
        if position.is_some() {
            self.0.pin_start.set(false);
        }
        match (position, scroll) {
            (Some(position), None) => self.place_cursor(position, true),
            (position, Some(scroll)) => {
                if let Some(position) = position {
                    self.place_cursor(position, false);
                }
                // The adjustment's range is only known after the next layout.
                let adjustment = self.scroll_adjustment();
                glib::idle_add_local_once(move || adjustment.set_value(scroll));
            }
            (None, None) => {}
        }
    }

    fn trace_since_load(&self, label: &str) {
        if let Some(started) = self.0.load_started.get() {
            tallboy_fs::trace::elapsed(label, started);
        }
    }

    /// Reports load time, sort time, the longest main-loop stall and memory
    /// once the incremental sort has caught up.
    fn trace_when_sorted(&self) {
        self.trace_since_load("folder loaded");
        let weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(tab) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if tab.0.model.sorted().pending() > 0 {
                return glib::ControlFlow::Continue;
            }
            tab.trace_since_load("folder loaded and sorted");
            if let Some(stalls) = tab.0.stalls.take() {
                debug::note(&format!(
                    "longest main-loop stall: {:.1} ms",
                    stalls.finish().as_secs_f64() * 1000.0
                ));
            }
            debug::note(&format!(
                "items: {}, rss: {} KiB",
                tab.0.model.total_count(),
                debug::rss_kib().unwrap_or_default()
            ));
            glib::ControlFlow::Break
        });
    }
}
