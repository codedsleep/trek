//! The path bar (PROMPT §8.3): a location entry with directory completion,
//! or breadcrumb buttons, swapped by the `pathbar_mode_buttons` setting.

use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
};
use trek_core::location::Location;
use trek_fs::complete;

/// What the path bar asks its window to do.
#[derive(Debug, Clone)]
pub enum PathEvent {
    /// Show this location in the active tab.
    Navigate(Location),
    /// Open this location in a new tab (middle-click on a segment).
    NewTab(Location),
    /// The user left the entry without navigating; focus belongs to the view.
    Dismissed,
    /// The typed text is not a location; the message says why.
    Invalid(String),
}

const ENTRY_PAGE: &str = "entry";
const CRUMBS_PAGE: &str = "crumbs";
/// Completion rows shown before the list scrolls.
const COMPLETION_HEIGHT: i32 = 280;

struct Inner {
    stack: gtk::Stack,
    entry: gtk::Entry,
    popover: gtk::Popover,
    list: gtk::ListBox,
    crumbs: gtk::Box,
    crumbs_scroller: gtk::ScrolledWindow,
    earlier: gtk::Button,
    later: gtk::Button,
    location: RefCell<Option<Location>>,
    /// The deepest path visited on this branch; its segments stay visible
    /// after going up.
    deepest: RefCell<Option<Location>>,
    home: RefCell<PathBuf>,
    show_hidden: Cell<bool>,
    buttons_mode: Cell<bool>,
    /// Set while the entry text is changed by code, not by typing.
    programmatic: Cell<bool>,
    completion_run: Cell<u64>,
    /// Text up to and including the last `/`, and the names offered after it.
    completing: RefCell<(String, Vec<String>)>,
    emit: Box<dyn Fn(PathEvent)>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A popover is a child of its parent, not of a container.
        self.popover.unparent();
    }
}

/// The path bar. Clones share one bar.
#[derive(Clone)]
pub struct PathBar(Rc<Inner>);

fn common_prefix(names: &[String]) -> String {
    let Some(first) = names.first() else {
        return String::new();
    };
    let mut length = first.len();
    for name in &names[1..] {
        length = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((index, a), b)| *index < length && a == b)
            .last()
            .map_or(0, |((index, a), _)| index + a.len_utf8());
    }
    first[..length].to_owned()
}

impl PathBar {
    pub fn new(emit: impl Fn(PathEvent) + 'static) -> Self {
        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_input_purpose(gtk::InputPurpose::Url);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        // Keyboard focus stays in the entry while the list is open.
        list.set_can_focus(false);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_max_content_height(COMPLETION_HEIGHT);
        scroller.set_child(Some(&list));
        let popover = gtk::Popover::new();
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_halign(gtk::Align::Start);
        popover.set_can_focus(false);
        popover.set_child(Some(&scroller));
        popover.set_parent(&entry);

        let crumbs = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        crumbs.add_css_class("linked");
        let crumbs_scroller = gtk::ScrolledWindow::new();
        crumbs_scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        crumbs_scroller.set_hexpand(true);
        crumbs_scroller.set_child(Some(&crumbs));
        let earlier = gtk::Button::from_icon_name("pan-start-symbolic");
        let later = gtk::Button::from_icon_name("pan-end-symbolic");
        for arrow in [&earlier, &later] {
            arrow.add_css_class("flat");
            arrow.set_focus_on_click(false);
            arrow.set_visible(false);
        }
        let crumbs_page = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        crumbs_page.add_css_class("pathbar-crumbs");
        crumbs_page.append(&earlier);
        crumbs_page.append(&crumbs_scroller);
        crumbs_page.append(&later);

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.add_named(&entry, Some(ENTRY_PAGE));
        stack.add_named(&crumbs_page, Some(CRUMBS_PAGE));

        let bar = Self(Rc::new(Inner {
            stack,
            entry,
            popover,
            list,
            crumbs,
            crumbs_scroller,
            earlier,
            later,
            location: RefCell::default(),
            deepest: RefCell::default(),
            home: RefCell::new(glib::home_dir()),
            show_hidden: Cell::new(false),
            buttons_mode: Cell::new(false),
            programmatic: Cell::new(false),
            completion_run: Cell::new(0),
            completing: RefCell::default(),
            emit: Box::new(emit),
        }));
        bar.connect_entry();
        bar.connect_crumbs();
        bar
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.0.stack
    }

    // ---- shared state ----

    /// Shows `location`. `home` and `show_hidden` steer `~` and completion.
    pub fn set_location(&self, location: &Location, home: PathBuf, show_hidden: bool) {
        let inner = &self.0;
        inner.home.replace(home);
        inner.show_hidden.set(show_hidden);
        if inner.location.borrow().as_ref() == Some(location) {
            return;
        }
        inner.location.replace(Some(location.clone()));
        self.restore_text();
        self.rebuild_crumbs();
    }

    /// Switches between the entry and the breadcrumb buttons.
    pub fn set_buttons_mode(&self, buttons: bool) {
        // Only on a change: a refresh must not take the bar back from
        // Go to Location, which borrows it for the entry.
        if self.0.buttons_mode.replace(buttons) == buttons {
            return;
        }
        self.0
            .stack
            .set_visible_child_name(if buttons { CRUMBS_PAGE } else { ENTRY_PAGE });
    }

    /// Go to Location: focus the entry, borrowing the bar from the
    /// breadcrumbs until focus leaves it again.
    pub fn focus_entry(&self) {
        self.0.stack.set_visible_child_name(ENTRY_PAGE);
        self.0.entry.grab_focus();
        self.0.entry.select_region(0, -1);
    }

    /// The Go button: navigate to what the entry says.
    pub fn submit(&self) {
        let inner = &self.0;
        self.hide_completion();
        let text = inner.entry.text();
        let home = inner.home.borrow().clone();
        let cwd = inner
            .location
            .borrow()
            .as_ref()
            .and_then(Location::path)
            .unwrap_or_else(|| home.clone());
        match Location::parse(text.trim(), &cwd, &home) {
            Ok(location) => (inner.emit)(PathEvent::Navigate(location)),
            Err(e) => (inner.emit)(PathEvent::Invalid(e.to_string())),
        }
    }

    /// Whether the keyboard focus is in the entry. The focus widget is the
    /// entry's inner text widget, so `has_focus()` on the entry is not it.
    fn editing(&self) -> bool {
        self.0
            .entry
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN)
    }

    /// One line for agent checks.
    pub fn debug_state(&self) -> String {
        format!(
            "entry={:?} completions={} completion_open={} page={}",
            self.0.entry.text(),
            self.0.completing.borrow().1.len(),
            self.0.popover.is_visible(),
            self.0.stack.visible_child_name().unwrap_or_default(),
        )
    }

    fn set_text(&self, text: &str) {
        self.0.programmatic.set(true);
        self.0.entry.set_text(text);
        self.0.entry.set_position(-1);
        self.0.programmatic.set(false);
    }

    fn restore_text(&self) {
        let text = self
            .0
            .location
            .borrow()
            .as_ref()
            .map(Location::display)
            .unwrap_or_default();
        self.set_text(&text);
    }

    fn dismiss(&self) {
        self.hide_completion();
        self.restore_text();
        if self.0.buttons_mode.get() {
            self.0.stack.set_visible_child_name(CRUMBS_PAGE);
        }
        (self.0.emit)(PathEvent::Dismissed);
    }

    // ---- entry and completion ----

    fn connect_entry(&self) {
        let inner = &self.0;
        let weak = self.weak();
        inner.entry.connect_activate(move |_| {
            let Some(bar) = weak.upgrade().map(PathBar) else {
                return;
            };
            match bar.selected_completion() {
                Some(name) => bar.accept(&name),
                None => bar.submit(),
            }
        });
        let weak = self.weak();
        inner.entry.connect_changed(move |_| {
            if let Some(bar) = weak.upgrade().map(PathBar)
                && !bar.0.programmatic.get()
            {
                bar.complete();
            }
        });
        let weak = self.weak();
        inner.list.connect_row_activated(move |_, row| {
            if let Some(bar) = weak.upgrade().map(PathBar) {
                let name = bar
                    .0
                    .completing
                    .borrow()
                    .1
                    .get(row.index() as usize)
                    .cloned();
                if let Some(name) = name {
                    bar.accept(&name);
                }
            }
        });

        // Tab, Up, Down and Esc are ours before the entry's own bindings.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.weak();
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(bar) = weak.upgrade().map(PathBar) else {
                return glib::Propagation::Proceed;
            };
            let open = bar.0.popover.is_visible();
            match key {
                gdk::Key::Tab if open => {
                    bar.complete_common_prefix();
                    glib::Propagation::Stop
                }
                gdk::Key::Down if open => {
                    bar.move_selection(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Up if open => {
                    bar.move_selection(-1);
                    glib::Propagation::Stop
                }
                gdk::Key::Escape => {
                    if open {
                        bar.hide_completion();
                    } else {
                        bar.dismiss();
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        inner.entry.add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        let weak = self.weak();
        focus.connect_leave(move |_| {
            let Some(bar) = weak.upgrade().map(PathBar) else {
                return;
            };
            bar.hide_completion();
            bar.restore_text();
            if bar.0.buttons_mode.get() {
                // Swapping the page inside a focus change confuses GTK.
                let weak = bar.weak();
                glib::idle_add_local_once(move || {
                    if let Some(inner) = weak.upgrade()
                        && inner.buttons_mode.get()
                        && !PathBar(inner.clone()).editing()
                    {
                        inner.stack.set_visible_child_name(CRUMBS_PAGE);
                    }
                });
            }
        });
        inner.entry.add_controller(focus);
    }

    fn hide_completion(&self) {
        // Invalidate any lookup still in flight.
        self.0.completion_run.set(self.0.completion_run.get() + 1);
        self.0.popover.popdown();
        self.0.completing.replace((String::new(), Vec::new()));
    }

    /// Looks up the child directories that match what has been typed.
    fn complete(&self) {
        let inner = &self.0;
        let text = inner.entry.text().to_string();
        let Some(split) = text.rfind('/') else {
            self.hide_completion();
            return;
        };
        let (dir_text, prefix) = (text[..=split].to_owned(), text[split + 1..].to_owned());
        let home = inner.home.borrow().clone();
        let Ok(parent) = Location::parse(&dir_text, &home, &home) else {
            self.hide_completion();
            return;
        };
        let run = inner.completion_run.get() + 1;
        inner.completion_run.set(run);
        let show_hidden = inner.show_hidden.get() || prefix.starts_with('.');
        let weak = self.weak();
        glib::spawn_future_local(async move {
            let file = gio::File::for_uri(parent.to_uri());
            let matches = complete::prefix(&file, &prefix, show_hidden).await;
            let Some(bar) = weak.upgrade().map(PathBar) else {
                return;
            };
            if bar.0.completion_run.get() != run || !bar.editing() {
                return;
            }
            let names: Vec<String> = matches
                .unwrap_or_default()
                .into_iter()
                .map(|meta| meta.display_name)
                .collect();
            bar.show_completion(dir_text, names);
        });
    }

    fn show_completion(&self, dir_text: String, names: Vec<String>) {
        let inner = &self.0;
        inner.list.remove_all();
        if names.is_empty() {
            inner.popover.popdown();
            inner.completing.replace((dir_text, names));
            return;
        }
        for name in &names {
            let label = gtk::Label::new(Some(name));
            label.set_xalign(0.0);
            inner.list.append(&label);
        }
        inner.list.unselect_all();
        inner.completing.replace((dir_text, names));
        inner
            .popover
            .set_size_request(inner.entry.width().max(200), -1);
        inner.popover.popup();
    }

    fn selected_completion(&self) -> Option<String> {
        if !self.0.popover.is_visible() {
            return None;
        }
        let row = self.0.list.selected_row()?;
        self.0
            .completing
            .borrow()
            .1
            .get(row.index() as usize)
            .cloned()
    }

    fn move_selection(&self, step: i32) {
        let list = &self.0.list;
        let count = self.0.completing.borrow().1.len() as i32;
        if count == 0 {
            return;
        }
        let next = match list.selected_row() {
            Some(row) => (row.index() + step).rem_euclid(count),
            None if step > 0 => 0,
            None => count - 1,
        };
        if let Some(row) = list.row_at_index(next) {
            list.select_row(Some(&row));
            // Scroll without taking the keyboard focus from the entry.
            if let Some(scroller) = row
                .ancestor(gtk::ScrolledWindow::static_type())
                .and_downcast::<gtk::ScrolledWindow>()
                && let Some(bounds) = row.compute_bounds(list)
            {
                let adjustment = scroller.vadjustment();
                let (top, bottom) = (
                    f64::from(bounds.y()),
                    f64::from(bounds.y() + bounds.height()),
                );
                if top < adjustment.value() {
                    adjustment.set_value(top);
                } else if bottom > adjustment.value() + adjustment.page_size() {
                    adjustment.set_value(bottom - adjustment.page_size());
                }
            }
        }
    }

    /// Takes a completion: the entry becomes that directory, ready for the
    /// next level.
    fn accept(&self, name: &str) {
        let dir_text = self.0.completing.borrow().0.clone();
        self.0.entry.set_text(&format!("{dir_text}{name}/"));
        self.0.entry.set_position(-1);
    }

    /// Tab: extend the text to what every match has in common.
    fn complete_common_prefix(&self) {
        let (dir_text, names) = self.0.completing.borrow().clone();
        match names.as_slice() {
            [] => {}
            [only] => self.accept(only),
            _ => {
                let common = common_prefix(&names);
                let typed = self.0.entry.text();
                let wanted = format!("{dir_text}{common}");
                if wanted.len() > typed.len() {
                    self.0.entry.set_text(&wanted);
                    self.0.entry.set_position(-1);
                }
            }
        }
    }

    // ---- breadcrumbs ----

    fn connect_crumbs(&self) {
        let inner = &self.0;
        let adjustment = inner.crumbs_scroller.hadjustment();
        for (button, direction) in [(&inner.earlier, -1.0), (&inner.later, 1.0)] {
            let adjustment = adjustment.clone();
            button.connect_clicked(move |_| {
                let step = adjustment.page_size() * 0.6 * direction;
                adjustment.set_value(adjustment.value() + step);
            });
        }
        let weak = self.weak();
        let update = move |adjustment: &gtk::Adjustment| {
            let Some(inner) = weak.upgrade() else { return };
            let overflow = adjustment.upper() > adjustment.page_size() + 1.0;
            inner.earlier.set_visible(overflow);
            inner.later.set_visible(overflow);
            inner.earlier.set_sensitive(adjustment.value() > 0.5);
            inner.later.set_sensitive(
                adjustment.value() + adjustment.page_size() < adjustment.upper() - 0.5,
            );
        };
        adjustment.connect_changed(update.clone());
        adjustment.connect_value_changed(update);
    }

    fn rebuild_crumbs(&self) {
        let inner = &self.0;
        let Some(current) = inner.location.borrow().clone() else {
            return;
        };
        let here = current.breadcrumbs();
        let on_branch = inner.deepest.borrow().as_ref().is_some_and(|deepest| {
            let all = deepest.breadcrumbs();
            all.len() >= here.len() && all[..here.len()] == here[..]
        });
        if !on_branch {
            inner.deepest.replace(Some(current.clone()));
        }
        let segments = inner
            .deepest
            .borrow()
            .as_ref()
            .map(Location::breadcrumbs)
            .unwrap_or_default();

        while let Some(child) = inner.crumbs.first_child() {
            inner.crumbs.remove(&child);
        }
        let mut active = None;
        for segment in segments {
            let button = gtk::ToggleButton::with_label(&segment.label);
            button.set_focus_on_click(false);
            let is_current = segment.location == current;
            button.set_active(is_current);
            if is_current {
                active = Some(button.clone());
            }
            let weak = self.weak();
            let target = segment.location.clone();
            button.connect_clicked(move |button| {
                // A click toggles; the state belongs to the location instead.
                button.set_active(is_current);
                if let Some(inner) = weak.upgrade() {
                    (inner.emit)(PathEvent::Navigate(target.clone()));
                }
            });
            let middle = gtk::GestureClick::new();
            middle.set_button(gdk::BUTTON_MIDDLE);
            let weak = self.weak();
            let target = segment.location;
            middle.connect_released(move |_, _, _, _| {
                if let Some(inner) = weak.upgrade() {
                    (inner.emit)(PathEvent::NewTab(target.clone()));
                }
            });
            button.add_controller(middle);
            inner.crumbs.append(&button);
        }

        // Bring the current segment into view once the buttons have sizes.
        if let Some(active) = active {
            let scroller = inner.crumbs_scroller.clone();
            let crumbs = inner.crumbs.clone();
            glib::idle_add_local_once(move || {
                let Some(bounds) = active.compute_bounds(&crumbs) else {
                    return;
                };
                let adjustment = scroller.hadjustment();
                let (start, end) = (
                    f64::from(bounds.x()),
                    f64::from(bounds.x() + bounds.width()),
                );
                if end > adjustment.value() + adjustment.page_size() {
                    adjustment.set_value(end - adjustment.page_size());
                } else if start < adjustment.value() {
                    adjustment.set_value(start);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::common_prefix;

    #[test]
    fn common_prefix_of_names() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(common_prefix(&names(&["Documents", "Downloads"])), "Do");
        assert_eq!(common_prefix(&names(&["level-00"])), "level-00");
        assert_eq!(common_prefix(&names(&["abc", "xyz"])), "");
        assert_eq!(common_prefix(&names(&["éclair", "école"])), "éc");
        assert_eq!(common_prefix(&names(&[])), "");
    }
}
