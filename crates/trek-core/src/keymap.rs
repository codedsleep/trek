//! The key bindings of the file view: vim motions.
//!
//! Keys are written in vim's notation (`j`, `G`, `<C-u>`, `<S-Enter>`,
//! `<Space>`) and a binding is a sequence of them (`g g`). The table below holds
//! every binding with its description; the view, the which-key popup and the help overlay all read
//! it. Nothing here knows about widgets: the GTK crate turns key events into
//! [`Key`] values and executes the [`Action`] a sequence resolves to.
use std::fmt;

/// The part of a key press that is not a modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A printable character, as typed: `g` and `G` are different keys.
    Char(char),
    /// A key with no character.
    Named(Named),
}

/// Keys without a character of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Named {
    Esc,
    Enter,
    Space,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
}

impl Named {
    const ALL: [(Named, &'static str); 15] = [
        (Named::Esc, "Esc"),
        (Named::Enter, "Enter"),
        (Named::Space, "Space"),
        (Named::Tab, "Tab"),
        (Named::Backspace, "Backspace"),
        (Named::Delete, "Delete"),
        (Named::Up, "Up"),
        (Named::Down, "Down"),
        (Named::Left, "Left"),
        (Named::Right, "Right"),
        (Named::Home, "Home"),
        (Named::End, "End"),
        (Named::PageUp, "PageUp"),
        (Named::PageDown, "PageDown"),
        (Named::F1, "F1"),
    ];

    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(named, _)| *named == self)
            .map_or("", |(_, name)| name)
    }
}

/// One key press. Shift is only recorded for named keys: for a character it
/// is already part of the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// Control is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
    /// Shift is held, for a named key.
    pub shift: bool,
    /// The key itself.
    pub code: Code,
}

impl Key {
    /// A plain character.
    pub const fn char(c: char) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            code: Code::Char(c),
        }
    }

    /// A plain named key.
    pub const fn named(named: Named) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            code: Code::Named(named),
        }
    }

    /// Parses vim's notation: a single character, or `<Mods-Name>` where the
    /// modifiers are `C-`, `A-` and `S-` and the name is a character or one
    /// of the [`Named`] keys.
    pub fn parse(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        if let (Some(only), None) = (chars.next(), chars.next()) {
            return Some(Self::char(only));
        }
        let mut rest = text.strip_prefix('<')?.strip_suffix('>')?;
        let mut key = Self::char(' ');
        // A modifier is a letter and a dash with something after them, so
        // that `<C-->` is Control with the minus key.
        while rest.len() > 2 && rest.as_bytes()[1] == b'-' {
            match rest.as_bytes()[0] {
                b'C' => key.ctrl = true,
                b'A' => key.alt = true,
                b'S' => key.shift = true,
                _ => return None,
            }
            rest = &rest[2..];
        }
        let mut chars = rest.chars();
        key.code = match (chars.next(), chars.next()) {
            (Some(only), None) => Code::Char(only),
            _ => Code::Named(Named::ALL.iter().find(|(_, name)| *name == rest)?.0),
        };
        // Shift on a character is the character's own business.
        if matches!(key.code, Code::Char(_)) {
            key.shift = false;
        }
        Some(key)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = !(self.ctrl || self.alt || self.shift);
        if let (Code::Char(c), true) = (self.code, plain) {
            return write!(f, "{c}");
        }
        f.write_str("<")?;
        for (held, prefix) in [(self.ctrl, "C-"), (self.alt, "A-"), (self.shift, "S-")] {
            if held {
                f.write_str(prefix)?;
            }
        }
        match self.code {
            Code::Char(c) => write!(f, "{c}>"),
            Code::Named(named) => write!(f, "{}>", named.name()),
        }
    }
}

/// Where a cursor movement goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Step {
    Prev,
    Next,
    Top,
    Bottom,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
}

/// What is shown beside each name in the line view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub enum Linemode {
    #[default]
    None,
    Size,
    Permissions,
    Btime,
    Mtime,
    Owner,
}

impl Linemode {
    /// The word the status line shows.
    pub fn name(self) -> &'static str {
        match self {
            Linemode::None => "none",
            Linemode::Size => "size",
            Linemode::Permissions => "permissions",
            Linemode::Btime => "btime",
            Linemode::Mtime => "mtime",
            Linemode::Owner => "owner",
        }
    }
}

/// What `c` copies to the clipboard as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum CopyWhat {
    Path,
    Url,
    DirPath,
    DirUrl,
    Filename,
    NameWithoutExt,
}

/// The orders `,` sorts by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SortBy {
    Mtime,
    Btime,
    Extension,
    Alphabetical,
    Natural,
    Size,
    Random,
}

/// The external program a search runs through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SearchVia {
    Fd,
    Rg,
}

/// What a key sequence asks for. One variant per command and argument
/// set that the default keymap uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Action {
    Escape,
    Quit,
    CloseTab,
    Suspend,
    Arrow(Step),
    Leave,
    Enter,
    Back,
    Forward,
    ToggleAndNext,
    SelectAll,
    InvertSelection,
    Visual { unset: bool },
    Seek(i32),
    Spot,
    Open { interactive: bool },
    Yank { cut: bool },
    Paste { force: bool },
    Link { relative: bool },
    Hardlink,
    Unyank,
    Remove { permanently: bool },
    Create,
    BulkCreate,
    Rename,
    Shell { block: bool },
    ToggleHidden,
    Search(SearchVia),
    CancelSearch,
    Fzf,
    Zoxide,
    Linemode(Linemode),
    Copy(CopyWhat),
    Filter,
    Find { previous: bool },
    FindArrow { previous: bool },
    Sort { by: SortBy, reverse: bool },
    GoHome,
    GoConfig,
    GoDownloads,
    GoTrash,
    GoDevices,
    GoInteractive,
    Follow,
    TabCreate,
    TabRename,
    TabSwitch(u8),
    TabStep(i8),
    TabSwap(i8),
    Tasks,
    Help,
}

/// A key sequence, what it does and a description of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The keys, in order.
    pub keys: Vec<Key>,
    /// What they do.
    pub action: Action,
    /// One line for the which-key popup and the help overlay.
    pub desc: &'static str,
}

impl Binding {
    /// The sequence in vim's notation, keys separated by spaces.
    pub fn label(&self) -> String {
        self.keys
            .iter()
            .map(Key::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the keys pressed so far amount to.
#[derive(Debug, PartialEq, Eq)]
pub enum Match<'a> {
    /// No binding starts like this.
    None,
    /// A prefix of these bindings: more keys are needed.
    Pending(Vec<&'a Binding>),
    /// Exactly this binding.
    Run(&'a Binding),
}

/// A set of bindings.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

use Action as A;

/// Every binding of the file view, in the order the key list shows them.
const BINDINGS: &[(&str, Action, &str)] = &[
    (
        "<Esc>",
        A::Escape,
        "Exit visual mode, clear selection, or cancel search",
    ),
    (
        "<C-[>",
        A::Escape,
        "Exit visual mode, clear selection, or cancel search",
    ),
    ("q", A::Quit, "Quit the process"),
    ("Q", A::Quit, "Quit without outputting cwd-file"),
    (
        "<C-c>",
        A::CloseTab,
        "Close the current tab, or quit if it's last",
    ),
    ("<C-z>", A::Suspend, "Suspend the process"),
    // Hop around
    ("k", A::Arrow(Step::Prev), "Previous file"),
    ("j", A::Arrow(Step::Next), "Next file"),
    ("<Up>", A::Arrow(Step::Prev), "Previous file"),
    ("<Down>", A::Arrow(Step::Next), "Next file"),
    ("g g", A::Arrow(Step::Top), "Go to top"),
    ("G", A::Arrow(Step::Bottom), "Go to bottom"),
    ("<Home>", A::Arrow(Step::Top), "Go to top"),
    ("<End>", A::Arrow(Step::Bottom), "Go to bottom"),
    (
        "<C-u>",
        A::Arrow(Step::HalfPageUp),
        "Move cursor up half page",
    ),
    (
        "<C-d>",
        A::Arrow(Step::HalfPageDown),
        "Move cursor down half page",
    ),
    ("<C-b>", A::Arrow(Step::PageUp), "Move cursor up one page"),
    (
        "<C-f>",
        A::Arrow(Step::PageDown),
        "Move cursor down one page",
    ),
    (
        "<S-PageUp>",
        A::Arrow(Step::HalfPageUp),
        "Move cursor up half page",
    ),
    (
        "<S-PageDown>",
        A::Arrow(Step::HalfPageDown),
        "Move cursor down half page",
    ),
    (
        "<PageUp>",
        A::Arrow(Step::PageUp),
        "Move cursor up one page",
    ),
    (
        "<PageDown>",
        A::Arrow(Step::PageDown),
        "Move cursor down one page",
    ),
    // Navigation
    ("h", A::Leave, "Back to the parent directory"),
    ("l", A::Enter, "Enter the child directory"),
    ("<Left>", A::Leave, "Back to the parent directory"),
    ("<Right>", A::Enter, "Enter the child directory"),
    ("H", A::Back, "Back to previous directory"),
    ("L", A::Forward, "Forward to next directory"),
    // Toggle
    (
        "<Space>",
        A::ToggleAndNext,
        "Toggle the current selection state",
    ),
    ("<C-a>", A::SelectAll, "Select all files"),
    ("<C-r>", A::InvertSelection, "Invert selection of all files"),
    // Visual mode
    (
        "v",
        A::Visual { unset: false },
        "Enter visual mode (selection mode)",
    ),
    (
        "V",
        A::Visual { unset: true },
        "Enter visual mode (unset mode)",
    ),
    // Seeking
    ("K", A::Seek(-5), "Seek up 5 units in the preview"),
    ("J", A::Seek(5), "Seek down 5 units in the preview"),
    // Spotting
    ("<Tab>", A::Spot, "Spot hovered file"),
    // Operation
    ("o", A::Open { interactive: false }, "Open selected files"),
    (
        "O",
        A::Open { interactive: true },
        "Open selected files interactively",
    ),
    (
        "<Enter>",
        A::Open { interactive: false },
        "Open selected files",
    ),
    (
        "<S-Enter>",
        A::Open { interactive: true },
        "Open selected files interactively",
    ),
    ("y", A::Yank { cut: false }, "Yank selected files (copy)"),
    ("x", A::Yank { cut: true }, "Yank selected files (cut)"),
    ("p", A::Paste { force: false }, "Paste yanked files"),
    (
        "P",
        A::Paste { force: true },
        "Paste yanked files (overwrite if the destination exists)",
    ),
    (
        "-",
        A::Link { relative: false },
        "Symlink the absolute path of yanked files",
    ),
    (
        "_",
        A::Link { relative: true },
        "Symlink the relative path of yanked files",
    ),
    ("<C-->", A::Hardlink, "Hardlink yanked files"),
    ("Y", A::Unyank, "Cancel the yank status"),
    ("X", A::Unyank, "Cancel the yank status"),
    (
        "d",
        A::Remove { permanently: false },
        "Trash selected files",
    ),
    (
        "D",
        A::Remove { permanently: true },
        "Permanently delete selected files",
    ),
    (
        "a",
        A::Create,
        "Create a file (ends with / for directories)",
    ),
    ("A", A::BulkCreate, "Bulk create files"),
    ("r", A::Rename, "Rename selected file(s)"),
    (";", A::Shell { block: false }, "Run a shell command"),
    (
        ":",
        A::Shell { block: true },
        "Run a shell command (block until finishes)",
    ),
    (
        ".",
        A::ToggleHidden,
        "Toggle the visibility of hidden files",
    ),
    ("s", A::Search(SearchVia::Fd), "Search files by name via fd"),
    (
        "S",
        A::Search(SearchVia::Rg),
        "Search files by content via ripgrep",
    ),
    ("<C-s>", A::CancelSearch, "Cancel the ongoing search"),
    ("z", A::Fzf, "Jump to a file/directory via fzf"),
    ("Z", A::Zoxide, "Jump to a directory via zoxide"),
    // Linemode
    ("m s", A::Linemode(Linemode::Size), "Linemode: size"),
    (
        "m p",
        A::Linemode(Linemode::Permissions),
        "Linemode: permissions",
    ),
    ("m b", A::Linemode(Linemode::Btime), "Linemode: btime"),
    ("m m", A::Linemode(Linemode::Mtime), "Linemode: mtime"),
    ("m o", A::Linemode(Linemode::Owner), "Linemode: owner"),
    ("m n", A::Linemode(Linemode::None), "Linemode: none"),
    // Copy
    ("c c", A::Copy(CopyWhat::Path), "Copy file path"),
    ("c C", A::Copy(CopyWhat::Url), "Copy file URL"),
    ("c d", A::Copy(CopyWhat::DirPath), "Copy directory path"),
    ("c D", A::Copy(CopyWhat::DirUrl), "Copy directory URL"),
    ("c f", A::Copy(CopyWhat::Filename), "Copy filename"),
    (
        "c n",
        A::Copy(CopyWhat::NameWithoutExt),
        "Copy filename without extension",
    ),
    // Filter
    ("f", A::Filter, "Filter files"),
    // Find
    ("/", A::Find { previous: false }, "Find next file"),
    ("?", A::Find { previous: true }, "Find previous file"),
    ("n", A::FindArrow { previous: false }, "Next found"),
    ("N", A::FindArrow { previous: true }, "Previous found"),
    // Sorting
    (
        ", m",
        A::Sort {
            by: SortBy::Mtime,
            reverse: false,
        },
        "Sort by modified time",
    ),
    (
        ", M",
        A::Sort {
            by: SortBy::Mtime,
            reverse: true,
        },
        "Sort by modified time (reverse)",
    ),
    (
        ", b",
        A::Sort {
            by: SortBy::Btime,
            reverse: false,
        },
        "Sort by birth time",
    ),
    (
        ", B",
        A::Sort {
            by: SortBy::Btime,
            reverse: true,
        },
        "Sort by birth time (reverse)",
    ),
    (
        ", e",
        A::Sort {
            by: SortBy::Extension,
            reverse: false,
        },
        "Sort by extension",
    ),
    (
        ", E",
        A::Sort {
            by: SortBy::Extension,
            reverse: true,
        },
        "Sort by extension (reverse)",
    ),
    (
        ", a",
        A::Sort {
            by: SortBy::Alphabetical,
            reverse: false,
        },
        "Sort alphabetically",
    ),
    (
        ", A",
        A::Sort {
            by: SortBy::Alphabetical,
            reverse: true,
        },
        "Sort alphabetically (reverse)",
    ),
    (
        ", n",
        A::Sort {
            by: SortBy::Natural,
            reverse: false,
        },
        "Sort naturally",
    ),
    (
        ", N",
        A::Sort {
            by: SortBy::Natural,
            reverse: true,
        },
        "Sort naturally (reverse)",
    ),
    (
        ", s",
        A::Sort {
            by: SortBy::Size,
            reverse: false,
        },
        "Sort by size",
    ),
    (
        ", S",
        A::Sort {
            by: SortBy::Size,
            reverse: true,
        },
        "Sort by size (reverse)",
    ),
    (
        ", r",
        A::Sort {
            by: SortBy::Random,
            reverse: false,
        },
        "Sort randomly",
    ),
    // Goto
    ("g h", A::GoHome, "Go home"),
    ("g c", A::GoConfig, "Go to ~/.config"),
    ("g d", A::GoDownloads, "Go to ~/Downloads"),
    ("g t", A::GoTrash, "Go to trash bin"),
    ("g m", A::GoDevices, "List drives and media"),
    ("g <Space>", A::GoInteractive, "Jump interactively"),
    ("g f", A::Follow, "Follow hovered symlink"),
    // Tabs
    ("t t", A::TabCreate, "Create a new tab in CWD"),
    ("t r", A::TabRename, "Rename current tab"),
    ("1", A::TabSwitch(0), "Switch to first tab"),
    ("2", A::TabSwitch(1), "Switch to second tab"),
    ("3", A::TabSwitch(2), "Switch to third tab"),
    ("4", A::TabSwitch(3), "Switch to fourth tab"),
    ("5", A::TabSwitch(4), "Switch to fifth tab"),
    ("6", A::TabSwitch(5), "Switch to sixth tab"),
    ("7", A::TabSwitch(6), "Switch to seventh tab"),
    ("8", A::TabSwitch(7), "Switch to eighth tab"),
    ("9", A::TabSwitch(8), "Switch to ninth tab"),
    ("[", A::TabStep(-1), "Switch to previous tab"),
    ("]", A::TabStep(1), "Switch to next tab"),
    ("{", A::TabSwap(-1), "Swap current tab with previous tab"),
    ("}", A::TabSwap(1), "Swap current tab with next tab"),
    // Tasks
    ("w", A::Tasks, "Show task manager"),
    // Help
    ("~", A::Help, "Open help"),
    ("<F1>", A::Help, "Open help"),
];

impl Keymap {
    /// The bindings of the file view.
    pub fn standard() -> Self {
        let bindings = BINDINGS
            .iter()
            .map(|(keys, action, desc)| Binding {
                keys: keys
                    .split(' ')
                    .map(|key| Key::parse(key).expect("the built-in keymap is well formed"))
                    .collect(),
                action: *action,
                desc,
            })
            .collect();
        Self { bindings }
    }

    /// Every binding, in the order of the table.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Resolves `key` pressed after the keys in `pending`.
    pub fn feed(&self, pending: &[Key], key: Key) -> Match<'_> {
        let typed = pending.len() + 1;
        let mut continuing = Vec::new();
        for binding in &self.bindings {
            if binding.keys.len() < typed
                || binding.keys[..pending.len()] != *pending
                || binding.keys[pending.len()] != key
            {
                continue;
            }
            if binding.keys.len() == typed {
                return Match::Run(binding);
            }
            continuing.push(binding);
        }
        if continuing.is_empty() {
            Match::None
        } else {
            Match::Pending(continuing)
        }
    }
}

/// Vim's "smartcase": a query with an upper-case letter is matched as
/// written, any other query without regard to case.
pub fn smart_matches(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    if query.chars().any(char::is_uppercase) {
        name.contains(query)
    } else {
        name.to_lowercase().contains(&query.to_lowercase())
    }
}

/// The next (or previous) name that matches `query`, starting beside
/// `cursor` and wrapping around. With `inclusive` the cursor's own name may
/// be the answer, which is what typing a query wants.
pub fn find_step(
    names: &[impl AsRef<str>],
    query: &str,
    cursor: usize,
    previous: bool,
    inclusive: bool,
) -> Option<usize> {
    let count = names.len();
    if count == 0 || query.is_empty() {
        return None;
    }
    let cursor = cursor.min(count - 1);
    (usize::from(!inclusive)..count + usize::from(!inclusive))
        .map(|offset| {
            if previous {
                (cursor + count - offset % count) % count
            } else {
                (cursor + offset) % count
            }
        })
        .find(|&index| smart_matches(names[index].as_ref(), query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn key(text: &str) -> Key {
        Key::parse(text).unwrap()
    }

    #[test]
    fn notation_round_trips() {
        for text in [
            "j",
            "G",
            "<C-u>",
            "<C-[>",
            "<C-->",
            "<Space>",
            "<S-Enter>",
            "<S-PageUp>",
            "<F1>",
            "~",
            "<A-x>",
            "<C-A-Delete>",
        ] {
            assert_eq!(key(text).to_string(), text);
        }
        assert_eq!(key("<C-->").code, Code::Char('-'));
        assert!(key("<C-->").ctrl);
        assert_eq!(key("<S-g>"), Key::char('g'));
        for bad in ["", "<>", "<X-a>", "<Nope>", "ab", "<C-Nope>"] {
            assert_eq!(Key::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_table_is_complete() {
        let map = Keymap::standard();
        assert_eq!(map.bindings().len(), 117);
        let mut seen = HashSet::new();
        for binding in map.bindings() {
            assert!(seen.insert(binding.keys.clone()), "{}", binding.label());
            assert!(!binding.desc.is_empty());
        }
        // A sequence that is a binding is never also the start of another:
        // nothing has to wait for a timeout.
        for binding in map.bindings() {
            for length in 1..binding.keys.len() {
                assert!(!seen.contains(&binding.keys[..length]));
            }
        }
    }

    #[test]
    fn sequences_resolve() {
        let map = Keymap::standard();
        let run = |pending: &[Key], next: &str| match map.feed(pending, key(next)) {
            Match::Run(binding) => Some(binding.action),
            _ => None,
        };
        assert_eq!(run(&[], "j"), Some(Action::Arrow(Step::Next)));
        assert_eq!(run(&[], "G"), Some(Action::Arrow(Step::Bottom)));
        assert_eq!(run(&[], "<C-u>"), Some(Action::Arrow(Step::HalfPageUp)));
        assert_eq!(run(&[], "<C-->"), Some(Action::Hardlink));
        assert_eq!(run(&[key("g")], "g"), Some(Action::Arrow(Step::Top)));
        assert_eq!(run(&[key("g")], "<Space>"), Some(Action::GoInteractive));
        assert_eq!(
            run(&[key(",")], "M"),
            Some(Action::Sort {
                by: SortBy::Mtime,
                reverse: true
            })
        );
        match map.feed(&[], key("g")) {
            Match::Pending(next) => {
                let labels: Vec<String> = next.iter().map(|b| b.label()).collect();
                assert_eq!(
                    labels,
                    ["g g", "g h", "g c", "g d", "g t", "g m", "g <Space>", "g f"]
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(map.feed(&[], key("b")), Match::None);
        assert_eq!(map.feed(&[key("g")], key("z")), Match::None);
        assert_eq!(map.feed(&[key("j")], key("j")), Match::None);
    }

    #[test]
    fn smart_case() {
        assert!(smart_matches("README.md", "read"));
        assert!(smart_matches("README.md", "READ"));
        assert!(!smart_matches("readme.md", "READ"));
        assert!(!smart_matches("readme.md", ""));
    }

    #[test]
    fn find_wraps_in_both_directions() {
        let names = ["alpha", "Beta", "apple", "berry"];
        assert_eq!(find_step(&names, "a", 0, false, true), Some(0));
        assert_eq!(find_step(&names, "ap", 0, false, true), Some(2));
        assert_eq!(find_step(&names, "al", 0, false, false), Some(0));
        assert_eq!(find_step(&names, "be", 1, false, false), Some(3));
        assert_eq!(find_step(&names, "be", 3, false, false), Some(1));
        assert_eq!(find_step(&names, "be", 3, true, false), Some(1));
        assert_eq!(find_step(&names, "be", 1, true, false), Some(3));
        assert_eq!(find_step(&names, "B", 3, false, false), Some(1));
        assert_eq!(find_step(&names, "zz", 0, false, true), None);
        assert_eq!(find_step(&[] as &[&str], "a", 0, false, true), None);
        assert_eq!(find_step(&names, "a", 99, false, true), Some(0));
    }
}
