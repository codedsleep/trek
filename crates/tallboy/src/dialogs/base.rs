//! The shape every custom dialog shares (PROMPT §10): transient for and
//! modal to its parent, Esc cancels, Enter activates the default button.

use gtk::{gdk, glib, prelude::*};

/// A modal window with a content area and a row of buttons.
pub struct Dialog {
    window: gtk::Window,
    content: gtk::Box,
    buttons: gtk::Box,
}

/// How a dialog button behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// An ordinary button.
    Plain,
    /// Activated by Enter.
    Default,
}

impl Dialog {
    /// `parent` is `None` only when a file is opened with no window open.
    pub fn new(parent: Option<&gtk::Window>, title: &str) -> Self {
        let window = gtk::Window::builder()
            .title(title)
            .modal(true)
            .resizable(false)
            .build();
        window.set_transient_for(parent);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 16);
        outer.add_css_class("dialog-body");
        outer.append(&content);
        outer.append(&buttons);
        window.set_child(Some(&outer));

        let keys = gtk::EventControllerKey::new();
        let weak = window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(window) = weak.upgrade()
            {
                window.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(keys);
        Self {
            window,
            content,
            buttons,
        }
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    /// The area above the buttons.
    pub fn content(&self) -> &gtk::Box {
        &self.content
    }

    /// Appends a button to the row at the bottom.
    pub fn button(&self, label: &str, role: Role) -> gtk::Button {
        let button = gtk::Button::with_mnemonic(label);
        self.buttons.append(&button);
        if role == Role::Default {
            self.window.set_default_widget(Some(&button));
        }
        button
    }

    /// A Cancel button that closes the dialog.
    pub fn cancel_button(&self) -> gtk::Button {
        let button = self.button("_Cancel", Role::Plain);
        let weak = self.window.downgrade();
        button.connect_clicked(move |_| {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        });
        button
    }

    pub fn present(&self) {
        self.window.present();
    }
}

/// A label that reads as the caption of the field below it.
pub fn caption(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_max_width_chars(60);
    label
}

/// Shows a message with one OK button.
pub fn alert(parent: Option<&gtk::Window>, message: &str, detail: &str) {
    gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .build()
        .show(parent);
}

/// Asks a question with several answers; `answer` receives the index of the
/// chosen button, or `cancel` when the dialog is dismissed.
pub fn choose(
    parent: Option<&gtk::Window>,
    message: &str,
    detail: &str,
    buttons: &[&str],
    cancel: usize,
    default: usize,
    answer: impl FnOnce(usize) + 'static,
) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .buttons(buttons)
        .cancel_button(cancel as i32)
        .default_button(default as i32)
        .build();
    dialog.choose(parent, gtk::gio::Cancellable::NONE, move |choice| {
        answer(choice.map_or(cancel, |index| index.max(0) as usize));
    });
}

/// Asks a question with Cancel and one action button; `answer` receives
/// whether the action was chosen. Cancel is what Esc and Enter both give.
pub fn confirm(
    parent: Option<&gtk::Window>,
    message: &str,
    detail: &str,
    action: &str,
    answer: impl FnOnce(bool) + 'static,
) {
    choose(
        parent,
        message,
        detail,
        &["Cancel", action],
        0,
        0,
        move |index| answer(index == 1),
    );
}

/// Asks for one line of text. `accept` returns an error message to keep the
/// dialog open, or `None` once it has taken the value.
pub fn prompt(
    parent: Option<&gtk::Window>,
    title: &str,
    label: &str,
    initial: &str,
    select_chars: Option<std::ops::Range<i32>>,
    accept: impl Fn(&str) -> Option<String> + 'static,
) {
    let dialog = Dialog::new(parent, title);
    let entry = gtk::Entry::new();
    entry.set_text(initial);
    entry.set_activates_default(true);
    entry.set_width_chars(40);
    let problem = gtk::Label::new(None);
    problem.set_xalign(0.0);
    problem.add_css_class("error");
    problem.set_visible(false);
    dialog.content().append(&caption(label));
    dialog.content().append(&entry);
    dialog.content().append(&problem);
    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let window = dialog.window().downgrade();
    let field = entry.clone();
    ok.connect_clicked(move |_| match accept(&field.text()) {
        Some(message) => {
            problem.set_text(&message);
            problem.set_visible(true);
        }
        None => {
            if let Some(window) = window.upgrade() {
                window.close();
            }
        }
    });
    dialog.present();
    entry.grab_focus();
    match select_chars {
        Some(range) => entry.select_region(range.start, range.end),
        None => entry.select_region(0, -1),
    }
}
