//! What floats over the folder area while the keyboard is busy with
//! something other than a command: the which-key list for a started key
//! sequence, the input line, and the yes/no question.
//!
//! None of them is a window. They are overlay children of the folder area,
//! so the files stay visible behind them and nothing has to be raised,
//! focused or placed by the compositor.

use gtk::{gdk, glib, pango, prelude::*};
use std::{cell::RefCell, rc::Rc};
use trek_core::keymap::Binding;

// ---- which-key ----

/// Lists the bindings a started key sequence can still become.
pub struct WhichKey {
    root: gtk::Box,
    grid: gtk::Grid,
}

impl WhichKey {
    const COLUMNS: usize = 3;

    pub fn new() -> Self {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(28);
        grid.set_row_spacing(2);
        grid.set_column_homogeneous(true);
        grid.set_hexpand(true);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("which-key");
        root.add_css_class("floating");
        root.set_valign(gtk::Align::End);
        root.set_halign(gtk::Align::Fill);
        root.set_can_target(false);
        root.set_visible(false);
        root.append(&grid);
        Self { root, grid }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    /// Shows the continuations of a sequence `typed` keys long.
    pub fn show(&self, typed: usize, bindings: &[&Binding]) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        let rows = bindings.len().div_ceil(Self::COLUMNS).max(1);
        for (index, binding) in bindings.iter().enumerate() {
            let rest: Vec<String> = binding.keys[typed..]
                .iter()
                .map(ToString::to_string)
                .collect();
            let key = gtk::Label::new(Some(&rest.join(" ")));
            key.add_css_class("which-key-key");
            key.set_xalign(1.0);
            key.set_width_chars(8);
            let desc = gtk::Label::new(Some(binding.desc));
            desc.set_xalign(0.0);
            desc.set_hexpand(true);
            desc.set_ellipsize(pango::EllipsizeMode::End);
            let entry = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            entry.append(&key);
            entry.append(&desc);
            // Filled column by column.
            self.grid
                .attach(&entry, (index / rows) as i32, (index % rows) as i32, 1, 1);
        }
        self.root.set_visible(true);
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
    }
}

// ---- what every entry over the folder area shares ----

/// Esc, Ctrl+C and Ctrl+[: the keys that close an entry without doing
/// anything, as in vim's command line.
pub(super) fn is_cancel(key: gdk::Key, state: gdk::ModifierType) -> bool {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    let alt = state.contains(gdk::ModifierType::ALT_MASK);
    key == gdk::Key::Escape || (ctrl && !alt && matches!(key, gdk::Key::c | gdk::Key::bracketleft))
}

/// The editing keys of vim's insert mode that an entry does not
/// already have. Returns whether the key was one of them.
pub(super) fn edit_key(entry: &gtk::Entry, key: gdk::Key, state: gdk::ModifierType) -> bool {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    let alt = state.contains(gdk::ModifierType::ALT_MASK);
    let at = entry.position();
    let text = entry.text();
    let chars: Vec<char> = text.chars().collect();
    let length = chars.len() as i32;
    let word_back = || {
        let mut to = at.clamp(0, length) as usize;
        while to > 0 && !chars[to - 1].is_alphanumeric() {
            to -= 1;
        }
        while to > 0 && chars[to - 1].is_alphanumeric() {
            to -= 1;
        }
        to as i32
    };
    let word_forward = || {
        let mut to = at.clamp(0, length) as usize;
        while to < chars.len() && !chars[to].is_alphanumeric() {
            to += 1;
        }
        while to < chars.len() && chars[to].is_alphanumeric() {
            to += 1;
        }
        to as i32
    };
    match (key, ctrl, alt) {
        (gdk::Key::a, true, false) => entry.set_position(0),
        (gdk::Key::e, true, false) => entry.set_position(-1),
        (gdk::Key::b, true, false) => entry.set_position((at - 1).max(0)),
        (gdk::Key::f, true, false) => entry.set_position((at + 1).min(length)),
        (gdk::Key::b, false, true) => entry.set_position(word_back()),
        (gdk::Key::f, false, true) => entry.set_position(word_forward()),
        (gdk::Key::h, true, false) if at > 0 => entry.delete_text(at - 1, at),
        (gdk::Key::d, true, false) if at < length => entry.delete_text(at, at + 1),
        (gdk::Key::u, true, false) => entry.delete_text(0, at),
        (gdk::Key::k, true, false) => entry.delete_text(at, -1),
        (gdk::Key::w, true, false) => entry.delete_text(word_back(), at),
        (gdk::Key::d, false, true) => entry.delete_text(at, word_forward()),
        (gdk::Key::h | gdk::Key::d, true, false) => {}
        _ => return false,
    }
    true
}

/// Calls `gone` when the keyboard has left `entry` for another widget.
pub(super) fn when_keyboard_leaves(entry: &gtk::Entry, gone: impl Fn() + 'static) {
    let focus = gtk::EventControllerFocus::new();
    let gone = Rc::new(gone);
    let weak = entry.downgrade();
    focus.connect_leave(move |_| {
        // The window going to the background also "leaves", with the entry
        // still the window's focus widget: that is not the keyboard going
        // elsewhere.
        let (weak, gone) = (weak.clone(), gone.clone());
        glib::idle_add_local_once(move || {
            let Some(entry) = weak.upgrade() else { return };
            let still_here = entry
                .root()
                .and_then(|root| root.focus())
                .is_some_and(|focus| focus.is_ancestor(&entry));
            if !still_here {
                gone();
            }
        });
    });
    entry.add_controller(focus);
}

// ---- input line ----

/// Where the text cursor starts in an input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    /// After the text.
    End,
    /// At this character, as `rename --cursor=before_ext` wants.
    At(i32),
    /// After the text, with all of it selected so that typing replaces it.
    SelectAll,
}

/// Called with the text of an input line.
pub type Changed = Box<dyn Fn(&str)>;
/// Takes the text on Enter; an error message keeps the line open.
pub type Submit = Box<dyn Fn(&str) -> Option<String>>;

/// What an input line does with its text.
pub struct Session {
    /// Called for every edit; `/` and `f` act while typing.
    pub changed: Option<Changed>,
    /// Enter. An error message keeps the line open and is shown under it.
    pub submit: Submit,
    /// Esc, Ctrl+C or the keyboard going elsewhere.
    pub cancel: Option<Box<dyn Fn()>>,
}

struct InputInner {
    root: gtk::Box,
    title: gtk::Label,
    entry: gtk::Entry,
    problem: gtk::Label,
    session: RefCell<Option<Rc<Session>>>,
    /// Gives the keyboard back to the files.
    closed: RefCell<Option<Rc<dyn Fn()>>>,
}

/// One line of text asked for at the top of the folder area: the name of a
/// new file, a new name, a find or filter query, a shell command.
#[derive(Clone)]
pub struct InputLine(Rc<InputInner>);

impl InputLine {
    pub fn new() -> Self {
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let entry = gtk::Entry::new();
        entry.set_width_chars(48);
        let problem = gtk::Label::new(None);
        problem.set_xalign(0.0);
        problem.set_wrap(true);
        problem.set_max_width_chars(48);
        problem.add_css_class("error");
        problem.set_visible(false);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("input-line");
        root.add_css_class("floating");
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::Start);
        root.set_visible(false);
        root.append(&title);
        root.append(&entry);
        root.append(&problem);
        let this = Self(Rc::new(InputInner {
            root,
            title,
            entry,
            problem,
            session: RefCell::default(),
            closed: RefCell::default(),
        }));
        this.connect();
        this
    }

    fn connect(&self) {
        let entry = &self.0.entry;
        let weak = Rc::downgrade(&self.0);
        entry.connect_changed(move |entry| {
            let Some(inner) = weak.upgrade() else { return };
            inner.problem.set_visible(false);
            let session = inner.session.borrow().clone();
            if let Some(changed) = session.as_ref().and_then(|s| s.changed.as_ref()) {
                changed(&entry.text());
            }
        });
        let weak = Rc::downgrade(&self.0);
        entry.connect_activate(move |entry| {
            let Some(this) = weak.upgrade().map(Self) else {
                return;
            };
            let session = this.0.session.borrow().clone();
            let Some(session) = session else { return };
            match (session.submit)(&entry.text()) {
                Some(message) => {
                    this.0.problem.set_text(&message);
                    this.0.problem.set_visible(true);
                }
                None => this.finish(false),
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
        // Clicking elsewhere, or anything else that takes the keyboard away,
        // is a cancel: an input line nobody is typing into must not linger.
        let weak = Rc::downgrade(&self.0);
        when_keyboard_leaves(entry, move || {
            if let Some(this) = weak.upgrade().map(Self)
                && this.is_open()
            {
                this.finish(true);
            }
        });
    }

    /// Esc and its like close the line; the rest is editing.
    fn key(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        if is_cancel(key, state) {
            self.finish(true);
            return true;
        }
        edit_key(&self.0.entry, key, state)
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }

    /// Sets what gives the keyboard back to the files when a line closes.
    pub fn connect_closed(&self, closed: impl Fn() + 'static) {
        self.0.closed.replace(Some(Rc::new(closed)));
    }

    pub fn is_open(&self) -> bool {
        self.0.session.borrow().is_some()
    }

    /// The title of the open line, for agent checks.
    pub fn debug_state(&self) -> String {
        if self.is_open() {
            format!("{:?}:{:?}", self.0.title.text(), self.0.entry.text())
        } else {
            "none".into()
        }
    }

    /// Opens the line. A line that is already open is cancelled first.
    pub fn open(&self, title: &str, initial: &str, caret: Caret, session: Session) {
        if self.is_open() {
            self.finish(true);
        }
        let inner = &self.0;
        inner.title.set_text(title);
        inner.problem.set_visible(false);
        // The session is set after the text, so that filling the entry in
        // is not taken for typing.
        inner.entry.set_text(initial);
        inner.session.replace(Some(Rc::new(session)));
        inner.root.set_visible(true);
        inner.entry.grab_focus_without_selecting();
        match caret {
            Caret::End => inner.entry.set_position(-1),
            Caret::At(position) => inner.entry.set_position(position),
            Caret::SelectAll => inner.entry.select_region(0, -1),
        }
    }

    fn finish(&self, cancelled: bool) {
        let Some(session) = self.0.session.take() else {
            return;
        };
        self.0.root.set_visible(false);
        if cancelled && let Some(cancel) = &session.cancel {
            cancel();
        }
        let closed = self.0.closed.borrow().clone();
        if let Some(closed) = closed {
            closed();
        }
    }
}

// ---- confirm ----

type Answer = Box<dyn FnOnce()>;

struct ConfirmInner {
    root: gtk::Box,
    title: gtk::Label,
    body: gtk::Label,
    yes: RefCell<Option<Answer>>,
}

/// A yes/no question answered with `y` or Enter, and `n` or Esc.
#[derive(Clone)]
pub struct Confirm(Rc<ConfirmInner>);

impl Confirm {
    /// How many file names a question lists before it says "and N more".
    const LISTED: usize = 8;

    pub fn new() -> Self {
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_max_width_chars(60);
        title.add_css_class("heading");
        let body = gtk::Label::new(None);
        body.set_xalign(0.0);
        body.set_ellipsize(pango::EllipsizeMode::Middle);
        body.set_max_width_chars(60);
        let yes = gtk::Button::with_label("Yes  (y)");
        yes.add_css_class("suggested-action");
        yes.set_focus_on_click(false);
        yes.set_can_focus(false);
        let no = gtk::Button::with_label("No  (n)");
        no.set_focus_on_click(false);
        no.set_can_focus(false);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&no);
        buttons.append(&yes);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("confirm");
        root.add_css_class("floating");
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::Center);
        root.set_visible(false);
        root.append(&title);
        root.append(&body);
        root.append(&buttons);
        let this = Self(Rc::new(ConfirmInner {
            root,
            title,
            body,
            yes: RefCell::default(),
        }));
        let weak = Rc::downgrade(&this.0);
        yes.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.answer(true);
            }
        });
        let weak = Rc::downgrade(&this.0);
        no.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.answer(false);
            }
        });
        this
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }

    pub fn is_open(&self) -> bool {
        self.0.yes.borrow().is_some()
    }

    /// The question that is up, for agent checks.
    pub fn debug_state(&self) -> String {
        if self.is_open() {
            format!("{:?}", self.0.title.text())
        } else {
            "none".into()
        }
    }

    /// Asks `question` about `names`. A question that is still up is
    /// answered "no" first.
    pub fn ask(&self, question: &str, names: &[String], yes: impl FnOnce() + 'static) {
        self.answer(false);
        let inner = &self.0;
        inner.title.set_text(question);
        let mut lines: Vec<String> = names.iter().take(Self::LISTED).cloned().collect();
        if names.len() > Self::LISTED {
            lines.push(format!("… and {} more", names.len() - Self::LISTED));
        }
        inner.body.set_text(&lines.join("\n"));
        inner.body.set_visible(!lines.is_empty());
        inner.yes.replace(Some(Box::new(yes)));
        inner.root.set_visible(true);
    }

    /// Closes the question; `accepted` runs what was asked about.
    pub fn answer(&self, accepted: bool) {
        let Some(yes) = self.0.yes.take() else {
            return;
        };
        self.0.root.set_visible(false);
        if accepted {
            yes();
        }
    }
}
