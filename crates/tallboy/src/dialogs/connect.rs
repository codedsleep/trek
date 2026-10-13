//! Go ▸ Connect to Server… (PROMPT §8.17). The dialog only builds a URI
//! through `core::location::Server`; GVFS asks for credentials when the
//! location is opened.

use super::base::{Dialog, Role, caption};
use gtk::prelude::*;
use tallboy_core::location::{Location, Server};

/// (label, URI scheme, anonymous only) — the six kinds upstream offers.
const KINDS: [(&str, &str, bool); 6] = [
    ("SSH", "sftp", false),
    ("FTP (with login)", "ftp", false),
    ("Public FTP", "ftp", true),
    ("WebDAV", "dav", false),
    ("Secure WebDAV", "davs", false),
    ("Windows share", "smb", false),
];

/// Builds the location for the dialog's fields, or says what is wrong.
pub fn build(
    kind: usize,
    host: &str,
    port: u16,
    path: &str,
    user: Option<&str>,
) -> Result<Location, String> {
    let (_, scheme, anonymous_only) = KINDS.get(kind).ok_or("Choose a type of server.")?;
    let host = host.trim();
    if host.is_empty() {
        return Err("Enter the name or address of the server.".into());
    }
    let user = if *anonymous_only {
        ""
    } else {
        user.map(str::trim).unwrap_or_default()
    };
    Server {
        scheme: (*scheme).into(),
        host: host.into(),
        port: (port != 0).then_some(port),
        user: user.into(),
        path: path.trim().into(),
    }
    .location()
    .map_err(|_| "The server name is not valid.".to_owned())
}

/// Shows the dialog; `connect` receives the location to open.
pub fn show(parent: &gtk::Window, connect: impl Fn(Location) + 'static) {
    let dialog = Dialog::new(Some(parent), "Connect to remote server");
    dialog
        .content()
        .append(&caption("Specify remote folder to connect"));
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    let label = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.0);
        label
    };
    let labels: Vec<&str> = KINDS.iter().map(|(label, _, _)| *label).collect();
    let kind = gtk::DropDown::from_strings(&labels);
    let host = gtk::Entry::new();
    host.set_activates_default(true);
    host.set_hexpand(true);
    host.set_width_chars(30);
    // 0 means "the usual port for this kind of server".
    let port = gtk::SpinButton::with_range(0.0, 65535.0, 1.0);
    port.set_tooltip_text(Some("0 uses the default port"));
    let path = gtk::Entry::new();
    path.set_text("/");
    path.set_activates_default(true);
    grid.attach(&label("Type:"), 0, 0, 1, 1);
    grid.attach(&kind, 1, 0, 1, 1);
    grid.attach(&label("Host:"), 0, 1, 1, 1);
    grid.attach(&host, 1, 1, 1, 1);
    grid.attach(&label("Port:"), 0, 2, 1, 1);
    grid.attach(&port, 1, 2, 1, 1);
    grid.attach(&label("Path:"), 0, 3, 1, 1);
    grid.attach(&path, 1, 3, 1, 1);
    dialog.content().append(&grid);

    let anonymous = gtk::CheckButton::with_mnemonic("_Anonymous login");
    let login = gtk::CheckButton::with_mnemonic("_Login as:");
    login.set_group(Some(&anonymous));
    anonymous.set_active(true);
    let user = gtk::Entry::new();
    user.set_activates_default(true);
    user.set_hexpand(true);
    user.set_sensitive(false);
    let login_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    login_row.append(&login);
    login_row.append(&user);
    dialog.content().append(&anonymous);
    dialog.content().append(&login_row);
    let weak_user = user.downgrade();
    login.connect_toggled(move |login| {
        if let Some(user) = weak_user.upgrade() {
            user.set_sensitive(login.is_active());
        }
    });
    // Public FTP has no login to offer.
    let (weak_login, weak_anonymous) = (login.downgrade(), anonymous.downgrade());
    kind.connect_selected_notify(move |kind| {
        let anonymous_only = KINDS
            .get(kind.selected() as usize)
            .is_some_and(|(_, _, only)| *only);
        if let (Some(login), Some(anonymous)) = (weak_login.upgrade(), weak_anonymous.upgrade()) {
            if anonymous_only {
                anonymous.set_active(true);
            }
            login.set_sensitive(!anonymous_only);
        }
    });

    let problem = gtk::Label::new(None);
    problem.set_xalign(0.0);
    problem.add_css_class("error");
    problem.set_visible(false);
    dialog.content().append(&problem);
    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let window = dialog.window().downgrade();
    let first = host.clone();
    ok.connect_clicked(move |_| {
        let login_name = login.is_active().then(|| user.text().to_string());
        match build(
            kind.selected() as usize,
            &host.text(),
            port.value_as_int().clamp(0, 65535) as u16,
            &path.text(),
            login_name.as_deref(),
        ) {
            Ok(location) => {
                if let Some(window) = window.upgrade() {
                    window.close();
                }
                connect(location);
            }
            Err(message) => {
                problem.set_text(&message);
                problem.set_visible(true);
            }
        }
    });
    dialog.present();
    first.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::build;

    fn uri(kind: usize, host: &str, port: u16, path: &str, user: Option<&str>) -> String {
        build(kind, host, port, path, user)
            .map(|location| location.to_uri().to_owned())
            .unwrap_or_else(|e| format!("error: {e}"))
    }

    #[test]
    fn each_kind_builds_its_uri() {
        assert_eq!(
            uri(0, "host", 0, "/home/me", Some("me")),
            "sftp://me@host/home/me"
        );
        assert_eq!(uri(0, "host", 2222, "/", None), "sftp://host:2222/");
        assert_eq!(
            uri(1, "ftp.example", 0, "/pub", Some("me")),
            "ftp://me@ftp.example/pub"
        );
        // Public FTP never carries a user name, whatever was typed.
        assert_eq!(
            uri(2, "ftp.example", 0, "/pub", Some("me")),
            "ftp://ftp.example/pub"
        );
        assert_eq!(
            uri(3, "dav.example", 8080, "/files", None),
            "dav://dav.example:8080/files"
        );
        assert_eq!(
            uri(4, "dav.example", 0, "/", Some("a b")),
            "davs://a%20b@dav.example/"
        );
        assert_eq!(
            uri(5, "nas", 0, "/share/docs", None),
            "smb://nas/share/docs"
        );
    }

    #[test]
    fn bad_input_is_explained() {
        assert!(uri(0, "  ", 0, "/", None).starts_with("error: Enter"));
        assert!(uri(0, "a/b", 0, "/", None).starts_with("error: The server name"));
        assert!(uri(9, "host", 0, "/", None).starts_with("error: Choose"));
    }
}
