# SCOPE.md — `tallboy` v1

Scope and working checklist for v1. Written 2026-10-13 from `PROMPT.md` and `DISCOVERY.md`.

**Precedence:** `SCOPE.md` > `DISCOVERY.md` > `PROMPT.md`. A `§` reference means PROMPT.md; `D`, `R` and `O` numbers mean DISCOVERY.md.

**How to use it:** work the tasks in order, tick the box when its acceptance line holds, commit with the prefix `phase N:`, push at the end of each phase. A phase is done when every box is ticked and `scripts/check.sh` is green.

---

## 1. Overview

`tallboy` is a behavioural port of PCManFM 1.4.0 to Rust on GTK 4, used as the owner's daily file manager on one Fedora + Hyprland workstation. It operates on real files, so the job engine is treated as safety-critical.

**In v1:** main window with the full menu bar and toolbar; location entry and breadcrumbs; tabs; dual pane; icon, thumbnail, compact and detailed-list views; sorting, hidden files, filter, zoom, per-folder settings; type-ahead; opening files, Open With, executable and launcher prompts, terminal; copy, move, link, trash, delete and restore jobs with progress, pause, conflict and error dialogs; clipboard and drag-and-drop that interoperate with other file managers; context menus; Places and Directory Tree; bookmarks shared with GTK; trash; volumes, automount, autorun; remote locations and Connect to Server; Properties with permissions; image thumbnails; Preferences; profiles, single instance, daemon mode; `--import-pcmanfm`; `org.freedesktop.FileManager1`; install script.

**After v1, in order:** Find Files (slice 13); templates, archivers, custom actions, applications folder and sandboxed external thumbnailers (slice 14); desktop manager (slices 16–17).

**Never:** the PROMPT §4 non-goals, plus `admin://` (D17).

---

## 2. How the work is split

Three goals, each run by one agent from start to finish. There are exactly two hand-offs between agents.

| Goal | Agent | Phases | Produces | Ends with |
|---|---|---|---|---|
| **A — Backend** | Codex | 1–4 | `tallboy-core` and `tallboy-fs` complete for v1, with headless tests and benchmarks | `HANDOFF-A.md` → Claude |
| **B — Application** | Claude | 5–9 | The `tallboy` GTK crate, data files, scripts, `QA.md`, README | `HANDOFF-B.md` → Codex |
| **C — Review and release** | Codex | 10 | Review, fixes, performance and leak pass, guard removal, `v1.0.0` | `HANDOFF-C.md` → owner |

**Ownership**

- Codex owns `crates/tallboy-core/`, `crates/tallboy-fs/`, the workspace `Cargo.toml`, `scripts/check.sh` and `scripts/mkfixture.sh`.
- Claude owns `crates/tallboy/`, `data/`, `scripts/agent-run.sh`, `scripts/dev-run.sh`, `scripts/install.sh`, `README.md` and `.planning/QA.md`.
- Both tick boxes here and append to §9–§12.

**Rules that keep the hand-offs to two**

1. **The contract is fixed up front.** PROMPT §7 plus the module list in §4 below is what Goal A must deliver. All decision logic lives in `core` and `fs`; the GTK crate only wires widgets to it.
2. **Claude may patch the backend when blocked.** Smallest change that unblocks, with a test, listed under "Backend patches" in `HANDOFF-B.md`. No message to Codex, no waiting.
3. **Codex fixes what it finds in Goal C**, in any crate. Only findings that change behaviour or design are left for the owner, in `HANDOFF-C.md`.
4. **Neither agent stops to ask the other.** Ambiguities follow PROMPT §10: choose the simplest behaviour consistent with §1, write one line in §12, continue.

**Owner checkpoints.** These involve no agent switch. Goal A needs none. Goal B has one at the end of each phase (tasks 5.16, 6.17, 7.13, 8.12, 9.13); the owner may batch them. Goal C ends with the sign-off that unlocks real-file use.

**Trade-off accepted by this grouping.** The backend is finished before any window exists, so a poor fit between the `fs` API and the UI shows up later than it would with interleaved goals. Rules 1 and 2, the headless benchmarks in Goal A, and the spike in task 5.14 are the mitigations.

---

## 3. Changes to DISCOVERY

DISCOVERY §6 (five interleaved goals) is replaced by §2 above. Consequences:

- **S1 — D24 (split by crate) stays; its per-goal interleaving goes.** One backend goal, one application goal, one review goal.
- **S2 — D5 (QA per goal) becomes QA per Goal B phase.** Five checkpoints, same content as the former goals A–E.
- **S3 — D26 (two reviews of the job engine) is met without an extra hand-off.** Claude's data-loss review opens Phase 7 (task 7.1); Codex's review of everything, including the UI paths that feed the engine, is Phase 10. This settles O-2.
- **S4 — D13 (job guard): unlock moves to the Phase 10 sign-off.** Until then the owner uses the fixture. The guard gains two rules so QA is still possible: `TALLBOY_ALLOW_ROOTS` adds specific native paths or URI prefixes (a USB mount point, one SMB scratch folder), and `trash://` operations are allowed only when the user data directory is itself under an allowed root, as it is in an isolated run.
- **S5 — D14 (isolation) adds a private D-Bus session.** Agent and dev runs start under `dbus-run-session`, so the trash and volume daemons see the isolated directories and the run cannot collide with the owner's instance. `dbus-run-session` is not installed; it comes with `dbus-daemon` (§8 prerequisites).
- **S6 — O-1 settled:** Codex creates the Cargo workspace (task 1.2). **O-5 included by default:** recursive delete stops and asks at a mount point (task 3.6). Delete that acceptance clause to revert to upstream behaviour.
- **S7 — Commit prefix is `phase N:`**, replacing `slice N:` in §10.
- **S8 — Thumbnail hand-over.** `fs` cannot depend on `gdk`, so the thumbnail pool delivers decoded pixel data and the GTK crate turns it into a `gdk::Texture`. §8.19 says the worker produces the texture; the cost is the same.

---

## 4. Architecture

The diagram and layering rules are in DISCOVERY §4. Dependencies point downward only: `tallboy` → `tallboy-fs` → `tallboy-core`.

Modules beyond PROMPT §6, added so that logic sits with the backend:

| Module | Holds | Used by |
|---|---|---|
| `core::open` | What activating an item does, including the D15 removable-media rule | 6.12–6.14 |
| `core::enable` | Which actions are enabled for a selection, location and clipboard state | 7.8 |
| `core::transfer` | Clipboard payload codecs and drop-action resolution | 7.9–7.11 |
| `core::perms` | Mode bits ↔ the three Properties dropdowns, mixed states | 8.11 |
| `core::invocation` | Command-line arguments + caller cwd → a list of requests | 5.4, 6.16 |
| `fs::guard` | Allowed-roots check (D13, S4) | every mutating call |
| `fs::complete` | Directory-only listing for the tree and path completion | 6.2, 8.5 |
| `fs::import` | PCManFM / libfm config → `Config` | 9.6 |
| `fs::filemanager1` | The D-Bus object; requests surface as events | 9.7 |

`fs` never takes a GTK type. Where upstream needs one, the API takes the GIO parent class: `gio::MountOperation` for mounts, `gio::AppLaunchContext` for launches.

---

## 5. Goal A — Backend (Codex)

### Phase 1 — Workspace and `tallboy-core`

*Covers:* slices 0–1, backend half. *Needs:* owner has run `sudo dnf install gtk4-devel rustfmt`.

- [x] **1.1 Toolchain check.** *Accept:* `pkg-config --modversion gtk4 glib-2.0 gio-2.0 gdk-pixbuf-2.0` all resolve and `cargo fmt --version` runs; versions written into §11. If not, stop and tell the owner.
- [x] **1.2 Workspace.** Root `Cargo.toml` with the D22 pins, the §5 release profile and three crates; `#![deny(unsafe_code)]` and `clippy::unwrap_used` in `core` and `fs`; the `tallboy` crate is a stub `main` that links `gtk4` and prints its version. *Accept:* `cargo build --workspace` succeeds; `cargo tree -p tallboy-core` shows no `glib`; `cargo tree -p tallboy-fs` shows no `gtk4`.
- [x] **1.3 Repository files.** `COPYING` (GPL-2.0), `.gitignore`, a README stub with build packages. *Accept:* present before the first code commit.
- [x] **1.4 `scripts/check.sh`.** fmt check, clippy `-D warnings`, tests, the layering assertion from 1.2, and a smoke step that prints a visible "skipped" notice until 5.13. *Accept:* green on the workspace; adding `glib` to `core` makes it fail.
- [x] **1.5 `scripts/mkfixture.sh`.** Builds `~/.cache/tallboy-fixture` (D8) with `big/`, `deep/`, `images/` (ImageMagick is installed), `names/`, `links/`, `perm/` per §11; idempotent; `--clean`. *Accept:* second run changes nothing; `big/` holds 50,000 files.
- [x] **1.6 `core::config`.** `Config` mirroring §9 without `[desktop]`, `Default`, and a codec for every enum and list key. *Accept:* round-trip test per codec; unknown or out-of-range values fall back to the default with one warning.
- [x] **1.7 `core::sort`.** `SortSpec`, `SortKeys`, comparator. *Accept:* folders first in both directions, mingle, case, natural order, name tie-break — all tested.
- [x] **1.8 `core::naming`.** Unique names, extension splitting, name validation, rename selection range. *Accept:* `archive.tar.gz`, dotfiles and no-extension cases; `(copy)`, `(copy 2)` sequence; empty, `/`, `.`, `..` rejected.
- [x] **1.9 `core::format`.** Sizes in both unit systems, dates, permission strings, transfer speed and time remaining, the §8.26 status-bar strings. *Accept:* table-driven tests.
- [x] **1.10 `core::glob`.** `*`, `?`, `[..]`. *Accept:* tests including `IMG_??.jpg` and character classes.
- [x] **1.11 `core::location`.** `Location`, `parse`, `to_uri`, display form, breadcrumb segments, the Connect to Server URI builder. *Accept:* round-trip tests; `admin://` returns a typed error (D17); `search://` and `menu://` return "not in this version" (D4).
- [x] **1.12 `core::history`.** Back, forward, truncate on navigate; entries carry scroll position and cursor item. *Accept:* tests.
- [x] **1.13 `core::open`.** Classify activation: navigate, follow target, mount, launch default, ask (text script / binary), launcher (trusted / confirm), open-with. *Accept:* decision table covering §8.9, including a launcher and a script on vfat (D15).
- [x] **1.14 `core::enable`.** Action enablement per §7.7. *Accept:* table test: Paste off in a read-only folder and with no files on the clipboard; Trash hidden in `trash://`; Rename only for a single item.
- [x] **1.15 `core::transfer`.** Codecs for `x-special/gnome-copied-files`, `text/uri-list`, `application/x-kde-cutselection` and plain paths; drop-action resolution from modifiers, `drop_default_action` and filesystem ids. *Accept:* round-trip with the newline-in-name fixture name yields one entry; same-folder move is a no-op; folder into itself is refused.
- [x] **1.16 `core::perms`.** Mode bits ↔ Nobody / Only owner / Owner and group / Anyone; mixed state for heterogeneous selections; special bits; building an `AttrChange`. *Accept:* tests against known modes; an unchanged dropdown changes no bits.
- [x] **1.17 `core::invocation`.** Options + arguments + caller cwd + home → requests. *Accept:* relative paths resolve against the caller's cwd; `~` is Home; desktop options yield the D3 message and a non-zero exit code; `--find-files` yields "not in this version".
- [x] **1.18 Small pure helpers.** Zoom steps; type-ahead matcher with cycling. *Accept:* tests.

### Phase 2 — `tallboy-fs`: reading, settings, launching

*Covers:* backend of slices 1–5. *Depends on:* Phase 1.

- [x] **2.1 `settings_store`.** Load chain (§9), atomic save, profiles. *Accept:* `load → save → load` is identity; unknown keys preserved; a corrupt file is renamed `.broken` and defaults load; a symlinked config is replaced, not followed.
- [x] **2.2 Per-folder settings.** `dir-settings.conf` groups: read, write, delete. *Accept:* tests for a path and for a URI group name.
- [x] **2.3 `meta`.** One attribute-query constant; `FileMeta` from `gio::FileInfo`, including collation keys, filesystem type and id; the `defer_content_test` variant. *Accept:* tests for regular, directory, symlink, broken symlink, hidden, backup, non-UTF-8 name.
- [x] **2.4 `folder` loading.** Async enumeration in batches of about 200, cancellable, typed `FolderError`, `FsInfo`. *Accept:* a 5,000-file directory arrives complete; cancel mid-load delivers nothing further.
- [x] **2.5 `folder` monitoring.** Move tracking, 200 ms coalescing, re-query on change, `RootGone` with a nearest-existing-ancestor helper. *Accept:* create, modify, delete and rename each produce the expected single event.
- [x] **2.6 Mount on demand.** `NotMounted` → `mount_enclosing_volume` with a caller-supplied `gio::MountOperation`, one retry. *Accept:* unit test with a failing mount; live check deferred to 8.12.
- [x] **2.7 `complete`.** Directory-only children, lazy, monitored; has-subdirectories probe; prefix completion. *Accept:* tests on `fixture/deep`.
- [x] **2.8 `bookmarks`.** Parse and write `gtk-3.0/bookmarks`, atomic; monitor for external change; add, rename, remove, move. *Accept:* round-trip keeps labels and unknown lines; an external edit raises one change event.
- [x] **2.9 `apps`.** Default application, recommended and all applications for a type, grouped launch with a caller-supplied `gio::AppLaunchContext`, set default, custom command line, launcher launch. *Accept:* three files of two types produce two launches; tests use a temp XDG data dir with fixture `.desktop` files.
- [x] **2.10 `terminal` and execution.** Parse `data/terminals.list`; resolve terminal (setting → `$TERMINAL` → first installed); open in a directory; execute in terminal; run a command with keep-open. *Accept:* argv only, no shell (D18); a file named `; rm -rf ~` reaches the child as one argument.
- [x] **2.11 Trace and benchmark.** `TALLBOY_TRACE` timing helpers; `examples/fsls`. *Accept:* on `fixture/big`, first batch ≤ 100 ms and load finished ≤ 1.0 s headless; figures written into §11.

### Phase 3 — `tallboy-fs`: guard and job engine

*Covers:* backend of slices 6, 8, 10. *Depends on:* Phase 2. This phase is the safety-critical one.

- [x] **3.1 `guard`.** Rules from D13 and S4: allowed roots, `TALLBOY_ALLOW_ROOTS`, the trash rule, `TALLBOY_UNLOCKED`. *Accept:* tests for each rule; one test enumerates every mutating entry point and fails if any bypasses the guard.
- [x] **3.2 Engine.** `spawn`, `JobHandle`, event channel, prepare phase with totals, pause, resume, cancel, `Ask`/`Reply`, progress throttled to ≤ 10/s. *Accept:* a scripted-responder harness drives a job end to end without GTK.
- [x] **3.3 Copy.** Per-file copy with `NOFOLLOW_SYMLINKS | ALL_METADATA`; directories walked by the job and merged; conflict question with apply-to-all; self-copy auto-rename; copy-into-self rejected before any write. *Accept:* every conflict answer tested; cancel mid-file leaves no partial file; a metadata-copy failure is a warning, not a failed copy (R11).
- [x] **3.4 Move.** Native move first; fall back to copy-then-delete per item, deleting the source only after its copy fully succeeded. *Accept:* real cross-filesystem test between `/tmp` and `CARGO_TARGET_TMPDIR`; an injected copy failure leaves the source intact.
- [x] **3.5 Link.** Symlinks in the destination directory, with conflict handling. *Accept:* test.
- [x] **3.6 Delete.** Recursive; never follows symlinks; stops and asks at a mount point inside the tree (S6). *Accept:* a symlink to a directory is removed as a link and its target is untouched; a symlink loop terminates.
- [x] **3.7 Trash, restore, empty.** `TrashUnsupported` question leading to delete-instead; removable-media helper for `no_usb_trash`; restore to `trash::orig-path` with conflicts; empty trash; trash empty/full state. *Accept:* tests gated by `TALLBOY_TEST_GVFS=1` pass on this machine.
- [x] **3.8 `SetAttrs` and `Count`.** Recursive mode and owner changes without following symlinks; live size and file counts. *Accept:* totals match `du` and `find | wc -l` on `fixture/deep`; cancel stops promptly.
- [x] **3.9 Single-item operations.** Rename via `set_display_name` with validation; create folder and empty file with a unique name. All guarded. *Accept:* tests including a name clash.
- [x] **3.10 Data-loss test pack.** *Accept:* one named test each for: unreadable directory, read-only destination, newline in name, destination disappears mid-job, pause then cancel, two concurrent jobs on the same tree.
- [x] **3.11 Throughput.** `examples/jobbench`. *Accept:* cross-filesystem copy within 10% of `cp -a`; same-filesystem result on btrfs recorded as cloned or not (D7, O-4).

### Phase 4 — `tallboy-fs`: system services and hand-off

*Covers:* backend of slices 9, 11, 15 and D10. *Depends on:* Phase 3.

- [x] **4.1 Volume model.** Wrap `gio::VolumeMonitor` into plain entries (name, icon, mounted, removable, internal, can eject) with change events and the `places_unmounted` / `show_internal_volumes` filters. *Accept:* unit tests on the filter logic; `examples/volumes` lists `sda1` and the second NVMe correctly.
- [x] **4.2 Volume operations.** Mount, unmount, eject with a caller-supplied `gio::MountOperation`; never force; helper returning which open locations lie inside a mount. *Accept:* the helper is tested; a busy unmount surfaces the error.
- [x] **4.3 Automount and autorun.** `mount_on_startup`, `mount_removable`; content-type guess; applications for a content type; `[autorun]` choices with the `*` prefix. *Accept:* codec tests; `x-content/unix-software` can never be stored as remembered (D15).
- [x] **4.4 Thumbnail cache.** Paths (the §8.19 MD5 vector), size-directory choice from pixel size × scale, validity check, fail markers, PNG written with the four `Thumb::` chunks at mode 0600 via temp file and rename, skip rules. *Accept:* tests for each; chunks read back correctly with the installed gdk-pixbuf loader, otherwise a small chunk reader is added to `core` and noted in §12.
- [x] **4.5 Thumbnail pool.** `min(4, cores)` workers, most-recent-first queue, request and cancel handles, built-in image generator with orientation, decoded pixels delivered over a channel (S8). *Accept:* 500 requests on `fixture/images` complete; cancelling a request before it starts does no work.
- [x] **4.6 `import`.** Read `pcmanfm.conf` and `libfm.conf` into `Config`; report imported and ignored keys (D11). *Accept:* test against a copy of the owner's two files; `quick_exec` is reported explicitly if imported as 1.
- [x] **4.7 `filemanager1`.** Register the object on a given connection; surface `ShowFolders`, `ShowItems`, `ShowItemProperties` as events; never replace an existing owner. *Accept:* test on a private bus with `gdbus call`.
- [x] **4.8 Documentation and `HANDOFF-A.md`.** Doc comments on every public item; `cargo doc` without warnings. The hand-off lists the API as built, every departure from §7, known gaps, and how to run the examples. *Accept:* a reader can find the entry point for each Goal B task from it.
- [x] **4.9 Goal A gate.** *Accept:* `check.sh` green; boxes ticked; pushed.

---

## 6. Goal B — Application (Claude)

*Needs before Phase 5:* owner has run `sudo dnf install cage dbus-daemon`.

### Phase 5 — Shell and first view

*Covers:* slices 0–2, UI half. *Depends on:* Goal A.

- [x] **5.1 Read `HANDOFF-A.md`.** *Accept:* any gap that blocks Goal B is patched now and listed for `HANDOFF-B.md`.
- [x] **5.2 `agent-run.sh` and `dev-run.sh`.** Private D-Bus session, isolated XDG directories, `--profile agent` (D14, S5). `agent-run.sh` runs inside headless `cage`, captures with `grim` and can activate actions over D-Bus (D25); `dev-run.sh` uses the live display for owner QA. *Accept:* a screenshot is produced with nothing appearing on the owner's monitors; if `cage` capture fails, the fallback is recorded in §12 (R9).
- [x] **5.3 Application.** `gtk::Application` with the profile in the app ID, `HANDLES_COMMAND_LINE`, logging bridge, crash-log panic hook (D23), clean quit on `SIGTERM`/`SIGINT`/`SIGHUP`. *Accept:* a second invocation is received by the first; `--profile x` is a separate instance.
- [x] **5.4 Command line.** Every §8.27 option, executed through `core::invocation`. *Accept:* `--desktop` prints the D3 message and exits non-zero.
- [x] **5.5 Action table.** Every command as a `gio::SimpleAction` with its accelerators; stateful toggles and radios; stubs log their name. *Accept:* one table, no command logic in handlers.
- [x] **5.6 Menu bar.** The §8.2 tree exactly; deferred items disabled or omitted per D4. *Accept:* screenshots of each menu match the installed PCManFM for labels, order, separators and accelerators.
- [x] **5.7 `Settings`.** Typed getters and setters, `connect_changed`, debounced save, save on shutdown; window size, maximised state and splitter persist. *Accept:* hand-editing `tallboy.conf` changes start-up state.
- [x] **5.8 First-frame baseline.** Measure the empty window (D6). *Accept:* baseline and the resulting budget written into §11.
- [x] **5.9 `FileItem` and the model stack.** List store → filter → sort (`core::sort`) → multi-selection; batches appended with one `splice`. *Accept:* no per-file append anywhere.
- [x] **5.10 Tab page and icon view.** `GridView` with cheap factories: build in `setup`, bind in `bind`, disconnect everything in `unbind`. Activation navigates; Backspace goes to the parent. *Accept:* browsing works; no strong reference from an item closure to the tab.
- [x] **5.11 Status bar and placeholders.** Counts, hidden count, free space; "Folder is empty"; error with Retry. *Accept:* `fixture/perm` shows the error state.
- [x] **5.12 Live folder events.** Add, change, remove, rename; `RootGone` moves to the nearest ancestor. *Accept:* changes made in a terminal appear within a second.
- [x] **5.13 Smoke mode.** `TALLBOY_SMOKE=1` loads a folder and exits 0; wired into `check.sh` under `G_DEBUG=fatal-criticals`. *Accept:* a GTK critical fails the run.
- [x] **5.14 Spike: selection, drag, compact layout.** Prototype multi-selection drag, rubber band alongside a drag source, and a horizontal `GridView` on the fixture (R2, R4). *Accept:* the chosen gesture design and compact layout are written into §10 before Phase 7 depends on them.
- [x] **5.15 Large-folder check.** *Accept:* on `fixture/big`: first batch visible ≤ 100 ms, loaded and sorted ≤ 1.5 s, no main-loop stall > 50 ms, memory growth per 10,000 files ≤ 15 MB (R6). A miss is fixed here, not carried.
- [ ] **5.16 Owner checkpoint 1.** `QA.md` section written. *Owner:* browse folders; compare the menus with PCManFM. *(Section written 2026-10-13; the box is the owner's to tick.)*

### Phase 6 — Navigation, views, opening

*Covers:* slices 3–5. *Depends on:* Phase 5.

- [x] **6.1 Toolbar.** Groups and their visibility from `toolbar`; history popover on right-click and long-press. *Accept:* each View ▸ Toolbar toggle applies live.
- [x] **6.2 Location entry.** Enter navigates, Esc restores; hand-rolled completion popover (`fs::complete`). *Accept:* Tab completes the common prefix; Down enters the list.
- [x] **6.3 Breadcrumbs.** Segment buttons, deepest path kept, overflow arrows, middle-click opens a tab, `Ctrl+L` swaps to the entry. *Accept:* behaves as upstream on `fixture/deep`.
- [x] **6.4 Tabs.** Labels, tooltips, close buttons, reorder, middle-click close, every §8.4 shortcut, `always_show_tabs`, `focus_previous`; New Window. *Accept:* closing the last tab closes the window.
- [x] **6.5 History.** Back, forward, up, home, reload; scroll and cursor restored; going up selects the folder left; Go menu. *Accept:* every Go command works from menu, toolbar and shortcut.
- [x] **6.6 Shortcut safety.** File shortcuts must not fire while a text entry has focus (R3). *Accept:* an `agent-run.sh` check types Delete, Backspace and `Ctrl+C` into the location entry with a file selected and nothing happens to the file; the mechanism is recorded in §10.
- [x] **6.7 Compact and thumbnail-size views.** *Accept:* switching mode keeps selection and scroll position.
- [x] **6.8 Detailed list.** All columns, header menu to toggle them, widths persisted, the §8.6 header-sorting arrangement. *Accept:* folders stay first when descending; header arrow and Sort menu stay in step.
- [x] **6.9 Sort, hidden, filter, zoom.** Sort menu, Show Hidden, Filter dialog, zoom steps persisted. *Accept:* the status bar shows the hidden count and the active filter.
- [x] **6.10 Per-folder settings.** "Preserve This Folder's Settings" on and off. *Accept:* survives restart; unchecking removes the group.
- [x] **6.11 View interaction.** Type-ahead; single-click with hover-select; rubber band; symlink emblem; shadowed hidden items; full names. *Accept:* each setting applies live.
- [x] **6.12 Opening.** Default application with grouped launch and a `gdk::AppLaunchContext`; shortcuts and mountables; middle-click and `Ctrl+Enter` open a tab. *Accept:* the launched application takes focus under Hyprland.
- [x] **6.13 Open With.** Dialog and submenu per §8.9; set-as-default. *Accept:* a default set here is seen by `xdg-mime query default`.
- [x] **6.14 Executable and launcher prompts.** Driven by `core::open`. *Accept:* wording matches upstream; `quick_exec` skips the prompt.
- [x] **6.15 Terminal and Run a Command.** *Accept:* `F4` opens kitty in the folder; no terminal found leads to the error dialog.
- [x] **6.16 Arguments and lifetime.** Positional arguments, `-n`, daemon mode, the stay-alive rules. *Accept:* `tallboy file.txt dir/` from a second invocation with relative paths does the right thing.
- [ ] **6.17 Owner checkpoint 2.** *Owner:* navigate, switch views, sort, open files. *(QA.md section written 2026-10-13; the box is the owner's to tick.)*

### Phase 7 — File operations, clipboard, drag-and-drop

*Covers:* slices 6–7. *Depends on:* Phase 6 and the 5.14 decision.

- [x] **7.1 Data-loss review of `fs::jobs` and `fs::guard`.** Read as a reviewer, not as a user of the API: symlinks, partial files, cancel, move fallback, guard coverage (D26, S3). *Accept:* findings written for `HANDOFF-B.md`; anything that could lose data is fixed with a test before 7.2.
- [x] **7.2 Progress dialog.** Appears after 500 ms; current file, destination, bar, speed and time remaining; pause, resume, cancel; expandable error list. *Accept:* copying `fixture/big` stays responsive.
- [x] **7.3 Conflict dialog.** Both files with icon, size, time; Overwrite, Skip, Rename, Cancel; apply to all. *Accept:* each answer has the expected result on disk.
- [x] **7.4 Error and trash-unsupported dialogs.** *Accept:* Skip, Skip All, Retry, Cancel behave as §8.10.
- [x] **7.5 Trash and Remove.** Confirmations per `confirm_trash` and `confirm_del`; `use_trash`; `no_usb_trash`. *Accept:* wording matches upstream, singular and plural.
- [x] **7.6 Rename and Create New.** Rename dialog with base-name selection; folder and empty file; the new item is selected and scrolled to. *Accept:* invalid names rejected in the dialog.
- [x] **7.7 Create Link, Move to, Copy to, Copy Path(s).** *Accept:* each spawns the right job or sets the clipboard text.
- [x] **7.8 Action enablement.** Wire `core::enable` to selection and location changes. *Accept:* menu, toolbar and context menu agree at all times.
- [x] **7.9 Clipboard.** One union provider with all §8.11 formats; paste; cut then paste moves and clears. *Accept:* agent check of the offered MIME types; interop is owner QA.
- [x] **7.10 Drag source.** Whole selection or the single item; drag icon with count badge. *Accept:* per the 5.14 design, a drag never collapses the selection.
- [x] **7.11 Drop targets.** Folder items, view background, tab labels with hover-switch; modifier keys; the "ask" popover; drops from other applications. *Accept:* same-folder move does nothing; folder into itself is refused.
- [x] **7.12 Locked-build messages.** An operation refused by the guard explains why and how the fixture is used. *Accept:* no raw error text reaches the user.
- [ ] **7.13 Owner checkpoint 3.** *Owner, in the fixture:* copy, move, trash, delete, rename; cut/copy/paste and drag both ways with PCManFM. *(QA.md section written 2026-10-13; the box is the owner's to tick.)*

### Phase 8 — Side pane, menus, trash, volumes, Properties

*Covers:* slices 8–10. *Depends on:* Phase 7. *Needs for 8.12:* `sudo dnf install gvfs-smb gvfs-mtp`.

- [x] **8.1 Context menus.** File, folder-background and trash variants per §8.13; right-click selection rules. *Accept:* item lists match upstream minus the D4 omissions.
- [x] **8.2 Places: fixed rows.** Gated by `[places]` keys, live; current location highlighted; trash icon reflects state; Empty Trash Can. *Accept:* toggling a key updates the pane without restart.
- [x] **8.3 Places: volumes.** Rows from the volume model, eject button, context menu, mount-operation prompts. *Accept:* clicking an unmounted volume mounts then opens it.
- [x] **8.4 Bookmarks.** Places section with drag reorder and drop-to-add; Bookmarks menu; Add and Edit dialogs; `bm_open_method`. *Accept:* a bookmark added in a GTK file chooser appears without restart.
- [x] **8.5 Directory tree.** `TreeListModel` on `fs::complete`; lazy, monitored, follows the active tab, honours Show Hidden and `no_child_non_expandable`. *Accept:* navigating expands and selects the path.
- [x] **8.6 Side-pane chrome.** Mode switch, `F9`, `Ctrl+6`/`Ctrl+7`, persisted mode. *Accept:* state survives restart.
- [x] **8.7 Trash location.** Restore, Empty, deletion-date tooltip; Cut, Rename and Create New disabled. *Accept:* restore returns a file to its origin; verified in an isolated run (S4).
- [x] **8.8 Automount, autorun, unmount handling.** Autorun dialog with remembered choices; tabs close or go Home on unmount. *Accept:* behaviour follows each `[volume]` and `[ui]` key.
- [x] **8.9 Devices, Network, Connect to Server.** *Accept:* the dialog builds the right URI for each of the six types.
- [x] **8.10 Properties: General.** Single and multiple files; live `Count`; Open-with dropdown; rename on OK. *Accept:* values match `stat`; closing the window cancels the count.
- [x] **8.11 Properties: Permissions.** Three dropdowns with mixed state, owner and group, special bits under `advanced_mode`, recursion prompt, `SetAttrs` job. *Accept:* chmod on a mixed selection matches the `core::perms` tests on disk.
- [ ] **8.12 Owner checkpoint 4.** *Owner:* USB stick (with its mount point in `TALLBOY_ALLOW_ROOTS`), internal disks (mount and unmount only), SMB, MTP, trash, Properties. `QA.md` marks SFTP, FTP and WebDAV "not verified by hand" (D9). *(QA.md section written 2026-10-13; the box is the owner's to tick.)*

### Phase 9 — Thumbnails, dual pane, Preferences, packaging

*Covers:* slices 11, 12, 15 and the packaging half of 18. *Depends on:* Phase 8.

- [x] **9.1 Thumbnails in views.** Request on bind, cancel on unbind and on folder change; texture cache of about 500 keyed by URI, mtime and size directory; `show_thumbnail`, `thumbnail_local`, `thumbnail_max`. *Accept:* `fixture/images` fills in without stutter; a second visit is instant; sharp at scale 1.5.
- [x] **9.2 Dual pane.** Second notebook in a `Paned`; active-pane tracking and styling; `F6` focus cycle. *Accept:* every `win.*` file action targets the active pane.
- [x] **9.3 Fullscreen, Keyboard Navigation, About.** *Accept:* About carries the GPL notice and the upstream credit line.
- [x] **9.4 Leak counters.** Debug counters of live tab pages and folders. *Accept:* closing tabs, panes and windows returns both to zero.
- [x] **9.5 Preferences.** Five pages with the v1 controls, bound live; `--show-pref=N`. *Accept:* every §9 key that has a control round-trips and applies without restart.
- [x] **9.6 `--import-pcmanfm`.** Runs `fs::import`, prints the report, exits. *Accept:* run in an isolated profile against a copy of the owner's config.
- [x] **9.7 `FileManager1`.** Own the name in the default profile; `ShowItems` opens the folder with the item selected. *Accept:* a `gdbus call` in the private session selects the file.
- [x] **9.8 Data files.** `.desktop` file (handles `inode/directory`), icon, D-Bus service file, man page; `style.css` holds no colours (D20). *Accept:* `desktop-file-validate` passes.
- [x] **9.9 `install.sh`.** Release build to `~/.local`; `--set-default` registers the handler and the service file (D27). *Accept:* without the flag, `xdg-mime query default inode/directory` is unchanged.
- [x] **9.10 README.** Features, build, Hyprland notes, differences from PCManFM, the locked build and how it is unlocked. *Accept:* the differences list equals §10.
- [x] **9.11 X11 check.** One run under XWayland with `GDK_BACKEND=x11` (D12). *Accept:* main window works; result noted in `QA.md`.
- [x] **9.12 `HANDOFF-B.md`.** Backend patches made, 7.1 findings, measurements, QA results and which were by test, by screenshot or by hand, areas where review is most wanted. *Accept:* `check.sh` green; pushed.
- [ ] **9.13 Owner checkpoint 5.** *Owner:* thumbnails, dual pane, Preferences, importer, install side by side. *(QA.md section written 2026-10-13; the box is the owner's to tick.)*

---

## 7. Goal C — Review and release (Codex)

### Phase 10 — Review, hardening, release

*Depends on:* Goal B.

- [x] **10.1 Review the backend patches** listed in `HANDOFF-B.md`. *Accept:* each kept, reworked or reverted, with a reason.
- [x] **10.2 Review the GTK crate.** Reference cycles and weak captures, `unbind` disconnects, synchronous GIO on the main thread, logic that belongs in `core` or `fs`. *Accept:* findings logged with file and line.
- [x] **10.3 Review every path from the UI into a job.** Selection resolution, paste target, drop target, trash versus delete, the active pane (D26). *Accept:* each `JobKind` call site is listed with what was checked.
- [x] **10.4 Security checklist.** Every item in DISCOVERY §7. *Accept:* each ticked with a test name or a file reference, or logged as a finding.
- [x] **10.5 Fix findings.** In any crate (§2 rule 3). *Accept:* each fix has a test where one is possible; behaviour-changing items go to `HANDOFF-C.md` unfixed.
- [x] **10.6 Performance pass.** All §13 budgets with the calibrated first-frame figure and the D7 copy cases. *Accept:* figures in §11; a miss is fixed or explained.
- [x] **10.7 Session cleanliness.** A scripted `agent-run.sh` session exercising every menu. *Accept:* no GTK warnings or criticals; leak counters at zero on exit.
- [x] **10.8 Records.** §10 deviations complete and equal to the README list. *Accept:* nothing in the code departs from upstream without a line there.
- [x] **10.9 `HANDOFF-C.md`.** Review summary, what was fixed, what is left for the owner, and a sign-off checklist. *Accept:* `check.sh` green; pushed.
- [ ] **10.10 Release.** After the owner signs off: delete `fs::guard` and its call sites (D13), re-run `check.sh`, tag `v1.0.0`. *Accept:* no reference to `TALLBOY_UNLOCKED` remains.

**Review status (2026-10-13):** 10.1–10.9 completed; see HANDOFF-C and CODE-REVIEW. 10.10 awaits owner sign-off. The guard and its override remain, and no release tag has been created.

---

## 8. Owner actions

| When | Action |
|---|---|
| Before Phase 1 | `sudo dnf install gtk4-devel rustfmt` |
| Before Phase 5 | `sudo dnf install cage dbus-daemon` |
| Before task 8.12 | `sudo dnf install gvfs-smb gvfs-mtp` (and `gvfs-fuse` if wanted) |
| Tasks 5.16, 6.17, 7.13, 8.12, 9.13 | Run that phase's `QA.md` section with `scripts/dev-run.sh` |
| Before task 3.6 | Say no to S6 if recursive delete should descend into mount points as upstream does |
| After Phase 10 | Read `HANDOFF-C.md`, sign off, then `scripts/install.sh`; later `--set-default` |

---

## 9. Slice map

| PROMPT slice | Backend | Application |
|---|---|---|
| 0 Scaffold | 1.1–1.5 | 5.2–5.6, 5.13 |
| 1 Core and settings | 1.6–1.18, 2.1–2.2 | 5.7 |
| 2 Folder loading, icon view | 2.3–2.6, 2.11 | 5.9–5.12, 5.15 |
| 3 Navigation chrome | 2.7 | 6.1–6.6 |
| 4 Views, sorting, filtering | — | 6.7–6.11 |
| 5 Opening files | 2.9–2.10 | 6.12–6.16 |
| 6 Jobs and operations | 3.1–3.11 | 7.1–7.8, 7.12 |
| 7 Clipboard, drag-and-drop | 1.15 | 7.9–7.11 |
| 8 Menus, side pane, bookmarks, trash | 2.8, 3.7 | 8.1–8.7 |
| 9 Volumes and remote | 4.1–4.3 | 8.3, 8.8–8.9 |
| 10 Properties | 1.16, 3.8 | 8.10–8.11 |
| 11 Thumbnails | 4.4–4.5 | 9.1 |
| 12 Dual pane and window features | — | 9.2–9.4 |
| 15 Preferences and importer | 4.6 | 9.5–9.6 |
| 18 Hardening and packaging | — | 9.8–9.11, Phase 10 |
| 13, 14, 16, 17 | deferred | deferred |

---

## 10. Deviations from upstream

Known at the start; agents append as they go.

| # | Deviation | Source |
|---|---|---|
| 1 | Find Files, templates, archivers, custom actions and the applications folder are absent; their menu items are disabled or omitted | D2, D4 |
| 2 | No desktop manager; its command-line options report that it is not built | D3 |
| 3 | `--role` accepted with no effect | §8.27 |
| 4 | `admin://` rejected | D17 |
| 5 | Launchers and scripts on filesystems with synthetic execute bits are never trusted or offered for execution | D15 |
| 6 | Autorun choice cannot be remembered for `x-content/unix-software` | D15 |
| 7 | No external thumbnailers | D16 |
| 8 | Owns `org.freedesktop.FileManager1` | D10 |
| 9 | Recursive delete stops and asks at a mount point | S6, pending owner |
| 10 | Follows the system GTK 4 theme, which is not the GTK 3 theme PCManFM uses | D20 |
| 11 | Bookmarks ▸ Edit Bookmarks… exists; PCManFM 1.4.0 edits bookmarks only from the side pane | PROMPT §8.2, §8.15 |
| 12 | Compact view columns are all the same width and long names are ellipsised; upstream fits each column to its longest name. GTK 4's `GridView` has uniform cells | 5.14, R4 |
| 13 | The selection highlight covers the whole grid cell, as GTK 4 themes draw it, not only the icon and name | D20 |
| 14 | A press on the padding around an item selects it on release, and a drag from there starts a rubber band; upstream deselects and rubber-bands from the gaps only | 5.14, R2 |
| 15 | During a drag, Ctrl, Shift and Ctrl+Shift are read from the keyboard by the destination. For a drag that starts in another program they are only seen while tallboy's window has the keyboard, and a link can only be made by tallboy as the destination. Wayland has no link action and sources do not narrow their offer | 7.11 |
| 16 | Preparation errors stop the whole job; an execution error on one child ends that top-level folder’s copy, move, delete or attribute change. Skip abandons its remaining children; Retry repeats completed entries and conflicts. Upstream asks per file | HANDOFF-C, review decision 1 |
| 17 | A kill or crash in mid-copy can leave a hidden `.tallboy-copy-*` file beside the destination; upstream leaves the partial destination file instead | 7.1 |
| 18 | A folder dropped on the empty part of Places becomes a bookmark. Dropping exactly *between* two bookmarks is not told apart from dropping *on* one, which puts the files into that folder; files cannot be dropped on the Trash Can row | 8.4 |
| 19 | The directory tree shows a hidden folder that lies on the way to the current folder even when hidden files are off, so the current folder can always be seen in it | 8.5 |
| 20 | Owner and group names in Properties are looked up in `/etc/passwd` and `/etc/group`; an account known only to a directory service must be given as a number | 8.11 |
| 21 | When a volume is unmounted and `close_on_unmount` is on, a window's last tab goes Home instead of closing the window | 8.8 |
| 22 | Preferences has no controls for templates, archivers, the Applications place or the desktop, which v1 leaves out; icon sizes are chosen from a fixed list of sizes | 9.5, D4 |
| 23 | `ShowItems` of `org.freedesktop.FileManager1` selects the first item asked for in each folder, not all of them | 9.7 |
| 24 | Closing the last tab of one of two panes leaves dual-pane mode and keeps the other pane's tabs | 9.2 |
| 25 | Passwords embedded in a location’s userinfo are rejected; credentials must be entered through the mount prompt | DISCOVERY §7, Phase 10 |
| 26 | Recursive permission changes prepare the tree and visit children before parents; an inaccessible subtree cannot be repaired by granting access recursively | HANDOFF-C, review decision 2 |
| 27 | A listing error after an initial batch leaves the partial view in Loading; partial-result error presentation awaits an owner decision | HANDOFF-C, review decision 3 |

### Design notes that later phases depend on

**Gestures (task 5.14, settles R2 and R4).** Tested with real pointer input in the headless session.

- GTK 4.22 selects a list item on button *release*, starts a rubber band from any drag on the list, including one that begins on an item, and its `DragSource` refuses to start until the button has been held for 100 ms (`MIN_TIME_TO_DND`). Left alone, a quick drag of an item becomes a rubber band and a multi-selection collapses.
- So each cell's content box (icon and name, not the whole grid cell) carries its own `GestureClick` and `DragSource` in one gesture group (`view/dnd.rs`). A primary press on the content claims the event sequence, which keeps GTK's click handling and the rubber band out of it.
- Pressing an unselected item selects it immediately. Ctrl and Shift go through GTK's own `list.select-item` action, so toggle and range rules are GTK's. Pressing an already-selected item changes nothing until release: a drag then carries the whole selection, a plain click narrows the selection to that item. A double press activates through `list.activate-item`.
- A press on the padding is left to GTK: select on release, rubber band on drag.
- Verified: drag of one of three selected items carries three files and leaves three selected; drag of an unselected item carries one; Ctrl toggles; Shift extends; rubber band from the padding selects a rectangle; double-click enters a folder.
- Phase 7 builds on this: the drag source gains move and link actions and the count badge, and drop targets go on the same content box and on the view.

**Shortcut safety (task 6.6, settles R3).** Application accelerators run in the capture phase, before the focused widget sees a key, so two mechanisms keep file commands away from text entries:

- Keys that are also text-editing keys and belong to the folder view — Delete, Shift+Delete, BackSpace, Ctrl+Enter — are not accelerators at all. They are bound on the view widget (`view/keys.rs`) and fire only while the view has the keyboard focus. This is also why upstream's menu shows no shortcut beside Move to Trash and Remove.
- Cut, Copy, Paste and Select All are accelerators, because the menu shows them. While the window's focus widget is a `gtk::Text` or `gtk::TextView`, `MainWindow::refresh_enabled` disables those four actions; a disabled action does not consume its key, so the entry gets it. Opening the Edit menu moves the focus into the menu, which enables them again for the files.
- Dialogs are separate windows without `win.*` actions, so no file accelerator resolves there.
- Checked with real key input by `scripts/agent/checks/navigation.sh`. Phase 7 must run it again once Move to Trash and Copy do something.

**Drag and drop actions (tasks 7.10–7.11).** Learned from GDK's source, `gdkdrop-wayland.c`:

- On Wayland `gdk::Drop::actions()` is the source's whole offer only until an action has been negotiated; after that it is that one action. It cannot be used to see which modifier the user holds, and GTK sources do not narrow their offer for modifiers. The destination decides: `view/dnd.rs` asks the seat's keyboard for Ctrl and Shift and feeds `core::transfer::resolve`.
- A move is only carried out if the negotiated action is a move, so a source that offers copy alone is never moved from.
- Preferring `ASK` makes GDK report the source's whole offer at the drop, which is what the ask menu needs; both our source and our targets include `ASK`.
- Wayland knows copy, move and ask. Link is shown as a copy while hovering and made at the drop.
- For the default `auto`, the source filesystem of our own drags is remembered at drag-begin; for a foreign drag the file list is read while it hovers and its first file is asked. The final decision at the drop asks both filesystems again.

**Compact layout (task 5.14).** A horizontal `GridView` behaves as upstream's compact view: columns fill top to bottom, Down and Right move as expected, the view scrolls sideways. Two additions were needed: the wheel is mapped to the horizontal adjustment, and names get a fixed width (deviation 12). No fallback to a vertical list was needed.

**Rules learned the hard way (task 5.15).**

- `GridView` keeps `max-columns × 8` cells alive around its anchor, visible or not. `max-columns` must track the number of cells that fit across the view (`view/grid.rs`); a large constant builds a widget per file.
- Never call into a list widget from a handler of its model's `items-changed`: the widget has not processed the change yet and GTK crashes. Queue the call (`TabPage::keep_at_start`).
- A wrapping `Label` needs equal `width-chars` and `max-width-chars`; a small maximum doubles the cost of the first screenful.
- While a folder loads, the view is re-anchored on its first item until the user scrolls, clicks or types; otherwise GTK keeps the first item it happened to show in place and the view drifts.

---

## 11. Dependencies and measurements

**Dependencies** (additions need a line here saying what they replace and why):

| Crate | Version | Used by |
|---|---|---|
| `gtk4` (feature `v4_14`) | 0.11.5 | `tallboy` |
| `glib`, `gio` | 0.22.10 (`gio/v2_78`) | `tallboy-fs`; raw non-UTF-8 trash paths require `FileInfo::attribute_file_path` |
| `gdk-pixbuf` | 0.22.0 | `tallboy-fs` |
| `gio-unix` | 0.22.8 | `tallboy-fs`; verified `DesktopAppInfo` is in this crate |
| `async-channel` | 2.5.0 | `tallboy-fs` |
| `thiserror` | 2.0.21 | `core`, `fs` |
| `log` | 0.4.34 | all |
| `tempfile` (dev) | 3.27.0 | tests |
| `glib-unix` | 0.22.8 | `tallboy`; `SIGTERM`/`SIGINT`/`SIGHUP` sources. gtk-rs moved `unix_signal_add_local` out of `glib` in 0.22, as it did `DesktopAppInfo` into `gio-unix`; the alternative was hand-written `unsafe` `sigaction` |
| `gio` feature `v2_80` | — | `tallboy`; `ApplicationCommandLine::printerr_literal`, the only way to print an option error on the *calling* terminal when the invocation is forwarded to a running instance |
| `glib` feature `log` | — | `tallboy`; `GlibLogger`, the `log` → GLib bridge |

**Measurements** (filled in by tasks 1.1, 2.11, 3.11, 5.8, 5.15, 10.6). UI figures come from `scripts/agent/bench.sh`: release build, headless `cage` at 3840×2160 and scale 1.5 like the owner's monitor, timed from just before `exec`. They were not taken on the live GPU session, which agents must not use:

| Item | Budget | Measured |
|---|---|---|
| System library versions | — | GTK 4.22.5; GLib/GIO 2.88.4; gdk-pixbuf 2.44.4; Rust/Cargo 1.98.1; rustfmt 1.9.0 |
| Empty GTK 4 window, first frame | baseline | 156 ms (median of 10) |
| `$HOME` window, first frame | baseline + 100 ms = 256 ms | 169 ms (median of 10) |
| ≈3,000 entries, load finished | ≤ 150 ms | **Missed.** `/usr/bin`, 2,259 entries: 274 ms in a shown window; the backend alone (`fsls`) takes 206–217 ms. GIO sniffs the content of every executable while enumerating (about 90 µs a file); `fixture/big` avoids it because `.txt` is decided by name. Not a UI cost. For task 10.6 |
| 50,000 entries, first batch | ≤ 100 ms | 17.3 ms headless (Phase 2); **painted** in a shown window: 77–93 ms |
| 50,000 entries, loaded and sorted | ≤ 1.5 s | 653.4 ms headless (Phase 2); in the UI 552–580 ms |
| Re-sort 50,000 entries | ≤ 300 ms | 28.3 ms headless (Phase 2); in the UI 34–43 ms for a direction flip. The fixture has no second distinct order to sort by |
| Longest main-loop stall | ≤ 50 ms | 37–40 ms while loading `fixture/big` in a shown window. Start-up has one longer pause of about 110 ms, the window's own first frame, which the empty baseline window has too |
| Memory per 10,000 files | ≤ 15 MB | 10.2 MiB (resident set, 10,000 → 50,000 files); 13.8 MiB counting the fixed cost of the first screenful |
| Copy, cross-filesystem vs `cp -a` | within 10% | tmpfs → btrfs, 1 GiB + 128 small files; median of 3: job 145.768 ms / cp 144.983 ms = 1.005 |
| Copy, same filesystem on btrfs | clones | shared extents confirmed by filefrag; 380.715 ms |
| Thumbnails, `fixture/images` (500 images) | no stutter; second visit instant | Longest stall 17 ms while 459 thumbnails were generated; second visit loaded in 78 ms with a 31 ms stall, textures from memory |

**Re-run at the end of Goal B (task 9.12), same script, with thumbnails, dual pane, Preferences and the `FileManager1` service in the binary:** empty window 159 ms; `$HOME` window 186 ms (budget 259 ms; it was 169 ms after Phase 5, the difference is the thumbnail workers and the icon theme lookup at start); `/usr/bin` loaded 384 ms after `exec`, still the missed budget above; `fixture/big` in a shown window: first batch painted 69–96 ms, loaded and sorted 552–604 ms, longest stall 35–37 ms, re-sort 34–41 ms; memory 10.2 MiB per 10,000 files.


### Phase 10 measurements (2026-10-13)

Release, warm caches, isolated headless cage, 3840×2160 at scale 1.5; frame medians are ten runs. `target/phase10-bench.log`, `phase10-copybench.log`, `phase10-fsls.log` and `target/agent/review-*` hold local artifacts (not committed). Historical measurements above are retained for comparison.

| Item | Budget / result | Phase 10 measurement |
|---|---|---|
| Empty / Home first frame | Home ≤ baseline + 100 ms; pass | 163.100 / 184.905 ms; calibrated limit 263.100 ms |
| `/usr/bin`, 2,259 entries | ≤150 ms; **miss explained** | 286.740 ms loaded/sorted in a shown window; backend 228.097 ms to load, 228.887 ms including sort. Startup median 386.730 ms includes the first frame. GIO content sniffing of executables remains the dominant backend cost; changing content-test policy was not part of hardening. |
| 50,000 entries, first batch painted | ≤100 ms; pass in shown window | 69.813–98.590 ms (four navigations) |
| 50,000 entries, fully loaded/sorted | ≤1.5 s; pass | 548.313–591.682 ms in shown window; startup 686.983–713.765 ms |
| Re-sort 50,000 entries | ≤300 ms; pass | 42.401 / 44.849 ms |
| Longest loading stall | ≤50 ms; pass in shown window | 31.3–42.3 ms for 50,000 files. Startup first-batch paint is 208.8–209.7 ms and stall 133.5–136.5 ms; these include GTK's first frame and are reported separately, not claimed to satisfy shown-window latency. Smooth scrolling on the owner's GPU remains owner QA. |
| Memory growth per 10,000 files | ≤15 MB; pass | Empty 167.6 MiB, 10k 193.8 MiB, 50k 235.1 MiB; 10.3 MiB slope (10k→50k), 13.5 MiB/10k including initial view cost |
| Memory after closing loaded view | Within a few MB of baseline; **miss explained** | Release daemon RSS KiB: baseline 171,640; loaded 232,016; navigated empty 232,016; loaded again 231,808; last window closed 232,024. About 59 MiB retained; repeated visits reuse it and tab/folder counters are zero. Allocator high-water retention is consistent with these results, not conclusively proven. Unbounded caches were fixed; no allocator-specific unsafe trim was added. |
| D7 cross-filesystem copy, tmpfs→btrfs | ≤1.100 ratio vs cp; pass | 1 GiB + 128 small files, three runs: median cp 150.629 ms / job 149.252 ms = 0.991 |
| D7 same-filesystem btrfs copy | Shared extents; pass | `filefrag` confirms cloning; first copy 386.385 ms includes first-use flushing of newly written extents |
| Supplementary warm native comparison | Original PROMPT ≤1.100 ratio; **miss explained** | Synced sources, three runs: cp 4.793 ms / job 32.029 ms = 6.683. Preparation, guard canonicalization, staged publication and per-entry metadata dominate near-free clones. D7 replaces this native throughput gate with extent-cloning verification; safety checks retained. |
| Thumbnails, 500-image view | Progressive, reuse; verified with limits | Fresh isolated cache generated 417 visible-cell thumbnails; first load/sort 57.360 ms, longest loading stall 17.6 ms; second visit 70.306 ms / 24.1 ms. Screenshot reviewed at 1.5. The loading timer does not instrument every later decode callback; fresh 500-image backend generation/reuse test also passes. |

---

## 12. Decisions and open questions

Agents append one line per choice made under §2 rule 4.

- Phase 4: `invocation::parse_os` is the CLI/GApplication boundary for raw native argv; non-UTF-8 filenames are escaped to file URIs, preserving caller cwd, Home expansion and `--`. `parse` remains the text-input adapter.

- Phase 4: Symbolic-link jobs derive an absolute backend path through GIO, preserving escaped raw bytes; cross-server/backend targets are rejected with NotSupported rather than creating a misleading link. Remote backend support remains subject to the backend itself and owner QA.

- Phase 4: FileManager1 registration acquires with DO_NOT_QUEUE and without replacement, even rejecting a duplicate owner on the same connection. Drop sends asynchronous ReleaseName. Importer reports originals without modifying them; Goal B chooses when to persist the returned Config.

- Phase 4: Thumbnail cache uses the installed gdk-pixbuf PNG loader for Thumb:: chunk readback (verified); no extra PNG reader is needed. Pool results are owned RGB/RGBA pixels with rowstride; requests accept fractional scales and retain a 500-entry LRU. UI must retain each request until delivery and cancel on unbind.

- Phase 4: The read-only volume example confirmed `/dev/sda1` and `/dev/nvme0n1p3` as unmounted internal disks. Busy GIO errors are preserved without FORCE; actual busy physical-device unmount is reserved for owner checkpoint 8.12. Automount uses the unfiltered inventory, independent of Places visibility.

- Phase 3: `TALLBOY_ALLOW_ROOTS` uses semicolon/newline separators so URI ports remain intact. Parent canonicalization is a guard check only; user-file mutations all use GIO. URI dot traversal is refused while guarded.
- Phase 3: Job protocol boxes conflict metadata and separates `Answer` from single-use `Reply::answer`; `JobOptions` carries `no_usb_trash` and optional initial pause. Count logical sizes exclude directory entry sizes (matching this btrfs `du -sb`); allocated sizes are separate.
- Phase 3: Private GVFS tests and remote headless tools must poll the default GLib context. New isolated contexts work for local GIO, but GVFS asynchronous completion hung until the default context was dispatched.
- Phase 3: Jobs use a lightweight metadata query for traversal and full metadata only for conflict dialogs. Copies stage to an absent private path (exclusive GIO create); avoiding nested GIO overwrite persistence meets the throughput gate. Mixed file-type replacements retain a backup until publication succeeds and roll back failures.

- Phase 2: Terminal definitions use `program|exec argv|hold argv`; `terminal::DEFAULT_LIST` is shipped in fs until Goal B supplies `data/terminals.list`. Main must dispatch `terminal::supervise_if_requested` before GApplication; the single-binary supervisor provides keep-open without a shell for terminals lacking a hold flag.

- Phase 2: GLib filename collation keys expose comparison but no safe byte accessor; `core::sort::compare_by_name` accepts a comparator closure so fs retains opaque GLib keys without unsafe code.

- Phase 1: Pure date formatting uses UTC; the UI may use GLib local-time formatting. Rename ranges use UTF-8 byte offsets and must be converted to character offsets by GTK. Profile ids escape hyphens/underscores to avoid instance collisions.
- Phase 1: Fedora system rustfmt works; stale `~/.cargo/bin/rustfmt` lacks its driver library. Select `/usr/bin/rustfmt` via `RUSTFMT` in the check script when available; no global toolchain changes.

- Phase 5: `dev-run.sh` uses the profile `dev` under `~/.cache/tallboy-dev`, not `agent`: `agent-run.sh` wipes the agent profile on every run, and owner QA should keep its window size and settings between runs.
- Phase 5: Move to Trash, Remove and Backspace-for-parent are bound on the folder view, not as application accelerators. That is what upstream does (its menu shows no shortcut for them), and it is half of the R3 answer: those keys cannot reach a file while a text entry has focus. The other half is in the window: while an editable has focus, Cut, Copy, Paste and Select All are disabled so the entry gets the keys.
- Phase 5: Listings follow symbolic links for display metadata (type, size, content type of the target) while `symlink` and `symlink_target` still identify the link; upstream does the same. Jobs never follow links.
- Phase 5: Zoom In and Zoom Out list the keypad shortcut first so the menu reads as upstream's; `Ctrl+=` and `Ctrl+-` work as well.
- Phase 5: `agent-run.sh` drives the UI two ways: actions over D-Bus, and real keyboard and pointer input through `scripts/agent/vinput.c`, a small wlroots virtual-keyboard and virtual-pointer client built on demand with `cc` and `wayland-scanner`. `wtype` and `ydotool` are not installed.
- Phase 5: A hidden `win.debug-dump` action prints one `tallboy state:` line (location, load state, counts, selection, focus); agent checks read it back. `win.go-to` takes a path or URI. Neither is in any menu.

- Phase 6: The toolbar has upstream's history arrow between Back and Forward as well as the right-click and long-press that PROMPT §8.3 asks for; all three open the same menu, newest entry first.
- Phase 6: Activating an item that is part of the selection opens the whole selection, as upstream does; one folder is entered in place, several folders open as tabs.
- Phase 6: The Open With list shows the applications a menu would show plus any registered for the type; `gio::AppInfo::all` also returns hidden helper entries, which are left out.
- Phase 6: Column toggles in the detailed list's header menu are a small action group on the list itself, not rows of the window's action table: they exist only while a list view does.
- Phase 6: Type-ahead leaves Space to the list (it toggles the item under the cursor), so a search cannot start with a space.
- Phase 6: A missing terminal shows the error with its hint; the jump to Preferences ▸ Advanced is added with the Preferences dialog in Phase 9.
- Phase 6: The Open With *submenu* belongs to the context menu and arrives with it in task 8.1; the `win.open-with` action and the dialog are done.
- Phase 6: Dialog questions with fixed answers use `gtk::AlertDialog`; its buttons have no mnemonics, so the Execute File question is answered with the pointer, Tab or Esc.

- Phase 7: Task 7.1's review changed the engine twice before any UI used it: a file can no longer replace a folder, and trashed folders are deleted as a whole. Both are in `HANDOFF-B.md` §2 with the observations left for Goal C.
- Phase 7: The UI checks the guard before it starts a job, so a refusal is one dialog that explains the lock, not a failed job; the engine's own check stays in place behind it. Guard text never reaches the user (`ops::guard_message`).
- Phase 7: The progress window is its own top-level window, not transient for the main window: closing the main window must not take a running job's only control with it, and the application is held until the job ends.
- Phase 7: A metadata warning (R11) never keeps the progress window open; it is logged, and listed only when an error keeps the window open anyway.
- Phase 7: Move to Trash is removed from the window's action map inside `trash://`, and its menu item is hidden when the action is missing. A disabled action would only grey it out, and the menu bar model is shared by all windows.
- Phase 7: Rename and Create New run the backend's synchronous helpers on a worker (`gio::spawn_blocking`); their dialogs validate the name first and stay open on an invalid one.
- Phase 7: Create Link…, Move to… and Copy to… use `gtk::FileDialog::select_folder`, which is the portal's chooser when one runs.
- Phase 7: Paste after Cut empties the clipboard only when tallboy owns it; another program's clipboard cannot be cleared from here.

- Phase 8: Two more engine defects surfaced through the trash view and were fixed with tests: the metadata conversion aborted on a file info lacking an attribute, and the folder monitor reported the watched folder's own attribute changes as a child. Both are in `HANDOFF-B.md` §1.
- Phase 8: Context menus are built when shown, from the window's actions; "Open With" lists the applications registered for the type. Templates, archiver items and custom actions are left out (D4).
- Phase 8: Right-click on an item's padding counts as empty space and clears the selection, consistent with deviation 14.
- Phase 8: The autorun choice "Open in File Manager" is stored in `[autorun]` as `tallboy`.
- Phase 8: Autorun is only offered for a volume that tallboy itself mounted because it was inserted, not for one mounted by hand or by another program.
- Phase 8: Bookmarks and volumes are application-wide; one task each follows the bookmarks file and the volume monitor and tells every window's side pane to rebuild.
- Phase 8: The tree does not list a folder's subfolders until its row is expanded; `no_child_non_expandable` is honoured with one cheap probe per row as it comes into view.
- Phase 8: Properties validates the name, the owner and group, and the default application before applying anything, so a rejected value changes nothing.

- Phase 9: Thumbnail textures are kept for 500 files or 96 MiB, whichever comes first; 500 of the largest size would be about 2 GiB.
- Phase 9: A cell's image always takes a square of the icon size, so a thumbnail arriving cannot change a row's height.
- Phase 9: Dual pane is one more `Pane` in a `Paned`; the active pane is whichever was last clicked or given the keyboard. Turning dual pane off closes the second pane's tabs. `F6` is the window action `win.focus-next-pane`.
- Phase 9: Preferences is one non-modal window per application, without OK or Cancel; text fields apply on Enter, on losing the keyboard and on close. The page builder (`dialogs/prefs.rs`) is the only place that binds a widget to a setting.
- Phase 9: `--import-pcmanfm` reads PCManFM's profile of the same name as ours and falls back to its `default` profile, so `--profile dev --import-pcmanfm` imports something. Imported keys are applied on top of the profile's current values, not on top of defaults.
- Phase 9: The importer and `--show-pref` errors are answered from the command-line handler; a first instance's failing status is also kept in the application's own exit code, because GLib resets it once the main loop has turned.
- Phase 9: `FileManager1` is only asked for by the `default` profile. `TALLBOY_FILEMANAGER1=1` overrides that for agent checks on a private bus; it is not documented for users.
- Phase 9: `install.sh` takes `--prefix` and `--uninstall` beyond what D27 names. The launcher is written with the binary's full path. Agents verified it in a scratch prefix only; installing into the owner's `~/.local` is the owner's step at checkpoint 5.
- Phase 9: The icon is installed under the application id, `io.github.codedsleep.Tallboy`; About falls back to the theme's file-manager icon when it is not installed.
- Phase 9: The leak counters count what the UI holds: a tab page, and a folder being listed and watched by a tab or by a directory-tree row.

- Phase 10: Preserve a folder that replaces a conflict's file before publication, and query the activated target's filesystem before trusting execution; these restore D15 and existing overwrite policy.
- Phase 10: Shared metadata strings/icons are FIFO bounded at 1,024/256; decoded thumbnails enforce both 500 entries and 96 MiB, matching the texture byte cap. Live values survive eviction.
- Phase 10: A combined Properties change applies attributes before rename, and only renames after a successful job without skips/cancellation. Sparse conflict names use the metadata fallback; recycled tree probes validate row identity.
- Phase 10: Reject password-bearing location userinfo at the shared parser boundary; use the mount prompt for credentials (deviation 25).
- Phase 10: Preparation/child error recovery, inaccessible permission repair, and partial listing error presentation remain owner decisions (HANDOFF-C decisions 1–3; deviations 16, 26, 27). Guard removal and v1.0.0 await sign-off under 10.10.

- **Open — S6:** mount points inside a recursive delete. Included unless the owner says otherwise before task 3.6.
- **Settled — R9:** headless `cage` + `grim` capture works, and `cage` accepts virtual keyboard and pointer input (task 5.2). No fallback was needed.
- **Settled — O-4:** GIO copies clone btrfs extents; `filefrag -v` reports shared extents (Phase 3).

---

## 13. Implementation log

One entry per completed phase: date, agent, commit range, what was verified by test, by screenshot and by hand.

- 2026-10-13 — Codex, Phase 1, `6833248` through the phase-completion commit: workspace build and GTK-version stub verified; 23 table-driven core tests; fmt/clippy/layering gate green, including a deliberate GLib-in-core rejection. Fixture verified at 50,000 files and unchanged on second run. No screenshots or owner QA required for this backend phase.

- 2026-10-13 — Codex, Phase 2, `a2b6422` through the phase-completion commit: settings, metadata, 5,000-item loading/cancellation, coalesced create/modify/rename/delete/RootGone, bookmarks, completion and private-XDG two-application launches pass. Shell-special filenames reach children as one argv entry. Release 50,000-item benchmark and warning-free cargo doc verified. `check.sh` green with fatal GLib criticals. Live mounts deferred to Goal B as scoped.

- 2026-10-13 — Codex, Phase 3, guarded-engine commit through the phase-completion commit: all job and data-loss tests pass, including a real tmpfs→btrfs move and injected unreadable-source copy failure. Private-bus GVFS trash/restore/empty and unsupported-trash delete offer pass with `TALLBOY_TEST_GVFS=1`. Cross-filesystem throughput ratio 1.005; btrfs clones verified. `check.sh` green, warning-free docs, no owner-file mutations or screenshots. Mount-boundary detection is unit-tested; no physical disk mount state was changed.

- 2026-10-13 — Codex, Phase 4, `4b1ed0c` through the Goal A gate commit: volume filters/autorun/affected locations and Busy error preservation, installed PNG Thumb:: chunk readback at mode 0600, fractional size classes, 500 thumbnail requests, orientation and pre-start cancellation, owner config copies and quick_exec advisory, private-bus gdbus requests/name ownership, raw argv and same-server symlink paths verified by tests. Read-only volume example lists sda1 and the second NVMe correctly. All 68 workspace tests pass with fatal GLib criticals (including private GVFS); fmt/clippy/layering and warning-free docs pass. Public API documentation is enforced by the compiler; HANDOFF-A maps every Goal B task. GTK smoke stays explicitly deferred to 5.13; no screenshots, owner QA or physical-device mount changes in Goal A.

- 2026-10-13 — Claude, Phase 5, `423e9f7` through the phase-completion commit. **By test** (`check.sh` green, 68 backend tests plus 5 in the GTK crate): action-table uniqueness, profile pre-scan, the URI index including a forced hash collision, extension sorting, symlink-following listings, shared metadata strings, and the GTK smoke run under `G_DEBUG=fatal-criticals`. **By agent** (headless `cage`, private bus, real key and pointer input, state read back): every menu and submenu against PCManFM 1.4.0 screenshots; forwarding to the first instance; a separate instance per profile; option errors on the caller's terminal; `SIGTERM`; hand-edited config and unknown-key preservation; browsing, Backspace, history; status bar; empty and error placeholders; live create, rename, delete; a deleted folder; the 5.14 gesture spike; the 5.15 budgets with `scripts/agent/bench.sh`. **By hand:** nothing yet — owner checkpoint 1 is open. Backend patches (three, each with a test, for `HANDOFF-B.md`): `Column::Extension` and the `ext` list column in core; folder listings follow symbolic links; `FileMeta` shares repeated strings and drops its duplicated sort keys (`SortFacts`). The `/usr/bin` load budget is missed in the backend and is left for 10.6 with its cause recorded in §11.

- 2026-10-13 — Claude, Phase 6, `524873a` through the phase-completion commit. **By test** (`check.sh` green): history menu jump, path-completion prefix, type-ahead backward search, plus the earlier suites. **By agent** (`scripts/agent/checks/navigation.sh`, `opening.sh`, `single-click.sh`, all PASS, and scripted runs listed in QA.md): toolbar toggles, tabs, completion, the 6.6 shortcut-safety check with real keys, breadcrumbs, history menu, list headers and column menu, mode switching on `fixture/big`, zoom, filter, per-folder settings, type-ahead, single click with hover, emblems, grouped launches, the shell-name file, Open With and set-default, execute and launcher prompts, terminal, Run a Command, command-line files and folders, `-n`, daemon mode and lifetime. **By hand:** owner checkpoint 2 is open; focus of a launched application under Hyprland can only be checked there. Backend patch (for `HANDOFF-B.md`): `History::entries`, `index`, `go_to`. Agent tooling fix: `agent-run.sh` no longer races a stale log.

- 2026-10-13 — Claude, Phase 7, `b565fa6` through the phase-completion commit. **By review:** task 7.1, written up in `HANDOFF-B.md` §2. **By test** (`check.sh` green; trash tests also with `TALLBOY_TEST_GVFS=1`): `a_file_never_replaces_a_folder`, `a_folder_may_replace_a_file_and_keeps_its_source`, the non-empty trashed folder in `gvfs_trash_subprocess`, `copying_a_folder_keeps_its_mode_and_time_without_a_warning`. **By agent** (`scripts/agent/checks/fileops.sh` 37 checks, `dnd.sh` 13, `dnd-ask.sh` 3, `progress.sh` 8, and `navigation.sh` and `opening.sh` again; all PASS): create, rename, copy, cut, paste, every conflict answer, copy path, trash and remove with confirmations, file keys in a text entry, enablement, the locked-build refusal, progress with pause, resume and cancel on 50,000 files, drags with each modifier, selection drags, no-op and into-self, tab-label hover and drop, the ask menu. **By hand:** owner checkpoint 3 is open; clipboard and drag with other programs can only be checked there. Backend patches (for `HANDOFF-B.md`): trashed folders deleted whole; `Question::Conflict::can_overwrite` and `JobError::ReplaceFolder`; no warning for the unsettable `standard::size` on folders.

- 2026-10-13 — Claude, Phase 8, `8e551f8` through the phase-completion commit. **By test** (`check.sh` green): `sparse_info_from_a_backend_is_not_fatal`, `a_change_to_the_watched_folder_itself_is_not_a_child_event`, the six Connect to Server URIs and its error cases, the id lookup. **By agent** (`scripts/agent/checks/menus.sh`, `sidepane.sh`, `properties.sh`, all PASS, and the earlier scripts again): the context menus and right-click rules, trash listing, restore and emptying in the agent profile's trash, Places rows and their config keys, the shared bookmarks file including an external edit, the tree following through a hidden folder and live changes, Properties with rename, live count, a mixed-selection chmod and a recursive one, the Connect dialog. Volumes were only *listed*: nothing was mounted, unmounted or ejected. **By hand:** owner checkpoint 4 is open and carries everything that needs a device, a share, a phone or another program; SFTP, FTP and WebDAV are marked "not verified by hand" in QA.md (D9). Backend patches (for `HANDOFF-B.md`): attribute-safe `FileMeta::from_info`; monitor events about the watched folder itself ignored.

- 2026-10-13 — Claude, Phase 9, `cf128fb` through the phase-completion commit. **By test** (`check.sh` green): `preferences_pages_are_one_based` and the adjusted `invalid`, with every earlier suite. **By agent** (`scripts/agent/checks/run-all.sh`: 14 scripts, 200 assertions, all PASS): thumbnails at scale 1.5 with stall figures from a release build, dual pane, leak counters back to zero, Preferences by keyboard on every page, the importer against a copy of the owner's settings in the isolated profile, `FileManager1` over the private bus, the installer in a scratch prefix, the desktop file and man page validators, the README's differences list generated from §10. **X11:** navigation, file operations, menus and dual pane pass under XWayland; modifier keys during an X11 drag could not be checked because the headless compositor crashes (QA.md). **By hand:** owner checkpoint 5 is open; nothing was installed into the owner's `~/.local` and no default was changed. Backend patch: `--show-pref` pages 1–5 (`abd0c6d`). A leak found by the new counter and fixed: directory-tree rows kept their folder monitor after being collapsed.

- 2026-10-13 — Codex, Phase 10 review/hardening, `c49ce0e` through the handoff-record commit: ten fixes with regression coverage; all ten HANDOFF-B patches audited; GTK lifetimes, every UI job launch, and all 26 security items reviewed. Final fmt/clippy/workspace/layering/harness/headless smoke/tree tests and warning-free cargo doc pass; actual private-GVFS trash run passes. Baseline 14-script UI run passed 200 functional assertions and exposed one toolkit warning; corrected Properties/drag/tree/menu-session runs passed 56 assertions with no warnings and counters zero. Menu/submenu, emblem and fresh scale-1.5 thumbnail screenshots inspected. Release benchmarks and misses recorded in §11. README has all 27 deviations in identical order/text. Owner live-session QA remains unchecked; 10.10 is pending sign-off.
