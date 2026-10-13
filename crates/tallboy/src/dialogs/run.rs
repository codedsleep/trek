//! Tools ▸ Run a Command in Current Folder… (PROMPT §8.9).

use super::base::{Dialog, Role, caption};
use gtk::prelude::*;

/// What the user asked to run.
pub struct Request {
    pub command: String,
    pub in_terminal: bool,
    pub keep_open: bool,
}

/// Shows the dialog. `run` returns an error message to keep it open.
pub fn show(parent: &gtk::Window, run: impl Fn(Request) -> Option<String> + 'static) {
    let dialog = Dialog::new(Some(parent), "Run a command");
    let command = gtk::Entry::new();
    command.set_activates_default(true);
    command.set_width_chars(44);
    let in_terminal = gtk::CheckButton::with_label("Execute in terminal emulator");
    let keep_open =
        gtk::CheckButton::with_label("Keep terminal window open after command execution");
    keep_open.set_sensitive(false);
    let dependent = keep_open.downgrade();
    in_terminal.connect_toggled(move |toggle| {
        if let Some(keep_open) = dependent.upgrade() {
            keep_open.set_sensitive(toggle.is_active());
        }
    });
    let problem = gtk::Label::new(None);
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_max_width_chars(50);
    problem.add_css_class("error");
    problem.set_visible(false);
    let content = dialog.content();
    content.append(&caption("Enter a command to run:"));
    content.append(&command);
    content.append(&in_terminal);
    content.append(&keep_open);
    content.append(&problem);
    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let window = dialog.window().downgrade();
    let entry = command.clone();
    ok.connect_clicked(move |_| {
        let request = Request {
            command: entry.text().to_string(),
            in_terminal: in_terminal.is_active(),
            keep_open: in_terminal.is_active() && keep_open.is_active(),
        };
        match run(request) {
            Some(message) => {
                problem.set_text(&message);
                problem.set_visible(true);
            }
            None => {
                if let Some(window) = window.upgrade() {
                    window.close();
                }
            }
        }
    });
    dialog.present();
    command.grab_focus();
}
