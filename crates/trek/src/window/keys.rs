//! Keys of the file view: turning key events into the sequences of
//! `core::keymap` and running what they resolve to.
//!
//! One controller on the window sees every key first. It acts only while the
//! files have the keyboard: with the keyboard in a text entry, a menu or the
//! side pane, keys go where they always went. A started sequence (`g`, `m`,
//! `c`, `,`, `t`) lists its continuations and waits; any key that does not
//! continue it, Esc included, drops it.

use super::MainWindow;
use gtk::{gdk, glib, prelude::*};
use trek_core::keymap::{Code, Key, Match, Named};

/// Keys beside the vim motions that a file manager with a window is
/// expected to have. They name window actions, and like the keymap they only apply
/// while the files have the keyboard.
const EXTRAS: &[(&str, &str)] = &[
    ("<Backspace>", "win.up"),
    ("<Delete>", "win.trash"),
    ("<S-Delete>", "win.delete"),
    ("<C-Enter>", "win.open-new-tab"),
];

/// A key event in the keymap's terms; `None` for modifier keys and for keys
/// the keymap has no name for.
fn key_of(key: gdk::Key, state: gdk::ModifierType) -> Option<Key> {
    if state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
        return None;
    }
    let named = match key {
        gdk::Key::Escape => Some(Named::Esc),
        gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => Some(Named::Enter),
        gdk::Key::space | gdk::Key::KP_Space => Some(Named::Space),
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab | gdk::Key::KP_Tab => Some(Named::Tab),
        gdk::Key::BackSpace => Some(Named::Backspace),
        gdk::Key::Delete | gdk::Key::KP_Delete => Some(Named::Delete),
        gdk::Key::Up | gdk::Key::KP_Up => Some(Named::Up),
        gdk::Key::Down | gdk::Key::KP_Down => Some(Named::Down),
        gdk::Key::Left | gdk::Key::KP_Left => Some(Named::Left),
        gdk::Key::Right | gdk::Key::KP_Right => Some(Named::Right),
        gdk::Key::Home | gdk::Key::KP_Home => Some(Named::Home),
        gdk::Key::End | gdk::Key::KP_End => Some(Named::End),
        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => Some(Named::PageUp),
        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => Some(Named::PageDown),
        gdk::Key::F1 => Some(Named::F1),
        _ => None,
    };
    let code = match named {
        Some(named) => Code::Named(named),
        None => Code::Char(key.to_unicode().filter(|c| !c.is_control())?),
    };
    Some(Key {
        ctrl: state.contains(gdk::ModifierType::CONTROL_MASK),
        alt: state.contains(gdk::ModifierType::ALT_MASK),
        // A character already is its shifted self.
        shift: named.is_some() && state.contains(gdk::ModifierType::SHIFT_MASK),
        code,
    })
}

impl MainWindow {
    pub(super) fn watch_keys(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, state| {
            let handled = weak
                .upgrade()
                .is_some_and(|window| window.key_pressed(key, state));
            if handled {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.0.window.add_controller(keys);
    }

    /// Whether keys are commands right now: the keyboard is not in a text
    /// entry, a menu, the side pane or the toolbar. An empty folder has no
    /// item to hold the keyboard, which then rests on some container or on
    /// nothing at all; the keys are commands all the same.
    fn files_have_keyboard(&self) -> bool {
        let Some(focus) = gtk::prelude::GtkWindowExt::focus(&self.0.window) else {
            return true;
        };
        if focus.is::<gtk::Text>() || focus.is::<gtk::TextView>() {
            return false;
        }
        if focus.ancestor(gtk::Popover::static_type()).is_some() {
            return false;
        }
        let within = |widget: &gtk::Widget| &focus == widget || focus.is_ancestor(widget);
        !within(self.0.side.widget().upcast_ref()) && !within(self.0.toolbar.widget().upcast_ref())
    }

    fn key_pressed(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        let inner = &self.0;
        // A question takes every key until it is answered.
        if inner.confirm.is_open() {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::y | gdk::Key::Y | gdk::Key::Return | gdk::Key::KP_Enter => {
                    inner.confirm.answer(true);
                }
                gdk::Key::n | gdk::Key::N | gdk::Key::Escape => inner.confirm.answer(false),
                gdk::Key::c | gdk::Key::bracketleft if ctrl => inner.confirm.answer(false),
                _ => {}
            }
            return true;
        }
        if !self.files_have_keyboard() {
            return false;
        }
        let Some(key) = key_of(key, state) else {
            return false;
        };
        let pending = inner.pending.borrow().clone();
        let keymap = inner.app.keymap();
        match keymap.feed(&pending, key) {
            Match::Run(binding) => {
                self.clear_pending();
                self.vim(binding.action);
                true
            }
            Match::Pending(next) => {
                inner.pending.borrow_mut().push(key);
                let typed: Vec<String> = inner
                    .pending
                    .borrow()
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                inner.status.set_pending(&typed.join(" "));
                inner.which_key.show(typed.len(), &next);
                true
            }
            Match::None if !pending.is_empty() => {
                self.clear_pending();
                true
            }
            Match::None => {
                let extra = EXTRAS
                    .iter()
                    .find(|(keys, _)| Key::parse(keys) == Some(key));
                match extra {
                    Some((_, action)) => {
                        let _ = WidgetExt::activate_action(&inner.window, action, None);
                        true
                    }
                    None => false,
                }
            }
        }
    }

    pub(super) fn clear_pending(&self) {
        self.0.pending.borrow_mut().clear();
        self.0.status.set_pending("");
        self.0.which_key.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions;
    use trek_core::keymap::Keymap;

    #[test]
    fn events_become_keys() {
        let none = gdk::ModifierType::empty();
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        let shift = gdk::ModifierType::SHIFT_MASK;
        let parse = |text: &str| Key::parse(text);
        assert_eq!(key_of(gdk::Key::j, none), parse("j"));
        assert_eq!(key_of(gdk::Key::G, shift), parse("G"));
        assert_eq!(key_of(gdk::Key::u, ctrl), parse("<C-u>"));
        assert_eq!(key_of(gdk::Key::minus, ctrl), parse("<C-->"));
        assert_eq!(key_of(gdk::Key::bracketleft, ctrl), parse("<C-[>"));
        assert_eq!(key_of(gdk::Key::space, none), parse("<Space>"));
        assert_eq!(key_of(gdk::Key::Return, shift), parse("<S-Enter>"));
        assert_eq!(key_of(gdk::Key::Page_Up, shift), parse("<S-PageUp>"));
        assert_eq!(key_of(gdk::Key::asciitilde, shift), parse("~"));
        assert_eq!(key_of(gdk::Key::question, shift), parse("?"));
        assert_eq!(key_of(gdk::Key::Shift_L, none), None);
        assert_eq!(key_of(gdk::Key::Control_L, none), None);
        assert_eq!(key_of(gdk::Key::j, gdk::ModifierType::SUPER_MASK), None);
    }

    /// A key that is both a binding and a window accelerator would do two
    /// things at once, or whichever GTK happened to ask first.
    #[test]
    fn bindings_and_accelerators_do_not_overlap() {
        let mut accelerators = Vec::new();
        for def in actions::TABLE {
            accelerators.extend(def.accels.iter().copied());
        }
        for (_, accels) in actions::TARGETED {
            accelerators.extend(accels.iter().copied());
        }
        let keymap = Keymap::standard();
        let mut taken: Vec<Key> = keymap
            .bindings()
            .iter()
            .filter(|binding| binding.keys.len() == 1)
            .map(|binding| binding.keys[0])
            .collect();
        taken.extend(EXTRAS.iter().filter_map(|(keys, _)| Key::parse(keys)));
        for accelerator in accelerators {
            // `gtk::accelerator_parse` wants GTK initialised, which a unit
            // test does not have: the three modifiers used are read here.
            let mut name = accelerator;
            let mut state = gdk::ModifierType::empty();
            for (prefix, modifier) in [
                ("<Control>", gdk::ModifierType::CONTROL_MASK),
                ("<Shift>", gdk::ModifierType::SHIFT_MASK),
                ("<Alt>", gdk::ModifierType::ALT_MASK),
            ] {
                if let Some(rest) = name.strip_prefix(prefix) {
                    name = rest;
                    state |= modifier;
                }
            }
            let key = gdk::Key::from_name(name)
                .unwrap_or_else(|| panic!("{accelerator} is not an accelerator"));
            // With Shift held a letter arrives as its capital.
            let key = if state.contains(gdk::ModifierType::SHIFT_MASK) {
                key.to_upper()
            } else {
                key
            };
            if let Some(key) = key_of(key, state) {
                assert!(
                    !taken.contains(&key),
                    "{accelerator} is also bound as {key}"
                );
            }
        }
        for (keys, action) in EXTRAS {
            let key = Key::parse(keys).expect(keys);
            assert_eq!(keymap.feed(&[], key), Match::None, "{keys}");
            let name = action.trim_start_matches("win.");
            assert!(
                actions::TABLE.iter().any(|def| def.name == name),
                "{action}"
            );
        }
    }
}
