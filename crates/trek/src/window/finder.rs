//! The fuzzy finder, `z`: a query, and under it the files below the folder
//! that match it, best first. Enter goes to the one under the cursor.
//!
//! Yazi hands this to fzf, which needs a terminal to draw in. Here the list
//! is drawn by the window and `fs::finder` does the walking and matching,
//! off the main loop. Like the input line it is an overlay child of the
//! folder area, not a window.

use super::{
    MainWindow,
    prompt::{edit_key, is_cancel, when_keyboard_leaves},
};
use crate::tab::TabPage;
use gtk::{gdk, glib, pango, prelude::*};
use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};
use trek_fs::finder::{self, Counts, Finder, Hit};

/// Rows shown at once. The list keeps this height whatever matches, so
/// that it does not jump about while a query is typed.
const ROWS: u32 = 15;

/// The least time between two redraws while files are still being listed.
const PACE: Duration = Duration::from_millis(30);

const HINT: &str =
    "Enter: go there    Esc: close    words in any order, 'exact, ^start, end$, !not";

struct Row {
    root: gtk::Box,
    icon: gtk::Image,
    label: gtk::Label,
}

struct Search {
    finder: Finder,
    /// The row under the cursor, counted from the best match.
    cursor: u32,
    /// The first row shown.
    top: u32,
    shown: Vec<Hit>,
    accept: Rc<dyn Fn(&Hit)>,
    /// Redraws as the walk and the matcher report.
    task: glib::JoinHandle<()>,
}

struct Inner {
    root: gtk::Box,
    entry: gtk::Entry,
    count: gtk::Label,
    rows: Vec<Row>,
    /// Never shown: it has the colour matched characters are drawn in, which
    /// is the theme's and so can only be asked of a widget.
    accent: gtk::Label,
    search: RefCell<Option<Search>>,
    /// Gives the keyboard back to the files.
    closed: RefCell<Option<Rc<dyn Fn()>>>,
}

/// The finder's query line and result list.
#[derive(Clone)]
pub struct Picker(Rc<Inner>);

impl Picker {
    pub fn new() -> Self {
        let title = gtk::Label::new(Some("Jump to:"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("heading");
        let count = gtk::Label::new(None);
        count.add_css_class("finder-note");
        count.add_css_class("numeric");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        head.append(&title);
        head.append(&count);
        let entry = gtk::Entry::new();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("finder-list");
        let rows: Vec<Row> = (0..ROWS)
            .map(|_| {
                let icon = gtk::Image::new();
                icon.set_pixel_size(16);
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_hexpand(true);
                // The end of a long path says more than its start.
                label.set_ellipsize(pango::EllipsizeMode::Start);
                let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                root.add_css_class("finder-row");
                root.append(&icon);
                root.append(&label);
                list.append(&root);
                Row { root, icon, label }
            })
            .collect();
        let hint = gtk::Label::new(Some(HINT));
        hint.set_xalign(0.0);
        hint.set_ellipsize(pango::EllipsizeMode::End);
        hint.add_css_class("finder-note");
        let accent = gtk::Label::new(None);
        accent.add_css_class("finder-match");
        accent.set_visible(false);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("finder");
        root.add_css_class("floating");
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::Start);
        root.set_size_request(680, -1);
        root.set_visible(false);
        root.append(&head);
        root.append(&entry);
        root.append(&list);
        root.append(&hint);
        root.append(&accent);
        let this = Self(Rc::new(Inner {
            root,
            entry,
            count,
            rows,
            accent,
            search: RefCell::default(),
            closed: RefCell::default(),
        }));
        this.connect(&list);
        this
    }

    fn connect(&self, list: &gtk::Box) {
        let entry = &self.0.entry;
        let weak = Rc::downgrade(&self.0);
        entry.connect_changed(move |entry| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.query_changed(&entry.text());
            }
        });
        let weak = Rc::downgrade(&self.0);
        entry.connect_activate(move |_| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.accept(None);
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&self.0);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(this) = weak.upgrade().map(Self) else {
                return glib::Propagation::Proceed;
            };
            if this.key(key, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        entry.add_controller(keys);
        let weak = Rc::downgrade(&self.0);
        when_keyboard_leaves(entry, move || {
            if let Some(this) = weak.upgrade().map(Self) {
                this.close();
            }
        });
        // The pointer: the wheel moves the cursor, a click goes there.
        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
        );
        let weak = Rc::downgrade(&self.0);
        scroll.connect_scroll(move |_, _, dy| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.step(if dy > 0.0 { 1 } else { -1 });
            }
            glib::Propagation::Stop
        });
        list.add_controller(scroll);
        for (index, row) in self.0.rows.iter().enumerate() {
            let click = gtk::GestureClick::new();
            click.set_button(gdk::BUTTON_PRIMARY);
            let weak = Rc::downgrade(&self.0);
            click.connect_released(move |_, _, _, _| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.accept(Some(index));
                }
            });
            row.root.add_controller(click);
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }

    /// Sets what gives the keyboard back to the files when the finder
    /// closes.
    pub fn connect_closed(&self, closed: impl Fn() + 'static) {
        self.0.closed.replace(Some(Rc::new(closed)));
    }

    /// The query, the counts and the row under the cursor, for agent
    /// checks. `settled` says the counts are final.
    pub fn debug_state(&self) -> String {
        match self.0.search.borrow().as_ref() {
            None => "none".into(),
            Some(search) => {
                let counts = search.finder.counts();
                let at = (search.cursor - search.top) as usize;
                format!(
                    "{:?}:{}/{}:{}:{:?}",
                    self.0.entry.text(),
                    counts.matched,
                    counts.total,
                    if search.finder.settled() {
                        "settled"
                    } else {
                        "busy"
                    },
                    search.shown.get(at).map_or("", |hit| hit.text.as_str()),
                )
            }
        }
    }

    /// Opens the finder on `folder`. `accept` is called with the row Enter
    /// was pressed on, after the finder has closed.
    pub fn open(&self, folder: &Path, options: finder::Options, accept: impl Fn(&Hit) + 'static) {
        self.close();
        let inner = &self.0;
        let finder = Finder::start(folder, options);
        let wake = finder.wake();
        let weak = Rc::downgrade(inner);
        let task = glib::spawn_future_local(async move {
            while wake.recv().await.is_ok() {
                let Some(this) = weak.upgrade().map(Self) else {
                    return;
                };
                this.tick();
                drop(this);
                glib::timeout_future(PACE).await;
            }
        });
        // The search is set after the text, so that emptying the entry is
        // not taken for a query.
        inner.entry.set_text("");
        inner.search.replace(Some(Search {
            finder,
            cursor: 0,
            top: 0,
            shown: Vec::new(),
            accept: Rc::new(accept),
            task,
        }));
        inner.root.set_visible(true);
        inner.entry.grab_focus();
        self.tick();
    }

    /// Closes the finder and stops its search.
    pub fn close(&self) {
        let Some(search) = self.0.search.take() else {
            return;
        };
        search.task.abort();
        drop(search);
        self.0.root.set_visible(false);
        let closed = self.0.closed.borrow().clone();
        if let Some(closed) = closed {
            closed();
        }
    }

    /// Enter, or a click on the row at `index`: closes the finder and
    /// hands over the row. Without a match Enter does nothing.
    fn accept(&self, index: Option<usize>) {
        let picked = {
            let search = self.0.search.borrow();
            let Some(search) = search.as_ref() else {
                return;
            };
            let index = index.unwrap_or((search.cursor - search.top) as usize);
            search
                .shown
                .get(index)
                .map(|hit| (hit.clone(), search.accept.clone()))
        };
        if let Some((hit, accept)) = picked {
            self.close();
            accept(&hit);
        }
    }

    fn key(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        if is_cancel(key, state) {
            self.close();
            return true;
        }
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = state.contains(gdk::ModifierType::ALT_MASK);
        // fzf's keys for the list come before the entry's for the text:
        // Ctrl+K is "up" here, not "delete to the end".
        let by = match (key, ctrl && !alt) {
            (gdk::Key::Down | gdk::Key::KP_Down | gdk::Key::Tab, false)
            | (gdk::Key::n | gdk::Key::j, true) => 1,
            (gdk::Key::Up | gdk::Key::KP_Up | gdk::Key::ISO_Left_Tab, false)
            | (gdk::Key::p | gdk::Key::k, true) => -1,
            (gdk::Key::Page_Down | gdk::Key::KP_Page_Down, false) => ROWS as i64,
            (gdk::Key::Page_Up | gdk::Key::KP_Page_Up, false) => -(ROWS as i64),
            _ => return edit_key(&self.0.entry, key, state),
        };
        self.step(by);
        true
    }

    /// Moves the cursor by `by` rows; it stops at either end.
    fn step(&self, by: i64) {
        if let Some(search) = self.0.search.borrow_mut().as_mut() {
            let last = i64::from(search.finder.counts().matched) - 1;
            search.cursor = (i64::from(search.cursor) + by).clamp(0, last.max(0)) as u32;
        }
        self.render();
    }

    fn query_changed(&self, query: &str) {
        if let Some(search) = self.0.search.borrow_mut().as_mut() {
            search.finder.set_query(query);
            // A new query is a new list, read from its top.
            search.cursor = 0;
            search.top = 0;
            search.finder.tick();
        }
        self.render();
    }

    fn tick(&self) {
        let changed = self
            .0
            .search
            .borrow_mut()
            .as_mut()
            .is_some_and(|search| search.finder.tick());
        if changed {
            self.render();
        }
    }

    fn render(&self) {
        let inner = &self.0;
        let mut search = inner.search.borrow_mut();
        let Some(search) = search.as_mut() else {
            return;
        };
        let counts = search.finder.counts();
        search.cursor = search.cursor.min(counts.matched.saturating_sub(1));
        if search.cursor < search.top {
            search.top = search.cursor;
        } else if search.cursor >= search.top + ROWS {
            search.top = search.cursor + 1 - ROWS;
        }
        search.top = search.top.min(counts.matched.saturating_sub(ROWS));
        search.shown = search.finder.hits(search.top, ROWS);
        inner.count.set_text(&count_text(counts));
        let accent = inner.accent.color();
        let current = (search.cursor - search.top) as usize;
        for (index, row) in inner.rows.iter().enumerate() {
            let is_current = index == current && !search.shown.is_empty();
            if is_current {
                row.root.add_css_class("current");
            } else {
                row.root.remove_css_class("current");
            }
            row.label.remove_css_class("finder-note");
            match search.shown.get(index) {
                Some(hit) => {
                    row.icon.set_icon_name(Some(if hit.dir {
                        "folder-symbolic"
                    } else {
                        "text-x-generic-symbolic"
                    }));
                    row.label.set_text(&hit.text);
                    // Under the cursor the row is filled with the colour
                    // the accent is made from: weight alone marks the match.
                    row.label
                        .set_attributes(Some(&marks(hit, (!is_current).then_some(accent))));
                }
                None => {
                    row.icon.set_icon_name(None);
                    row.label.set_attributes(None);
                    row.label.set_text(match index {
                        0 if counts.total == 0 && counts.walking => "Listing the folder…",
                        0 if counts.total == 0 => "Nothing to find here",
                        0 if counts.matched == 0 && counts.matching => "Matching…",
                        0 if counts.matched == 0 => "No match",
                        _ => "",
                    });
                    row.label.add_css_class("finder-note");
                }
            }
        }
    }
}

/// "12 of 3456", with what is still to come.
fn count_text(counts: Counts) -> String {
    let mut text = format!("{} of {}", counts.matched, counts.total);
    if counts.truncated {
        text.push_str(" (the first million)");
    } else if counts.walking {
        text.push_str(", listing…");
    } else if counts.matching {
        text.push_str(", matching…");
    }
    text
}

/// Bold, and `colour` where there is one, for what the query matched.
fn marks(hit: &Hit, colour: Option<gdk::RGBA>) -> pango::AttrList {
    let attrs = pango::AttrList::new();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0) as u16;
    for range in &hit.matched {
        let (start, end) = (range.start as u32, range.end as u32);
        let mut bold = pango::AttrInt::new_weight(pango::Weight::Bold);
        bold.set_start_index(start);
        bold.set_end_index(end);
        attrs.insert(bold);
        if let Some(colour) = colour {
            let mut tint = pango::AttrColor::new_foreground(
                channel(colour.red()),
                channel(colour.green()),
                channel(colour.blue()),
            );
            tint.set_start_index(start);
            tint.set_end_index(end);
            attrs.insert(tint);
        }
    }
    attrs
}

impl MainWindow {
    /// `z`: the fuzzy finder over everything below the folder shown. As
    /// Yazi's fzf plugin does, Enter enters a folder and goes to a file's
    /// folder with the cursor on the file.
    pub(super) fn ask_jump(&self, tab: &TabPage) {
        let Some(folder) = tab.location().path() else {
            self.notice("Only folders on this computer can be searched");
            return;
        };
        let options = finder::Options {
            hidden: tab.model().show_hidden(),
        };
        let weak = tab.downgrade();
        self.0.finder.open(&folder, options, move |hit| {
            if let Some(tab) = weak.upgrade() {
                tab.jump_to(&hit.path, hit.dir);
            }
        });
    }
}
