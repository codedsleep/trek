# PROMPT.md — `tallboy`: PCManFM, rebuilt in Rust

> Working name `tallboy` (a tall chest of drawers). Binary `tallboy`, app ID `io.github.codedsleep.Tallboy`.
> The name appears only in `Cargo.toml`, the app ID, config paths and `data/` — rename before the first commit if wanted.

You are building a complete desktop file manager. Read this whole file before writing code. It is the single source of truth for scope, architecture, conventions and build order.

---

## 1. Mission

Recreate [PCManFM](https://github.com/lxde/pcmanfm) 1.3 (the GTK file manager from LXDE) in Rust, so that a PCManFM user can sit down at `tallboy` and not relearn anything: the same menus and shortcuts, four view modes, tabs, dual pane, Places / Directory Tree side pane, trash, volume management, remote locations, Find Files, Properties, Preferences, and the `--desktop` wallpaper-and-icons manager.

The upstream tagline is "extremely fast and lightweight file manager". That is a requirement, not decoration: see the budgets in §13.

This is a **behavioural port**, not a line-by-line translation. Upstream is C on GTK 2/3 with X11 assumptions; `tallboy` is idiomatic Rust on GTK 4, Wayland-first.

### Source-of-truth order

1. This file.
2. Where this file is silent on a detail (a label, a dialog layout, an edge case), upstream behaviour wins. Read the upstream source: `lxde/pcmanfm` `src/*.c` and [`lxde/libfm`](https://github.com/lxde/libfm) `src/`.
3. Where upstream is X11-only or relies on deprecated GTK, pick the closest GTK 4 / Wayland equivalent, and record the deviation in `.planning/SCOPE.md` under "Deviations".

---

## 2. What you are actually porting

PCManFM itself is a thin shell. Almost everything a user thinks of as "PCManFM" lives in **libfm** (folder model, views, file operations, thumbnails, places, properties, search, templates, actions) and **libfm-gtk** (the widgets). libfm is, in turn, mostly a wrapper around **GIO/GVFS** plus its own job system.

So the port has three layers, and the bulk of the work is re-creating libfm in Rust on top of GIO:

| Upstream | Role | `tallboy` home |
|---|---|---|
| `pcmanfm.c`, `single-inst.c` | entry point, CLI options, single instance, profiles, daemon mode | `tallboy/src/main.rs`, `app.rs`, `cli.rs` |
| `app-config.c` + libfm `fm-config.c` | settings, per-folder settings | `core/config`, `fs/settings_store.rs`, `tallboy/src/settings.rs` |
| `main-win.c`, `main-win-ui.c` | window, menus, toolbar, tabs, dual pane | `tallboy/src/window/` |
| `tab-page.c` | one tab: side pane + folder view + status bar, history | `tallboy/src/tab/` |
| `pref.c` | Preferences dialog | `tallboy/src/dialogs/prefs/` |
| `volume-manager.c` | automount, autorun | `fs/volumes.rs`, `tallboy/src/volumes.rs` |
| `connect-server.c` | Connect to Server dialog | `tallboy/src/dialogs/connect.rs` |
| `desktop.c`, `desktop-ui.c` | wallpaper + desktop icons | `tallboy/src/desktop/` |
| libfm `fm-folder`, `fm-file-info`, `fm-dir-list-job` | directory loading + monitoring | `fs/folder.rs`, `fs/meta.rs` |
| libfm `fm-folder-model`, `fm-standard-view` | sort/filter model, the four views | `core/sort.rs`, `tallboy/src/view/` |
| libfm `fm-file-ops-job*`, `fm-progress-dlg`, `fm-file-ops` | copy/move/delete/trash/link jobs and dialogs | `fs/jobs/`, `tallboy/src/dialogs/progress.rs` |
| libfm `fm-thumbnail*` | thumbnail cache + generation | `fs/thumbs/` |
| libfm `fm-places-*`, `fm-dir-tree-*`, `fm-bookmarks` | side pane | `fs/bookmarks.rs`, `tallboy/src/sidepane/` |
| libfm `fm-file-properties`, `fm-app-chooser-dlg` | Properties, Open With | `tallboy/src/dialogs/` |
| libfm `vfs-search`, `fm-file-search-ui` | Find Files | `core/search.rs`, `fs/search.rs` |
| libfm `fm-templates`, `fm-archiver`, `fm-actions`, `vfs-menu` | templates, archivers, custom actions, applications folder | `core/actions.rs`, `fs/{templates,archiver,appmenu}.rs` |

---

## 3. Decisions already made — do not relitigate

1. **GUI toolkit: GTK 4 via `gtk4-rs`.** Not ratatui (a TUI is not a PCManFM recreation), not iced/egui/Slint. A file manager needs icon themes, MIME/app associations, cross-app drag-and-drop, trash, mounts and remote filesystems; GTK 4 + GIO/GVFS provides all of them and is the same platform upstream stands on.
2. **No libadwaita.** PCManFM is a traditional menubar + toolbar application. No header bars, no hamburger menus, no Adwaita widgets or styling.
3. **GIO for all file access.** Every location is a `gio::File`. Never `std::fs` for user-visible file operations — that would silently break `trash://`, `sftp://`, `smb://`, `mtp://`, `computer://`, `network://`. (`std::fs` is fine for our own config/cache files and for tests.)
4. **Wayland-first.** Must run correctly under Hyprland (primary target), and under X11 for the main window. The desktop manager is Wayland-only, via `wlr-layer-shell`.
5. **Licence: GPL-2.0-or-later**, same as upstream, because this work is derived from upstream's behaviour and you are permitted — expected — to read its source. Include `COPYING` and credit the original authors in the About dialog and `README.md`.
6. **One binary**, three crates in one workspace (§6). No plugin loading.
7. **No async runtime.** The GLib main loop is the only event loop. No tokio, no async-std.
8. **Widgets are built in code.** No `.ui` XML, no Blueprint, no composite templates. Menus are `gio::Menu` models built in Rust.
9. **English only.** No gettext. Keep user-visible strings as plain literals.

---

## 4. Scope

### In scope

Everything in §8. In summary: main window with full menu bar and toolbar; location entry and breadcrumb path bar; tabs; dual pane; icon / thumbnail / compact / detailed-list views; sorting, hidden files, filter, zoom, per-folder settings; type-ahead; opening files with default apps and Open With; executable-file prompt; copy / move / link / trash / delete / restore jobs with progress, pause, conflict and error dialogs; clipboard cut/copy/paste interoperable with other file managers; drag-and-drop; context menus; Places and Directory Tree side panes; bookmarks shared with GTK; trash; volume mounting, automount, autorun; remote locations and Connect to Server; Properties with permissions; thumbnails per the freedesktop spec; Find Files; templates; archiver integration; custom actions (subset); Applications virtual folder; Preferences dialog; profiles; single instance; daemon mode; desktop manager with wallpaper and icons.

### Non-goals

- libfm's module/plugin system (`.so` loading, `tab_page_status` plugins, blacklist/whitelist prefs page).
- X11 desktop manager (root-window wallpaper, `show_wm_menu`, `_NET_WM_DESKTOP` tricks).
- Translations.
- Undo/redo (upstream has none).
- "Duplicate" (a TODO upstream) and "Open Current Folder as Root" (commented out upstream).
- XDS direct-save drag protocol; dragging out of archives.
- Changing folder/launcher icons from Properties.
- Custom-action *menus* (`Type=Menu`) and the rarer DES-EMA keys (`ShowIfTrue`, `ShowIfRunning`, `TryExec`, D-Bus conditions).
- Windows and macOS.
- Sandboxing external thumbnailers.
- Reading PCManFM's config in place. (A one-shot importer is in scope — §8.24.)

If something is not listed in either list, treat it as in scope if PCManFM 1.3 does it and it does not depend on X11 or modules; otherwise note it in SCOPE.md and move on.

---

## 5. Stack and dependencies

- **Rust**: stable, edition 2024. MSRV = whatever the pinned `gtk4` crate requires.
- **`gtk4`** (import as `gtk`) — the 0.11.x series at time of writing, with `glib`/`gio` 0.22.x. Pin to the latest release at scaffold time, enable the `v4_14` feature (needed for modern list widgets, `FileDialog`, `AlertDialog`, `scroll_to`). Do not raise the GTK feature level without a concrete need; record it in SCOPE.md if you do.
- Use the re-exports: `gtk::glib`, `gtk::gio`, `gtk::gdk`, `gtk::gdk_pixbuf`, `gtk::pango`, `gtk::graphene`, `gtk::gsk`. In the `fs` crate depend on `gio`, `glib`, `gdk-pixbuf` directly at matching versions.
- **`gio-unix`** if `DesktopAppInfo` lives there in the pinned release (it has been moving out of `gio`); check docs.rs for the version you pin.
- **`gtk4-layer-shell`** (0.8.x pairs with `gtk4` 0.11) — behind the cargo feature `desktop`, **on by default**.
- **`async-channel`** — worker threads → main loop.
- **`regex`** — Find Files regex name/content matching.
- **`thiserror`** — typed errors in `core` and `fs`.
- **`log`** — logging facade; the binary bridges it to GLib logging.
- Dev-only: **`tempfile`**.

That is the complete allow-list. No clap (GApplication parses options), no serde (config is GKeyFile), no `image` crate (gdk-pixbuf), no `md5` crate (`glib::compute_checksum_for_string`), no `walkdir`, no `trash` crate, no `notify`, no relm4. Adding a dependency requires a line in SCOPE.md saying what it replaces and why hand-rolling was worse.

**Before using any API named in this document, verify it against docs.rs for the pinned versions.** API names here are accurate to the best of the author's knowledge but gtk-rs renames things between releases; the docs win.

### System packages (Fedora)

Build: `gtk4-devel gtk4-layer-shell-devel glib2-devel gdk-pixbuf2-devel gcc pkgconf-pkg-config`
Runtime: `gvfs` (plus `gvfs-smb`, `gvfs-mtp`, `gvfs-fuse` etc. as wanted), `shared-mime-info`, an icon theme.

Put these in `README.md`.

### Release profile

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
```

---

## 6. Repository layout

```
tallboy/
├── Cargo.toml                  # workspace
├── PROMPT.md
├── README.md
├── COPYING                     # GPL-2.0
├── .planning/
│   ├── SCOPE.md                # scope, deviations, slice checklist (you maintain this)
│   └── QA.md                   # manual QA checklist, grown per slice
├── crates/
│   ├── tallboy-core/           # pure Rust. NO glib, NO gio, NO gtk.
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/         # Config struct, defaults, enum<->string codecs
│   │       ├── sort.rs         # SortSpec, comparator
│   │       ├── naming.rs       # unique names, extension splitting, validation
│   │       ├── format.rs       # sizes, dates, permission strings
│   │       ├── glob.rs         # * ? [..] matcher
│   │       ├── location.rs     # Location enum, user-input parsing
│   │       ├── history.rs      # back/forward stack
│   │       ├── search.rs       # SearchQuery <-> search:// URI, predicates
│   │       ├── actions.rs      # custom action file model + condition matching
│   │       ├── exec.rs         # %f %F %u %U %d … field-code expansion
│   │       └── desktop/        # wallpaper geometry, icon grid layout
│   ├── tallboy-fs/             # glib + gio + gdk-pixbuf. NO gtk, NO gdk.
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── meta.rs         # FileMeta (plain Send struct) from gio::FileInfo
│   │       ├── folder.rs       # async loader + monitor -> FolderEvent stream
│   │       ├── jobs/           # engine, copy, move_, delete, trash, link, attrs, count
│   │       ├── thumbs/         # cache paths, validity, generators, worker pool
│   │       ├── settings_store.rs  # KeyFile load/save, profiles, dir-settings
│   │       ├── bookmarks.rs
│   │       ├── volumes.rs
│   │       ├── apps.rs         # default apps, app lists, launching
│   │       ├── terminal.rs
│   │       ├── archiver.rs
│   │       ├── templates.rs
│   │       ├── appmenu.rs      # menu://applications
│   │       ├── search.rs       # walker thread
│   │       └── trash.rs
│   └── tallboy/                # the binary. gtk4 (+ gtk4-layer-shell).
│       └── src/
│           ├── main.rs  app.rs  cli.rs  settings.rs  actions.rs
│           ├── window/         # main window, menubar, toolbar, pathbar, tabs, dual pane
│           ├── tab/            # TabPage, status bar
│           ├── view/           # FileItem GObject, model stack, grid, list, type-ahead, dnd, menus
│           ├── sidepane/       # places, dirtree
│           ├── dialogs/        # progress, conflict, rename, properties, open_with, find, connect, prefs/, …
│           ├── clipboard.rs
│           ├── volumes.rs      # mount-operation UI, autorun dialog
│           └── desktop/        # layer-shell windows, canvas, desktop prefs   [feature = "desktop"]
├── data/
│   ├── io.github.codedsleep.Tallboy.desktop
│   ├── tallboy-desktop-pref.desktop
│   ├── tallboy.svg
│   ├── style.css
│   ├── archivers.list
│   ├── terminals.list
│   └── tallboy.1
└── scripts/
    ├── check.sh                # fmt + clippy + test + smoke
    ├── mkfixture.sh            # builds /tmp/tallboy-fixture (see §11)
    └── install.sh              # cargo build --release + install binary/desktop file/icon to ~/.local
```

### Layering rules (enforced by `Cargo.toml`, not by good intentions)

- `tallboy-core` depends on `regex`, `thiserror`, `log` only. Everything in it is deterministic and unit-tested.
- `tallboy-fs` depends on `tallboy-core`, `glib`, `gio`, `gdk-pixbuf`, `async-channel`. It never touches a widget and never needs a display, so its tests run headless against temp directories.
- `tallboy` depends on both. It contains **no file-operation logic** — it sends requests to `fs`, renders events, and answers questions.
- Dependencies never point upward.

---

## 7. Architecture

### 7.1 Locations

```rust
// tallboy-core
pub enum Location {
    /// Anything GIO can enumerate: file://, trash://, sftp://, smb://, computer://, network://, recent:// …
    Uri(String),
    /// Virtual: live search results.
    Search(SearchQuery),
    /// Virtual: installed applications. None = category list, Some(id) = one category.
    AppMenu(Option<String>),
}
```

`Location::parse(input, cwd, home)` accepts: absolute paths, `~`, `~/x`, any `scheme://` URI, `search://…`, `menu://applications/…`, and paths relative to `cwd`. `Location::to_uri()` round-trips. Display form: native paths show as paths, everything else as URIs.

### 7.2 File metadata

`fs::FileMeta` is a plain `Send` struct built from one `gio::FileInfo`: name (raw), display name, edit name, URI, file type, is_hidden (`standard::is-hidden`, or `standard::is-backup` when `backup_as_hidden`), is_symlink + target, content type, description (from the content type), icon (serialised `gio::Icon` string), size, allocated size, mtime/atime, unix mode/uid/gid, owner/group names, access flags (read/write/execute/delete/trash/rename), target URI (for shortcuts/mountables), filesystem id, trash orig-path + deletion date, and a precomputed filename collation key (GLib's natural, locale-aware filename collation — one case-folded, one raw).

Define the attribute query string once, as a constant, in `meta.rs`. With `defer_content_test=1` request `standard::fast-content-type` instead of `standard::content-type`.

### 7.3 Folder pipeline

```
fs::Folder (per Location, loads + monitors)
   └─ FolderEvent stream ──► tab::TabPage (main thread)
        gio::ListStore<FileItem>
          └─ gtk::FilterListModel   (hidden filter ∧ glob filter)
               └─ gtk::SortListModel (CustomSorter → core::sort::compare)
                    └─ gtk::MultiSelection
                         └─ GridView | ColumnView   (swapped on view-mode change; model + selection survive)
```

- **Loading** is async on the main context: `enumerate_children` then `next_files` in batches of ~200, each batch appended with one `splice`. Never one `append` per file. A `gio::Cancellable` per load; navigating away cancels.
- `FolderEvent`: `Started`, `Batch(Vec<FileMeta>)`, `Finished`, `Error(FolderError)`, `Added(FileMeta)`, `Changed(FileMeta)`, `Removed(uri)`, `Renamed{from, to: FileMeta}`, `FsInfo{free, total}`, `RootGone`.
- **Monitoring**: `monitor_directory` with move tracking. Coalesce events for the same URI inside a ~200 ms window; a changed file is re-queried asynchronously. `RootGone` (folder deleted or unmounted) makes the tab navigate to the nearest existing ancestor, or Home.
- `NotMounted` errors trigger `mount_enclosing_volume` with a `gtk::MountOperation`, then one retry.
- `Location::Search` and `Location::AppMenu` produce the same `FolderEvent` stream from `fs::search` / `fs::appmenu`, so views never special-case them.
- `FileItem` is a GObject (in the UI crate) wrapping one `FileMeta` plus an optional thumbnail `gdk::Texture`. It is the only item type in the list store.

### 7.4 Job engine

All mutating operations are **jobs**. Each job runs on its own `std::thread`, uses synchronous GIO calls with a shared `gio::Cancellable`, and talks to the main thread only through channels.

```rust
// tallboy-fs
pub enum JobKind {
    Copy   { sources: Vec<String>, dest_dir: String },
    Move   { sources: Vec<String>, dest_dir: String },
    Link   { sources: Vec<String>, dest_dir: String },
    Trash  { files: Vec<String> },
    Delete { files: Vec<String> },
    Restore{ files: Vec<String> },                 // trash:// items back to trash::orig-path
    EmptyTrash,
    SetAttrs { files: Vec<String>, change: AttrChange, recursive: bool },
    Count  { files: Vec<String> },                 // recursive size + file count (Properties)
}

pub enum JobEvent {
    Preparing,
    Prepared { total_files: u64, total_bytes: u64 },
    Current  { src: String, dest: Option<String> },
    Progress { done_files: u64, done_bytes: u64, bytes_per_sec: u64 },
    Ask(Question, Reply),                          // worker blocks until Reply is answered
    Finished(JobSummary),                          // ok / cancelled / n errors
}

pub enum Question {
    Conflict { src: FileMeta, dest: FileMeta, suggested_name: String },
    Error    { uri: String, message: String, retryable: bool },
    TrashUnsupported { uri: String },
}
pub enum Answer {
    Overwrite, Skip, Rename(String), Retry, DeleteInstead, Cancel,
    // each of Overwrite/Skip/DeleteInstead can carry apply_to_all
}

pub struct JobHandle { /* events: async_channel::Receiver<JobEvent>; pause(); resume(); cancel(); */ }
pub fn spawn(kind: JobKind) -> JobHandle;
```

Rules:

- Two phases: **prepare** (walk sources, total files/bytes) then **execute**. Progress is by bytes.
- Copy: `gio::File::copy` per file with `NOFOLLOW_SYMLINKS | ALL_METADATA` and a progress callback; directories are traversed by the job, not by GIO.
- Move: first try a native move with no fallback; on "not supported"/cross-filesystem, fall back to copy-then-delete per item.
- Copying into the source's own directory never asks: it auto-renames using `core::naming::unique_name` (`name (copy).ext`, `name (copy 2).ext`).
- Copying/moving a directory into itself is rejected up front.
- Directory conflicts merge; file conflicts raise `Question::Conflict`.
- Pause is cooperative: checked between files and inside the progress callback.
- Cancellation leaves no half-written destination file (delete the partial).
- Progress events are throttled to ≤ 10/s per job.
- Jobs run concurrently; each gets its own progress window.
- The engine is fully testable without GTK: tests drive `JobHandle` and answer `Ask` with a scripted responder.

### 7.5 Threading model

- **Main thread**: all GTK, all `FileItem`s, all async GIO (`glib::spawn_future_local`).
- **Job threads**: one per job (§7.4).
- **Thumbnail pool**: `min(4, cores)` threads (§8.19).
- **Search walker**: one thread per search tab.
- Worker → main: `async_channel`. Main → worker: `Cancellable`, atomics, and reply channels. Never call GTK from a worker. Never block the main thread on a channel.

### 7.6 Settings

- `core::config::Config` is a plain struct mirroring §9, with `Default`.
- `fs::settings_store` loads it from GKeyFile files (`glib::KeyFile`) and saves atomically (write temp, rename).
- `tallboy::settings::Settings` is the one handle the UI uses: typed getters, typed setters, and `connect_changed(Key, callback)`. Every setter marks dirty and schedules a debounced save (~1 s); also save on shutdown. **No widget reads a key file or caches a setting it should observe.** Preferences apply live.

### 7.7 Actions

Every command is a `gio::SimpleAction` on the window (`win.*`) or app (`app.*`), declared in one table in `actions.rs`: name, parameter/state type, accelerators. Menus, toolbar buttons, context menus and shortcuts all activate actions — no command logic lives in a click handler. Toggles and radios are stateful actions. Actions that operate on files resolve against the **active tab's** current selection; `enabled` is kept in sync with selection and location capabilities (e.g. Paste disabled in a read-only folder, Move to Trash hidden inside `trash://`).

---

## 8. Feature specification

### 8.1 Main window

A plain `gtk::ApplicationWindow` (never call `set_titlebar`). Top to bottom: menu bar, toolbar (containing the path bar), then a horizontal split of side pane and folder area (one notebook, or two in dual-pane mode), then the status bar. Window size, maximised state and splitter position persist (`win_width`, `win_height`, `maximized`, `splitter_pos`).

Title: `<folder display name>`. Fullscreen (F11) hides nothing but the decorations.

### 8.2 Menu bar

Reproduce this tree exactly — labels, order, separators, accelerators (taken from upstream `main-win-ui.c`):

**File**
- New Window `Ctrl+N`
- New Tab `Ctrl+T`
- ―
- Create New… ▸ Folder `Ctrl+Shift+N` · Empty File `Ctrl+Alt+N` · ― · *(templates, §8.21)*
- ―
- Folder Properties
- ―
- Close Tab `Ctrl+W`
- Close Window `Ctrl+Q`

**Edit**
- Open
- ―
- Cut `Ctrl+X` · Copy `Ctrl+C` · Paste `Ctrl+V` · Move to Trash `Delete` · Remove `Shift+Delete` · Copy Path(s)
- ―
- Properties `Alt+Return`
- ―
- Rename… `F2` · Create Link… · Move to… · Copy to…
- ―
- Select All `Ctrl+A` · Invert Selection `Ctrl+I`
- ―
- Preferences

**View**
- Reload Folder `F5` (also `Ctrl+R`)
- ―
- Show Hidden `Ctrl+H` ☑
- Sort Files ▸ ◉ Ascending · ◉ Descending · ― · ◉ By Name `Ctrl+Alt+1` · ◉ By Modification Time `Ctrl+Alt+2` · ◉ By Size `Ctrl+Alt+3` · ◉ By File Type `Ctrl+Alt+4` · ◉ By Extension `Ctrl+Alt+5` · ― · ☑ Mingle Files and Folders · ☑ Ignore Name Case
- Folder View Mode ▸ ◉ Icon View `Ctrl+1` · ◉ Compact View `Ctrl+2` · ◉ Thumbnail View `Ctrl+3` · ◉ Detailed List View `Ctrl+4`
- Preserve This Folder's Settings ☑
- ―
- Toolbar ▸ ☑ Show Toolbar · ― · ☑ Show 'New Window' Button · ☑ Show 'New Tab' Button · ☑ Show Navigation Buttons · ☑ Show 'Home' Button
- Path Bar ▸ ◉ Location · ◉ Buttons
- Side Pane ▸ ☑ Show Side Pane `F9` · ― · ◉ Places `Ctrl+6` · ◉ Directory Tree `Ctrl+7`
- Show Status Bar `Ctrl+B` ☑
- ―
- Dual Pane Mode `F3` ☑
- Fullscreen Mode `F11` ☑
- ―
- Zoom In `Ctrl+=` / `Ctrl+KP_Add` · Zoom Out `Ctrl+-` / `Ctrl+KP_Subtract` · Normal Size `Ctrl+0`
- ―
- Filter… `Ctrl+E`

**Bookmarks**
- Add to Bookmarks… `Ctrl+D`
- Edit Bookmarks…
- ―
- *(one item per bookmark)*

**Go**
- Previous Folder `Alt+Left` (also `XF86Back`) · Next Folder `Alt+Right` (also `XF86Forward`) · Parent Folder `Alt+Up`
- ―
- Home Folder `Alt+Home` · Desktop · Trash Can · Applications · Devices · Network
- ―
- Go to Location… `Ctrl+L` (also `Alt+D`) · Connect to Server…

**Tools**
- Open Current Folder in Terminal `F4`
- Find Files… `Ctrl+Shift+F`
- Run a Command in Current Folder…

**Help**
- About
- Keyboard Navigation

### 8.3 Toolbar and path bar

Toolbar, left to right: New Window, New Tab, Back, Forward, Up, Home, path bar (expands), Go/Reload. Each group's visibility follows the `toolbar` setting. Back and Forward show a history popover on right-click or long-press. Use theme icon names (`window-new`, `tab-new`, `go-previous`, `go-next`, `go-up`, `go-home`, `go-jump`); bundle no icons except the app icon.

Path bar has two modes (`pathbar_mode_buttons`):

- **Location**: a text entry showing the display path/URI. `Enter` navigates via `Location::parse`; `Esc` restores. Hand-rolled completion: a popover list of child *directories* of the typed parent, filtered by the typed prefix; `Tab` completes the common prefix; `Down` enters the list. (`GtkEntryCompletion` is deprecated — do not use it.)
- **Buttons**: one toggle button per path segment (breadcrumbs); the deepest visited path stays visible after going up, with the current segment active; scroll arrows when it overflows; middle-click a segment opens it in a new tab. `Ctrl+L` temporarily swaps to the entry until focus leaves.

### 8.4 Tabs

`gtk::Notebook`. Tab label = folder display name, ellipsised to `max_tab_chars`, tooltip = full path, close button unless `hide_close_btn`. Tab bar hidden with one tab unless `always_show_tabs`. Reorderable by drag. Middle-click closes. New Tab duplicates the current folder. Closing the last tab closes the window.

Shortcuts: `Ctrl+Tab` / `Ctrl+PageDown` next, `Ctrl+Shift+Tab` / `Ctrl+PageUp` previous, `Alt+1`…`Alt+9`, `Alt+0` jump to tab 1–10.

`focus_previous`: after closing a tab, activate the previously active tab instead of the neighbour.

Each tab owns: its `Location`, a `core::history::History` (back/forward, each entry remembering scroll position and cursor item), view mode, sort spec, show-hidden, filter, and its `fs::Folder`. Going **up** selects and scrolls to the folder you came from.

### 8.5 Dual pane

`F3` toggles a second notebook beside the first, inside a `gtk::Paned`. Each pane has independent tabs. Exactly one pane is **active** (marked with a CSS class on its frame); the menu bar, path bar, side pane, status bar and all `win.*` file actions follow the active pane. Clicking or focusing a pane activates it; `F6` cycles focus side pane → pane 1 → pane 2. Turning dual pane on opens the second pane at the current folder. Drag-and-drop between panes is the primary copy/move workflow.

### 8.6 Folder views

Four modes sharing one model and one selection:

| Mode | Widget | Icon size key | Layout |
|---|---|---|---|
| Icon | `GridView` (vertical) | `big_icon_size` (48) | icon above, name below, wrapped to 3 lines (all lines when `show_full_names`) |
| Thumbnail | `GridView` (vertical) | `thumbnail_size` (128) | as Icon, larger |
| Compact | `GridView` (horizontal orientation) | `small_icon_size` (24) | icon left, single-line name right; fills columns top-to-bottom, scrolls horizontally |
| Detailed list | `ColumnView` | `small_icon_size` | columns below |

Detailed-list columns (ids are libfm's): `name` (icon + name), `desc` (file type description), `size`, `perm` (e.g. `-rw-r--r--`), `owner`, `mtime`, `dirname` (search results only), `ext`. Default visible: name, desc, size, mtime. Header right-click toggles columns; visibility and widths persist as `columns=name:240;desc;size;mtime;`. Directories show no size.

Common to all modes:

- Thumbnails replace icons when `show_thumbnail` is on, requested at the mode's icon size (§8.19).
- Symlinks get a small emblem overlay. Hidden items render at reduced opacity when `shadow_hidden`.
- Rubber-band selection (`enable-rubberband`), Ctrl/Shift click extension.
- `single_click`: single click activates; hovering selects after `auto_selection_delay` ms.
- Zoom changes the current mode's size key in steps (16, 24, 32, 48, 64, 96, 128, 192, 256) and persists it.
- Empty folder shows a centred "Folder is empty" placeholder; a failed load shows the error and a Retry button.
- List item factories must be cheap: build widgets in `setup`, bind data in `bind`, and **disconnect everything in `unbind`**. No per-item closures that hold strong references to the tab.

**Column-header sorting.** `ColumnViewSorter` inverts the whole comparison when descending, which would put folders last. So: the `SortListModel` always uses *our* `CustomSorter`. Give each column a trivial sorter only so headers are clickable and show arrows; listen to the column view sorter's `changed`, read its primary column and order, and write them into the tab's `SortSpec`. When the spec changes from the menu, call `sort_by_column` to keep the header arrow in sync (guard against feedback loops).

### 8.7 Sorting, hidden files, filter, per-folder settings

```rust
pub struct SortSpec { pub by: SortBy, pub descending: bool, pub case_sensitive: bool, pub mingle: bool }
pub enum SortBy { Name, Desc, Size, Perm, Owner, Mtime, Dirname, Ext }
```

- Folders sort before files unless `mingle`; this holds in **both** directions.
- Name comparison uses the precomputed collation keys (natural order: `file2` < `file10`).
- Any non-name key ties break by name ascending.
- Serialised as `name;ascending;` plus optional `case;` and `mingle;` (upstream format).
- Show Hidden toggles the hidden filter; the status bar reports how many are hidden.
- **Filter…** asks for a glob (`*.rs`, `IMG_??.jpg`); applied to the current tab on top of the hidden filter; shown in the status bar; empty pattern clears it. Folders always pass.
- **Preserve This Folder's Settings**: when off, changing view mode / sort / show-hidden / columns updates the global defaults. When on, they are saved for this folder only, in `dir-settings.conf` under a group named by the folder's path or URI (`Sort=`, `ViewMode=`, `ShowHidden=`, `Columns=`). Unchecking deletes the group. Search folders are always detailed list, show hidden, columns `name;desc;dirname;size;mtime`.

### 8.8 Selection and keyboard

`Enter` open · `Backspace` parent · `Alt+Return` properties · `Delete` trash (or delete when `use_trash=0`) · `Shift+Delete` delete · `F2` rename · `Menu` / `Shift+F10` context menu · `Esc` clears selection and closes type-ahead · arrows / Home / End / PageUp / PageDown move the cursor.

**Type-ahead**: typing a printable character over a view opens a small entry overlaid at the view's bottom-right; the first item whose name starts with the text (case-folded) is selected and scrolled to; `Up`/`Down` cycle matches; `Enter` opens; `Esc` or 5 s idle closes.

**Critical**: file shortcuts (`Delete`, `Ctrl+C`, `Backspace`, …) must never fire while any text entry has focus. Verify this for the location entry, type-ahead entry and every dialog.

Help ▸ Keyboard Navigation shows a read-only window listing all of the above plus the tab and pane shortcuts.

### 8.9 Opening things

- Folder → navigate in place. Middle-click or `Ctrl+Enter` → new tab.
- File → default application for its content type; multiple selected files are grouped per application and passed in one launch. Launch with a `gdk::AppLaunchContext` so startup notification / activation tokens work.
- No default application → Open With dialog.
- `standard::target-uri` shortcuts (`computer://`, `network://`) → follow the target. Mountables → mount, then open.
- **Executables**: for an executable text file ask "This text file seems to be an executable script. What do you want to do with it?" — Execute / Execute in Terminal / Open / Cancel. For an executable binary: Execute / Execute in Terminal / Cancel. `quick_exec=1` executes without asking. Working directory = the file's folder.
- `.desktop` launchers: launch the application. If the file is neither executable nor under an XDG applications directory, confirm first ("Untrusted application launcher").
- **Open With dialog** (hand-built; `GtkAppChooserDialog` is deprecated): searchable list of installed applications, recommended ones for the type first; an expander "Custom Command Line" with command entry, "Execute in terminal emulator", and an application name; a checkbox "Set selected application as default action for this file type". Returns an `AppInfo`.
- **Terminal**: `terminal` setting is a command line. Open-in-terminal spawns it with the folder as working directory. Execute-in-terminal appends the terminal's exec flag from `data/terminals.list` (ship entries for foot, kitty, alacritty, wezterm, ghostty, ptyxis, gnome-terminal, konsole, xfce4-terminal, lxterminal, xterm; unknown → `-e`). Unset → `$TERMINAL`, else the first installed entry; none → error dialog that opens Preferences ▸ Advanced. Disabled for locations with no local path.
- **Run a Command in Current Folder…**: command entry, "Execute in terminal emulator", "Keep terminal window open after command execution".

### 8.10 File operations (UI side of §7.4)

- **Progress dialog**: appears only if the job is still running after 500 ms. Shows operation title, current file, destination, a progress bar, "x of y (speed, time remaining)", Pause/Resume, Cancel. Errors collected into an expandable list; the dialog stays open after completion only if there were errors.
- **Conflict dialog**: both files with icon, size and modification time; Overwrite / Skip / Rename (with editable new name, pre-filled with the suggested unique name) / Cancel; "Apply this option to all existing files".
- **Error dialog**: message; Skip / Skip All / Retry (when retryable) / Cancel.
- **Move to Trash**: confirm when `confirm_trash` ("Do you want to move the file 'x' to trash can?" / "…the N selected files…"). If trashing is unsupported, or the file is on removable media with `no_usb_trash`, ask to delete instead.
- **Remove (delete)**: confirm when `confirm_del`.
- **Rename…**: a dialog (upstream renames in a dialog, not inline) pre-filled with the edit name, selection covering the base name without extension. Uses `set_display_name`. Reject empty, `/`, `.` and `..`.
- **Create New ▸ Folder / Empty File**: dialog asking for the name, default "New" — pick a unique name if it exists. The new item is selected and scrolled to when it appears.
- **Create Link…**, **Move to…**, **Copy to…**: choose a destination folder with `gtk::FileDialog::select_folder`, then spawn the job.
- **Copy Path(s)**: newline-separated native paths (URIs for non-native) to the clipboard as text.

### 8.11 Clipboard

Cut/Copy must interoperate with Nautilus, Thunar, Nemo, PCManFM and Dolphin. Publish one content provider (a union) offering:

- `x-special/gnome-copied-files`: `cut\n` or `copy\n` followed by newline-separated URIs,
- `text/uri-list`,
- `application/x-kde-cutselection` (`1` when cut),
- a `gdk::FileList` value,
- plain text (paths).

Paste reads `x-special/gnome-copied-files` first to learn cut vs copy, then falls back to `text/uri-list` / `gdk::FileList` (treated as copy). Paste after cut spawns a Move and clears the clipboard. Paste targets the current folder, or the single selected folder when invoked from its context menu. Paste is disabled when the clipboard holds no files.

### 8.12 Drag and drop

- **Source**: dragging an item drags the whole selection if the item is selected, otherwise only that item. Offers `gdk::FileList` + `text/uri-list`; actions copy | move | link. Drag icon = the item icon (with a count badge for multiple).
- **Targets**: a folder item (drop into it), the view background (current folder), tab labels (the tab's folder), places and directory-tree rows, bookmarks area (dropping a folder between bookmarks adds a bookmark), desktop.
- **Action**: Ctrl = copy, Shift = move, Ctrl+Shift = link. With no modifier use `drop_default_action`: `auto` (same filesystem id → move, otherwise copy), `copy`, `move`, or `ask` (popover at the drop point: Copy Here / Move Here / Link Here / Cancel).
- Hovering a drag over a tab label for 600 ms switches to that tab when `change_tab_on_drop`. Hovering a collapsed tree row for 1 s expands it.
- Dropping onto the source's own folder with move is a no-op; dropping a folder into itself is refused.
- Drops from other applications (browsers, other file managers) work.

### 8.13 Context menus

`gtk::PopoverMenu` from `gio::Menu`, no arrow, positioned at the pointer. Right-clicking an unselected item selects only it first; right-clicking empty space clears the selection.

**File/selection menu**: Open · Open With ▸ (applications for the type, ―, Open With…) · *[single folder]* Open in New Tab, Open in New Window, Open in Terminal · ― · Cut · Copy · Paste *(single folder only)* · Move to Trash / Remove · Copy Path(s) · ― · Rename… · *(custom actions, §8.23)* · *[archiver]* Compress…, Extract Here, Extract To… · ― · Properties.
Inside `trash://`: Restore · Remove · Properties only.

**Folder background menu**: Create New ▸ · Paste · ― · Select All · Invert Selection · ― · Sort Files ▸ · Show Hidden · ― · *(custom actions for the folder)* · Open in Terminal · ― · Properties.

### 8.14 Side pane

A header button switches mode (Places / Directory Tree); `F9` hides the pane.

**Places** — three sections:

1. Fixed places, each gated by its `[places]` key: Home, Desktop, Trash (icon reflects empty/full; context menu Empty Trash Can), Filesystem Root, Applications, Devices (`computer:///`), Network (`network:///`).
2. Volumes and mounts from `gio::VolumeMonitor`: mounted ones show an eject/unmount button; unmounted volumes listed when `places_unmounted`; internal volumes only when `show_internal_volumes`. Clicking an unmounted volume mounts then opens it. Context menu: Open in New Tab / New Window, Mount Volume, Unmount Volume, Eject Removable Media.
3. Bookmarks. Context menu: Open in New Tab / New Window, Rename Bookmark Item, Remove from Bookmarks, Move Bookmark Up / Down. Reorder by drag.

The row matching the current location is highlighted. Activation honours `bm_open_method` for bookmarks. Icons at `pane_icon_size`.

**Directory Tree** — `gtk::TreeListModel` + `gtk::TreeExpander` in a `ListView`; roots are Home and Filesystem Root (plus mounted volumes). Children are directories only, loaded lazily on expand, monitored while expanded, sorted by name. Follows the active tab: navigating expands and selects the path. Honours Show Hidden. Rows without subdirectories show no expander when `no_child_non_expandable`.

### 8.15 Bookmarks

Stored in `~/.config/gtk-3.0/bookmarks` — the file GTK file choosers and other file managers share — one `URI[ label]` per line. Monitor the file and reload on external change; write atomically. **Add to Bookmarks…** asks for a label (default: folder name). **Edit Bookmarks…** is a list dialog with rename, remove, move up/down.

### 8.16 Trash

`trash:///` is an ordinary GIO location. Extras: Restore (from `trash::orig-path`, as a `Restore` job with conflict handling), Empty Trash Can (confirm, then `EmptyTrash` job), deletion date available as a column tooltip. Deleting inside trash is permanent. Cut/Rename/Create New are disabled there.

### 8.17 Volumes and remote locations

- On startup, when `mount_on_startup`: mount every volume that wants automounting.
- On volume added, when `mount_removable` and the drive is removable: mount it.
- After such a mount, when `autorun`: show the **autorun dialog** — "Removable medium is inserted", the detected content type (`x-content/*`), a list of applications handling it plus "Open in File Manager", and "Always perform this action for this type of media". Choices persist in `[autorun]` (`content-type=app.desktop`, prefixed `*` when not to ask again). "Open in File Manager" honours `media_in_new_tab`.
- On mount removed: tabs inside it close when `close_on_unmount`, otherwise go Home.
- Mount/unmount/eject use `gtk::MountOperation` for passwords and "busy" prompts. Unmount failures show the error; never force.
- **Connect to Server…**: type (SSH, FTP, WebDAV, Secure WebDAV, Windows share, public FTP), host, port, path, anonymous or username → builds the URI and navigates; GVFS prompts for credentials.
- Free-space display and trash gracefully degrade on remotes. Thumbnails are skipped on non-native files when `thumbnail_local`.

### 8.18 Properties dialog

One window for one or many files; two tabs.

**General**: icon; Name (editable for a single file → rename on OK); Location; File type (description + MIME); Open with (dropdown of applications for the type, "Customize…" → Open With dialog; sets the default) — hidden for folders and mixed selections; Link target for symlinks; Total size of files and Size on disk — for folders and multi-selections computed by a `Count` job with a live-updating label and file count, cancelled on close; Last modification; Last access; Last permissions change.

**Permissions**: Owner and Group (editable only when permitted); three dropdowns — "View content", "Change content", and "Execute" (files) / "Access content" (folders) — each: Nobody / Only owner / Only owner and group / Anyone, plus a mixed state for heterogeneous selections; with `advanced_mode`, Special bits (Set SUID / Set SGID / Sticky). On OK, if any folder is selected and permissions changed, ask "Do you want to recursively apply these changes to all files and sub-folders?". Applied through a `SetAttrs` job.

### 8.19 Thumbnails

Implement the freedesktop **Thumbnail Managing Standard**, sharing the cache with every other compliant application.

- Cache path: `$XDG_CACHE_HOME/thumbnails/<dir>/<md5(uri)>.png`, where `<dir>` is `normal` (128), `large` (256), `x-large` (512) or `xx-large` (1024) — choose the smallest ≥ requested pixel size × scale factor.
- Test vector: the MD5 of `file:///home/jens/photos/me.png` is `c6ee772d9e49320e97ec29a7eb5b1697`.
- A cached thumbnail is valid when its `Thumb::URI` tEXt chunk equals the URI and `Thumb::MTime` equals the file's mtime (seconds).
- Failures are recorded as an empty-image PNG with the same chunks in `thumbnails/fail/tallboy-<version>/` and not retried until mtime changes.
- Generators: (1) built-in for `image/*` via gdk-pixbuf — load at scale, apply embedded orientation; (2) external thumbnailers from `<XDG data dirs>/thumbnailers/*.thumbnailer` (`[Thumbnailer Entry]`, `Exec` with `%i %u %o %s`, `MimeType`), each run with a 10 s timeout and killed on overrun.
- Write to a temp file in the same directory, mode 0600, then rename. Store `Thumb::URI`, `Thumb::MTime`, `Thumb::Size`, `Thumb::Mimetype`.
- Skip files larger than `thumbnail_max` KiB (0 = no limit), files inside the thumbnail cache itself, and non-native files when `thumbnail_local`.
- **Scheduling**: a request is made when an item is *bound* to a visible cell and cancelled when unbound or when the folder changes. Most-recent-first queue. Decode PNGs into `gdk::Texture` on the worker; deliver textures to the main thread. Keep an in-memory LRU (~500 textures) keyed by (URI, mtime, size dir).
- Deleting/renaming a file does not need cache cleanup.

### 8.20 Find Files

`Ctrl+Shift+F` or `tallboy --find-files` opens the dialog, four tabs:

- **Name/Location**: file name patterns (comma-separated globs), case insensitive ☑, use regular expression ☐; "Places to search" list with Add/Remove (pre-filled with the current folder); search in sub directories ☑; search for hidden files ☐.
- **File Type**: only search for files of these types — Text files, Image files, Audio files, Video files, Documents, Folders, Other (free-form MIME list).
- **Content**: file contains (text), case insensitive, regular expression.
- **Properties**: bigger than / smaller than (value + KiB/MiB/GiB); last modified earliest / latest (dates).

OK opens a **new tab** at a `search://` location:

```
search://<path1>,<path2>?recursive=1&show_hidden=0&name=<globs>&name_regex=<re>&name_ci=1
        &content=<text>&content_regex=<re>&content_ci=1&mime_types=<a/b;c/*>
        &min_size=<bytes>&max_size=<bytes>&min_mtime=<YYYY-MM-DD>&max_mtime=<YYYY-MM-DD>
```

`core::search` owns `SearchQuery`, the URI codec (percent-encoded, round-trip tested) and the pure predicate `matches(&FileFacts) -> bool`. `fs::search` walks on a thread, applies cheap predicates first (name, type, size, mtime), content last (streamed, skip files containing NUL in the first 8 KiB), and streams results as `FolderEvent::Batch`. The tab shows a spinner and "Searching…" in the status bar; closing the tab or navigating cancels. Results support every normal file action; `dirname` column shows where each lives; "Open containing folder" in the context menu.

### 8.21 Templates

Sources: the user's Templates directory (`glib::user_special_dir`), plus `<XDG data dirs>/templates/` unless `only_user_templates`. Each file is a template; a `.desktop` file with `Type=Link`/`URL=` points at one and supplies `Name`, `Icon`, `Comment`. With `template_type_once`, keep one template per MIME type (user's wins). They appear under Create New, below Folder and Empty File. Choosing one asks for a file name (default `New <template name>.<ext>`), copies the template, selects the result, and when `template_run_app` opens it.

### 8.22 Archiver integration

`data/archivers.list` (key file, one group per archiver: `create=`, `extract=`, `extract_to=`, `mime_types=`, with `%F`/`%U`/`%d` codes) ships entries for file-roller, xarchiver, engrampa, ark and lxqt-archiver. The `archiver` setting names one; empty picks the first installed. Menu items: **Compress…** (any selection), **Extract Here** and **Extract To…** (only when every selected file's type is in the archiver's `mime_types`). No archiver installed → items absent.

### 8.23 Custom actions (subset of DES-EMA)

Load `*.desktop` from `<XDG data dirs>/file-manager/actions/` (user dir wins on id clash).

- `[Desktop Entry]`: `Type=Action`, `Name`, `Tooltip`, `Icon`, `Enabled`, `Hidden`, `TargetContext` (default true), `TargetLocation` (default false), `Profiles=id1;id2;`.
- `[X-Action-Profile <id>]`: `Name`, `Exec`, `Path`, `ExecutionMode` (`Normal` | `Terminal`), and conditions `MimeTypes`, `Basenames`, `MatchCase`, `SelectionCount` (`=N`, `>N`, `<N`), `Schemes`, `Folders`, `Capabilities` (`Owner`, `Readable`, `Writable`, `Executable`, `Local`). List conditions support `*` wildcards and `!` negation.

An action shows when its first matching profile matches the whole selection (context) or the current folder (location). Parsing, condition matching and `Exec` parameter expansion (`%b %B %d %D %f %F %u %U %w %W %x %X %n %%`) live in `core::actions` and are unit-tested from fixture files. `Type=Menu` files are ignored.

### 8.24 Applications folder, About, importer

- **`menu://applications/`**: root lists the standard main categories that have at least one visible application (AudioVideo, Development, Education, Game, Graphics, Network, Office, Science, Settings, System, Utility, plus Other); a category lists its applications. Activating launches. Items are read-only: only Open and Properties (read-only) apply; dragging one out copies its `.desktop` file.
- **About**: `gtk::AboutDialog` — name, version, GPL-2.0-or-later, and a credit line: "Based on PCManFM by Hong Jen Yee (PCMan) and Andriy Grytsenko (LStranger)".
- **`tallboy --import-pcmanfm`**: one-shot. Reads `~/.config/pcmanfm/<profile>/pcmanfm.conf` and `~/.config/libfm/libfm.conf`, maps same-named keys into our config (§9 deliberately keeps upstream key names), writes it, prints what was imported, exits. Never runs automatically.

### 8.25 Preferences dialog

`Edit ▸ Preferences`, or `tallboy --show-pref=N` (1-based page). Every control is bound to `Settings` and applies immediately.

1. **General** — Behavior: open files with single click; delay of automatic selection in single click mode; move deleted files to "trash can" instead of erasing; erase files on removable media instead of "trash can" creation; confirm before moving files into "trash can"; confirm before deleting files; don't ask options on launch executable file; default drag and drop action; use SI decimal prefixes instead of IEC binary prefixes; treat backup files as hidden; always show full file names; default view mode; bookmarks open in (current tab / new tab / new window).
2. **Display** — size of big icons, small icons, thumbnails, side pane icons; show thumbnails of files; only show thumbnails for local files; do not generate thumbnails for image files exceeding this size (KB); display hidden files' icons shadowed; hide 'Close tab' buttons; maximum tab label width.
3. **Layout** — always show the tab bar; show Home / Desktop / Trash can / Filesystem root / Applications / Devices / Network in Places; show unmounted volumes; show internal volumes; no expander for folders without children in the tree; path bar mode; show status bar; activate previous tab on close.
4. **Volume Management** — mount mountable volumes automatically on program startup; mount removable media automatically when they are inserted; show available options for removable media when they are inserted; close tab containing removable medium when it is unmounted; open removable media in a new tab; a "forget remembered autorun choices" button.
5. **Advanced** — terminal emulator; archiver integration (dropdown of installed archivers); templates: show only user defined templates in menu / show only one template for each MIME type / run default application after creation from template; show special permission bits in Properties; home folder override.

### 8.26 Status bar

Left: `N item(s)` and ` (M hidden)` when applicable; with a selection: one file → `"name" (size) description`, several → `N item(s) selected (total size)`; plus `[filter: *.rs]` when filtered. Right: `Free space: X (Total: Y)` when the filesystem reports it. Sizes use `si_unit`.

### 8.27 Command line, single instance, profiles

Options (names and letters as upstream):

| Option | Meaning |
|---|---|
| `-p, --profile=PROFILE` | configuration profile (default `default`). First-instance only. |
| `-d, --daemon-mode` | start with no window and stay running. First-instance only. |
| `--no-desktop` | accepted, no effect (Nautilus compatibility). |
| `--desktop` | start the desktop manager. |
| `--desktop-off` | stop the desktop manager. |
| `--desktop-pref` | open Desktop Preferences. |
| `--one-screen` | with `--desktop`: manage only the monitor under the pointer/focus. |
| `-w, --set-wallpaper=FILE` | set wallpaper (must be a loadable image; if mode is `color`, switch to `fit`). |
| `--wallpaper-mode=MODE` | `color` `stretch` `fit` `crop` `center` `tile` `screen`. |
| `--show-pref=N` | open Preferences at page N. |
| `-n, --new-win` | open in a new window. |
| `-f, --find-files` | open Find Files. |
| `--role=ROLE` | accepted, no effect (X11 window role has no GTK 4 / Wayland equivalent). |
| `--import-pcmanfm` | §8.24. |
| `[FILE…]` | paths or URIs to open. `~` means Home. Relative paths resolve against the **caller's** cwd. |

- `gtk::Application` with `HANDLES_COMMAND_LINE`. GApplication provides single-instance and forwards later invocations (arguments + cwd) to the primary instance, which runs the same `run(args, cwd)` function.
- The profile is part of the application ID, so each profile is its own instance: `io.github.codedsleep.Tallboy` for `default`, `io.github.codedsleep.Tallboy.p_<sanitised>` otherwise. Pre-scan `argv` for the profile before constructing the application.
- With no files: open the caller's cwd in a new window — unless this is the first run in daemon mode. Folders given on the command line open as tabs in the last active window (or a new window with `-n`); files are opened with their applications.
- The process stays alive while any window, the desktop manager, a running job, or daemon mode holds it.
- `SIGTERM`/`SIGINT`/`SIGHUP`: save settings and quit cleanly.

### 8.28 Desktop manager  *(cargo feature `desktop`)*

`tallboy --desktop` draws wallpaper and desktop icons. Wayland only, via `gtk4-layer-shell`. If layer shell is unsupported (X11, GNOME), print `Desktop manager requires a Wayland compositor with wlr-layer-shell.` and exit non-zero for a first instance (or show the error and carry on for a running one). Nothing else in the application depends on this feature.

**Surfaces.** One layer-shell window per monitor (only one with `--one-screen`): layer `Background`, anchored to all four edges, exclusive zone `-1`, namespace `tallboy-desktop`, keyboard mode `OnDemand`. Follow monitor hot-plug. Icons are laid out inside the monitor area minus `[desktop] margins` (default `0;0;0;0`, order top;right;bottom;left) so they can clear panels. Read the gtk4-layer-shell README about link order before debugging a surface that will not anchor.

**Canvas.** One custom widget (`DesktopCanvas`, the app's only non-`FileItem` GObject subclass besides trivially small row objects) that draws everything itself in `snapshot`: wallpaper, icons, labels, selection, rubber band. No child widgets per icon; hit-testing is against the layout rectangles.

**Wallpaper.** Modes: `color` (solid `desktop_bg`), `stretch`, `fit` (letterboxed on `desktop_bg`), `crop` (cover), `center`, `tile`, `screen` (one image stretched across the bounding box of all monitors). `core::desktop::wallpaper_ops(mode, image_size, monitor_rect, screen_rect)` returns the draw rectangles/tiling — pure and unit-tested. `wallpaper_common=1` uses `wallpaper` everywhere; otherwise per-monitor `wallpaper.<connector>` keys. Scaled textures are cached per (file, mode, monitor size); changing wallpaper or mode at runtime (CLI or dialog) redraws without restart.

**Icons.** Items of the desktop folder (`folder` key, default the XDG Desktop dir), loaded through the same `fs::Folder`, plus optional extras: Trash (`show_trash`), Documents (`show_documents`), mounted volumes (`show_mounts`). Icon size `big_icon_size`; label in `desktop_font`, colour `desktop_fg`, 1 px shadow in `desktop_shadow`, up to 3 lines.

**Layout.** `core::desktop::layout(items, area, cell)`: items with a saved position are placed first; the rest, sorted by the desktop `sort` (default mtime ascending, mingled), fill the grid column by column, top to bottom, left to right, skipping occupied cells. Positions persist per monitor in `desktop-items-<connector>.conf` (group = file name, `x=`, `y=`). Dragging an icon moves it and pins it.

**Interaction.** Single/double click per `single_click`; rubber band; keyboard navigation; full drag-and-drop as a target for the desktop folder and for folder icons. The item context menu is the normal file menu plus ☑ **Stick to Current Position** and **Snap to Grid**; extras (Trash, Documents, volumes) get **Remove from Desktop** instead. The background menu is the folder menu plus **Desktop Preferences**. Opening a folder uses the last active window, or a new one when `desktop_folder_new_win`.

**Desktop Preferences** (`--desktop-pref`): *Appearance* — wallpaper mode, wallpaper file (`gtk::FileDialog`), use the same wallpaper on all monitors, background colour, label text colour, label shadow colour, label font; *Desktop Icons* — show Documents / Trash Can / connected volumes; *Advanced* — desktop folder, margins. Live-applied.

Ship `data/tallboy-desktop-pref.desktop` and document a Hyprland `exec-once = tallboy --desktop` line in the README.

---

## 9. Configuration reference

Files, per profile `<p>` (default `default`):

- `$XDG_CONFIG_DIRS/tallboy/<p>/tallboy.conf` — system defaults, loaded first (each dir, lowest priority first)
- `$XDG_CONFIG_HOME/tallboy/<p>/tallboy.conf` — user
- `$XDG_CONFIG_HOME/tallboy/<p>/dir-settings.conf` — per-folder settings
- `$XDG_CONFIG_HOME/tallboy/<p>/desktop-items-<connector>.conf` — pinned desktop icon positions

GKeyFile format. Booleans are `0`/`1`. Unknown keys are preserved on save. Key names match PCManFM/libfm on purpose.

```ini
[config]
bm_open_method=0            ; bookmarks open in: 0 current tab, 1 new tab, 2 new window
home_path=                  ; Home override; empty = $HOME
single_click=0
auto_selection_delay=600    ; ms, 0 disables hover-select
use_trash=1
confirm_del=1
confirm_trash=1
no_usb_trash=1
quick_exec=0
drop_default_action=auto    ; auto | copy | move | ask
si_unit=0                   ; 0 = KiB/MiB (1024), 1 = kB/MB (1000)
backup_as_hidden=1
show_full_names=0
advanced_mode=0
only_user_templates=0
template_type_once=0
template_run_app=0
defer_content_test=0
no_child_non_expandable=0
show_internal_volumes=0
terminal=
archiver=
thumbnail_local=1
thumbnail_max=2048          ; KiB, 0 = unlimited

[volume]
mount_on_startup=1
mount_removable=1
autorun=1

[autorun]
; x-content/image-dcf=*org.gnome.Loupe.desktop      (* = don't ask again)

[ui]
always_show_tabs=0
hide_close_btn=0
max_tab_chars=32
win_width=640
win_height=480
maximized=0
splitter_pos=150
media_in_new_tab=0
desktop_folder_new_win=0
change_tab_on_drop=1
close_on_unmount=1
focus_previous=0
side_pane_mode=places       ; [hidden;]places | [hidden;]dirtree
view_mode=icon              ; icon | compact | thumbnail | list
show_hidden=0
sort=name;ascending;        ; <col>;ascending|descending;[case;][mingle;]
columns=                    ; e.g. name:240;desc;size;mtime;
toolbar=newtab;navigation;home;   ; any of hidden; newwin; newtab; navigation; home;
show_statusbar=1
pathbar_mode_buttons=0
dual_pane=0
big_icon_size=48
small_icon_size=24
pane_icon_size=24
thumbnail_size=128
show_thumbnail=1
shadow_hidden=0

[places]
places_home=1
places_desktop=1
places_root=0
places_computer=1
places_trash=1
places_applications=1
places_network=0
places_unmounted=1

[desktop]
wallpaper_mode=color        ; color | stretch | fit | crop | center | tile | screen
wallpaper_common=1
wallpaper=
; wallpaper.DP-1=/path      ; per-monitor when wallpaper_common=0
desktop_bg=#000000
desktop_fg=#ffffff
desktop_shadow=#000000
desktop_font=
folder=
margins=0;0;0;0             ; top;right;bottom;left
sort=mtime;ascending;mingle;
show_documents=0
show_trash=1
show_mounts=0
```

Parsing rules: an unknown enum string or out-of-range number falls back to the default and logs one warning; a corrupt file is renamed to `tallboy.conf.broken` and defaults are used. Round-trip (`load → save → load`) must be identity.

---

## 10. Conventions

**Rust**

- `cargo fmt` defaults. `cargo clippy --workspace --all-targets -- -D warnings`.
- `#![deny(unsafe_code)]` in `tallboy-core` and `tallboy-fs`. No hand-written `unsafe` anywhere; if a binding genuinely forces it, isolate it in one function with a `// SAFETY:` comment and note it in SCOPE.md.
- No `unwrap()`/`expect()`/`panic!` outside tests, except `expect` on true invariants with a message saying why it cannot fail. Enable `clippy::unwrap_used` in `core` and `fs`.
- Errors: `thiserror` enums per module in `core`/`fs`. The UI converts them to user-facing messages in one place (`dialogs/error.rs`). Never show a `Debug` dump to the user; never swallow an error silently — log it at least.
- No `String` paths for user files in `fs` APIs beyond URIs; convert at the boundary with `gio::File::for_uri` / `for_path`. Never assume file names are UTF-8: use display/edit names for text, raw names for operations.
- Enums over booleans for modes. Exhaustive `match` — no `_ =>` on our own enums.
- Files stay under ~400 lines; split by responsibility. One public type per file where practical.
- Public items in `core` and `fs` have doc comments stating behaviour and edge cases.

**GTK**

- GObject subclasses only where a list model or custom drawing demands one: `FileItem`, small row objects for places/tree/bookmarks, `DesktopCanvas`. Everything else is a plain Rust struct owning widgets, shared as `Rc<Inner>`.
- Signal closures capture **weak** references (`glib::clone!` with `#[weak]`) to anything that owns the widget they are connected to. No reference cycles; closing a tab or window must free its folder, monitor and thumbnails (verify with a debug counter of live `TabPage`s and `Folder`s).
- No deprecated widgets: no `TreeView`, `IconView`, `Dialog`, `MessageDialog`, `FileChooserDialog`, `AppChooserDialog`, `EntryCompletion`, `ComboBox`. Use `ColumnView`/`GridView`/`ListView`, `Window`, `AlertDialog`, `FileDialog`, `ColorDialog`, `FontDialog`, `DropDown`.
- Custom dialogs are transient-for and modal to their parent window, `Esc` cancels, `Enter` activates the default button, initial focus is set deliberately.
- Styling: theme defaults. `data/style.css` holds only what the theme cannot know (active-pane frame, hidden-item opacity, type-ahead overlay, desktop). No hard-coded colours outside the desktop canvas.
- Never block the main loop: no synchronous GIO on the main thread for anything that can touch a disk or network beyond trivial local config reads at startup.

**Process**

- Commit at least once per slice; message prefix `slice N:`.
- `scripts/check.sh` must pass before a slice is called done.
- Keep `.planning/SCOPE.md` current: slice checklist, deviations from upstream, dependency additions, open questions.
- Do not start slice N+1 with slice N failing its "done when".
- When a detail is ambiguous and upstream does not settle it, choose the simplest behaviour consistent with §1, write one line in SCOPE.md, and continue. Stop and ask only for a decision that would change the architecture in §7.

---

## 11. Testing and verification

**`tallboy-core` — unit tests, exhaustive.** Sort comparator (folders-first in both directions, mingle, case, natural order, ties); `SortSpec` and all enum string codecs; unique-name generation; extension splitting (`archive.tar.gz`, dotfiles, no extension); size formatting in both unit systems; permission strings; glob matcher; `Location::parse` (paths, `~`, relative, URIs, virtual schemes); history (back/forward/truncate-on-navigate); `search://` codec round-trip and every predicate; custom-action parsing, conditions and `Exec` expansion from fixture files; wallpaper geometry for all seven modes; desktop grid layout with pinned items.

**`tallboy-fs` — integration tests on temp dirs, headless.** `FileMeta` extraction (regular, dir, symlink, broken symlink, hidden, backup, non-UTF-8 name); folder load of 5,000 files arrives complete; monitor reports create/modify/delete/rename; jobs: copy tree, merge directories, every conflict answer, apply-to-all, self-copy auto-rename, copy-into-self rejection, cancel mid-file leaves no partial, pause/resume, move with forced copy+delete fallback, link, delete, recursive attribute change, count; settings round-trip, corrupt-file recovery, unknown-key preservation, dir-settings; bookmarks parse/write; thumbnail cache path (the §8.19 vector), validity check, fail marker, generation of a PNG with correct tEXt chunks; templates discovery; archivers/terminals list parsing.

Trash, mounts and remote schemes depend on the host's GVFS: gate those tests behind an env var and cover them in QA.md.

**UI — smoke test + manual QA.** With `TALLBOY_SMOKE=1` the binary opens the given folder, waits for the first load to finish, then quits with status 0. `scripts/check.sh` runs it with `G_DEBUG=fatal-criticals` under a headless Wayland compositor when one is installed (`weston --backend=headless`, or `cage`), and skips with a visible notice otherwise. A GTK critical is a failure.

`.planning/QA.md` is a checklist you extend in every slice: concrete steps and expected results for everything that cannot be automated (menus, DnD, clipboard interop with another file manager, volumes, desktop). Run through the slice's section before marking it done, and state in the commit message what was verified by hand versus by test.

**Fixture.** `scripts/mkfixture.sh` creates `/tmp/tallboy-fixture/`: `big/` (50,000 small files), `deep/` (20 levels), `images/` (500 generated PNGs), `names/` (unicode, very long, hidden, `backup~`, spaces, leading dashes, newline in name), `links/` (file link, dir link, broken link, loop), `perm/` (unreadable dir, read-only file).

**Tracing.** `TALLBOY_TRACE=1` logs milliseconds from process start to first frame, and per-folder time to first batch and to load-finished. Use it to check §13.

---

## 12. Build slices

Vertical slices. Each ends runnable, tested and committed.

**0 — Scaffold.** Workspace, three crates, dependency pins, release profile, `scripts/check.sh`, `README.md` with build deps, `COPYING`, `.planning/SCOPE.md` and `QA.md`. `tallboy` opens an empty `ApplicationWindow` with the full menu bar from §8.2 wired to stub actions that log their name. App ID, logging bridge, `TALLBOY_SMOKE`.
*Done when:* `check.sh` passes; every menu item exists with the right label and accelerator; a second `tallboy` invocation is received by the first.

**1 — Core foundations and settings.** `core::config` with all of §9, codecs, `sort`, `naming`, `format`, `glob`, `location`, `history`. `fs::settings_store` (load chain, atomic save, profiles, dir-settings). `tallboy::settings::Settings` with change notification and debounced save. Window geometry persists.
*Done when:* core tests for these modules pass; editing `tallboy.conf` by hand changes startup state; `--profile x` uses a separate directory and a separate instance.

**2 — Folder loading and the icon view.** `fs::meta`, `fs::folder` (async batches, cancel, monitor, coalescing, mount-on-demand). `FileItem`. Model stack. Icon-view `GridView` with theme icons and names. Double-click/Enter navigates into folders. Status bar counts and free space. Empty/error placeholders.
*Done when:* `fixture/big` loads without a frozen frame and scrolls smoothly; creating/deleting/renaming a file in a terminal updates the view within a second; deleting the open folder moves the tab to its parent.

**3 — Navigation chrome.** Toolbar, per-tab history with scroll/cursor restore, Up/Home/Reload, Go menu, location entry with completion, breadcrumb mode, tabs and all tab shortcuts, New Window, window title, toolbar/path-bar/status-bar View toggles.
*Done when:* every Go and tab command works by menu, toolbar and shortcut; going up selects the folder you left; typing in the location entry triggers no file shortcut.

**4 — Views, sorting, filtering.** Compact, thumbnail-size and detailed-list views with all columns; header sorting trick; Sort menu; Show Hidden; Filter; zoom; per-folder settings; type-ahead; single-click mode with hover-select; rubber band; emblems and shadowed hidden items.
*Done when:* switching modes preserves selection and scroll position; sort state stays in sync between menu and column headers; "Preserve This Folder's Settings" survives restart.

**5 — Opening files.** `fs::apps`, default-app launch with grouping and launch context, Open With dialog and submenu, executable and launcher prompts, terminal support with `terminals.list`, Run a Command, positional CLI arguments, `-n`, daemon mode, signals.
*Done when:* `tallboy file.txt dir/ sftp://…` does the right thing from a second invocation with relative paths; setting a default app sticks system-wide.

**6 — Job engine and core operations.** `fs::jobs` complete with tests. Progress, conflict and error dialogs. Trash, Remove, Rename, Create New Folder/Empty File, Create Link, Move to, Copy to, Copy Path(s). Confirmations.
*Done when:* all job tests pass; copying `fixture/big` shows live progress, pauses, resumes, cancels cleanly; every conflict option behaves; the UI stays responsive throughout.

**7 — Clipboard and drag-and-drop.** §8.11 and §8.12 in full, including tab-hover switching and the ask popover.
*Done when:* cut/copy/paste round-trips in both directions with at least one other file manager; dragging to and from another application works; modifier keys select the action.

**8 — Context menus, side pane, bookmarks, trash.** §8.13–§8.16.
*Done when:* Places reflects config toggles live; the tree follows navigation; bookmarks edited in a GTK file chooser appear without restart; restore from trash returns files to their origin.

**9 — Volumes and remote.** Volume section in Places, mount/unmount/eject with mount operations, automount, autorun dialog and remembered choices, unmount tab handling, Devices and Network locations, Connect to Server.
*Done when:* a USB stick inserted and removed behaves per every `[volume]` toggle; an SFTP location can be browsed, copied to and from, and bookmarked.

**10 — Properties.** Both tabs, `Count` job, default-app change, permission editing with recursion prompt, multi-file aggregation.
*Done when:* folder sizes count live and cancel on close; chmod on a mixed selection applies correctly; values match `stat`.

**11 — Thumbnails.** `fs::thumbs` in full with tests; binding-driven scheduling; LRU; settings.
*Done when:* `fixture/images` fills in progressively without stutter; a second visit is instant; thumbnails written by `tallboy` are reused by another file manager and vice versa.

**12 — Dual pane and remaining window features.** Dual pane with active-pane tracking, F6 focus cycling, Fullscreen, Keyboard Navigation help, About.
*Done when:* every `win.*` file action demonstrably targets the active pane; no tab or folder leaks after closing panes (debug counter).

**13 — Find Files.** `core::search`, `fs::search`, dialog, `search://` tabs, `--find-files`.
*Done when:* predicate tests pass; searching `fixture/` by name, type, content, size and date returns correct live results and cancels promptly.

**14 — Templates, archivers, custom actions, applications folder.** §8.21–§8.24 (excluding the importer).
*Done when:* a custom action fixture with conditions appears only for matching selections and runs with correct arguments; `menu://applications/` launches apps.

**15 — Preferences dialog and importer.** All five pages bound live; `--show-pref`; `--import-pcmanfm`.
*Done when:* every setting in §9 that has a UI control round-trips through the dialog and takes effect without restart.

**16 — Desktop manager: wallpaper.** Feature gate, layer-shell windows per monitor, hot-plug, all wallpaper modes, `--desktop`, `--desktop-off`, `--set-wallpaper`, `--wallpaper-mode`, unsupported-compositor path.
*Done when:* wallpaper geometry tests pass; under Hyprland each mode renders correctly on one and two monitors and changes live from a second invocation.

**17 — Desktop manager: icons and preferences.** Canvas drawing, layout, pinning, snap, selection, keyboard, DnD, menus, extras, Desktop Preferences, `--desktop-pref`, `--one-screen`.
*Done when:* pinned positions survive restart and monitor replug; all file actions work from the desktop.

**18 — Hardening and packaging.** Performance pass against §13, leak check, full QA.md run, man page, `.desktop` files (register as an `inode/directory` handler), icon, `scripts/install.sh`, README (features, build, Hyprland notes, differences from PCManFM).
*Done when:* §13 holds.

---

## 13. Definition of done

- Every item in §8 implemented; every deviation from upstream listed in SCOPE.md.
- `scripts/check.sh` green: fmt, clippy `-D warnings`, all tests, smoke.
- Full QA.md pass on Fedora + Hyprland, and the main window verified once under X11.
- **Performance budgets** (release build, warm cache, measured with `TALLBOY_TRACE`):
  - process start → first frame of a window on `$HOME`: ≤ 300 ms
  - `/usr/bin`-sized folder (≈3,000 entries): load finished ≤ 150 ms
  - `fixture/big` (50,000 entries): first batch visible ≤ 100 ms, fully loaded and sorted ≤ 1.5 s, no single main-loop stall > 50 ms, scrolling stays smooth
  - re-sorting 50,000 entries ≤ 300 ms
  - memory: growth per additional 10,000 listed files ≤ 15 MB; closing a tab returns to within a few MB of baseline
  - copy throughput within 10% of `cp -a` on the same local disk
- No GTK warnings or criticals on stderr during a normal session.
- A PCManFM user can run `tallboy --import-pcmanfm`, launch `tallboy`, and work.

---

## 14. First steps

1. Read this file fully. Skim upstream `src/main-win.c`, `src/tab-page.c` and libfm's `src/gtk/fm-standard-view.c` and `src/job/fm-file-ops-job-xfer.c` to calibrate.
2. Write `.planning/SCOPE.md` (scope summary, non-goals, the slice checklist, empty "Deviations" and "Decisions" sections) and `.planning/QA.md` (headings per slice).
3. Verify the dependency versions in §5 against crates.io and docs.rs; pin them.
4. Start slice 0.
