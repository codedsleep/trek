//! Keys that need a focused item rather than the window: the context menu
//! opens at it. Every other key of the file view is in `core::keymap` and
//! `window::keys`.
//!
//! These are bound on the view widget itself, in the bubble phase, so they
//! only fire while the view has keyboard focus.

use gtk::prelude::*;

/// (key, action) pairs activated on the focused folder view.
const KEYS: &[(&str, &str)] = &[
    ("Menu", "win.context-menu"),
    ("<Shift>F10", "win.context-menu"),
];

/// Attaches the view-local shortcuts to `view`.
pub fn install(view: &impl IsA<gtk::Widget>) {
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Local);
    for (key, action) in KEYS {
        if let Some(trigger) = gtk::ShortcutTrigger::parse_string(key) {
            controller.add_shortcut(gtk::Shortcut::new(
                Some(trigger),
                Some(gtk::NamedAction::new(action)),
            ));
        }
    }
    view.add_controller(controller);
}
