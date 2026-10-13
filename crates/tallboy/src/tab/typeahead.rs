//! Type-ahead find (PROMPT §8.8): typing over a view opens a small entry at
//! its bottom-right and selects the first item whose name starts with the
//! text. Up and Down cycle through matches, Enter opens, Esc or five idle
//! seconds close it.

use super::TabPage;
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tallboy_core::helpers;

const IDLE: Duration = Duration::from_secs(5);

pub(super) struct TypeAhead {
    entry: gtk::Entry,
    /// Visible names in view order, built once per session of typing.
    names: RefCell<Option<Rc<Vec<String>>>>,
    current: Cell<Option<usize>>,
    idle: RefCell<Option<glib::SourceId>>,
}

impl TypeAhead {
    pub(super) fn new() -> Self {
        let entry = gtk::Entry::new();
        entry.add_css_class("type-ahead");
        entry.set_halign(gtk::Align::End);
        entry.set_valign(gtk::Align::End);
        entry.set_width_chars(20);
        entry.set_visible(false);
        Self {
            entry,
            names: RefCell::default(),
            current: Cell::new(None),
            idle: RefCell::default(),
        }
    }

    pub(super) fn widget(&self) -> &gtk::Entry {
        &self.entry
    }
}

/// The previous match before `cursor`, wrapping around.
fn previous_match(names: &[String], prefix: &str, cursor: Option<usize>) -> Option<usize> {
    let count = names.len();
    if count == 0 || prefix.is_empty() {
        return None;
    }
    let prefix = prefix.to_lowercase();
    let start = cursor.map_or(count - 1, |c| (c + count - 1) % count);
    (0..count)
        .map(|offset| (start + count - offset) % count)
        .find(|&index| names[index].to_lowercase().starts_with(&prefix))
}

impl TabPage {
    pub(super) fn connect_type_ahead(&self) {
        let entry = &self.0.type_ahead.entry;
        let weak = self.downgrade();
        entry.connect_changed(move |_| {
            if let Some(tab) = weak.upgrade() {
                tab.type_ahead_search(0);
            }
        });
        let weak = self.downgrade();
        entry.connect_activate(move |_| {
            let Some(tab) = weak.upgrade() else { return };
            let position = tab.0.type_ahead.current.get();
            tab.close_type_ahead();
            if let Some(position) = position {
                tab.activate(position as u32);
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(tab) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gdk::Key::Down => tab.type_ahead_search(1),
                gdk::Key::Up => tab.type_ahead_search(-1),
                gdk::Key::Escape => tab.close_type_ahead(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        let weak = self.downgrade();
        focus.connect_leave(move |_| {
            if let Some(tab) = weak.upgrade() {
                tab.hide_type_ahead();
            }
        });
        entry.add_controller(focus);
    }

    /// Watches a view widget for the printable key that starts a search.
    pub(super) fn watch_type_ahead_keys(&self, view: &gtk::Widget) {
        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, state| {
            let shortcut = gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK;
            if state.intersects(shortcut) {
                return glib::Propagation::Proceed;
            }
            match (key.to_unicode(), weak.upgrade()) {
                // Space belongs to the list: it toggles the item under the cursor.
                (Some(character), Some(tab)) if !character.is_control() && character != ' ' => {
                    tab.open_type_ahead(character);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        view.add_controller(keys);
    }

    fn open_type_ahead(&self, first: char) {
        let type_ahead = &self.0.type_ahead;
        type_ahead.current.set(None);
        type_ahead.entry.set_visible(true);
        // Plain grab_focus would select the text and the next key replace it.
        type_ahead.entry.grab_focus_without_selecting();
        type_ahead.entry.set_text(&first.to_string());
        type_ahead.entry.set_position(-1);
    }

    /// Closes the entry and gives the keyboard back to the view.
    pub(super) fn close_type_ahead(&self) {
        let had_focus = self
            .0
            .type_ahead
            .entry
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN);
        self.hide_type_ahead();
        if had_focus {
            self.grab_focus();
        }
    }

    fn hide_type_ahead(&self) {
        let type_ahead = &self.0.type_ahead;
        if let Some(timer) = type_ahead.idle.take() {
            timer.remove();
        }
        type_ahead.names.replace(None);
        type_ahead.current.set(None);
        type_ahead.entry.set_visible(false);
        type_ahead.entry.remove_css_class("error");
    }

    /// The model changed: positions in the cached name list are stale.
    pub(super) fn forget_type_ahead_names(&self) {
        self.0.type_ahead.names.replace(None);
    }

    fn type_ahead_names(&self) -> Rc<Vec<String>> {
        let cached = self.0.type_ahead.names.borrow().clone();
        if let Some(names) = cached {
            return names;
        }
        let model = &self.0.model;
        let names: Rc<Vec<String>> = Rc::new(
            (0..model.visible_count() as u32)
                .filter_map(|position| model.item_at(position))
                .map(|item| item.meta().display_name.clone())
                .collect(),
        );
        self.0.type_ahead.names.replace(Some(names.clone()));
        names
    }

    /// Finds a match: `0` from the top for new text, `1` the next match,
    /// `-1` the previous one.
    fn type_ahead_search(&self, step: i32) {
        let type_ahead = &self.0.type_ahead;
        if !WidgetExt::is_visible(&type_ahead.entry) {
            return;
        }
        self.restart_type_ahead_timer();
        let text = type_ahead.entry.text();
        if text.is_empty() {
            return;
        }
        let names = self.type_ahead_names();
        let current = type_ahead.current.get();
        let found = match step {
            0 => helpers::type_ahead(&names, &text, current, false),
            1 => helpers::type_ahead(&names, &text, current, true),
            _ => previous_match(&names, &text, current),
        };
        match found {
            Some(index) => {
                type_ahead.entry.remove_css_class("error");
                type_ahead.current.set(Some(index));
                self.0.model.selection().select_item(index as u32, true);
                if let Some(view) = self.0.view.borrow().as_ref() {
                    // No FOCUS flag: the keyboard stays in the entry.
                    view.scroll_to(index as u32, gtk::ListScrollFlags::NONE);
                }
            }
            None => type_ahead.entry.add_css_class("error"),
        }
    }

    fn restart_type_ahead_timer(&self) {
        let weak = self.downgrade();
        let timer = glib::timeout_add_local_once(IDLE, move || {
            if let Some(tab) = weak.upgrade() {
                tab.0.type_ahead.idle.replace(None);
                tab.close_type_ahead();
            }
        });
        if let Some(old) = self.0.type_ahead.idle.replace(Some(timer)) {
            old.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::previous_match;

    #[test]
    fn previous_match_wraps() {
        let names: Vec<String> = ["alpha", "Beta", "apple", "berry"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(previous_match(&names, "a", Some(2)), Some(0));
        assert_eq!(previous_match(&names, "a", Some(0)), Some(2));
        assert_eq!(previous_match(&names, "b", None), Some(3));
        assert_eq!(previous_match(&names, "B", Some(3)), Some(1));
        assert_eq!(previous_match(&names, "z", Some(1)), None);
        assert_eq!(previous_match(&[], "a", None), None);
    }
}
