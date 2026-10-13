//! What each command does. Actions only deliver a `Cmd`; the behaviour lives
//! here and always targets the active tab.

use super::{MainWindow, fileops::Transfer};
use crate::{
    actions::{Cmd, Value},
    settings,
    tab::location_from_uri,
};
use gtk::prelude::*;
use tallboy_core::{
    config::{PaneMode, SidePane, Toolbar, ViewMode},
    helpers,
    location::Location,
    sort::SortSpec,
};

impl MainWindow {
    /// Executes one command.
    pub fn run(&self, cmd: Cmd, value: Value) {
        let inner = &self.0;
        let settings = inner.app.settings();
        let tab = self.active_tab();
        match cmd {
            Cmd::NewWindow => {
                if let Some(location) = tab.map(|t| t.location()).or(inner.app.home_location()) {
                    inner.app.new_window(location);
                }
            }
            Cmd::NewTab => {
                if let Some(location) = tab.map(|t| t.location()).or(inner.app.home_location()) {
                    self.add_tab(location);
                }
            }
            Cmd::CloseTab => {
                if let Some(tab) = tab {
                    self.close_tab(&tab);
                }
            }
            Cmd::NextTab => self.pane().step(1),
            Cmd::PrevTab => self.pane().step(-1),
            Cmd::GotoTab => {
                if let Value::Index(index) = value {
                    self.pane().go_to(index.max(0) as u32);
                }
            }
            Cmd::HistoryGo => {
                if let (Some(tab), Value::Text(index)) = (tab, value)
                    && let Ok(index) = index.parse::<usize>()
                {
                    tab.go_to_history(index);
                }
            }
            Cmd::GoLocation => inner.path_bar.focus_entry(),
            Cmd::PathbarGo => inner.path_bar.submit(),
            Cmd::PathbarMode => {
                if let Value::Text(mode) = value {
                    settings::set::pathbar_mode_buttons(settings, mode == "buttons");
                }
            }
            Cmd::ToolbarShow
            | Cmd::ToolbarNewWin
            | Cmd::ToolbarNewTab
            | Cmd::ToolbarNav
            | Cmd::ToolbarHome => {
                if let Value::Bool(on) = value {
                    settings::set::toolbar(settings, toolbar_with(settings.toolbar(), cmd, on));
                }
            }
            Cmd::CloseWindow => inner.window.close(),
            Cmd::SelectAll => {
                if let Some(tab) = tab {
                    tab.select_all();
                }
            }
            Cmd::UnselectAll => {
                if let Some(tab) = tab {
                    tab.model().selection().unselect_all();
                }
            }
            Cmd::InvertSelection => {
                if let Some(tab) = tab {
                    tab.invert_selection();
                }
            }
            Cmd::Reload => {
                if let Some(tab) = tab {
                    tab.reload();
                }
            }
            // View settings go through the tab, which saves them either
            // for this folder or as the global defaults (§8.7).
            Cmd::ShowHidden => {
                if let (Some(tab), Value::Bool(show)) = (tab, value) {
                    tab.set_show_hidden(show);
                }
            }
            Cmd::SortOrder | Cmd::SortBy | Cmd::SortMingle | Cmd::SortIgnoreCase => {
                if let Some(tab) = tab {
                    tab.set_sort(sort_with(tab.model().sort(), cmd, &value));
                }
            }
            Cmd::ViewMode => {
                if let (Some(tab), Value::Text(text)) = (tab, value)
                    && let Ok(mode) = text.parse::<ViewMode>()
                {
                    tab.set_mode(mode);
                }
            }
            Cmd::PreserveSettings => {
                if let (Some(tab), Value::Bool(on)) = (tab, value) {
                    tab.set_preserve(on);
                }
            }
            Cmd::ZoomIn | Cmd::ZoomOut | Cmd::ZoomNormal => {
                if let Some(tab) = tab {
                    zoom(settings, tab.mode(), cmd);
                }
            }
            Cmd::Open => {
                if let Some(tab) = tab {
                    tab.open(tab.targets());
                }
            }
            Cmd::Trash => self.trash_selection(),
            Cmd::Delete => self.delete_selection(),
            Cmd::Rename => self.rename_selection(),
            Cmd::CreateFolder => self.ask_create(Some(true)),
            Cmd::CreateFile => self.ask_create(Some(false)),
            Cmd::CreateLink => self.transfer_selection_to_chosen_folder(Transfer::Link),
            Cmd::MoveTo => self.transfer_selection_to_chosen_folder(Transfer::Move),
            Cmd::CopyTo => self.transfer_selection_to_chosen_folder(Transfer::Copy),
            Cmd::CopyPath => self.copy_paths(),
            Cmd::Cut => self.copy_selection(true),
            Cmd::Copy => self.copy_selection(false),
            Cmd::Paste => self.paste(false),
            Cmd::OpenWith => self.open_selection_with(),
            Cmd::OpenWithApp => {
                if let Value::Text(id) = value {
                    self.open_selection_with_app(&id);
                }
            }
            Cmd::OpenNewWindow => {
                for item in self.selected() {
                    if item.is_dir()
                        && let Some(location) = location_from_uri(&item.meta().uri)
                    {
                        inner.app.new_window(location);
                    }
                }
            }
            Cmd::OpenTerminalHere => {
                if let [folder] = self.selected().as_slice()
                    && folder.is_dir()
                {
                    self.opener()
                        .terminal_in(&gtk::gio::File::for_uri(&folder.meta().uri));
                }
            }
            Cmd::PasteInto => {
                if let [folder] = self.selected().as_slice()
                    && folder.is_dir()
                    && let Some(destination) = location_from_uri(&folder.meta().uri)
                {
                    self.paste_into(destination, false);
                }
            }
            Cmd::Restore => self.restore_selection(),
            Cmd::EmptyTrash => self.empty_trash(),
            Cmd::ContextMenu => {
                if let Some(tab) = tab {
                    tab.show_context_menu(None);
                }
            }
            Cmd::Terminal => {
                if let Some(directory) = self.current_folder() {
                    self.opener().terminal_in(&directory);
                }
            }
            Cmd::RunCommand => self.run_command(),
            Cmd::OpenNewTab => {
                if let Some(tab) = tab {
                    for item in tab.targets() {
                        if item.is_dir()
                            && let Some(location) = location_from_uri(&item.meta().uri)
                        {
                            self.add_tab(location);
                        }
                    }
                }
            }
            Cmd::Filter => {
                if let Some(tab) = tab {
                    self.ask_filter(&tab);
                }
            }
            Cmd::ShowStatusbar => {
                if let Value::Bool(show) = value {
                    settings::set::show_statusbar(settings, show);
                }
            }
            Cmd::SidePaneShow => {
                if let Value::Bool(show) = value {
                    let mode = settings.side_pane_mode().mode;
                    settings::set::side_pane_mode(
                        settings,
                        SidePane {
                            hidden: !show,
                            mode,
                        },
                    );
                }
            }
            Cmd::SidePaneMode => {
                if let Value::Text(text) = value
                    && let Ok(mode) = text.parse::<PaneMode>()
                {
                    settings::set::side_pane_mode(
                        settings,
                        SidePane {
                            hidden: false,
                            mode,
                        },
                    );
                }
            }
            Cmd::Fullscreen => {
                if value == Value::Bool(true) {
                    inner.window.fullscreen();
                } else {
                    inner.window.unfullscreen();
                }
            }
            Cmd::Back | Cmd::Forward => {
                if let Some(tab) = tab {
                    tab.go_history(cmd == Cmd::Forward);
                }
            }
            Cmd::Up => {
                if let Some(tab) = tab {
                    tab.go_up();
                }
            }
            Cmd::Home => self.go(inner.app.home_location()),
            Cmd::GoDesktop => self.go(gtk::glib::user_special_dir(
                gtk::glib::UserDirectory::Desktop,
            )
            .and_then(|path| Location::from_path(&path).ok())),
            Cmd::GoTrash => self.go(location_from_uri("trash:///")),
            Cmd::GoDevices => self.go(location_from_uri("computer:///")),
            Cmd::GoNetwork => self.go(location_from_uri("network:///")),
            Cmd::GoTo => {
                if let Value::Text(input) = value {
                    let home = settings.home();
                    match Location::parse(&input, &home, &home) {
                        Ok(location) => self.go(Some(location)),
                        Err(e) => log::warn!("go-to {input}: {e}"),
                    }
                }
            }
            Cmd::Properties => self.show_properties(false),
            Cmd::FolderProperties => self.show_properties(true),
            Cmd::ConnectServer => {
                let this = self.clone();
                crate::dialogs::connect::show(inner.window.upcast_ref(), move |location| {
                    this.go(Some(location));
                });
            }
            Cmd::AddBookmark => {
                if let Some(tab) = tab {
                    crate::bookmarks::add_dialog(
                        &inner.app,
                        inner.window.upcast_ref(),
                        tab.location(),
                    );
                }
            }
            Cmd::EditBookmarks => {
                crate::bookmarks::edit_dialog(&inner.app, inner.window.upcast_ref())
            }
            Cmd::OpenBookmark => {
                if let Value::Text(uri) = value
                    && let Some(location) = location_from_uri(&uri)
                {
                    self.open_bookmark(location);
                }
            }
            Cmd::DualPane => {
                if let Value::Bool(on) = value {
                    self.set_dual_pane(on);
                }
            }
            Cmd::FocusNextPane => self.focus_next_pane(),
            Cmd::ShowMenubar => {
                if let Value::Bool(show) = value {
                    settings::set::show_menubar(settings, show);
                }
            }
            Cmd::Menu => inner.toolbar.menu_button().popup(),
            Cmd::MillerParent => {
                if let Value::Bool(show) = value {
                    settings::set::miller_parent(settings, show);
                }
            }
            Cmd::MillerPreview => {
                if let Value::Bool(show) = value {
                    settings::set::miller_preview(settings, show);
                }
            }
            Cmd::DebugDump => self.debug_dump(),
            Cmd::Preferences => {
                crate::dialogs::prefs::show(&inner.app, Some(inner.window.upcast_ref()), 1);
            }
            Cmd::About => crate::dialogs::help::about(inner.window.upcast_ref()),
            Cmd::KeyboardNavigation => {
                crate::dialogs::help::keyboard(inner.window.upcast_ref(), inner.app.keymap());
            }
            // Left out of v1 (SCOPE D4); their menu items are not built.
            Cmd::GoApplications | Cmd::FindFiles => log::info!("{cmd:?}: not in v1"),
        }
        self.queue_refresh();
    }

    /// View ▸ Filter…: a name pattern for the active tab; `*` or nothing
    /// clears it (§8.7).
    fn ask_filter(&self, tab: &crate::tab::TabPage) {
        let current = tab.model().glob().unwrap_or_else(|| "*".into());
        let tab = tab.downgrade();
        let window = self.downgrade();
        crate::dialogs::base::prompt(
            Some(self.0.window.upcast_ref()),
            "Select filter",
            "Choose a new filter",
            &current,
            None,
            move |pattern| {
                if let Some(tab) = tab.upgrade() {
                    let pattern = pattern.trim();
                    tab.model()
                        .set_glob((!pattern.is_empty() && pattern != "*").then(|| pattern.into()));
                }
                if let Some(window) = window.upgrade() {
                    window.queue_refresh();
                }
                None
            },
        );
    }

    fn go(&self, location: Option<Location>) {
        if let (Some(tab), Some(location)) = (self.active_tab(), location) {
            tab.navigate(location);
        }
    }

    fn debug_dump(&self) {
        let focus = gtk::prelude::GtkWindowExt::focus(&self.0.window)
            .map_or_else(|| "none".into(), |w| w.type_().name().to_owned());
        let tab = self
            .active_tab()
            .map_or_else(|| "no tab".into(), |t| t.debug_state());
        // Visible top-level windows; the ones that are not main windows are
        // listed by title so that a check can tell which dialog is up.
        let shown: Vec<gtk::Window> = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .filter(|window| window.is_visible())
            .collect();
        let dialogs: Vec<String> = shown
            .iter()
            .filter(|window| !window.is::<gtk::ApplicationWindow>())
            .map(|window| window.title().unwrap_or_default().to_string())
            .collect();
        eprintln!(
            "tallboy state: {tab} focus={focus} input={} finder={} confirm={} pending={:?} which_key={} notice={:?} yanked={:?} toplevels={} dialogs={:?} title={:?} {} clipboard={:?} {} panes={} active_pane={} tabs={} {}",
            self.0.input.debug_state(),
            self.0.finder.debug_state(),
            self.0.confirm.debug_state(),
            self.0
                .pending
                .borrow()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            self.0.which_key.is_open(),
            self.0.status.debug_state(),
            crate::view::item::yanked(),
            shown.len(),
            dialogs,
            self.0.window.title().unwrap_or_default(),
            self.0.path_bar.debug_state(),
            crate::clipboard::offered_mime_types(),
            self.0.side.debug_state(),
            self.0.panes.borrow().len(),
            self.0.active.get(),
            self.all_tabs().len(),
            crate::debug::live(),
        );
    }
}

/// Zooms the icon size that the current view mode uses (§8.6).
fn zoom(settings: &crate::settings::Settings, mode: ViewMode, cmd: Cmd) {
    /// The largest step §8.6 lists.
    const LARGEST: u32 = 256;
    let (current, normal) = match mode {
        ViewMode::Icon => (settings.big_icon_size(), 48),
        ViewMode::Thumbnail => (settings.thumbnail_size(), 128),
        ViewMode::Compact | ViewMode::List | ViewMode::Lines => (settings.small_icon_size(), 24),
    };
    let size = match cmd {
        Cmd::ZoomIn => helpers::zoom(current, true).min(LARGEST),
        Cmd::ZoomOut => helpers::zoom(current, false),
        _ => normal,
    };
    match mode {
        ViewMode::Icon => settings::set::big_icon_size(settings, size),
        ViewMode::Thumbnail => settings::set::thumbnail_size(settings, size),
        ViewMode::Compact | ViewMode::List | ViewMode::Lines => {
            settings::set::small_icon_size(settings, size)
        }
    }
}

/// Applies one View ▸ Toolbar toggle to the `toolbar` setting.
fn toolbar_with(mut toolbar: Toolbar, cmd: Cmd, on: bool) -> Toolbar {
    // "Show Toolbar" is stored inverted, as the word `hidden`.
    let (name, present) = match cmd {
        Cmd::ToolbarShow => ("hidden", !on),
        Cmd::ToolbarNewWin => ("newwin", on),
        Cmd::ToolbarNewTab => ("newtab", on),
        Cmd::ToolbarNav => ("navigation", on),
        Cmd::ToolbarHome => ("home", on),
        _ => return toolbar,
    };
    toolbar.0.retain(|item| item != name);
    if present {
        toolbar.0.push(name.to_owned());
    }
    toolbar
}

/// Applies one Sort Files menu choice to a sort order.
fn sort_with(mut sort: SortSpec, cmd: Cmd, value: &Value) -> SortSpec {
    match (cmd, value) {
        (Cmd::SortOrder, Value::Text(order)) => sort.descending = order == "descending",
        (Cmd::SortBy, Value::Text(column)) => {
            if let Ok(parsed) = format!("{column};ascending;").parse::<SortSpec>() {
                sort.column = parsed.column;
            }
        }
        (Cmd::SortMingle, Value::Bool(on)) => sort.mingle = *on,
        (Cmd::SortIgnoreCase, Value::Bool(on)) => sort.case_sensitive = !*on,
        _ => {}
    }
    sort
}
