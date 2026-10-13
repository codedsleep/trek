# Goal B handoff — application to review

Claude · Goal B, Phases 5–9 · started 2026-10-13

**Status: complete, 2026-10-13.** Goal B, Phases 5–9, is finished and pushed; the five owner checkpoints in `QA.md` are open. The build is still locked (`fs::guard`), and nothing in Goal B set `TALLBOY_UNLOCKED`.

`SCOPE.md` §10–§13 hold the deviations, measurements, decisions and the per-phase log; `QA.md` holds what was checked and how. This file is what Goal C needs on top of those.

## 1. Backend patches

Each was made under SCOPE §2 rule 2: smallest change that unblocks, with a test. Task 10.1 keeps, reworks or reverts each.

| # | Commit | Crate | Change | Why | Test |
|---|---|---|---|---|---|
| 1 | `423e9f7` | core | `sort::Column::Extension`; `ext` accepted in `Columns` | View ▸ Sort Files ▸ By Extension and the `ext` list column had nothing behind them | `sort::tests::extension_column` |
| 2 | `67ffbd2` | fs | Folder listing and monitor re-queries follow symbolic links (`FileQueryInfoFlags::NONE`) | A link to a folder was listed as an `inode/symlink` file: sorted with files, could not be entered, missing from the directory tree. `symlink` and `symlink_target` still identify the link; a broken link keeps the `SymbolicLink` type. Job traversal is untouched and never follows links | `folder::listing_follows_symlinks_for_display_metadata` |
| 3 | `e678996` | core, fs | `sort::SortFacts` and `compare_facts`; `FileMeta` drops `sort_keys` for `sort_facts()`; content type, description, icon, owner, group and filesystem strings become `Arc<str>` shared through `meta::intern` | 2.1 KiB per listed file missed the 15 MB per 10,000 files budget (R6). Now 10.1 MiB | `meta::metadata_variants` |
| 4 | `524873a` | core | `History::entries`, `index`, `go_to` | The Back/Forward history menu | `history::tests::menu_jump` |
| 5 | `b565fa6` | fs | `delete` does not descend below `trash://` items | Finding 2 below | `trash::gvfs_trash_subprocess` (needs `TALLBOY_TEST_GVFS=1`) |
| 6 | `9cd78b7` | fs | `Question::Conflict::can_overwrite`; `JobError::ReplaceFolder`; a remembered Overwrite is not applied when the destination is a folder and the source is not | Finding 1 below | `jobs::a_file_never_replaces_a_folder`, `jobs::a_folder_may_replace_a_file_and_keeps_its_source` |
| 7 | `43acaa1` | fs | `NotSupported` from `copy_attributes` on a folder is not recorded as a warning | GLib applies mode and times and then reports the read-only `standard::size`; every copied folder carried a spurious metadata warning | `jobs::copying_a_folder_keeps_its_mode_and_time_without_a_warning` |
| 8 | `8e551f8` | fs | `FileMeta::from_info` checks every attribute before reading it; unreported access rights are assumed except execute | The typed getters log a critical or, for `edit_name`, panic on a missing attribute; the trash root has no edit name or size, and nothing obliges a remote backend to report everything. With `panic = "abort"` that ended the process | `meta::sparse_info_from_a_backend_is_not_fatal` |
| 9 | `a4976f4` | fs | The monitor ignores events about the watched folder itself unless they say it is gone | A chmod of the open folder, or the trash can's item count changing, was delivered as `Changed` for a file with the folder's own URI: the folder could appear inside itself, and for `trash:///` it hit patch 8's crash, so the trash view never updated | `folder::a_change_to_the_watched_folder_itself_is_not_a_child_event` |
| 10 | `abd0c6d` | core | `--show-pref` accepts pages 1–5 instead of 0–4 | PROMPT §8.25 and upstream count pages from 1 and there are five; page 5, Advanced, was rejected, which the missing-terminal dialog and the man page both need | `invocation::preferences_pages_are_one_based` |

Patch 3 changes public field types of `FileMeta`; patch 6 adds a field to a public enum variant and replaces the test `overwrite_between_file_and_directory_keeps_source_and_publishes_complete_item`, which asserted the behaviour finding 1 removes. Those two deserve the closest look.

The workspace `Cargo.toml` was not touched. The `tallboy` crate adds `glib-unix` and the `gio/v2_80` and `glib/log` features in its own manifest (SCOPE §11).

## 2. Task 7.1 — data-loss review of `fs::jobs` and `fs::guard`

Read as a reviewer on 2026-10-13: `guard.rs`, `jobs/{mod,engine,walk,operations,transfer}.rs`, `jobs/transfer/{file_copy,replacement}.rs`, `trash.rs`, and the tests in `tests/{jobs,data_loss,trash,guard}.rs`.

### Fixed before any UI was built on the engine

1. **A file could replace a folder, recursively deleting it — also through "overwrite all".** `transfer_to` treated a source file meeting a destination folder as an ordinary conflict. On Overwrite, `publish` → `replace_mismatch` moved the folder aside, published the file and then ran `delete` on the old folder. Because `Runner::ask` replays a remembered `Overwrite { apply_to_all: true }`, a user who answered "overwrite all" to the first file conflict of a large copy would silently lose every destination folder that happened to share a name with a source file. An existing test asserted this replacement as intended behaviour. PCManFM, GIO and `cp` refuse it. Fixed as patch 6: the conflict is always asked, Overwrite is not an accepted answer to it, and the UI disables the button. A folder replacing a file is left as it was: it costs exactly the one file the user agreed to overwrite.
2. **Empty Trash and Remove failed on any trashed folder with contents.** `delete` walked into the trashed folder and tried to delete its children, which the trash backend refuses. Not data loss, but both commands were unusable for folders. Fixed as patch 5. The existing GVFS test only trashed a single file.

### Checked and found sound

- **Guard coverage.** Every write is preceded by `guard::check` on the entry it touches: preflight for all sources and the destination, then again per entry in `transfer_to`, `conflict`, `publish`, `delete`, `attrs`, `trash`, `restore`, `reserve_temp`, `cleanup` and `remove_temporary`. Parent symbolic links are resolved for the check, so a destination that passes through a link out of the allowed roots is refused entry by entry, not just at the top.
- **Symbolic links are never followed.** `info` and `children` query with `NOFOLLOW_SYMLINKS`; "directory" always means `Directory && !is_symlink`. A link to a folder is deleted, copied, moved and chmod-skipped as a link. A destination that is a link to a folder is a conflict, not a merge target.
- **No partial destination files.** Contents go to an exclusively created private temporary beside the destination and are published by rename. Cancel, a failed copy and the metadata-retry path all remove the temporary; the user's existing destination is not opened for writing at any point.
- **Move never deletes an uncopied source.** A file's source is deleted only after `publish` returned true. A folder's source is removed with a non-recursive `delete` only when the whole subtree went through and nothing in it was skipped; an error anywhere propagates before that line.
- **Into-self is refused before any write**, lexically and on canonical paths.
- **The same file reached through two paths** (a symlinked parent): the native rename is a no-op on identical directory entries, and the copy-then-delete fallback cannot be reached for it because the entries are on one filesystem. Traced by hand; no test.
- **Trash never becomes delete without an explicit answer.** Only `Answer::DeleteInstead` reaches `delete` from `trash`.
- **A dropped `Reply` cancels**; a dropped `JobHandle` cancels.

### Left for Goal C to weigh (none loses data as far as I can tell)

3. **Path-based traversal can be raced.** Between `info` saying "directory" and `children` enumerating it, another process can swap the directory for a symbolic link; the walk then continues through the link. This is the classic recursive-delete race and is inherent to walking by path with GIO (no `openat`). While the build is locked the per-entry guard check still refuses anything that resolves outside the allowed roots. After the guard is removed nothing does. Upstream has the same exposure. Worth one line in the README's safety notes rather than an attempt to fix.
4. **An error inside a folder abandons the rest of that top-level item.** `transfer_to`, `delete` and `attrs` propagate a child's error with `?` up to `Runner::run`, which asks Retry / Skip for the *top-level* source. Skip leaves the remaining children of that folder unprocessed (for a move, still safely in the source). Upstream asks per file and carries on. Copying a folder that contains one unreadable file therefore stops at that file. A per-entry ask-and-continue loop would fix it; I did not make that change inside a safety-critical engine without Codex.
5. **Retry after a mid-folder copy error re-asks conflicts** for the files the first attempt already copied, because the destination folder now exists and is merged. Safe, noisy.
6. **A kill or crash mid-copy leaves a hidden `.tallboy-copy-<pid>-<n>` file** in the destination folder. `panic = "abort"` makes this possible. Nothing cleans these up later.
7. **Recursive chmod applies children before the folder.** Removing read permission works; *adding* search permission to a tree that lacks it fails at the first level. Non-destructive.
8. **Copy sources are guard-checked like move sources**, so while locked the owner cannot copy *from* a real folder *into* the fixture. Stricter than D13 needs; it only matters for QA, and QA.md says so.
9. **`Policy::from_environment` bakes `CARGO_MANIFEST_DIR/../../target/tmp` in at compile time.** Harmless for a build that is run in place, meaningless once installed; goes away with the guard.
10. **`folder::FsInfo::id` is never filled in.** The loader asks `query_filesystem_info` for `id::filesystem`, which is a file attribute, so the field is always `None`. The tab asks for the folder's own `id::filesystem` instead (`tab/events.rs`, `query_folder_facts`). Either fill the field from a file query or drop it.

## 3. Measurements

`scripts/agent/bench.sh`, release build, headless `cage` at 3840×2160 and scale 1.5, timed from just before `exec`; run again on 2026-10-13 with everything in. SCOPE §11 has the history and the Goal A figures.

| Item | Budget | Measured |
|---|---|---|
| Empty GTK 4 window, first frame | baseline | 159 ms (median of 10) |
| `$HOME` window, first frame | baseline + 100 ms | 186 ms |
| `/usr/bin`, 2,259 entries, loaded | ≤ 150 ms | **Missed:** 384 ms from `exec`, about 225 ms of it the listing. The backend alone takes 206–217 ms because GIO sniffs every executable's content. For task 10.6 |
| 50,000 entries, first batch painted | ≤ 100 ms | 69–96 ms in a shown window |
| 50,000 entries, loaded and sorted | ≤ 1.5 s | 552–604 ms |
| Re-sort 50,000 entries | ≤ 300 ms | 34–41 ms |
| Longest main-loop stall while loading | ≤ 50 ms | 35–37 ms |
| Memory per 10,000 files | ≤ 15 MB | 10.2 MiB |
| 500 thumbnails generated | no stutter | longest stall 17 ms; second visit 78 ms to loaded |

The first frame of any window costs one pause of 120–150 ms that the empty baseline window has too; the stall figures above are for a window that is already shown.

## 4. What was verified, and how

`QA.md` has one section per phase with every item marked test, agent or hand. In short:

- **By test.** `scripts/check.sh`: format, clippy with warnings denied, the workspace's tests, the GVFS trash test when `TALLBOY_TEST_GVFS=1`, and a start-up smoke run. Goal B added tests only where it patched the backend (§1) and for pure UI-side logic: the action table (unique names and accelerators), the profile pre-scan, Connect to Server URIs, owner and group lookup, type-ahead, path completion and the view's item index. The GTK code itself has no unit tests; it is covered by the agent checks.
- **By agent.** `scripts/agent/checks/run-all.sh` runs 14 scripts, 200 assertions, in a headless compositor with real keyboard and pointer input (`scripts/agent/vinput`), a private D-Bus session and isolated XDG folders. All pass at the head of the branch:

  | Script | Assertions | Covers |
  |---|---|---|
  | `navigation` | 21 | toolbar, path bar, tabs, history, shortcut safety in text entries |
  | `opening` | 20 | default applications, Open With, executable prompt, terminal, Run a Command |
  | `single-click` | 4 | single-click activation and hover selection |
  | `fileops` | 37 | trash, delete, rename, create, link, move-to, copy-to, clipboard, conflicts, the locked-build refusal |
  | `dnd`, `dnd-ask` | 13, 3 | drags inside one window with Ctrl, Shift, both, and the ask menu |
  | `progress` | 8 | the progress window on a 50,000-file copy: pause, resume, cancel |
  | `menus` | 10 | context menus, right-click selection rules, trash restore and emptying |
  | `sidepane` | 15 | Places, bookmarks, the directory tree |
  | `properties` | 12 | Properties, permissions, Connect to Server |
  | `dual-pane` | 19 | dual pane, active pane, `F6` |
  | `prefs` | 18 | Preferences by keyboard |
  | `leaks` | 10 | live tab pages and watched folders back to zero |
  | `filemanager1` | 10 | the D-Bus service |

- **By screenshot,** looked at by the agent: every dialog, each view mode, thumbnails at scale 1.5, all five Preferences pages, About, the icon.
- **By hand: nothing yet.** The owner checkpoints are open. They carry everything an agent could not or must not do: real hardware (USB stick, internal disks, MTP phone), SMB, drags between applications, a GTK file chooser adding a bookmark, the real install, the look on the live monitors.
- **Not verified at all:** SFTP, FTP and WebDAV (D9: built to the spec, covered by tests and this review only); password prompts and "remember forever" (no secret service on this machine); modifier keys during a drag on X11 (the headless compositor crashes on it).

## 5. Where review is most wanted

In order of how much a defect would cost.

1. **Every path from the UI into a job.** `ops.rs` (the one entry, its guard pre-check and the locked message), `window/fileops.rs`, `clipboard.rs` (cut state and what a paste does with it), `view/dnd.rs` (which action a drop resolves to: a move only if the negotiated action is a move), `sidepane/places.rs` drops, `dialogs/properties/` (the `SetAttrs` job and the recursion question). The question is always the same: can a gesture the user did not mean end in a move or a delete?
2. **Backend patches 3, 6 and 10** (§1): the first two change public types and replaced a test; the third changes an accepted range.
3. **The findings left open in §2**, above all 4 (an error inside a folder abandons the rest of that folder) and 3 (the path race), which the README now describes.
4. **`tab/events.rs` and `tab/views.rs`.** The view is rebuilt on mode and settings changes, and the idle that waits for the incremental sort now does three things: releases the scroll pin, moves keyboard focus to the first item, and applies an awaited selection. It is correct as far as the checks go and it is the least tidy code in the crate.
5. **`window/mod.rs` pane handling.** Focus is moved out of a pane before the pane is removed (GTK warns otherwise), and closing the first pane's last tab moves the second pane's tabs across. `close_tab` is where to look.
6. **`view/thumbs.rs`.** A result is matched to its cell by a ticket; the texture cache is bounded by an *estimate* of bytes (width × height × 4).
7. **`sidepane/dirtree.rs`.** The listing task is aborted on collapse, reload and dispose. Before this it leaked a folder monitor per collapsed row; `leaks.sh` now guards it, but only for the paths that script walks.
8. **`dialogs/prefs.rs` and `Settings::import`.** Handlers are disconnected when the window closes; `import` calls every registered handler once per registration, which rebuilds open views several times. Harmless, wasteful.
9. **`filemanager1.rs`.** The `TALLBOY_FILEMANAGER1` override, and that `ShowItems` selects one item per folder.
10. **Line counts.** The crate is 14,152 lines. `window/mod.rs` (876) is the largest file and would split along the seams its sections already mark: panes, tabs, settings, enablement. `dialogs/prefs.rs` (704) is long because each control is spelled out.

Known and accepted, not asking for review: deviations 12–15 (GTK 4 list widgets and Wayland drag semantics), the `/usr/bin` budget (backend, task 10.6), and that the guard's allowed roots include the system temporary directory.
