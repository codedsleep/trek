//! Keys that belong to the folder view rather than the window (PROMPT §8.8).
//!
//! These are bound on the view widget itself, in the bubble phase, so they
//! only fire while the view has keyboard focus. A text entry anywhere else in
//! the window — the location bar, the type-ahead box, a dialog — never sees
//! Backspace or Delete turned into a file command.

use gtk::prelude::*;

/// (key, action) pairs activated on the focused folder view.
const KEYS: &[(&str, &str)] = &[
    ("BackSpace", "win.up"),
    ("Delete", "win.trash"),
    ("KP_Delete", "win.trash"),
    ("<Shift>Delete", "win.delete"),
    ("<Shift>KP_Delete", "win.delete"),
    ("Escape", "win.unselect-all"),
    ("Menu", "win.context-menu"),
    ("<Shift>F10", "win.context-menu"),
    ("<Control>Return", "win.open-new-tab"),
    ("<Control>KP_Enter", "win.open-new-tab"),
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
