//! Help ▸ About and Help ▸ Keyboard Navigation (PROMPT §8.8, §8.24).

use super::base::{Dialog, Role};
use gtk::prelude::*;

/// The upstream credit the licence and the spec both ask for.
pub const CREDIT: &str =
    "Based on PCManFM by Hong Jen Yee (PCMan) and Andriy Grytsenko (LStranger)";

/// The name `scripts/install.sh` installs the icon under.
pub const ICON: &str = "io.github.codedsleep.Tallboy";

pub fn about(parent: &gtk::Window) {
    // The program's own icon when it is installed, the theme's otherwise.
    let installed =
        gtk::IconTheme::for_display(&gtk::prelude::WidgetExt::display(parent)).has_icon(ICON);
    let dialog = gtk::AboutDialog::builder()
        .program_name("tallboy")
        .version(env!("CARGO_PKG_VERSION"))
        .logo_icon_name(if installed {
            ICON
        } else {
            "system-file-manager"
        })
        .comments(format!(
            "A file manager for people who know PCManFM.\n{CREDIT}."
        ))
        .license_type(gtk::License::Gpl20)
        .website("https://github.com/codedsleep/pcmanfm-port-rust")
        .modal(true)
        .build();
    dialog.add_credit_section(
        "PCManFM, which this is a port of",
        &["Hong Jen Yee (PCMan)", "Andriy Grytsenko (LStranger)"],
    );
    dialog.set_transient_for(Some(parent));
    dialog.present();
}

/// Every key the file views, the tabs and the panes answer to.
const KEYS: &[(&str, &[(&str, &str)])] = &[
    (
        "Files",
        &[
            ("Enter", "Open"),
            ("Backspace", "Go to the parent folder"),
            ("Alt+Enter", "Properties"),
            (
                "Delete",
                "Move to the trash can, or delete when the trash can is off",
            ),
            ("Shift+Delete", "Delete"),
            ("F2", "Rename"),
            ("Ctrl+Shift+N, Ctrl+Alt+N", "New folder, new empty file"),
            ("Menu, Shift+F10", "Context menu"),
            ("Esc", "Clear the selection, close type-ahead search"),
            ("Arrows, Home, End, Page Up, Page Down", "Move the cursor"),
            ("Ctrl+A", "Select all"),
            ("Ctrl+I", "Invert the selection"),
            ("Ctrl+X, Ctrl+C, Ctrl+V", "Cut, copy, paste"),
            (
                "Any letter or digit",
                "Type-ahead search; Up and Down step through matches",
            ),
        ],
    ),
    (
        "Going places",
        &[
            ("Alt+Left, Alt+Right", "Back, forward"),
            ("Alt+Up", "Parent folder"),
            ("Alt+Home", "Home folder"),
            ("Ctrl+L, Alt+D", "Location entry"),
            ("F5, Ctrl+R", "Reload"),
            ("Ctrl+H", "Show hidden files"),
            ("Ctrl+D", "Bookmark this folder"),
            ("F4", "Open a terminal here"),
        ],
    ),
    (
        "Views",
        &[
            ("Ctrl+1 … Ctrl+4", "Icon, compact, thumbnail, detailed list"),
            ("Ctrl++, Ctrl+−, Ctrl+0", "Zoom in, zoom out, normal size"),
            (
                "Ctrl+Alt+1 … Ctrl+Alt+5",
                "Sort by name, date, size, type, extension",
            ),
            ("Ctrl+E", "Filter"),
            ("F9", "Show or hide the side pane"),
            ("Ctrl+6, Ctrl+7", "Places, directory tree"),
            ("F11", "Full screen"),
        ],
    ),
    (
        "Tabs and panes",
        &[
            ("Ctrl+T", "New tab"),
            ("Ctrl+W", "Close tab"),
            ("Ctrl+Page Up, Ctrl+Page Down", "Previous tab, next tab"),
            ("Ctrl+Shift+Tab, Ctrl+Tab", "Previous tab, next tab"),
            ("Alt+1 … Alt+9, Alt+0", "Tab 1 to 9, tab 10"),
            ("F3", "Dual-pane mode"),
            ("F6", "Move between the side pane and the panes"),
            ("Ctrl+N", "New window"),
            ("Ctrl+Q", "Close the window"),
        ],
    ),
];

pub fn keyboard(parent: &gtk::Window) {
    let dialog = Dialog::new(Some(parent), "Keyboard Navigation");
    let grid = gtk::Grid::new();
    grid.set_column_spacing(24);
    grid.set_row_spacing(4);
    let mut row = 0;
    for (section, keys) in KEYS {
        let heading = gtk::Label::new(Some(section));
        heading.set_xalign(0.0);
        heading.add_css_class("heading");
        if row > 0 {
            heading.set_margin_top(10);
        }
        grid.attach(&heading, 0, row, 2, 1);
        row += 1;
        for (key, action) in *keys {
            let key = gtk::Label::new(Some(key));
            key.set_xalign(0.0);
            key.set_selectable(false);
            let action = gtk::Label::new(Some(action));
            action.set_xalign(0.0);
            grid.attach(&key, 0, row, 1, 1);
            grid.attach(&action, 1, row, 1, 1);
            row += 1;
        }
    }
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(520);
    scroller.set_child(Some(&grid));
    dialog.content().append(&scroller);
    let close = dialog.button("_Close", Role::Default);
    let window = dialog.window().downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
    dialog.present();
}
