//! What each key binding does: the vim motions' commands, carried out by
//! this file manager's own parts. The table of keys is `core::keymap`.
//!
//! Where a binding has no counterpart here yet it says so in the
//! status line; it never does something else under the same key.

use super::{
    MainWindow,
    fileops::Transfer,
    prompt::{Caret, Session},
};
use crate::{clipboard, tab::TabPage, tab::location_from_uri};
use gtk::{gio, glib, prelude::*};
use trek_core::{
    keymap::{Action, CopyWhat, Linemode, SortBy},
    location::Location,
    naming, sort,
};
use trek_fs::terminal;

impl MainWindow {
    /// Shows a short message in the status line.
    pub fn notice(&self, text: &str) {
        self.0.status.notice(text);
    }

    /// Runs one command of the keymap on the active tab.
    pub(super) fn vim(&self, action: Action) {
        let inner = &self.0;
        let Some(tab) = self.active_tab() else {
            return;
        };
        // Visual mode lasts for as long as the cursor is only being moved.
        let keeps_visual = matches!(
            action,
            Action::Arrow(_)
                | Action::Escape
                | Action::Visual { .. }
                | Action::Seek(_)
                | Action::Find { .. }
                | Action::FindArrow { .. }
                | Action::Linemode(_)
                | Action::Help
        );
        if !keeps_visual {
            tab.end_visual();
        }
        if let Some(why) = crate::dialogs::help::unavailable(action) {
            self.notice(why);
            return;
        }
        match action {
            Action::Escape => {
                tab.escape();
            }
            Action::Quit => inner.window.close(),
            Action::CloseTab => self.close_tab(&tab),
            // There is no shell to drop back to; getting out of the way is
            // what is left of the idea.
            Action::Suspend => inner.window.minimize(),
            Action::Arrow(step) => tab.arrow(step),
            Action::Leave => tab.go_up(),
            Action::Enter => {
                if let Some(item) = tab.cursor_item().filter(|item| item.is_dir()) {
                    tab.open(vec![item]);
                }
            }
            Action::Back => tab.go_history(false),
            Action::Forward => tab.go_history(true),
            Action::ToggleAndNext => tab.toggle_and_next(),
            Action::SelectAll => tab.select_all(),
            Action::InvertSelection => tab.invert_selection(),
            Action::Visual { unset } => tab.start_visual(unset),
            Action::Seek(by) => tab.seek_preview(by),
            Action::Spot => self.show_properties(false),
            Action::Open { interactive: false } => tab.open(tab.targets()),
            Action::Open { interactive: true } => self.open_selection_with(),
            Action::Yank { cut } => self.yank(&tab, cut),
            Action::Paste { force } => self.paste(force),
            Action::Link { .. } => self.link_yanked(&tab),
            Action::Unyank => {
                if crate::view::item::yanked().is_some() {
                    clipboard::clear_own();
                }
            }
            Action::Remove { permanently: false } => self.trash_selection(),
            Action::Remove { permanently: true } => self.delete_selection(),
            Action::Create => self.ask_create(None),
            Action::Rename => self.rename_selection(),
            Action::Shell { block } => self.ask_shell(&tab, block),
            Action::ToggleHidden => tab.set_show_hidden(!tab.model().show_hidden()),
            // Answered above: there is no counterpart for these yet.
            Action::Hardlink
            | Action::BulkCreate
            | Action::Search(_)
            | Action::CancelSearch
            | Action::Zoxide => {}
            Action::Fzf => self.ask_jump(&tab),
            Action::Linemode(linemode) => tab.set_linemode(linemode),
            Action::Copy(what) => self.copy_text(&tab, what),
            Action::Filter => self.ask_filter_live(&tab),
            Action::Find { previous } => self.ask_find(&tab, previous),
            Action::FindArrow { previous } => match tab.find_arrow(previous) {
                None => self.notice("Nothing to find yet: / or ? first"),
                Some(false) => self.notice(&format!(
                    "No match for \"{}\"",
                    tab.find_query().unwrap_or_default()
                )),
                Some(true) => {}
            },
            Action::Sort { by, reverse } => self.sort_by(&tab, by, reverse),
            Action::GoHome => self.go_to(inner.app.home_location()),
            Action::GoConfig => {
                self.go_to(Location::from_path(&glib::user_config_dir()).ok());
            }
            Action::GoDownloads => {
                let downloads = glib::user_special_dir(glib::UserDirectory::Downloads)
                    .unwrap_or_else(|| glib::home_dir().join("Downloads"));
                self.go_to(Location::from_path(&downloads).ok());
            }
            Action::GoTrash => self.go_to(location_from_uri("trash:///")),
            Action::GoDevices => self.show_devices(),
            Action::GoInteractive => inner.path_bar.focus_entry(),
            Action::Follow => self.follow_link(&tab),
            Action::TabCreate => {
                let linemode = tab.linemode();
                self.add_tab(tab.location());
                if let Some(created) = self.active_tab() {
                    created.set_linemode(linemode);
                }
            }
            Action::TabRename => self.ask_tab_name(&tab),
            Action::TabSwitch(index) => self.pane().go_to(u32::from(index)),
            Action::TabStep(by) => self.pane().step(i32::from(by)),
            Action::TabSwap(by) => self.pane().swap(&tab, i32::from(by)),
            Action::Tasks => {
                if !crate::ops::present_running() {
                    self.notice("No file operation is running");
                }
            }
            Action::Help => {
                crate::dialogs::help::keyboard(inner.window.upcast_ref(), inner.app.keymap());
            }
        }
        self.queue_refresh();
    }

    fn go_to(&self, location: Option<Location>) {
        if let (Some(tab), Some(location)) = (self.active_tab(), location) {
            tab.navigate(location);
        }
    }

    /// `y` and `x`: the files go on the clipboard, where other programs can
    /// paste them too, and are marked until pasted or unyanked. This
    /// leaves nothing selected.
    fn yank(&self, tab: &TabPage, cut: bool) {
        let count = tab.targets().len();
        if count == 0 {
            return;
        }
        self.copy_selection(cut);
        tab.model().selection().unselect_all();
        self.notice(&format!(
            "{count} {} {}",
            if count == 1 { "file" } else { "files" },
            if cut { "cut" } else { "yanked" }
        ));
    }

    /// `-`: symbolic links to the yanked files, in this folder.
    fn link_yanked(&self, tab: &TabPage) {
        let destination = tab.location();
        let this = self.clone();
        clipboard::read(move |payload| match payload {
            Some(payload) => this.transfer(payload.files, &destination, Transfer::Link),
            None => this.notice("Nothing is yanked"),
        });
    }

    /// `c`: text for the clipboard, about the selection or the cursor.
    fn copy_text(&self, tab: &TabPage, what: CopyWhat) {
        let folder = tab.location();
        let per_file = |text: fn(&crate::view::FileItem) -> String| {
            tab.targets()
                .iter()
                .map(text)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let text = match what {
            CopyWhat::Path => per_file(|item| crate::ops::display(&item.meta().uri)),
            CopyWhat::Url => per_file(|item| item.meta().uri.clone()),
            CopyWhat::DirPath => folder.display(),
            CopyWhat::DirUrl => folder.to_uri().to_owned(),
            CopyWhat::Filename => per_file(|item| item.meta().display_name.clone()),
            CopyWhat::NameWithoutExt => per_file(|item| {
                let name = item.meta().display_name.clone();
                naming::split_extension(&name).0.to_owned()
            }),
        };
        if text.is_empty() {
            return;
        }
        clipboard::set_text(&text);
        self.notice(&format!(
            "Copied: {}",
            text.lines().next().unwrap_or_default()
        ));
    }

    /// `,`: the sort order, with the linemode that goes with it.
    fn sort_by(&self, tab: &TabPage, by: SortBy, reverse: bool) {
        let (column, linemode) = match by {
            SortBy::Mtime => (sort::Column::Modified, Some(Linemode::Mtime)),
            SortBy::Btime => (sort::Column::Created, Some(Linemode::Btime)),
            SortBy::Extension => (sort::Column::Extension, None),
            // Names are always compared the natural way, digits by value.
            SortBy::Alphabetical | SortBy::Natural => (sort::Column::Name, None),
            SortBy::Size => (sort::Column::Size, Some(Linemode::Size)),
            SortBy::Random => return,
        };
        let mut order = tab.model().sort();
        order.column = column;
        order.descending = reverse;
        tab.set_sort(order);
        if let Some(linemode) = linemode {
            tab.set_linemode(linemode);
        }
    }

    /// `g f`: goes to where the symbolic link under the cursor points and
    /// puts the cursor on its target.
    fn follow_link(&self, tab: &TabPage) {
        let Some(item) = tab.cursor_item() else {
            return;
        };
        let target = {
            let meta = item.meta();
            meta.symlink
                .then(|| meta.symlink_target.clone())
                .flatten()
                .zip(tab.location().path().map(|path| path.to_path_buf()))
                .map(|(target, folder)| folder.join(target))
        };
        let Some(target) = target else {
            self.notice("Not a symbolic link in a local folder");
            return;
        };
        let file = gio::File::for_path(&target);
        let Some(parent) = file
            .parent()
            .and_then(|parent| location_from_uri(&parent.uri()))
        else {
            return;
        };
        tab.navigate(parent);
        tab.select_when_listed(&file.uri());
    }

    // ---- input lines ----

    /// `/` and `?`: the cursor follows the query as it is typed. Esc puts it
    /// back where it was.
    fn ask_find(&self, tab: &TabPage, previous: bool) {
        let origin = tab.cursor().unwrap_or_default();
        let (on_change, on_submit, on_cancel) = (tab.downgrade(), tab.downgrade(), tab.downgrade());
        let window = self.downgrade();
        self.0.input.open(
            if previous {
                "Find previous:"
            } else {
                "Find next:"
            },
            "",
            Caret::End,
            Session {
                changed: Some(Box::new(move |query| {
                    // No match, as much as no query, leaves the cursor
                    // where the search started.
                    if let Some(tab) = on_change.upgrade()
                        && !tab.find(query, origin, previous)
                    {
                        tab.set_cursor(origin);
                    }
                })),
                submit: Box::new(move |query| {
                    let found = on_submit
                        .upgrade()
                        .is_none_or(|tab| query.is_empty() || tab.find(query, origin, previous));
                    if !found && let Some(window) = window.upgrade() {
                        window.notice(&format!("No match for \"{query}\""));
                    }
                    None
                }),
                cancel: Some(Box::new(move || {
                    if let Some(tab) = on_cancel.upgrade() {
                        tab.set_cursor(origin);
                    }
                })),
            },
        );
    }

    /// `f`: hides what does not contain the text, as it is typed. Enter
    /// keeps the filter, Esc drops it; later, Esc in the view clears it.
    fn ask_filter_live(&self, tab: &TabPage) {
        let before = tab.model().needle();
        let (on_change, on_cancel) = (tab.downgrade(), tab.downgrade());
        let restore = before.clone();
        self.0.input.open(
            "Filter:",
            before.as_deref().unwrap_or_default(),
            Caret::End,
            Session {
                changed: Some(Box::new(move |text| {
                    if let Some(tab) = on_change.upgrade() {
                        tab.set_needle(Some(text.to_owned()));
                    }
                })),
                submit: Box::new(|_| None),
                cancel: Some(Box::new(move || {
                    if let Some(tab) = on_cancel.upgrade() {
                        tab.set_needle(restore.clone());
                    }
                })),
            },
        );
    }

    /// `a`: a new file, or a new folder when the name ends with a slash;
    /// a name with slashes in it creates the folders before the last part
    /// (`test/.planning/`). The menu's Create New items ask the same way with the kind decided.
    pub(super) fn ask_create(&self, folder: Option<bool>) {
        let Some(tab) = self.active_tab() else { return };
        let this = self.downgrade();
        let tab = tab.downgrade();
        let (title, initial, caret) = match folder {
            None => ("Create (end with / for a folder):", "", Caret::End),
            Some(true) => ("New folder:", "New", Caret::SelectAll),
            Some(false) => ("New file:", "New", Caret::SelectAll),
        };
        self.0.input.open(
            title,
            initial,
            caret,
            Session {
                changed: None,
                submit: Box::new(move |text| {
                    let (Some(this), Some(tab)) = (this.upgrade(), tab.upgrade()) else {
                        return None;
                    };
                    // A path makes the folders it names (`a/b/c`), and a
                    // trailing slash asks for a folder, as `mkdir -p` would.
                    let trimmed = text.trim_end_matches('/');
                    let (name, as_folder) = match folder {
                        Some(true) => (trimmed, true),
                        Some(false) => (text, false),
                        None => (trimmed, trimmed.len() != text.len()),
                    };
                    this.create_named(&tab, name, as_folder)
                }),
                cancel: None,
            },
        );
    }

    /// `t r`: a name for the tab; an empty one gives it back the folder's.
    fn ask_tab_name(&self, tab: &TabPage) {
        let weak = tab.downgrade();
        self.0.input.open(
            "Tab name (empty for the folder's):",
            &tab.title(),
            Caret::SelectAll,
            Session {
                changed: None,
                submit: Box::new(move |name| {
                    if let Some(tab) = weak.upgrade() {
                        tab.set_custom_title(name);
                    }
                    None
                }),
                cancel: None,
            },
        );
    }

    /// `;` and `:`: a shell command in this folder. The selected files, or
    /// the one under the cursor, are its `"$@"`, and `$0` is the file under
    /// the cursor. `:` runs it in a terminal that stays open.
    fn ask_shell(&self, tab: &TabPage, block: bool) {
        let Some(folder) = self.current_folder() else {
            return;
        };
        let path_of = |item: &crate::view::FileItem| {
            gio::File::for_uri(&item.meta().uri)
                .path()
                .map(|path| path.to_string_lossy().into_owned())
        };
        let files: Vec<String> = tab.targets().iter().filter_map(path_of).collect();
        let hovered = tab
            .cursor_item()
            .as_ref()
            .and_then(path_of)
            .unwrap_or_else(|| "sh".to_owned());
        let app = self.0.app.clone();
        self.0.input.open(
            if block {
                "Shell (in a terminal):"
            } else {
                "Shell:"
            },
            "",
            Caret::End,
            Session {
                changed: None,
                submit: Box::new(move |command| {
                    if command.trim().is_empty() {
                        return None;
                    }
                    let quote = |text: &str| glib::shell_quote(text).to_string_lossy().into_owned();
                    let mut line = format!("sh -c {} {}", quote(command), quote(&hovered));
                    for file in &files {
                        line.push(' ');
                        line.push_str(&quote(file));
                    }
                    let terminal = if block {
                        match crate::open::resolve_terminal(&app) {
                            Ok(terminal) => Some(terminal),
                            Err(e) => return Some(crate::dialogs::error::terminal(&e)),
                        }
                    } else {
                        None
                    };
                    match terminal::run(&line, &folder, terminal.as_ref(), block) {
                        Ok(child) => {
                            crate::open::reap(child);
                            None
                        }
                        Err(e) => Some(crate::dialogs::error::terminal(&e)),
                    }
                }),
                cancel: None,
            },
        );
    }
}
