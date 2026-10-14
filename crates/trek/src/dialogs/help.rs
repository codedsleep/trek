//! Help ▸ About and the key list (`~`, F1, Help ▸ Keys).

use super::base::{Dialog, Role};
use gtk::{gdk, glib, prelude::*};
use trek_core::keymap::{Action, Keymap, SearchVia, SortBy};

/// The upstream credit the licence and the spec both ask for.
pub const CREDIT: &str =
    "Based on PCManFM by Hong Jen Yee (PCMan) and Andriy Grytsenko (LStranger)";

/// The name `scripts/install.sh` installs the icon under.
pub const ICON: &str = "io.github.codedsleep.Trek";

pub fn about(parent: &gtk::Window) {
    // The program's own icon when it is installed, the theme's otherwise.
    let installed =
        gtk::IconTheme::for_display(&gtk::prelude::WidgetExt::display(parent)).has_icon(ICON);
    let dialog = gtk::AboutDialog::builder()
        .program_name("trek")
        .version(env!("CARGO_PKG_VERSION"))
        .logo_icon_name(if installed {
            ICON
        } else {
            "system-file-manager"
        })
        .comments(format!(
            "A file manager driven with vim motions.\n{CREDIT}."
        ))
        .license_type(gtk::License::Gpl20)
        .website("https://github.com/codedsleep/trek")
        .modal(true)
        .build();
    dialog.add_credit_section(
        "PCManFM, which this is a port of",
        &["Hong Jen Yee (PCMan)", "Andriy Grytsenko (LStranger)"],
    );
    dialog.set_transient_for(Some(parent));
    dialog.present();
}

/// What the status line says for a binding that this program has no
/// counterpart for yet. The key list marks the same bindings, so the two
/// cannot disagree.
pub fn unavailable(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::Link { relative: true } => {
            "Relative links are not available; - links by absolute path"
        }
        Action::Hardlink => "Hard links are not available",
        Action::BulkCreate => "Bulk create is not available; a creates one",
        Action::Search(SearchVia::Fd) => "Search by name (fd) is not available",
        Action::Search(SearchVia::Rg) => "Search by content (ripgrep) is not available",
        Action::CancelSearch => "There is no search to cancel",
        Action::Zoxide => "Jumping with zoxide is not available",
        Action::Sort {
            by: SortBy::Random, ..
        } => "Random order is not available",
        _ => return None,
    })
}

/// Keys beside the vim motions: what a program with a window adds.
const WINDOW_KEYS: &[(&str, &str)] = &[
    ("Backspace", "Back to the parent directory"),
    ("Delete", "Trash selected files"),
    ("Shift+Delete", "Permanently delete selected files"),
    ("Ctrl+Enter", "Open selected folders in new tabs"),
    ("F2", "Rename"),
    ("Alt+Enter", "Properties"),
    ("Menu, Shift+F10", "Context menu"),
    ("Ctrl+Shift+N, Ctrl+Alt+N", "New folder, new empty file"),
    ("F5", "Reload"),
    ("Ctrl+L", "Location entry (also g Space)"),
    ("Alt+Left, Alt+Right, Alt+Up", "Back, forward, parent"),
    ("Alt+Home", "Home folder"),
    ("F4", "Open a terminal here"),
    ("Ctrl+T, Ctrl+W", "New tab, close tab"),
    ("Ctrl+Tab, Ctrl+Shift+Tab", "Next tab, previous tab"),
    ("Ctrl+N, Ctrl+Q", "New window, close the window"),
    (
        "Ctrl+1 … Ctrl+5",
        "Line, icon, compact, thumbnail, detailed list view",
    ),
    ("Ctrl+=, Alt+-, Ctrl+0", "Zoom in, zoom out, normal size"),
    ("F7, F8", "Parent column, preview column"),
    ("F9", "Side pane"),
    ("Ctrl+6, Ctrl+7", "Side pane: places, directory tree"),
    ("F3, F6", "Dual-pane mode, move between panes"),
    ("F10", "Menu"),
    ("F11", "Full screen"),
];

const INPUT_KEYS: &[(&str, &str)] = &[
    ("Enter", "Submit"),
    ("Esc, Ctrl+C, Ctrl+[", "Cancel"),
    ("Ctrl+A, Ctrl+E", "Start of the line, end of the line"),
    ("Ctrl+B, Ctrl+F", "Back a character, forward a character"),
    ("Alt+B, Alt+F", "Back a word, forward a word"),
    (
        "Ctrl+H, Ctrl+D",
        "Delete before the cursor, under the cursor",
    ),
    (
        "Ctrl+U, Ctrl+K",
        "Delete to the start, to the end of the line",
    ),
    ("Ctrl+W, Alt+D", "Delete the word before, after the cursor"),
];

const QUESTION_KEYS: &[(&str, &str)] = &[("y, Enter", "Yes"), ("n, Esc, Ctrl+C", "No")];

/// `~` and F1: every key, with a field that narrows the list as you type.
pub fn keyboard(parent: &gtk::Window, keymap: &Keymap) {
    let dialog = Dialog::new(Some(parent), "Keys");
    dialog.window().set_resizable(true);
    dialog.window().set_default_size(620, 640);
    let filter = gtk::SearchEntry::new();
    filter.set_placeholder_text(Some("Type to narrow the list"));
    let grid = gtk::Grid::new();
    grid.set_column_spacing(24);
    grid.set_row_spacing(3);

    // (heading, rows of (key label, description, both lower-cased)).
    let mut rows: Vec<(gtk::Label, gtk::Label, String)> = Vec::new();
    let mut headings: Vec<(gtk::Label, std::ops::Range<usize>)> = Vec::new();
    let mut row = 0;
    let mut section = |title: &str, entries: Vec<(String, String)>| {
        let heading = gtk::Label::new(Some(title));
        heading.set_xalign(0.0);
        heading.add_css_class("heading");
        if row > 0 {
            heading.set_margin_top(12);
        }
        grid.attach(&heading, 0, row, 2, 1);
        row += 1;
        let first = rows.len();
        for (keys, desc) in entries {
            let key = gtk::Label::new(Some(&keys));
            key.set_xalign(0.0);
            key.add_css_class("help-key");
            let action = gtk::Label::new(Some(&desc));
            action.set_xalign(0.0);
            action.set_wrap(true);
            action.set_max_width_chars(52);
            grid.attach(&key, 0, row, 1, 1);
            grid.attach(&action, 1, row, 1, 1);
            row += 1;
            rows.push((key, action, format!("{keys} {desc}").to_lowercase()));
        }
        headings.push((heading, first..rows.len()));
    };
    section(
        "Files — vim motions",
        keymap
            .bindings()
            .iter()
            .map(|binding| {
                let desc = match unavailable(binding.action) {
                    Some(_) => format!("{} — not available", binding.desc),
                    None => binding.desc.to_owned(),
                };
                (binding.label(), desc)
            })
            .collect(),
    );
    let fixed = |keys: &[(&str, &str)]| -> Vec<(String, String)> {
        keys.iter()
            .map(|(key, desc)| ((*key).to_owned(), (*desc).to_owned()))
            .collect()
    };
    section("Input line", fixed(INPUT_KEYS));
    section("Questions", fixed(QUESTION_KEYS));
    section("Window", fixed(WINDOW_KEYS));

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&grid));
    dialog.content().append(&filter);
    dialog.content().append(&scroller);
    dialog.content().set_vexpand(true);

    filter.connect_search_changed(move |filter| {
        let wanted = filter.text().to_lowercase();
        for (heading, range) in &headings {
            let mut any = false;
            for (key, action, text) in &rows[range.clone()] {
                let shown = wanted.is_empty() || text.contains(&wanted);
                key.set_visible(shown);
                action.set_visible(shown);
                any |= shown;
            }
            heading.set_visible(any);
        }
    });
    // The list scrolls from the filter field: nothing else takes the keys.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let adjustment = scroller.vadjustment();
    keys.connect_key_pressed(move |_, key, _, state| {
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let page = adjustment.page_size();
        let by = match key {
            gdk::Key::Down => 40.0,
            gdk::Key::Up => -40.0,
            gdk::Key::n if ctrl => 40.0,
            gdk::Key::p if ctrl => -40.0,
            gdk::Key::Page_Down => page,
            gdk::Key::Page_Up => -page,
            gdk::Key::d if ctrl => page / 2.0,
            gdk::Key::u if ctrl => -page / 2.0,
            _ => return glib::Propagation::Proceed,
        };
        let limit = (adjustment.upper() - page).max(0.0);
        adjustment.set_value((adjustment.value() + by).clamp(0.0, limit));
        glib::Propagation::Stop
    });
    dialog.window().add_controller(keys);

    // Esc in a search entry is the entry's; it means the same here.
    let window = dialog.window().downgrade();
    filter.connect_stop_search(move |_| {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
    let close = dialog.button("_Close", Role::Default);
    let window = dialog.window().downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
    dialog.present();
    filter.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the key list calls unavailable is exactly what answers with a
    /// notice instead of doing something.
    #[test]
    fn eight_bindings_are_marked_unavailable() {
        let keymap = Keymap::standard();
        let marked: Vec<String> = keymap
            .bindings()
            .iter()
            .filter(|binding| unavailable(binding.action).is_some())
            .map(|binding| binding.label())
            .collect();
        assert_eq!(marked, ["_", "<C-->", "A", "s", "S", "<C-s>", "Z", ", r"]);
    }
}
