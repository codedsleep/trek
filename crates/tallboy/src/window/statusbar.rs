//! The status line at the bottom of the window.
//!
//! Left to right: the mode (`NOR`, `SEL`, `UNS`), what the cursor is on or
//! what is selected, then at the right the keys of a started sequence, the
//! filter, selection and yank counts, the cursor's position and the free
//! space. A notice — "3 files yanked", "No match" — takes the place of the
//! left text for a few seconds.

use crate::tab::{Mode, Status};
use gtk::{glib, pango, prelude::*};
use std::{cell::RefCell, rc::Rc, time::Duration};

const NOTICE: Duration = Duration::from_secs(4);

struct Inner {
    root: gtk::Box,
    mode: gtk::Label,
    left: gtk::Label,
    notice: gtk::Label,
    pending: gtk::Label,
    segments: gtk::Label,
    position: gtk::Label,
    right: gtk::Label,
    timer: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
pub struct StatusBar(Rc<Inner>);

fn label(class: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class(class);
    label
}

impl StatusBar {
    pub fn new() -> Self {
        let mode = label("mode-pill");
        mode.set_text(Mode::Normal.label());
        let left = label("status-left");
        left.set_xalign(0.0);
        left.set_hexpand(true);
        left.set_ellipsize(pango::EllipsizeMode::Middle);
        let notice = label("status-notice");
        notice.set_xalign(0.0);
        notice.set_hexpand(true);
        notice.set_ellipsize(pango::EllipsizeMode::End);
        notice.set_visible(false);
        let pending = label("status-pending");
        pending.set_visible(false);
        let segments = label("status-segments");
        let position = label("status-position");
        position.add_css_class("numeric");
        let right = label("status-right");
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("statusbar");
        for widget in [
            &mode, &left, &notice, &pending, &segments, &position, &right,
        ] {
            root.append(widget);
        }
        Self(Rc::new(Inner {
            root,
            mode,
            left,
            notice,
            pending,
            segments,
            position,
            right,
            timer: RefCell::default(),
        }))
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }

    pub fn set(&self, status: &Status) {
        let inner = &self.0;
        inner.mode.set_text(status.mode.label());
        for (mode, class) in [(Mode::Select, "select"), (Mode::Unset, "unset")] {
            if status.mode == mode {
                inner.mode.add_css_class(class);
            } else {
                inner.mode.remove_css_class(class);
            }
        }
        inner.left.set_text(&status.left);
        let segments = status.segments.join("  ·  ");
        inner.segments.set_text(&segments);
        inner.segments.set_visible(!segments.is_empty());
        inner.position.set_text(&status.position);
        inner.position.set_visible(!status.position.is_empty());
        inner.right.set_text(&status.right);
        inner.right.set_visible(!status.right.is_empty());
    }

    /// Shows the keys of a started sequence; an empty string clears them.
    pub fn set_pending(&self, keys: &str) {
        self.0.pending.set_text(keys);
        self.0.pending.set_visible(!keys.is_empty());
    }

    /// Shows `text` in place of the left half for a few seconds.
    pub fn notice(&self, text: &str) {
        let inner = &self.0;
        inner.notice.set_text(text);
        inner.notice.set_visible(true);
        inner.left.set_visible(false);
        let weak = Rc::downgrade(&self.0);
        let timer = glib::timeout_add_local_once(NOTICE, move || {
            if let Some(inner) = weak.upgrade() {
                inner.timer.replace(None);
                inner.notice.set_visible(false);
                inner.left.set_visible(true);
            }
        });
        if let Some(old) = inner.timer.replace(Some(timer)) {
            old.remove();
        }
    }

    /// The notice that is showing, for agent checks.
    pub fn debug_state(&self) -> String {
        if self.0.notice.is_visible() {
            self.0.notice.text().into()
        } else {
            String::new()
        }
    }
}
