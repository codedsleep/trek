//! The Open With dialog (PROMPT §8.9), hand-built because
//! `GtkAppChooserDialog` is deprecated.

use super::base::{Dialog, Role, caption};
use gtk::{gio, prelude::*};
use std::rc::Rc;
use trek_fs::apps;

/// Asks which application should open files of `content_type`. `chosen`
/// receives the application; it has already been made the default when the
/// user ticked that box.
pub fn show(
    parent: Option<&gtk::Window>,
    content_type: &str,
    chosen: impl Fn(gio::AppInfo) + 'static,
) {
    let dialog = Dialog::new(parent, "Choose Application");
    let description = gio::content_type_get_description(content_type);
    dialog.content().append(&caption(&format!(
        "Use selected application to open files of this type: {description}"
    )));

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search applications"));
    search.set_activates_default(true);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Browse);
    list.set_activate_on_single_click(false);
    // Recommended applications first, then every application a menu would
    // show; hidden helpers only appear when they are registered for the type.
    let registered = apps::all_for_type(content_type);
    let installed: Rc<Vec<gio::AppInfo>> = Rc::new(
        apps::applications(content_type)
            .into_iter()
            .filter(|app| app.should_show() || registered.iter().any(|r| r.equal(app)))
            .collect(),
    );
    for app in installed.iter() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::new();
        icon.set_pixel_size(24);
        if let Some(gicon) = app.icon() {
            icon.set_from_gicon(&gicon);
        }
        let name = gtk::Label::new(Some(&app.display_name()));
        name.set_xalign(0.0);
        row.append(&icon);
        row.append(&name);
        list.append(&row);
    }
    let names: Rc<Vec<String>> = Rc::new(
        installed
            .iter()
            .map(|app| app.display_name().to_lowercase())
            .collect(),
    );
    let needle = search.clone();
    let haystack = names.clone();
    list.set_filter_func(move |row| {
        let text = needle.text().to_lowercase();
        text.is_empty()
            || haystack
                .get(row.index() as usize)
                .is_some_and(|name| name.contains(&text))
    });
    let filtered = list.downgrade();
    let searched = names.clone();
    search.connect_search_changed(move |search| {
        let Some(list) = filtered.upgrade() else {
            return;
        };
        list.invalidate_filter();
        // Keep a visible application selected so that Enter takes it.
        let text = search.text().to_lowercase();
        let first = searched.iter().position(|name| name.contains(&text));
        match first.and_then(|index| list.row_at_index(index as i32)) {
            Some(row) => list.select_row(Some(&row)),
            None => list.unselect_all(),
        }
    });
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_min_content_height(280);
    scroller.set_min_content_width(420);
    scroller.set_has_frame(true);
    scroller.set_child(Some(&list));
    dialog.content().append(&search);
    dialog.content().append(&scroller);

    let command = gtk::Entry::new();
    command.set_placeholder_text(Some("Command line to execute"));
    command.set_activates_default(true);
    let in_terminal = gtk::CheckButton::with_label("Execute in terminal emulator");
    let app_name = gtk::Entry::new();
    app_name.set_placeholder_text(Some("Application name (optional)"));
    app_name.set_activates_default(true);
    let custom = gtk::Box::new(gtk::Orientation::Vertical, 6);
    custom.append(&command);
    custom.append(&in_terminal);
    custom.append(&app_name);
    let expander = gtk::Expander::new(Some("Custom Command Line"));
    expander.set_child(Some(&custom));
    dialog.content().append(&expander);

    let make_default = gtk::CheckButton::with_mnemonic(
        "_Set selected application as default action for this file type",
    );
    dialog.content().append(&make_default);
    let problem = gtk::Label::new(None);
    problem.set_xalign(0.0);
    problem.add_css_class("error");
    problem.set_visible(false);
    dialog.content().append(&problem);

    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let window = dialog.window().downgrade();
    let content_type = content_type.to_owned();
    let picker = list.clone();
    ok.connect_clicked(move |_| {
        let typed = command.text();
        let selection: Result<gio::AppInfo, String> = if expander.is_expanded()
            && !typed.trim().is_empty()
        {
            let name = app_name.text();
            let name = if name.trim().is_empty() {
                typed
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            } else {
                name.to_string()
            };
            apps::custom(&typed, &name, in_terminal.is_active()).map_err(|e| super::error::app(&e))
        } else {
            picker
                .selected_row()
                .and_then(|row| installed.get(row.index() as usize).cloned())
                .ok_or_else(|| "Select an application or enter a command.".to_owned())
        };
        let result = selection.and_then(|app| {
            if make_default.is_active() {
                apps::set_default(&app, &content_type).map_err(|e| super::error::app(&e))?;
            }
            Ok(app)
        });
        match result {
            Ok(app) => {
                if let Some(window) = window.upgrade() {
                    window.close();
                }
                chosen(app);
            }
            Err(message) => {
                problem.set_text(&message);
                problem.set_visible(true);
            }
        }
    });
    // A double-click on an application is the same as OK.
    let accept = ok.downgrade();
    list.connect_row_activated(move |_, _| {
        if let Some(ok) = accept.upgrade() {
            ok.emit_clicked();
        }
    });
    // The search field swallows Esc to clear itself; make it cancel instead.
    let window = dialog.window().downgrade();
    search.connect_stop_search(move |_| {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
    dialog.present();
    // Initial focus: the search field, with the first application selected.
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }
    search.grab_focus();
}
