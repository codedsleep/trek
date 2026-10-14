//! The desktop's GTK theme, carried over to GTK 4.
//!
//! GTK loads a theme that has a `gtk-4.0` stylesheet by itself. One that was
//! written for GTK 3 only, as most are, leaves GTK 4 with its built-in look;
//! then the built-in stylesheet is recoloured from the palette the theme
//! names (`tallboy_core::palette`) and takes the built-in one's place.

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tallboy_core::palette::{self, NamedColours};
use tallboy_fs::terminal::HOST_PREFIX;

/// Names the desktop's theme for a session that cannot ask for it: the
/// isolated one of scripts/dev-run.sh.
const THEME_VARIABLE: &str = "TALLBOY_GTK_THEME";
/// Where GTK keeps its built-in theme.
const BUILTIN: &str = "/org/gtk/libgtk/theme/Default";
/// A theme of GTK's own with no rules at all.
const NO_THEME: &str = "Empty";
/// How deep `@import` is followed in a GTK 3 stylesheet.
const IMPORT_DEPTH: u32 = 4;

/// Dark is the standard look: the theme's dark variant is asked for,
/// whatever the desktop prefers. A theme that has no dark variant is used
/// as it is, since GTK would otherwise replace it with its built-in dark one.
pub fn follow_desktop(display: &gtk::gdk::Display) {
    let settings = gtk::Settings::for_display(display);
    settings.set_gtk_application_prefer_dark_theme(true);
    // GTK_THEME overrides the setting inside GTK; nothing here would show.
    if std::env::var_os("GTK_THEME").is_some() {
        return;
    }
    let named = std::env::var(THEME_VARIABLE)
        .ok()
        .filter(|name| !name.is_empty());
    let Some(name) = named
        .clone()
        .or_else(|| settings.gtk_theme_name().map(Into::into))
    else {
        return;
    };
    let Some(theme) = find(&name) else {
        return;
    };
    if let Some(has_dark_variant) = gtk4_part(&theme) {
        settings.set_gtk_application_prefer_dark_theme(has_dark_variant);
        if named.is_some() {
            settings.set_gtk_theme_name(Some(&name));
        }
        return;
    }
    let colours = NamedColours::parse(&gtk3_stylesheet(&theme));
    if colours.is_empty() {
        return;
    }
    match recoloured(colours.is_dark().unwrap_or(true), &colours) {
        Some(provider) => {
            log::info!("Theme {name} has no GTK 4 part; using its GTK 3 colours");
            // The empty theme has no dark variant either, and the stylesheet
            // is the right one already.
            settings.set_gtk_application_prefer_dark_theme(false);
            settings.set_gtk_theme_name(Some(NO_THEME));
            gtk::style_context_add_provider_for_display(
                display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_THEME,
            );
        }
        None => log::info!("Theme {name} has no GTK 4 part and no colours that can be read"),
    }
}

/// The theme's folder, looked for where GTK looks.
fn find(name: &str) -> Option<PathBuf> {
    let mut roots = vec![
        glib::user_data_dir().join("themes"),
        glib::home_dir().join(".themes"),
    ];
    // An isolated session has a data folder of its own; the theme is in the
    // one it was started from.
    match std::env::var_os(format!("{HOST_PREFIX}XDG_DATA_HOME")) {
        Some(host) if !host.is_empty() => roots.push(Path::new(&host).join("themes")),
        Some(_) => roots.push(glib::home_dir().join(".local/share/themes")),
        None => {}
    }
    roots.extend(
        glib::system_data_dirs()
            .into_iter()
            .map(|dir| dir.join("themes")),
    );
    roots
        .into_iter()
        .map(|root| root.join(name))
        .find(|theme| theme.is_dir())
}

/// Whether the theme has a GTK 4 stylesheet and, if so, a dark variant of it.
fn gtk4_part(theme: &Path) -> Option<bool> {
    let folders: Vec<PathBuf> = std::fs::read_dir(theme)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("gtk-4."))
        .map(|entry| entry.path())
        .collect();
    folders
        .iter()
        .any(|folder| folder.join("gtk.css").is_file())
        .then(|| {
            folders
                .iter()
                .any(|folder| folder.join("gtk-dark.css").is_file())
        })
}

/// The theme's GTK 3 stylesheet with what it imports from files beside it,
/// the dark variant when there is one.
fn gtk3_stylesheet(theme: &Path) -> String {
    let folder = theme.join("gtk-3.0");
    let mut text = String::new();
    for variant in ["gtk-dark.css", "gtk.css"] {
        read_with_imports(&folder.join(variant), IMPORT_DEPTH, &mut text);
        if !text.is_empty() {
            break;
        }
    }
    text
}

fn read_with_imports(file: &Path, depth: u32, text: &mut String) {
    let Ok(own) = std::fs::read_to_string(file) else {
        return;
    };
    if depth > 0 {
        for import in own.split("@import").skip(1) {
            let target = import.split(';').next().unwrap_or_default();
            let target = target
                .trim()
                .trim_start_matches("url(")
                .trim_end_matches(')');
            let target = target.trim().trim_matches(['"', '\'']);
            // Anything with a scheme (resource:, file:) is not beside the file.
            if let Some(folder) = file.parent().filter(|_| !target.contains(':')) {
                read_with_imports(&folder.join(target), depth - 1, text);
            }
        }
    }
    text.push_str(&own);
    text.push('\n');
}

/// GTK's built-in stylesheet in the theme's colours, or None when it cannot
/// be made or GTK does not take the result.
fn recoloured(dark: bool, colours: &NamedColours) -> Option<gtk::CssProvider> {
    let variant = if dark { "dark" } else { "light" };
    let builtin = gio::resources_lookup_data(
        &format!("{BUILTIN}/Default-{variant}.css"),
        gio::ResourceLookupFlags::NONE,
    )
    .ok()?;
    // The stylesheet names its pictures relative to itself.
    let css = palette::recolour(std::str::from_utf8(&builtin).ok()?, colours)?.replace(
        "url(\"assets/",
        &format!("url(\"resource://{BUILTIN}/assets/"),
    );
    let provider = gtk::CssProvider::new();
    let errors = Rc::new(Cell::new(0_u32));
    let handler = provider.connect_parsing_error({
        let errors = errors.clone();
        move |_, section, error| {
            // Deprecations are reported here too; the built-in theme has them.
            if !error.is::<gtk::CssParserWarning>() {
                log::debug!("Recoloured theme, {section}: {error}");
                errors.set(errors.get() + 1);
            }
        }
    });
    provider.load_from_string(&css);
    provider.disconnect(handler);
    if errors.get() > 0 {
        log::warn!(
            "The recoloured theme has {} errors and is not used",
            errors.get()
        );
        return None;
    }
    Some(provider)
}
