//! The single table of commands (PROMPT §7.7).
//!
//! Every command is a `gio::SimpleAction` on the window. Menus, toolbar
//! buttons, context menus, shortcuts and the agent's D-Bus driver all activate
//! these actions; the handlers only forward a [`Cmd`] to the window, which
//! owns the behaviour. Action state is never set here: the window derives it
//! from the settings and the active tab.

use gtk::{gio, glib, prelude::*};
use std::rc::Rc;

/// What an action asks the window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    NewWindow,
    NewTab,
    CreateFolder,
    CreateFile,
    FolderProperties,
    CloseTab,
    CloseWindow,
    Open,
    Cut,
    Copy,
    Paste,
    Trash,
    Delete,
    CopyPath,
    Properties,
    Rename,
    CreateLink,
    MoveTo,
    CopyTo,
    SelectAll,
    UnselectAll,
    InvertSelection,
    Preferences,
    Reload,
    ShowHidden,
    SortOrder,
    SortBy,
    SortMingle,
    SortIgnoreCase,
    ViewMode,
    PreserveSettings,
    ToolbarShow,
    ToolbarNewWin,
    ToolbarNewTab,
    ToolbarNav,
    ToolbarHome,
    PathbarMode,
    SidePaneShow,
    SidePaneMode,
    ShowStatusbar,
    DualPane,
    FocusNextPane,
    ShowMenubar,
    Menu,
    MillerParent,
    MillerPreview,
    Fullscreen,
    ZoomIn,
    ZoomOut,
    ZoomNormal,
    Filter,
    AddBookmark,
    EditBookmarks,
    OpenBookmark,
    Back,
    Forward,
    Up,
    Home,
    GoDesktop,
    GoTrash,
    GoApplications,
    GoDevices,
    GoNetwork,
    GoLocation,
    ConnectServer,
    Terminal,
    FindFiles,
    RunCommand,
    About,
    KeyboardNavigation,
    NextTab,
    PrevTab,
    GotoTab,
    GoTo,
    OpenNewTab,
    OpenNewWindow,
    OpenTerminalHere,
    OpenWith,
    OpenWithApp,
    PasteInto,
    Restore,
    EmptyTrash,
    ContextMenu,
    HistoryGo,
    PathbarGo,
    DebugDump,
}

/// Shape of an action: its parameter and state types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No parameter, no state.
    Plain,
    /// Boolean state; activating asks for the opposite value.
    Toggle,
    /// String state with a string target; the default is the initial state.
    Radio(&'static str),
    /// Stateless with a string parameter.
    Text,
    /// Stateless with an `i32` parameter.
    Index,
}

/// The value delivered with a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    None,
    Bool(bool),
    Text(String),
    Index(i32),
}

/// One row of the action table.
pub struct Def {
    pub name: &'static str,
    pub kind: Kind,
    pub accels: &'static [&'static str],
    pub cmd: Cmd,
    /// Permanently disabled because its feature is deferred (DISCOVERY D4).
    pub deferred: bool,
}

const fn def(name: &'static str, kind: Kind, accels: &'static [&'static str], cmd: Cmd) -> Def {
    Def {
        name,
        kind,
        accels,
        cmd,
        deferred: false,
    }
}

const fn deferred(name: &'static str, accels: &'static [&'static str], cmd: Cmd) -> Def {
    Def {
        name,
        kind: Kind::Plain,
        accels,
        cmd,
        deferred: true,
    }
}

use Kind::{Index, Plain, Radio, Text, Toggle};

/// Every window action.
///
/// The keys of the file view are not here: they are Yazi's, in
/// `core::keymap`, and `window::keys` runs them. An accelerator in this
/// table is a key Yazi leaves alone that a program with a window is expected
/// to have — function keys, Ctrl+T, Alt+Left. None may also be a binding of
/// the keymap; a test in `window::keys` holds that line. Accelerators are
/// switched off while a text entry has the keyboard, so that every key
/// typed there is the entry's.
pub const TABLE: &[Def] = &[
    // File
    def("new-window", Plain, &["<Control>n"], Cmd::NewWindow),
    def("new-tab", Plain, &["<Control>t"], Cmd::NewTab),
    def(
        "create-folder",
        Plain,
        &["<Control><Shift>n"],
        Cmd::CreateFolder,
    ),
    def("create-file", Plain, &["<Control><Alt>n"], Cmd::CreateFile),
    def("folder-properties", Plain, &[], Cmd::FolderProperties),
    def("close-tab", Plain, &["<Control>w"], Cmd::CloseTab),
    def("close-window", Plain, &["<Control>q"], Cmd::CloseWindow),
    // Edit
    def("open", Plain, &[], Cmd::Open),
    def("cut", Plain, &[], Cmd::Cut),
    def("copy", Plain, &[], Cmd::Copy),
    def("paste", Plain, &[], Cmd::Paste),
    def("trash", Plain, &[], Cmd::Trash),
    def("delete", Plain, &[], Cmd::Delete),
    def("copy-path", Plain, &[], Cmd::CopyPath),
    def("properties", Plain, &["<Alt>Return"], Cmd::Properties),
    def("rename", Plain, &["F2"], Cmd::Rename),
    def("create-link", Plain, &[], Cmd::CreateLink),
    def("move-to", Plain, &[], Cmd::MoveTo),
    def("copy-to", Plain, &[], Cmd::CopyTo),
    def("select-all", Plain, &[], Cmd::SelectAll),
    def("invert-selection", Plain, &[], Cmd::InvertSelection),
    // Esc on the view (§8.8); not in any menu.
    def("unselect-all", Plain, &[], Cmd::UnselectAll),
    def("preferences", Plain, &[], Cmd::Preferences),
    // View
    def("reload", Plain, &["F5"], Cmd::Reload),
    def("show-hidden", Toggle, &[], Cmd::ShowHidden),
    def("sort-order", Radio("ascending"), &[], Cmd::SortOrder),
    def("sort-by", Radio("name"), &[], Cmd::SortBy),
    def("sort-mingle", Toggle, &[], Cmd::SortMingle),
    def("sort-ignore-case", Toggle, &[], Cmd::SortIgnoreCase),
    def("view-mode", Radio("lines"), &[], Cmd::ViewMode),
    def("preserve-settings", Toggle, &[], Cmd::PreserveSettings),
    def("toolbar-show", Toggle, &[], Cmd::ToolbarShow),
    def("toolbar-newwin", Toggle, &[], Cmd::ToolbarNewWin),
    def("toolbar-newtab", Toggle, &[], Cmd::ToolbarNewTab),
    def("toolbar-nav", Toggle, &[], Cmd::ToolbarNav),
    def("toolbar-home", Toggle, &[], Cmd::ToolbarHome),
    def("pathbar-mode", Radio("location"), &[], Cmd::PathbarMode),
    def("sidepane-show", Toggle, &["F9"], Cmd::SidePaneShow),
    def("sidepane-mode", Radio("places"), &[], Cmd::SidePaneMode),
    def("show-statusbar", Toggle, &[], Cmd::ShowStatusbar),
    def("show-menubar", Toggle, &[], Cmd::ShowMenubar),
    // Opens the menu button's menu, the menu bar being hidden by default.
    def("menu", Plain, &["F10"], Cmd::Menu),
    def("miller-parent", Toggle, &["F7"], Cmd::MillerParent),
    def("miller-preview", Toggle, &["F8"], Cmd::MillerPreview),
    def("dual-pane", Toggle, &["F3"], Cmd::DualPane),
    def("fullscreen", Toggle, &["F11"], Cmd::Fullscreen),
    // §8.5: side pane, pane 1, pane 2. Not in any menu.
    def("focus-next-pane", Plain, &["F6"], Cmd::FocusNextPane),
    // Ctrl+minus is Yazi's hard link, on the keypad too, so zooming out is on Alt.
    def(
        "zoom-in",
        Plain,
        &["<Control>KP_Add", "<Control>equal", "<Control>plus"],
        Cmd::ZoomIn,
    ),
    def(
        "zoom-out",
        Plain,
        &["<Alt>minus", "<Alt>KP_Subtract"],
        Cmd::ZoomOut,
    ),
    def("zoom-normal", Plain, &["<Control>0"], Cmd::ZoomNormal),
    def("filter", Plain, &[], Cmd::Filter),
    // Bookmarks
    def("add-bookmark", Plain, &[], Cmd::AddBookmark),
    def("edit-bookmarks", Plain, &[], Cmd::EditBookmarks),
    // One item per bookmark in the Bookmarks menu; the target is its URI.
    def("open-bookmark", Text, &[], Cmd::OpenBookmark),
    // Go
    def("back", Plain, &["<Alt>Left", "Back"], Cmd::Back),
    def("forward", Plain, &["<Alt>Right", "Forward"], Cmd::Forward),
    def("up", Plain, &["<Alt>Up"], Cmd::Up),
    def("home", Plain, &["<Alt>Home"], Cmd::Home),
    def("go-desktop", Plain, &[], Cmd::GoDesktop),
    def("go-trash", Plain, &[], Cmd::GoTrash),
    deferred("go-applications", &[], Cmd::GoApplications),
    def("go-devices", Plain, &[], Cmd::GoDevices),
    def("go-network", Plain, &[], Cmd::GoNetwork),
    def("go-location", Plain, &["<Control>l"], Cmd::GoLocation),
    def("connect-server", Plain, &[], Cmd::ConnectServer),
    // Tools
    def("terminal", Plain, &["F4"], Cmd::Terminal),
    deferred("find-files", &["<Control><Shift>f"], Cmd::FindFiles),
    def("run-command", Plain, &[], Cmd::RunCommand),
    // Help
    def("about", Plain, &[], Cmd::About),
    def("keyboard-navigation", Plain, &[], Cmd::KeyboardNavigation),
    // Tabs (§8.4)
    def(
        "next-tab",
        Plain,
        &["<Control>Tab", "<Control>Page_Down"],
        Cmd::NextTab,
    ),
    def(
        "prev-tab",
        Plain,
        &["<Control><Shift>ISO_Left_Tab", "<Control>Page_Up"],
        Cmd::PrevTab,
    ),
    def("goto-tab", Index, &[], Cmd::GotoTab),
    // Not in any menu: navigate to a URI or path; dump state for agent checks.
    // Toolbar: the history menu's entries and the Go button.
    def("history-go", Radio("0"), &[], Cmd::HistoryGo),
    def("pathbar-go", Plain, &[], Cmd::PathbarGo),
    // Middle-click and Ctrl+Enter; also in the folder context menu.
    def("open-new-tab", Plain, &[], Cmd::OpenNewTab),
    // The Open With… item of the Open With submenu.
    def("open-with", Plain, &[], Cmd::OpenWith),
    // Context menus (§8.13): the remaining entries, and the Menu key.
    def("open-with-app", Text, &[], Cmd::OpenWithApp),
    def("open-new-window", Plain, &[], Cmd::OpenNewWindow),
    def("open-terminal-here", Plain, &[], Cmd::OpenTerminalHere),
    def("paste-into", Plain, &[], Cmd::PasteInto),
    def("restore", Plain, &[], Cmd::Restore),
    def("empty-trash", Plain, &[], Cmd::EmptyTrash),
    def("context-menu", Plain, &[], Cmd::ContextMenu),
    def("go-to", Text, &[], Cmd::GoTo),
    def("debug-dump", Plain, &[], Cmd::DebugDump),
];

/// Radio accelerators and other accelerators that carry a target value.
pub const TARGETED: &[(&str, &[&str])] = &[
    ("win.view-mode::lines", &["<Control>1"]),
    ("win.view-mode::icon", &["<Control>2"]),
    ("win.view-mode::compact", &["<Control>3"]),
    ("win.view-mode::thumbnail", &["<Control>4"]),
    ("win.view-mode::list", &["<Control>5"]),
    ("win.sidepane-mode::places", &["<Control>6"]),
    ("win.sidepane-mode::dirtree", &["<Control>7"]),
];

/// Registers every accelerator with the application, or with `enabled`
/// false takes them all away again: while a text entry has the keyboard no
/// key belongs to a window action.
pub fn install_accels(app: &gtk::Application, enabled: bool) {
    for def in TABLE {
        if !def.accels.is_empty() {
            app.set_accels_for_action(
                &format!("win.{}", def.name),
                if enabled { def.accels } else { &[] },
            );
        }
    }
    for (action, accels) in TARGETED {
        app.set_accels_for_action(action, if enabled { accels } else { &[] });
    }
}

/// Adds every action in [`TABLE`] to `window`, forwarding to `run`.
pub fn install(window: &gtk::ApplicationWindow, run: Rc<dyn Fn(Cmd, Value)>) {
    for def in TABLE {
        let action = match def.kind {
            Plain => gio::SimpleAction::new(def.name, None),
            Toggle => gio::SimpleAction::new_stateful(def.name, None, &false.to_variant()),
            Radio(initial) => gio::SimpleAction::new_stateful(
                def.name,
                Some(glib::VariantTy::STRING),
                &initial.to_variant(),
            ),
            Text => gio::SimpleAction::new(def.name, Some(glib::VariantTy::STRING)),
            Index => gio::SimpleAction::new(def.name, Some(glib::VariantTy::INT32)),
        };
        action.set_enabled(!def.deferred);
        let cmd = def.cmd;
        let kind = def.kind;
        let forward = run.clone();
        action.connect_activate(move |action, parameter| {
            let value = match kind {
                Plain => Value::None,
                Toggle => Value::Bool(
                    !action
                        .state()
                        .and_then(|s| s.get::<bool>())
                        .unwrap_or_default(),
                ),
                Radio(_) | Text => Value::Text(
                    parameter
                        .and_then(|p| p.get::<String>())
                        .unwrap_or_default(),
                ),
                Index => Value::Index(parameter.and_then(|p| p.get::<i32>()).unwrap_or_default()),
            };
            forward(cmd, value);
        });
        if matches!(kind, Toggle | Radio(_)) {
            let forward = run.clone();
            action.connect_change_state(move |_, requested| {
                let Some(requested) = requested else { return };
                if let Some(on) = requested.get::<bool>() {
                    forward(cmd, Value::Bool(on));
                } else if let Some(text) = requested.get::<String>() {
                    forward(cmd, Value::Text(text));
                }
            });
        }
        window.add_action(&action);
    }
}

fn lookup(window: &gtk::ApplicationWindow, name: &str) -> Option<gio::SimpleAction> {
    window.lookup_action(name).and_downcast()
}

/// Sets the state of a toggle action.
pub fn set_bool(window: &gtk::ApplicationWindow, name: &str, on: bool) {
    if let Some(action) = lookup(window, name) {
        action.set_state(&on.to_variant());
    }
}

/// Sets the state of a radio action.
pub fn set_text(window: &gtk::ApplicationWindow, name: &str, value: &str) {
    if let Some(action) = lookup(window, name) {
        action.set_state(&value.to_variant());
    }
}

/// Enables or disables an action. Deferred actions stay disabled.
pub fn set_enabled(window: &gtk::ApplicationWindow, name: &str, enabled: bool) {
    let deferred = TABLE.iter().any(|d| d.name == name && d.deferred);
    if let Some(action) = lookup(window, name) {
        action.set_enabled(enabled && !deferred);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_and_accelerators_are_unique() {
        let mut names = HashSet::new();
        let mut accels = HashSet::new();
        for def in TABLE {
            assert!(names.insert(def.name), "duplicate action {}", def.name);
            for accel in def.accels {
                assert!(accels.insert(*accel), "duplicate accelerator {accel}");
            }
        }
        for (action, list) in TARGETED {
            let name = action
                .trim_start_matches("win.")
                .split(['(', ':'])
                .next()
                .unwrap_or_default();
            assert!(names.contains(name), "{action} targets an unknown action");
            for accel in *list {
                assert!(accels.insert(*accel), "duplicate accelerator {accel}");
            }
        }
    }

    #[test]
    fn deferred_features_stay_registered() {
        for name in ["find-files", "go-applications"] {
            assert!(TABLE.iter().any(|d| d.name == name && d.deferred));
        }
    }
}
