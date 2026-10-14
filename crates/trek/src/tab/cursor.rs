//! The cursor, visual mode and the find and filter commands.
//!
//! The cursor ("hovered" file) and the selection are separate
//! things. Moving the cursor selects nothing; a command acts on the selected
//! files, or on the file under the cursor when none is selected.
//!
//! The pointer keeps its usual meaning: a plain click makes the clicked file
//! the selection. Such a selection is only a by-product of pointing, so the
//! next cursor movement from the keyboard drops it again; a selection built
//! with Space, visual mode, Ctrl or Shift stays until Esc.

use super::{Change, TabPage};
use crate::view::{FileItem, dnd::Pressed};
use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};
use trek_core::keymap::{self, Step};

/// What cursor movements do to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Nothing.
    #[default]
    Normal,
    /// Visual mode: the files between its start and the cursor are selected.
    Select,
    /// Visual mode started with `V`: they are deselected.
    Unset,
}

impl Mode {
    /// The three letters the status line shows.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NOR",
            Mode::Select => "SEL",
            Mode::Unset => "UNS",
        }
    }
}

struct Visual {
    /// Where visual mode was entered.
    anchor: u32,
    unset: bool,
    /// The selection as it was then; the range is applied on top of it.
    base: gtk::Bitset,
}

#[derive(Default)]
pub(super) struct Cursor {
    position: Cell<u32>,
    item: RefCell<Option<FileItem>>,
    visual: RefCell<Option<Visual>>,
    /// The selection is one file that a plain click selected.
    implicit: Cell<bool>,
    sync_queued: Cell<bool>,
    /// The last query of `/` or `?`, for `n` and `N`.
    query: RefCell<Option<String>>,
}

impl TabPage {
    /// The cursor's position in the view; `None` when the view is empty.
    pub fn cursor(&self) -> Option<u32> {
        self.0
            .cursor
            .item
            .borrow()
            .is_some()
            .then(|| self.0.cursor.position.get())
    }

    /// The file under the cursor.
    pub fn cursor_item(&self) -> Option<FileItem> {
        self.0.cursor.item.borrow().clone()
    }

    /// What a command acts on: the selected files, or else the file under
    /// the cursor.
    pub fn targets(&self) -> Vec<FileItem> {
        let selected = self.0.model.selected();
        if selected.is_empty() {
            self.cursor_item().into_iter().collect()
        } else {
            selected
        }
    }

    pub fn vim_mode(&self) -> Mode {
        match self.0.cursor.visual.borrow().as_ref() {
            None => Mode::Normal,
            Some(visual) if visual.unset => Mode::Unset,
            Some(_) => Mode::Select,
        }
    }

    /// Takes the cursor off its file before the tab stops showing the
    /// folder: the highlight is the tab's, and the listing may be shown by
    /// another column next.
    pub(super) fn lift_cursor(&self) {
        if let Some(item) = self.0.cursor.item.take() {
            item.hover(false);
        }
    }

    /// Puts the cursor on a position without the side effects of a movement
    /// the user asked for. `scroll` brings it into view.
    pub(super) fn place_cursor(&self, position: u32, scroll: bool) {
        let cursor = &self.0.cursor;
        let count = self.0.model.visible_count() as u32;
        let item = (count > 0)
            .then(|| self.0.model.item_at(position.min(count - 1)))
            .flatten();
        let position = if item.is_some() {
            position.min(count - 1)
        } else {
            0
        };
        let moved = cursor.position.replace(position) != position;
        let previous = cursor.item.replace(item.clone());
        let changed = previous != item;
        if changed && let Some(previous) = previous {
            previous.hover(false);
        }
        if let Some(item) = &item {
            item.hover(true);
            if scroll && let Some(view) = self.0.view.borrow().as_ref() {
                view.scroll_to(
                    position,
                    if self.has_focus() {
                        gtk::ListScrollFlags::FOCUS
                    } else {
                        gtk::ListScrollFlags::NONE
                    },
                );
            }
        }
        if moved || changed {
            self.emit(Change::Cursor);
        }
    }

    /// Moves the cursor because the user said so: the view stops following
    /// the start of a loading folder, a selection left behind by a click is
    /// dropped, and visual mode extends to the new position.
    pub fn set_cursor(&self, position: u32) {
        self.0.pin_start.set(false);
        if self.0.cursor.implicit.replace(false) {
            self.0.model.selection().unselect_all();
        }
        self.place_cursor(position, true);
        self.apply_visual();
    }

    /// How many items one page of the view holds.
    fn page_items(&self) -> u32 {
        let count = self.0.model.visible_count();
        let adjustment = self.scroll_adjustment();
        if count == 0 || adjustment.upper() <= 0.0 {
            return 1;
        }
        let per_item = adjustment.upper() / count as f64;
        ((adjustment.page_size() / per_item).floor() as u32).max(1)
    }

    /// `j`, `k`, `gg`, `G` and the page movements. Single steps wrap around
    /// from the last file to the first and back.
    pub fn arrow(&self, step: Step) {
        let count = self.0.model.visible_count() as u32;
        if count == 0 {
            return;
        }
        let last = count - 1;
        let at = self.0.cursor.position.get().min(last);
        let page = self.page_items();
        let target = match step {
            Step::Prev if at == 0 => last,
            Step::Prev => at - 1,
            Step::Next if at == last => 0,
            Step::Next => at + 1,
            Step::Top => 0,
            Step::Bottom => last,
            Step::HalfPageUp => at.saturating_sub((page / 2).max(1)),
            Step::HalfPageDown => (at + (page / 2).max(1)).min(last),
            Step::PageUp => at.saturating_sub(page),
            Step::PageDown => (at + page).min(last),
        };
        self.set_cursor(target);
    }

    /// Space: toggles the file under the cursor and steps down, without
    /// wrapping.
    pub fn toggle_and_next(&self) {
        let Some(position) = self.cursor() else {
            return;
        };
        let selection = self.0.model.selection();
        // A click's selection would otherwise be toggled off by the first
        // Space instead of being kept.
        self.0.cursor.implicit.set(false);
        if selection.is_selected(position) {
            selection.unselect_item(position);
        } else {
            selection.select_item(position, false);
        }
        let last = (self.0.model.visible_count() as u32).saturating_sub(1);
        self.set_cursor((position + 1).min(last));
    }

    /// `v` and `V`: starts visual mode at the cursor.
    pub fn start_visual(&self, unset: bool) {
        let Some(anchor) = self.cursor() else {
            return;
        };
        self.0.cursor.implicit.set(false);
        self.0.cursor.visual.replace(Some(Visual {
            anchor,
            unset,
            // A copy: what the model hands out is its own, live set.
            base: self.0.model.selection().selection().copy(),
        }));
        self.apply_visual();
        self.emit(Change::Cursor);
    }

    fn apply_visual(&self) {
        let cursor = &self.0.cursor;
        let wanted = {
            let visual = cursor.visual.borrow();
            let Some(visual) = visual.as_ref() else {
                return;
            };
            let at = cursor.position.get();
            let (low, high) = (visual.anchor.min(at), visual.anchor.max(at));
            let range = gtk::Bitset::new_range(low, high - low + 1);
            let wanted = visual.base.copy();
            if visual.unset {
                wanted.subtract(&range);
            } else {
                wanted.union(&range);
            }
            wanted
        };
        let selection = self.0.model.selection();
        let all = gtk::Bitset::new_range(0, selection.n_items());
        selection.set_selection(&wanted, &all);
    }

    /// Leaves visual mode, keeping what it selected.
    pub fn end_visual(&self) {
        if self.0.cursor.visual.take().is_some() {
            self.emit(Change::Cursor);
        }
    }

    /// Esc: leaves visual mode, or clears the `f` filter, or clears the
    /// selection — the first of these that applies. Returns whether one did.
    pub fn escape(&self) -> bool {
        if self.0.cursor.visual.borrow().is_some() {
            self.end_visual();
            return true;
        }
        if self.0.model.needle().is_some() {
            self.set_needle(None);
            return true;
        }
        if self.0.model.selected_count() > 0 {
            self.0.model.selection().unselect_all();
            return true;
        }
        false
    }

    /// The pointer pressed an item.
    pub(super) fn pressed(&self, position: u32, pressed: Pressed) {
        self.0.pin_start.set(false);
        self.end_visual();
        self.place_cursor(position, false);
        match pressed {
            Pressed::Selected => self.0.cursor.implicit.set(true),
            Pressed::Modified => self.0.cursor.implicit.set(false),
            Pressed::Kept => {}
        }
    }

    /// The selection changed, by whatever means. A click's handler marks it
    /// as its own afterwards.
    pub(super) fn selection_changed(&self) {
        self.0.cursor.implicit.set(false);
    }

    /// The model's items changed: the cursor stays on its file, or on its
    /// position when that file is gone. Queued, because this is called from
    /// the model's change notification, where the view must not be touched.
    pub(super) fn sync_cursor_later(&self) {
        // Positions recorded by visual mode no longer mean anything.
        self.0.cursor.visual.replace(None);
        if self.0.cursor.sync_queued.replace(true) {
            return;
        }
        let weak = self.downgrade();
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            if let Some(tab) = weak.upgrade() {
                tab.0.cursor.sync_queued.set(false);
                tab.sync_cursor();
            }
            glib::ControlFlow::Break
        });
    }

    fn sync_cursor(&self) {
        let cursor = &self.0.cursor;
        if self.0.pin_start.get() {
            // Still loading and untouched: the cursor is on the first file,
            // whichever that is by now. `keep_at_start` does the scrolling.
            self.place_cursor(0, false);
            return;
        }
        let item = cursor.item.borrow().clone();
        let position = item
            .and_then(|item| self.0.model.position_of_item(&item))
            .unwrap_or_else(|| cursor.position.get());
        self.place_cursor(position, true);
        // The file under the cursor may be the one that changed.
        self.update_preview();
    }

    /// Puts the cursor on a file by URI; returns whether it is listed.
    pub(super) fn hover_uri(&self, uri: &str) -> bool {
        let Some(position) = self.0.model.position_of(uri) else {
            return false;
        };
        self.0.pin_start.set(false);
        self.place_cursor(position, true);
        true
    }

    // ---- find and filter ----

    /// `/` and `?` while typing: the first match at or after `origin`
    /// (before it for `?`). Returns whether there is one.
    pub fn find(&self, query: &str, origin: u32, previous: bool) -> bool {
        if !query.is_empty() {
            self.0.cursor.query.replace(Some(query.to_owned()));
        }
        let found = keymap::find_step(
            &self.0.model.names(),
            query,
            origin as usize,
            previous,
            true,
        );
        if let Some(position) = found {
            self.set_cursor(position as u32);
        }
        found.is_some()
    }

    /// `n` and `N`: the next or previous match of the last query. `None`
    /// when nothing has been searched for yet.
    pub fn find_arrow(&self, previous: bool) -> Option<bool> {
        let query = self.0.cursor.query.borrow().clone()?;
        let at = self.cursor().unwrap_or_default() as usize;
        let found = keymap::find_step(&self.0.model.names(), &query, at, previous, false);
        if let Some(position) = found {
            self.set_cursor(position as u32);
        }
        Some(found.is_some())
    }

    /// The last `/` or `?` query.
    pub fn find_query(&self) -> Option<String> {
        self.0.cursor.query.borrow().clone()
    }

    /// Sets or clears the `f` filter.
    pub fn set_needle(&self, needle: Option<String>) {
        self.0.model.set_needle(needle);
        self.emit(Change::Status);
    }
}
