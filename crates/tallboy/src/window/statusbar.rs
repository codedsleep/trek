//! The status bar at the bottom of the window (PROMPT §8.26).

use crate::tab::Status;
use gtk::{pango, prelude::*};

pub struct StatusBar {
    root: gtk::Box,
    left: gtk::Label,
    right: gtk::Label,
}

impl StatusBar {
    pub fn new() -> Self {
        let left = gtk::Label::new(None);
        left.set_xalign(0.0);
        left.set_hexpand(true);
        left.set_ellipsize(pango::EllipsizeMode::End);
        let right = gtk::Label::new(None);
        right.set_xalign(1.0);
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("statusbar");
        root.append(&left);
        root.append(&right);
        Self { root, left, right }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn set(&self, status: &Status) {
        self.left.set_text(&status.left);
        self.right.set_text(&status.right);
        self.right.set_visible(!status.right.is_empty());
    }
}
