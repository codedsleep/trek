# QA.md — `trek`

What was checked, how, and what is left for the owner. One section per Goal B phase.

**How an item was checked**

| Mark | Meaning |
|---|---|
| **test** | An automated test run by `scripts/check.sh` |
| **agent** | A scripted headless run with `scripts/agent-run.sh`: state read back from the application, real key and pointer input, screenshots |
| **hand** | Only the owner can check it, on the live desktop |

Agents cannot see the owner's monitors, other applications, real devices or the real clipboard, so anything that needs those is **hand**.

## Running it

```sh
scripts/dev-run.sh              # opens ~/.cache/trek-fixture on the live display
scripts/dev-run.sh ~/Documents  # browsing anywhere is fine; it is read-only outside the fixture
scripts/dev-run.sh --reset      # start again with an empty dev profile
```

`dev-run.sh` uses its own D-Bus session, its own config, trash and thumbnail cache under `~/.cache/trek-dev`, and the profile `dev`. Your real bookmarks, trash and default applications are not touched, and are not shown.

The build is **locked** until the Phase 10 sign-off: copy, move, delete, trash, rename and create are refused outside the fixture. Browsing and opening files work everywhere.

The reference is the installed PCManFM 1.4.0. For English labels start it with `LANGUAGE=C LC_ALL=C pcmanfm`; your locale shows "Rubbish Bin" where trek says "Trash Can".

If trek crashes, the reason is in `~/.cache/trek-dev/state/trek/crash.log`.

---

## Phase 5 — Shell and first view

### Checked by agents

| Item | How | Result |
|---|---|---|
| Menu bar: every label, order, separator and accelerator of File, Edit, View (with Sort Files, Folder View Mode, Toolbar, Path Bar, Side Pane), Bookmarks, Go, Tools, Help | agent: screenshots of every menu and submenu beside the same shots of PCManFM 1.4.0 | Same, except the deviations below |
| Find Files… and Go ▸ Applications are present but greyed out | agent: screenshot | Pass |
| A second `trek` invocation is handled by the running one | agent: second invocation adds a tab, exit status 0 | Pass |
| `--profile other` is a separate instance with its own config directory | agent | Pass |
| `--desktop`, `--find-files`, an unknown option | agent: message on the caller's terminal, exit status 2 | Pass |
| `SIGTERM` saves settings and exits with status 0 | agent | Pass |
| Hand-edited `trek.conf` (`show_hidden`, `sort`, `show_statusbar`, `side_pane_mode`) changes the start-up state; an unknown key survives a save | agent | Pass |
| Browsing: double-click and Enter open a folder, Backspace goes to the parent and selects the folder left | agent: pointer and keyboard input, state read back | Pass |
| Status bar: item count, hidden count, selected file, free space | agent: screenshot | Pass |
| "Folder is empty"; unreadable folder shows "Permission denied." with Retry | agent: `fixture/empty`, `fixture/perm/unreadable` | Pass |
| A file created, renamed or deleted in a terminal shows up within a second | agent | Pass |
| Deleting the open folder moves the tab to its parent | agent | Pass |
| A GTK critical fails the smoke run | test: `check.sh` runs `agent-run.sh --smoke` with `G_DEBUG=fatal-criticals` | Pass |
| Large folder budgets on `fixture/big` | agent: `scripts/agent/bench.sh`, figures in SCOPE §11 | See SCOPE §11 |
| Multi-selection drag, rubber band, compact layout (the spike) | agent: real pointer input | See SCOPE §10 "Gestures" |

**Menu deviations from the installed PCManFM, all intended**

- Bookmarks ▸ Edit Bookmarks… exists in trek (PROMPT §8.2); 1.4.0 edits bookmarks only from the side pane.
- Create New… lists only Folder and Empty File; templates are a later version.
- Tools ▸ Find Files… and Go ▸ Applications are greyed out; both are later versions.

### Owner checkpoint 1

Run `scripts/dev-run.sh`. The side pane is empty and there is no toolbar yet; both arrive in later phases.

- [ ] **Menus.** Open each menu beside PCManFM. Labels, order, separators and shortcuts match, apart from the three deviations above.
- [ ] **Browse.** Double-click into `deep`, keep going a few levels, press Backspace to come back. The folder you left is selected each time.
- [ ] **Keyboard.** Arrow keys move, Enter opens a folder, `Alt+Up` goes up, `Alt+Left` / `Alt+Right` step through history, `Alt+Home` goes Home, `F5` reloads.
- [ ] **Hidden files.** In `names`, `Ctrl+H` shows `.hidden` and `backup~`; the status bar count changes from "6 item(s) (2 hidden)" to "8 item(s)".
- [ ] **Sorting.** View ▸ Sort Files ▸ Descending reverses the order with folders still first.
- [ ] **Big folder.** Open `big` (50,000 files). It fills in without freezing, and scrolling with the wheel and the scroll bar stays smooth.
- [ ] **Live updates.** With `live` open, run `touch ~/.cache/trek-fixture/live/x` in a terminal: the file appears. Delete it: it goes.
- [ ] **Errors.** `perm/unreadable` shows "Permission denied." and a Retry button.
- [ ] **Window state.** Resize the window, drag the splitter, close, start again: size and splitter are as you left them.
- [ ] **Look.** trek has the colours of your GTK theme (NordArc) on GTK 4's own widget shapes, so it is close to PCManFM without matching it (SCOPE §12 "Theme"). Say so if anything is unreadable or has the wrong colour rather than merely a different shape.

---

## Phase 6 — Navigation, views, opening

### Checked by agents

The scripts are in `scripts/agent/checks/`; each prints PASS or FAIL per line and can be re-run (the command is in the file's first lines).

| Item | How | Result |
|---|---|---|
| Toolbar buttons and each View ▸ Toolbar toggle applying live | agent: screenshots | Pass |
| Tabs: New Tab, `Ctrl+Tab`, `Ctrl+Shift+Tab`, `Alt+1`, `Ctrl+W`, closing the last tab closes the window | agent: `navigation.sh` | Pass |
| Location entry: completion list, Tab for the only match, Down and Enter, `~`, Enter navigates, Esc restores | agent: `navigation.sh`, screenshot | Pass |
| **Shortcut safety (task 6.6):** BackSpace, Delete, `Ctrl+A`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V` typed into the location entry leave the folder and the selection alone | agent: `navigation.sh`, real key input | Pass |
| Breadcrumbs keep the deepest path after Up; `Ctrl+L` borrows the bar; history menu jump; Up selects the folder left | agent: `navigation.sh` | Pass |
| Detailed list: header click sorts ascending then descending with the Sort menu following; header menu adds a column; `columns=` saved | agent: pointer input, state read back | Pass |
| Switching view mode keeps the selection and the place, with and without a selection, on `fixture/big` | agent | Pass |
| Zoom In / Out / Normal Size | agent: screenshot | Pass |
| Filter…: `*.txt` in `names` shows 3 of 8, `*` clears; shown in the status bar | agent | Pass |
| Preserve This Folder's Settings: saved, absent elsewhere, restored on return, group removed when unchecked | agent: `dir-settings.conf` read back | Pass |
| Type-ahead: typing selects the first match, Down and Up cycle, Esc returns the keyboard to the view | agent | Pass |
| Single-click mode: hover selects after the delay, one click opens, Ctrl+click still selects | agent: `single-click.sh` | Pass |
| Symbolic-link emblem on `fixture/links`; a link to a folder shows as a folder | agent: screenshot | Pass |
| A file named `; touch pwned` reaches its application as one argument and nothing runs | agent: `opening.sh` | Pass |
| Three files of two types make two launches | agent: `opening.sh` | Pass |
| No default application leads to Open With; Esc cancels; choosing with "set as default" changes what `gio mime` reports | agent: `opening.sh` | Pass |
| Script and binary prompts use upstream's wording; Cancel runs nothing; Execute runs the script | agent: `opening.sh`, screenshot | Pass |
| A launcher that is not executable asks before starting | agent: `opening.sh` | Pass |
| `trek file dir` with relative paths from a second invocation; `-n` | agent: `opening.sh` | Pass |
| `F4` starts the terminal in the folder; Run a Command runs there; a bad command shows its error in the dialog | agent: `opening.sh` | Pass |
| Daemon mode stays without a window, serves later invocations, survives its last window closing, ends on `SIGTERM`; without it the process ends with its last window | agent | Pass |
| The large-folder budgets after this phase's additions | agent: `scripts/agent/bench.sh` | See SCOPE §11 |

**Not checked by an agent:** that a launched application takes focus under Hyprland (needs the live compositor); the look of the completion list and dialogs on a 4K monitor at scale 1.5; long-press on Back and Forward.

### Owner checkpoint 2

- [ ] **Toolbar.** Back, Forward, Up, Home behave as in PCManFM. Right-click Back: the history menu opens. View ▸ Toolbar: each checkbox shows or hides its buttons at once.
- [ ] **Location bar.** `Ctrl+L`, type `~/Do`, press Tab: it completes as far as it can and lists the matches. Down moves into the list, Enter takes one, Enter again goes there. Esc puts the old path back.
- [ ] **Typing is safe.** With a file selected, press `Ctrl+L` and then Delete, BackSpace and `Ctrl+C`. Only the text changes.
- [ ] **Breadcrumbs.** View ▸ Path Bar ▸ Buttons. Go a few levels into `deep`, then Up twice: the deeper buttons stay, the current one is pressed. Middle-click a button: it opens in a new tab.
- [ ] **Tabs.** `Ctrl+T`, `Ctrl+Tab`, `Alt+1`, drag a tab to reorder, middle-click a tab to close it, `Ctrl+W` on the last tab closes the window.
- [ ] **Views.** `Ctrl+1` to `Ctrl+4` switch icon, compact, thumbnail-size and detailed list; the selected file stays selected and in view.
- [ ] **Detailed list.** Click a header to sort; click again to reverse; folders stay on top either way and View ▸ Sort Files agrees. Right-click a header to add Permissions. Drag a column border, restart: the width is kept.
- [ ] **Compact view.** Columns fill top to bottom and the wheel scrolls sideways. Columns are all the same width here (a known difference).
- [ ] **Zoom and filter.** `Ctrl+KP_Add` / `Ctrl+=`, `Ctrl+-`, `Ctrl+0`. `Ctrl+E`, enter `*.png`: only matching files and all folders remain, and the status bar says so.
- [ ] **Per-folder settings.** In one folder tick View ▸ Preserve This Folder's Settings and switch it to detailed list. Other folders are unchanged; this one comes back as a list after a restart.
- [ ] **Type-ahead.** Click the view and type the start of a name.
- [ ] **Open files.** Double-click a text file and an image: they open in your applications **and the application takes focus**. (The dev session has its own default applications; see "Running it".)
- [ ] **Open With.** Open a file of a type with no default: pick an application, tick "Set selected application as default", then double-click the file again.
- [ ] **Executables.** `chmod +x` a small script in the fixture and double-click it: the question matches PCManFM's. Cancel does nothing.
- [ ] **Terminal.** `F4` opens kitty in the current folder.
- [ ] **Command line.** From a terminal inside the fixture, with trek already running through `dev-run.sh`: `scripts/dev-run.sh names` opens `names` as a new tab.

---

## Phase 7 — File operations, clipboard, drag-and-drop

Everything here happens inside the allowed folders. The build is locked: outside them every operation is refused with an explanation, and that refusal is itself one of the checks.

### Checked by agents

| Item | How | Result |
|---|---|---|
| Data-loss review of the job engine and guard (task 7.1) | review; two defects fixed with tests; write-up in `HANDOFF-B.md` §2 | Done |
| A file can never replace a folder, also after "overwrite all" | test: `a_file_never_replaces_a_folder` | Pass |
| Empty Trash and Remove work on a trashed folder with contents | test with `TREK_TEST_GVFS=1`: `gvfs_trash_subprocess` | Pass |
| Create New Folder / Empty File: the name asked for, a taken name made unique, an invalid name refused in the dialog, the new item selected | agent: `fileops.sh` | Pass |
| Rename: base name selected, extension kept, renamed item selected | agent: `fileops.sh`, screenshot | Pass |
| Copy and Paste into the same folder (auto-renamed copy) and into another; Cut and Paste moves and empties the clipboard | agent: `fileops.sh` | Pass |
| Clipboard offers `x-special/gnome-copied-files`, `text/uri-list`, plain text, and the KDE cut marker only for a cut | agent: `fileops.sh` | Pass |
| Conflict dialog: Skip, Overwrite, Rename and Esc each leave the expected files on disk | agent: `fileops.sh`, screenshot | Pass |
| Copy Path(s) | agent: `fileops.sh` | Pass |
| Move to Trash and Remove: upstream's wording, No keeps the file, Yes acts | agent: `fileops.sh`, screenshot | Pass |
| Delete, BackSpace, `Ctrl+C`, `Ctrl+X` typed in the location entry with a file selected do nothing to it (task 6.6 again, now that they act) | agent: `fileops.sh` | Pass |
| Enablement: nothing selected; read-only folder (no Create New, no Paste); Paste follows the clipboard | agent: `fileops.sh` | Pass |
| Locked build: Create New outside the allowed folders creates nothing and explains why, listing the allowed folders | agent: `fileops.sh`, screenshot | Pass |
| Progress window on a 50,000-file copy: appears, main window answers in 7 ms, Pause, Resume, Cancel, no partial file | agent: `progress.sh` | Pass |
| Drag onto a folder: plain (move on one filesystem), Ctrl (copy), Shift (move), Ctrl+Shift (link) | agent: `dnd.sh`, real pointer | Pass |
| Dragging one of two selected files carries both | agent: `dnd.sh` | Pass |
| Move onto the same folder does nothing; a folder into itself is refused | agent: `dnd.sh` | Pass |
| A drag resting on a tab label switches tabs; dropping there moves into that tab's folder | agent: `dnd.sh` | Pass |
| `drop_default_action=ask`: menu at the drop point, Move Here moves, a modifier skips the menu | agent: `dnd-ask.sh`, screenshot | Pass |

**Not checked by an agent:** anything between trek and another program (clipboard and drag in both directions); Create Link…, Move to… and Copy to… past the point where the system folder chooser opens; the drag icon and its count badge; trash on removable media (`no_usb_trash`); the mount-point question.

### Owner checkpoint 3

Work in `~/.cache/trek-fixture` (`scripts/dev-run.sh` opens it). `scripts/mkfixture.sh --clean && scripts/mkfixture.sh` rebuilds it afterwards.

- [ ] **Copy, move, delete.** Copy a folder with `Ctrl+C` / `Ctrl+V`, move one with `Ctrl+X` / `Ctrl+V`, rename with `F2`, trash with Delete, remove with Shift+Delete. Each confirmation reads as PCManFM's does.
- [ ] **Progress.** Copy `big` into `empty`. The progress window appears, shows file, speed and time remaining; Pause, Resume and Cancel work; the main window stays usable.
- [ ] **Conflicts.** Paste the same file twice into one folder: Overwrite, Skip, Rename and "Apply this option to all existing files" do what they say.
- [ ] **Trash.** Trashed files appear under Go ▸ Trash Can (the dev session's own trash). Restore and Empty Trash Can are Phase 8.
- [ ] **Locked build.** Browse to `~/Documents` and press `Ctrl+Shift+N`: the dialog explains the lock and nothing is created.
- [ ] **Clipboard with PCManFM, both ways.** Copy in PCManFM, paste in trek (into the fixture); cut in PCManFM, paste in trek. Then copy in trek and paste in PCManFM. *Paste into PCManFM is PCManFM's operation and is not locked: use a scratch folder.*
- [ ] **Drag and drop with PCManFM, both ways.** Drag from PCManFM onto the trek view and onto a folder in it; drag from trek into PCManFM.
- [ ] **Modifiers.** Inside trek: plain drag onto a folder moves, Ctrl copies, Ctrl+Shift links. From another program the modifier keys are only seen while trek has the keyboard (a Wayland limit, SCOPE §10 deviation 15).
- [ ] **Dual-window drag.** `Ctrl+N`, then drag between the two windows.
- [ ] **Tab drop.** With two tabs, drag a file onto the other tab's label and hold: the tab comes forward; drop.
- [ ] **Chooser commands.** Edit ▸ Copy to…, Move to… and Create Link… open a folder chooser and act on the choice.
- [ ] **Folder with one unreadable file.** Copy `perm` somewhere: the copy stops at the unreadable folder and offers Skip. Skip ends that folder (deviation 16).

---

## Phase 8 — Side pane, menus, trash, volumes, Properties

### Checked by agents

| Item | How | Result |
|---|---|---|
| A sparse file info from a backend does not abort the process | test: `sparse_info_from_a_backend_is_not_fatal`, under `G_DEBUG=fatal-criticals` | Pass |
| A change to the watched folder itself is not reported as a child changing | test: `a_change_to_the_watched_folder_itself_is_not_a_child_event` | Pass |
| Context menus: files, one folder, background, trash; right-click selection rules; Menu key | agent: `menus.sh`, screenshots | Pass |
| Trash: listed live, Restore returns a file and a non-empty folder to their origin, Empty Trash Can; Create New disabled and Move to Trash absent there | agent: `menus.sh`, in the agent profile's own trash (S4) | Pass |
| Places: default rows; `places_root`, `places_network`, `show_internal_volumes` from the config | agent: `sidepane.sh`, seeded config | Pass |
| Volumes are listed as PCManFM lists them (the two internal disks, when internal volumes are shown) | agent: state read back; nothing was mounted | Pass |
| Add to Bookmarks writes the shared `gtk-3.0/bookmarks`; a line added by another program appears without restart; Bookmarks menu; Edit Bookmarks moves an entry | agent: `sidepane.sh` | Pass |
| The row of the current folder is highlighted; clicking Home Folder and Trash Can navigates | agent: `sidepane.sh` | Pass |
| Directory tree expands to the folder, through a hidden `~/.cache`, selects it, and follows a folder created and removed outside | agent: `sidepane.sh` | Pass |
| Side pane mode switch and Show Side Pane | agent: `sidepane.sh` | Pass |
| Connect to Server builds the right URI for each of the six types; refuses an empty or malformed host | test: `each_kind_builds_its_uri`, `bad_input_is_explained`; agent: the dialog | Pass |
| Properties, General: values for a file, a folder with a live count, several files; rename on OK; Esc closes | agent: `properties.sh`, screenshots | Pass |
| Properties, Permissions: a mixed selection (600 and 644) set to View content = Nobody becomes 200 and 200; a folder's change asks about recursion and Yes applies to everything inside | agent: `properties.sh`, modes read with `stat` | Pass |

**Not verified by hand or by agent — built to the spec and covered only by tests and review (D9):** SFTP, FTP and WebDAV locations.

**Not checked by an agent, needs real hardware or another program:** mounting, unmounting and ejecting; the busy-volume message; automount on insertion and on start-up; the autorun dialog and its remembered choices; tabs closing or going Home on unmount; SMB and MTP; password prompts ("remember forever" is not available: no secret service runs on this session bus); a bookmark added in a GTK file chooser; dragging bookmarks to reorder; dropping files on a place.

### Owner checkpoint 4

Run `TREK_ALLOW_ROOTS=/run/media/$USER/<stick> scripts/dev-run.sh` for the USB steps. **Internal disks: mount and unmount only — do not write to them.**

- [ ] **Context menus.** Right-click a file, a folder, several files and empty space; compare each with PCManFM's menu. Templates, archiver items and custom actions are absent on purpose.
- [ ] **Places.** Home, Desktop, Trash Can and Devices are listed; the current folder's row is highlighted. Right-click a row for its menu.
- [ ] **Bookmarks.** `Ctrl+D` adds one. Add another in a GTK file chooser (for example `gtk4-demo`'s, started from the same terminal session): it appears without restarting trek. Drag one bookmark onto another to reorder. Bookmarks ▸ Edit Bookmarks… renames and removes.
- [ ] **Directory tree.** `Ctrl+7`. Navigate somewhere deep: the tree expands to it. Click a row: the view follows.
- [ ] **Trash.** Trash a file, open the Trash Can, hover it (deleted when, from where), right-click ▸ Restore. Right-click the Trash Can row ▸ Empty Trash Can.
- [ ] **USB stick.** Insert it: it mounts and the autorun dialog offers "Open in File Manager". Tick "Always perform this action", remove and re-insert: no dialog. Copy a file to it and back. Click its eject button; a tab showing it closes.
- [ ] **Busy volume.** With a terminal's working directory on the stick, eject: the error says it is in use and nothing is forced.
- [ ] **Internal disks.** Set `show_internal_volumes=1` (Preferences arrive in Phase 9; until then edit `~/.cache/trek-dev/config/trek/dev/trek.conf`). Click `SSD`: the password prompt appears, it mounts and opens. Unmount it from its row.
- [ ] **SMB.** Go ▸ Connect to Server…, Windows share: browse, copy one file each way (with the share in `TREK_ALLOW_ROOTS` as `smb://host/share`).
- [ ] **MTP phone.** It appears in Places; browse it. An executable text file on it opens instead of offering to run (deviation 5).
- [ ] **Properties.** `Alt+Return` on a file, on a folder (the size counts up; close it mid-count), on several files. Compare the values with `stat`. Change permissions on a mixed selection and on a folder with "apply recursively".

---

## Phase 9 — Thumbnails, dual pane, Preferences, packaging

`scripts/agent/checks/run-all.sh` runs every saved check of Phases 6–9 and prints one line each.

### Checked by agents

| Item | How | Result |
|---|---|---|
| Thumbnails fill in for `fixture/images` in icon, thumbnail and list modes; sharp at scale 1.5 (the `large` cache directory is used); cache files are mode 600 | agent: screenshots at 1920×1080 and 3840×2160, scale 1.5; cache listed | Pass |
| No stutter while thumbnails are generated; a second visit is served from memory | agent, release build, `TREK_TRACE=1`: longest stall 17 ms generating, 31 ms and 78 ms to loaded on the second visit | Pass |
| A thumbnail arriving does not move rows; the view stays at the top while loading | agent: `scroll=0` after loading with thumbnails on (it had drifted to 2592 before the fix) | Pass |
| Dual pane: toggle, click or `F6` to change the active pane, commands follow it, copy in one pane and paste in the other, tabs per pane, both ways of losing a pane's last tab | agent: `dual-pane.sh`, real input | Pass |
| Fullscreen | agent: `navigation.sh` (Phase 6) | Pass |
| About shows the version, GPL-2.0-or-later and the upstream credit; Keyboard Navigation lists keys that match the accelerator table | agent: screenshots; the table compared with `actions.rs` by reading | Pass |
| Leak counters: live tab pages and watched folders return to zero after tabs, navigation, dual pane, the directory tree and two windows | agent: `leaks.sh`, daemon mode | Pass |
| Preferences: one control of each kind saved in its stored form; Places rows and path bar mode change at once; the dialog follows a change made elsewhere; `home_path`; `--show-pref=N` | agent: `prefs.sh`, keyboard only; screenshots of all five pages | Pass |
| `--show-pref` accepts pages 1 to 5 and nothing else | test: `preferences_pages_are_one_based`, `invalid` | Pass |
| A missing terminal offers Preferences and opens the Advanced page | agent: empty `PATH`, `F4`, Enter; screenshot | Pass |
| `--import-pcmanfm` against a copy of the owner's `pcmanfm.conf` and `libfm.conf`, in the isolated profile: 57 keys imported, 4 reported as not imported, sources byte-identical afterwards; exits 0 by itself; exits 1 with nothing to import | agent | Pass |
| `org.freedesktop.FileManager1`: `ShowItems` with no window, `ShowItems` in the 50,000-file folder selects and scrolls to the file, `ShowFolders`, `ShowItemProperties`, a bad URI refused; a non-default profile does not take the name | agent: `filemanager1.sh`, `gdbus` on the private bus | Pass |
| The desktop file validates; the man page has no warnings; the icon renders | `desktop-file-validate`, `man --warnings`, rendered and looked at | Pass |
| `install.sh`: a plain install leaves `xdg-mime query default inode/directory` unchanged; `--set-default` changes it and installs the service file; `--uninstall` removes what was installed | agent: in a scratch prefix with `XDG_CONFIG_HOME` and `XDG_DATA_HOME` inside it. **Not run against the owner's `~/.local`** | Pass |
| The README's differences list has the same 24 entries, in order, as SCOPE §10 | generated from §10; counted | Pass |

### X11 (task 9.11, D12)

Run under XWayland inside the headless compositor with `GDK_BACKEND=x11`; the owner's X display was not used.

| Item | Result |
|---|---|
| The main window opens, lists a folder, renders menus, the side pane and Preferences | Pass (screenshots) |
| `navigation.sh` (21), `fileops.sh` (37), `menus.sh` (10), `dual-pane.sh` (19) | Pass, all of them. Pointer checks need `GDK_SCALE=1`: on the headless output GTK's X11 backend chooses scale 2 |
| `sidepane.sh` | 13 of 15. The two failures are the script's: an X11 window has no title bar here, so the rows sit 38 px higher than the positions the script clicks. Clicked at their X11 positions, Home Folder, Desktop and Trash Can all navigate. The very first pointer click after start is not delivered to an X11 client in this compositor |
| Drag and drop | A plain drag onto a folder moves the file. **A drag with Ctrl held crashes the compositor itself** (`coredumpctl` shows `/usr/bin/cage`, not trek), so modifiers during an X11 drag are **not verified** |

### Owner checkpoint 5

- [ ] **Thumbnails.** `scripts/dev-run.sh ~/.cache/trek-fixture/images`, then a folder of real photographs (browsing is not locked). They should fill in without the view jumping, look sharp at 1.5, and appear at once on a second visit. Turn them off and on in Preferences ▸ Display.
- [ ] **Dual pane.** `F3`. Click each pane: the path bar, toolbar and status bar follow, and a thin line in the selection colour marks the active one. `F6` goes side pane → pane 1 → pane 2. Copy in one, paste in the other; drag from one to the other. `F3` again closes the second pane.
- [ ] **Preferences.** Go through the five pages beside PCManFM's. Every change should take effect at once: single click, icon sizes, the Places rows, the tab bar, the path bar mode, the status bar. Close and reopen trek: the values are kept.
- [ ] **Importer.** `scripts/dev-run.sh --import-pcmanfm` prints what it took from your real PCManFM settings into the `dev` profile. (`dev-run.sh` has its own configuration folder, so copy `~/.config/pcmanfm` and `~/.config/libfm` into `~/.cache/trek-dev/config/` first.)
- [ ] **Help.** Help ▸ Keyboard Navigation and Help ▸ About.
- [ ] **Install side by side.** `scripts/install.sh`, then check `xdg-mime query default inode/directory` still names what it named before, and start `trek` from the launcher. It is the locked build: it browses your real folders and refuses to change them. `scripts/install.sh --uninstall` removes it.
- [ ] **Only if you want it as the folder handler already:** `scripts/install.sh --set-default`; "Show in folder" from a browser's downloads list should open trek with the file selected. The script prints the command that restores the previous handler.
- [ ] **X11, optional.** `GDK_BACKEND=x11 scripts/dev-run.sh`: drag files with Ctrl and Shift held, which could not be checked headless.


## Phase 10 — review and hardening (Codex, 2026-10-13)

The review and fixes are detailed in [CODE-REVIEW.md](CODE-REVIEW.md) and [HANDOFF-C.md](HANDOFF-C.md). The guard remains; owner sign-off and release are pending.

| Verification | Method | Result |
|---|---|---|
| All workspace tests, fmt/clippy, dependency layering; runner exit/warning rejection; GTK smoke and recycled-tree regression | Final `scripts/check.sh`, fatal GLib criticals | Pass |
| Public documentation | `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps` | Pass |
| Actual GVFS trash/restore/empty | `TREK_TEST_GVFS=1` private-bus test | Three pass |
| Full baseline UI suite | 14 scripts; saved logs; every functional assertion | 200 pass; diagnostic gate exposed a symlink-emblem GTK warning, subsequently fixed |
| Final affected UI paths | Properties / drag / sidepane / new menu-session | 13 / 17 / 15 / 11 pass (56 total), no toolkit warnings |
| Every menu and Create/View submenus, four view factories, six dialogs, last window closure | New `scripts/agent/checks/session.sh`; screenshots and counters | Pass; tabs=0, folders=0 |
| Properties attributes+rename race | Script failed before fix; checks old name during question, new name and child modes afterward | Pass after fix |
| Menu/submenu layout, compact symlink emblem, fresh thumbnails at scale 1.5 | Screenshots inspected in isolated cage | Pass |
| Fresh thumbnail cache | 417 visible-cell thumbnails generated; second visit; 500-image backend integration | Pass, with timing limits recorded in SCOPE §11 |
| README deviations | Numbered descriptions mechanically compared with SCOPE §10 | Same 27 entries in order |

Release performance figures are in SCOPE §11. Startup calibration, large-directory first batch/load/sort/stall, memory growth, cross-filesystem copy and native extent cloning pass. `/usr/bin` load and RSS return after closure miss their budgets; supplementary warm native copy is slower than cp. Explanations and exact figures are in HANDOFF-C. Zero live counters and stable repeat-visit RSS do not establish that RSS returns to baseline. Loading-stall timers do not establish smooth scrolling on the owner's GPU or instrument every thumbnail callback.

The final changed-path run follows the baseline run; it is not a claimed single final run of all 216 current assertions. Local artifacts: `target/phase10-final-check.log`, `phase10-final-doc.log`, `phase10-trash.log`, `phase10-ui.log`, `phase10-ui-targeted.log`, `phase10-bench.log`, `phase10-copybench.log`, `phase10-fsls.log`, `phase10-final-performance.log`, plus session logs/screenshots under `target/agent/checks` and `target/agent/review-*`.

### Owner release checkpoint

- [ ] Review and resolve or accept the three behavior decisions in HANDOFF-C.
- [ ] Accept the recorded budget misses/limits or request further work.
- [ ] Complete or explicitly waive owner checkpoints 1–5, including real remote/device/password behavior and cross-application transfer. "Remember forever" remains unavailable in agent sessions without a secret service.
- [ ] Sign off on guard removal and `v1.0.0` under SCOPE 10.10. No release tag has been created.
