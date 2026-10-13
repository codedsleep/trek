# Phase 10 code review

Reviewed 2026-10-13 against `.planning/SCOPE.md` (precedence: SCOPE > DISCOVERY > PROMPT). Findings come first; status distinguishes fixes from decisions left to the owner. The release guard remains in place pending SCOPE 10.10.

## Findings

### High — overwrite could remove a folder created after the question — fixed

At `crates/tallboy-fs/src/jobs/transfer/replacement.rs:29`, publication previously moved the current destination aside and recursively deleted it without rechecking its type. Replacing the destination file with a non-empty folder while the conflict question was open caused that folder to be deleted by an answer intended for a file. The new test `overwrite_cannot_delete_a_folder_created_after_the_conflict_question` failed on the original implementation. Publication now inspects the entry actually moved aside, refuses file-over-folder replacement, and rolls back. Existing destination contents and source contents are preserved. This closes a gap in HANDOFF-B patch 6 without changing the intended overwrite policy.

### High — launcher execution trust used the containing folder's filesystem — fixed

`crates/tallboy/src/open.rs:68` inherited `FileMeta::filesystem_type` from the listing. A symlink to a launcher on another mount therefore used its parent's filesystem for D15 trust. Files opened from the command line had no filesystem type and could also be treated as trusted. Activation now asynchronously queries the activated file's filesystem before execution decisions; unknown types do not establish launcher trust. `execution_trust_queries_the_target_filesystem_and_refuses_unknown_trust` verifies stale parent metadata, symlink targets, command-line metadata and query failure; `core::open::decision_table` covers synthetic filesystems. No live vfat or network device was mounted for this test.

### Medium — password-bearing locations could reach logs, history and bookmarks — fixed

`crates/tallboy-core/src/location.rs:88` previously accepted `sftp://user:password@host/path`, while UI diagnostics, display, history and bookmarks retain location URIs. D15/DISCOVERY §7 requires credentials to go through MountOperation. The shared parser now rejects password-bearing userinfo, including escaped separators, with a message containing no credential. `embedded_passwords_never_enter_stored_or_logged_locations` covers SSH, FTP, WebDAV, SMB and IPv6 username-only URIs. This is recorded as deviation 25.

### Medium — caches retained unbounded strings/icons or up to 2 GiB of pixels — fixed

`crates/tallboy-fs/src/meta.rs:48` and `crates/tallboy/src/view/item.rs:42` kept every distinct value forever. Serialized file icons can contain a different path per file, so these sets were not limited to MIME types. Both caches now have FIFO limits (1,024 strings / 256 icons); eviction preserves values held by live items. Tests: `shared_strings_are_bounded_and_eviction_keeps_live_metadata_valid`, `distinct_file_icons_do_not_accumulate_after_tabs_close`.

The GTK texture cache had a 96 MiB limit, but `crates/tallboy-fs/src/thumbs/pool.rs:60` independently kept 500 decoded images, potentially about 2 GiB at 1024×1024 RGBA. It now enforces both 500 entries and 96 MiB, and replaces duplicate keys from concurrent workers. Test: `decoded_cache_evicts_large_images_and_keeps_the_newest`; the 500-image integration test still checks generation and reuse.

### Medium — a failed UI session could be reported as passing — fixed

`scripts/agent/checks/run-all.sh:18` ignored the session exit status whenever earlier PASS lines existed; most checks did not inspect toolkit warnings. `scripts/agent-run.sh:144` also ignored an application exit during a driver. Both now propagate failures; the suite scans GTK/GDK/GLib/GObject diagnostics and keeps separate logs per session. `scripts/agent/test-harness.sh` checks clean acceptance, exit-7 rejection and warning rejection and runs in `check.sh`. An isolated real headless session that printed PASS and subsequently exited 7 also returned 7.

### Low — symlink emblem CSS produced GTK negative-size warnings — fixed

`data/style.css:40` used -18/-10 vertical margins around 16/8-pixel emblems and a -16 horizontal margin around a 12-pixel emblem. The strengthened drag suite found `GtkImage reported min height -2`. The margins now stay within the icon dimensions; `dnd.sh` exercises the link in all four factories and the runner rejects toolkit warnings.

### Low — FsInfo never supplied filesystem identity — fixed

`crates/tallboy-fs/src/folder/load.rs:62` queried `id::filesystem` through the filesystem-info API rather than the file-info API, leaving `FsInfo::id` empty. It now uses the correct asynchronous file query. `folder_capacity_includes_the_file_filesystem_identity` verifies the emitted field; the tab's independent query remains useful before FsInfo arrives.

### Medium — Properties renamed before its attribute job — fixed

`crates/tallboy/src/dialogs/properties/mod.rs:355` independently launched rename and SetAttrs against the same original URI. For a folder, rename ran while the recursion question was still open; answering Yes then failed to find the old path. The extended `properties.sh` reproduced the failure before the fix. Rename now follows a completed attribute job only when it has no errors, skips or cancellation. The regression checks that the name stays unchanged while the question is pending, then checks the new name and all child modes. Standalone rename remains independent when no attributes change.

### Low — a tree probe could update a recycled cell — fixed

`crates/tallboy/src/sidepane/dirtree.rs:197` only checked that its expander widget still existed when an asynchronous has-subdirectories probe completed. A rebound cell could therefore inherit the previous folder's expandability. It now checks row identity, and unbind clears the expander's row reference. `stale_directory_probe_does_not_change_a_rebound_cell` uses actual GTK tree rows and a recycled expander in an isolated headless session, invoked by `check.sh`; the sidepane script also passes.

### Medium — conflict naming still required an optional edit-name — fixed

`crates/tallboy-fs/src/jobs/transfer.rs:73` called the non-null `FileInfo::edit_name` getter before converting through the sparse-metadata boundary. A backend omitting this optional attribute could abort during a conflict. It now uses `FileMeta::edit_name`, which falls back to display name. `sparse_info_from_a_backend_is_not_fatal` verifies that fallback; the conflict-answer integration tests remain green. Actual sparse remote conflict behavior remains unverified on a live server.

## Summary

Thirteen concrete findings: two High, eight Medium, three Low. Ten fixed; three behaviour decisions remain below. The architecture and declared v1 feature groups are implemented, with the recorded deviations and owner QA limitations. This is a reviewed locked build, not a release authorization.

## Behaviour decisions left to the owner

These remain unfixed under SCOPE 10.5; none warrants silently redesigning traversal, error recovery or partial-result presentation during the release review.

1. **Medium — preparation and child-error recovery are all-or-top-level.** `crates/tallboy-fs/src/jobs/engine.rs:236` propagates preparation failure before the execution error question. Later, errors bubble from a child to the top-level source: Skip abandons its remaining subtree and Retry repeats already copied entries/conflicts. A mixed readable/unreadable tree can prevent the entire job during prepare. HANDOFF-B findings 4–5 described only the execution part. Recommendation: decide on per-entry Retry/Skip and best-effort preparation together; test totals, skipped counts and source preservation before changing it.
2. **Medium — recursive permission repair cannot enter an inaccessible subtree.** `crates/tallboy-fs/src/jobs/walk.rs:18` prepares the entire tree before `crates/tallboy-fs/src/jobs/operations.rs:62` applies children-first chmod. Adding search/read permissions to an inaccessible tree can fail before applying the parent change. Recommendation: a separate permission-repair strategy with an explicit ordering decision; do not reverse chmod order globally, because removing permissions needs the current order. This extends HANDOFF-B finding 7.
3. **Medium — an initial listing error after its first batch leaves Loading set.** `crates/tallboy/src/tab/events.rs:103` only marks Failed if the model is empty. `fs::folder::load` emits Error without Finished on a next-batch failure. The partially populated view remains in Loading, with no Retry placeholder. Recommendation: choose whether to retain partial results with an error banner or show the failure placeholder; make that a terminal state, and distinguish later monitor errors from initial listing errors.

## Backend patches (SCOPE 10.1)

| HANDOFF-B patch | Decision and reason |
|---|---|
| 1: extension sorting / columns | Keep: implements the required upstream sort and list column; extension comparator and codec tests pass. |
| 2: listing follows symlinks | Keep: display/navigation needs target type; jobs retain NOFOLLOW. Execution filesystem trust is separately repaired above. |
| 3: SortFacts and Arc strings | Keep with bounded interning: removes duplicate sort fields and meets listing memory budget; arbitrary file icons need a cap. |
| 4: history menu methods | Keep: exposes existing history entries and validates jumps; `menu_jump` and navigation check cover it. |
| 5: delete trash directories whole | Keep: GVFS refuses child mutations below a trashed directory; private-bus trash tests cover non-empty folders. |
| 6: cannot overwrite a directory with a file | Keep with publication revalidation above: both remembered answers and raced destinations now preserve folders. |
| 7: folder metadata NotSupported warning | Keep: mode/time preservation tested; unsupported directory attributes must not turn a completed copy into failure. Other errors still warn. |
| 8: sparse FileInfo getters | Keep with conflict-name fallback: backend attributes are optional; fatal-critical test verifies fallback and non-executable unknown access. |
| 9: ignore watched-folder attribute events | Keep: prevents inserting the root as its own child; RootGone still delivered; monitor regression passes. |
| 10: Preferences pages 1–5 | Keep: upstream numbering and Advanced navigation require it; parser tests and UI Preferences checks pass. |

## UI-to-job audit (SCOPE 10.3)

These are all constructors/spawn sites found by `rg 'JobKind::|jobs::spawn' crates/tallboy/src`. Match arms in `ops::precheck` are policy dispatch, not additional launch sites.

| Call site | Input, target and safety checks |
|---|---|
| `window/fileops.rs`, Trash | Snapshot URI list from active pane selection; use_trash=false and trash locations route to Delete; confirmation names/counts captured before dialog. |
| `window/fileops.rs`, Delete | Same selection snapshot; Shift+Delete is view-scoped; confirm_del gates confirmation; engine never follows symlinks and asks at mount boundaries. |
| `window/fileops.rs`, Restore | Selected trash URI list; engine obtains original path and guard-checks restored destination, with ordinary conflict handling. |
| `window/fileops.rs`, EmptyTrash | Always confirms; engine's trash preflight checks isolated data directory before listing/deleting. |
| `window/fileops.rs`, Copy/Move/Link | Shared transfer method validates destinations and into-self/no-op rules; chooser captures original selection; paste captures current/selected-folder destination before async read. |
| `view/dnd.rs` → `window/fileops.rs` | Snapshot list and destination; source supplies permitted actions; move requires MOVE permission, including Ask; default Auto performs filesystem queries asynchronously. Places and tab-label drops use the same entry. Negotiated-action interop remains owner QA; cross-application behavior was not exercised. |
| `dialogs/properties/mod.rs`, Count | Read-only selected files; handle retained while open, dropped/cancelled on close; worker events update labels through weak refs. |
| `dialogs/properties/mod.rs`, SetAttrs | URI snapshot, masked changes from core::perms; recursive choice is explicit; worker skips links. Combined rename/permission ordering is covered by the new Properties regression. |
| `ops.rs`, spawn_with_options | Central mutation launch; upfront guard checks plus independent worker preflight; pause/cancel/Ask callbacks use weak Running captures; app hold lasts through Finished. |

Single-item rename/create in `window/fileops.rs` and Properties use `gio::spawn_blocking` and the same guarded backend functions, validate names, and retain original URIs; awaited selection uses weak tab refs. No widget performs copy/move/delete itself.

## GTK ownership, scheduling and layering (SCOPE 10.2)

Reviewed window/pane/tab lifecycle, model/index, grid/list factories, drag and clipboard, thumbnail loaders, Places/tree, Properties, Preferences, launcher paths, Settings, App and FileManager1. Window/tab callbacks generally capture WeakWindow/WeakTab; settings and clipboard handlers disconnect on close; list-column snapshots use weak columns. Tree tasks abort on collapse/dispose; row handlers disconnect on unbind. Thumbnail tickets prevent stale texture results and unbind drops requests. Shared caches were the concrete lifetime defect found above.

A tree has-subdirectories probe (`sidepane/dirtree.rs`) also needs row-identity validation before updating a reused expander; reviewed/fixed separately. Clipboard cut contents are cleared before a move succeeds, matching the declared cut/paste behavior but requiring another Cut after a refused move.

User-folder GIO reads use futures; writes run in backend workers. Remaining synchronous work on the main thread includes guard canonicalization during precheck, config/bookmark/per-folder-settings persistence, local account lookup and DesktopAppInfo parsing. These are measured rather than described as universally nonblocking; guard precheck disappears only after owner sign-off. Large or slow config paths and local account-file lookup remain limits. UI modules contain presentation helpers (completion prefix, type-ahead cycling, account-name lookup) with tests; dependencies remain downward and workers have no GTK.

## Verification and limits

Final command results and performance figures are recorded in HANDOFF-C, SCOPE §11 and QA Phase 10. Security checklist evidence is appended to DISCOVERY §7. All declared v1 feature groups map to implemented modules and the saved UI checks; deferred Find Files/templates/actions/archiver/applications/desktop functionality stays deferred. README differences are compared mechanically with SCOPE §10.

Hardware mounts, live SMB/MTP, SFTP/FTP/WebDAV service behavior, secret-service password persistence, cross-application clipboard/drag, native compositor focus, live-monitor appearance and X11 drag modifiers remain owner checks. The path-swap race inherent in GIO traversal and crash-left staging files remain documented. This report does not claim those checks passed or authorize guard removal.
