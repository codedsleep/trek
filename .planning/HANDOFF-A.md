# Goal A handoff — backend to application

2026-10-13 · Codex · Goal A, Phases 1–4

`SCOPE.md` is the contract. The workspace contains the v1 core and GIO backend,
headless regression tests, and three examples. `tallboy` is still the linking
stub: the application, installation, screenshots and owner QA belong to Goal B.
The write guard remains enabled until the Phase 10 sign-off.

## Build and verify

System versions and exact crate pins are in SCOPE §11. `gio/v2_78` exposes the
safe raw-byte trash-origin getter; `DesktopAppInfo` comes from `gio-unix`.
No handwritten unsafe code is permitted in core or fs. Both crates now deny
missing public documentation, and the check script treats clippy warnings as errors.

From the repository root:

```sh
scripts/mkfixture.sh
TALLBOY_TEST_GVFS=1 scripts/check.sh
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo run --release -p tallboy-fs --example fsls -- "$HOME/.cache/tallboy-fixture/big"
cargo run --release -p tallboy-fs --example jobbench
cargo run -p tallboy-fs --example volumes
```

`mkfixture.sh` needs ImageMagick and creates 50,000 files, 500 PNGs, special
names, symlink cases and permission fixtures. Re-running it leaves existing
fixtures unchanged. Tests use owned temporary trees; thumbnail acceptance tests
read its 500 images. `jobbench` creates and removes its own 1 GiB dataset under
`/tmp` and the fixture. `volumes` only reads inventory. `fsls` accepts a URI too.
Rustdoc is at `target/doc/tallboy_fs/index.html` and `target/doc/tallboy_core/index.html`.

Fedora's `/usr/bin/rustfmt` works; the old rustup shim does not. `check.sh`
selects it when `RUSTFMT` is unset. GTK smoke is visibly skipped until task 5.13;
replace that skip with the application's isolated smoke invocation.

## Main-context and lifetime contract

Construct folders, bookmarks, volume models and the D-Bus service on the UI's
GLib main context. Use the **default** context when GVFS is involved: private
contexts work for native files, but GVFS completions stalled in headless tests
until the default context was dispatched. The private-bus tests demonstrate the
working setup. Async receivers must use `recv().await` on the UI; never block it.

Retain handles in their owning tab/window/application. Event consumer tasks
should capture receiver clones and weak widget references, rather than owning
the tab that owns the handle. `Folder::cancel` closes and drains events;
`Children` drops its folder; bookmark and volume models disconnect signals on
drop. A dropped job handle cancels the worker, and a dropped thumbnail request
cancels that request. Keep progress handles until `Finished`, including after
cancellation. Thumbnail pool teardown cancels workers without joining on the UI.

Pass GTK subclasses as GIO parent types: a `gtk::MountOperation` upcast to
`gio::MountOperation`, and the display's `gdk::AppLaunchContext` as
`gio::AppLaunchContext`. Neither backend crate depends on GTK or GDK.
Use `gio::File::equal`, rather than Rust object equality, for location identity.

## Public API as built

All entry points are module-qualified (`tallboy_fs::folder::Folder`, etc.).
Source doc comments define individual fields and errors.

### Pure core

| Module | Entry points and purpose |
|---|---|
| `config` | `Config::default`, `apply(group,key,value,warnings)`, `entries`; codecs for view, pane, bookmark, drop, sort, columns, toolbar and autorun values. `apply` returns false for unknown keys, preserving storage policy in fs. Invalid known values revert to their defaults with a warning. |
| `location` | `Location::parse`, `from_path`, `to_uri`, `path`, `display`, `breadcrumbs`; `Server::location` for the six Connect to Server types. Native `PathBuf` and percent-escaped URI conversion preserve arbitrary bytes. Display strings are presentation only. Passwords are never included in server URIs. |
| `sort` | `SortSpec`, `SortKeys`, `compare`, `compare_by_name`, `natural`. Folder grouping remains first in descending order unless mingled. Prefer `fs::meta::compare` when sorting `FileMeta`, so GLib's cached locale-aware filename keys are used. |
| `naming` | `validate`, `split_extension`, `rename_range`, `unique_name`; compound tar extensions and dotfiles covered. Rename selection offsets are UTF-8 **byte** offsets; GTK selection needs character offsets. |
| `format` | SI/IEC sizes, permissions, dates, speed, remaining time and status/selection/free-space strings. Dates are UTC; use GLib for local-time presentation in the UI. |
| `glob`, `helpers` | `glob::matches`; `zoom`; `type_ahead` with cycling. |
| `history` | `History::navigate`, `back`, `forward`, `current_mut`; entries carry scroll and cursor URI. Record view state before leaving a location. |
| `open` | `Item`, `Kind`, `classify`, `Activation`, `synthetic_execution`. Fill native/filesystem/default-app/trust facts from metadata and application lookup; one policy serves every activation entry point. |
| `enable` | `Selection`, `actions`: selection, writability, trash and clipboard state determine action availability. |
| `transfer` | `Payload`: GNOME copied-files, URI list, KDE cut marker and plain-path codecs; `resolve` chooses the drag intent; `check_drop` handles no-op/into-self checks. Escaped URI formats preserve filenames containing newlines. |
| `perms` | `Audience`/`Permission`, `audience`, `aggregate`, masked `AttrChange::set`, `special`, `apply_mode`. Unchanged controls must leave the mask empty. |
| `invocation` | `parse_os` for process/GApplication argv, caller cwd and home; `parse` for UTF-8 text; `Invocation`, `Request`, `InvocationError`, `application_id(profile)`. Default profile is `default`; IDs encode profile hyphens/underscores distinctly. Desktop and search options fail before requests execute. |

### Settings, listing and launching

- `settings_store::Store::load(user_dir, system_dirs, profile)` or
  `from_environment(profile)` exposes mutable `config` and `warnings`.
  `save` atomically writes known settings and retained unknown values to
  `$XDG_CONFIG_HOME/tallboy/<profile>/tallboy.conf`. System dirs are merged from
  low to high priority, then user settings. Corrupt files are moved aside as
  `.conf.broken` with a collision suffix; a final config symlink is replaced.
  The UI owns live notifications and debounce; run persistence off the main loop
  if it would cause visible stalls.
- `dir_settings::DirectoryStore::load(path)`, `get(group, defaults)`, `set`,
  `remove`, and `FolderSettings`. Choose
  `Store::path().with_file_name("dir-settings.conf")`. Group names are a native
  path string or URI; for a non-UTF-8 native path use its escaped URI.
- `meta::ATTRIBUTES` / `attributes(defer_content_test)` is the shared metadata
  query. `FileMeta::from_info` and synchronous `query` expose names, URI, type,
  icon, capabilities, times, permissions, filesystem identity/type, target and
  trash metadata. `name`, `symlink_target`, and `trash_orig_path` retain raw
  `PathBuf`s. `compare` applies core policy with opaque GLib collation keys.
  Optional attributes are checked before access; keep this when adding queries
  because missing attributes can otherwise produce fatal GLib criticals.
- `folder::Folder::new(file, Options, Option<gio::MountOperation>)` starts
  async enumeration and monitoring. Its `events` carries Started, Batch,
  Finished, Error, Added, Changed, Removed, Renamed, FsInfo and RootGone.
  Splice batches into the store; update monitor events by URI. Options capture
  `backup_as_hidden`, `defer_content_test`, and batch size (default 200).
  The loader mounts on NotMounted once and retries once. Changes coalesce for
  200 ms; RootGone uses `nearest_existing_ancestor` or Home.
- `complete::Children::new` lazily projects monitored folder events to
  directories; consume through its `recv`. `has_subdirectories` probes only
  until the first matching directory. Async `prefix` supplies directory-name
  completion; the UI controls common-prefix and keyboard behavior.
- `bookmarks::Store::open(config_home)` watches `gtk-3.0/bookmarks`.
  `snapshot` returns a `Bookmarks` document; `edit` applies add/rename/remove/
  move operations atomically. `changes` reports external and local changes.
  Unknown/comment lines survive; invalid entries cannot be activated.
- `apps::default`, `applications` (recommended then other installed apps),
  `all_for_type`, `group`, `launch`, `set_default`, `custom`, and `launcher`.
  Group by application and launch each group once with a supplied launch
  context. `launcher` accepts an already-confirmed native `.desktop` file;
  use `core::open` first. Custom commands are parsed as argv, without a shell.
- `terminal::parse_list`, `DEFAULT_LIST`, `resolve`, `Terminal::open`,
  `execute`, `command_argv`, `run`, `spawn`. Resolution is configured value,
  `$TERMINAL`, then first installed terminal. Format:
  `program|execution argv|hold argv`. A requested missing executable is an
  error. **Keep `supervise_if_requested(&args_os)` before GApplication startup**
  in main: it provides keep-open without a shell for terminals lacking a hold
  flag. The current stub already does this. User command parsing can use
  `glib::shell_parse_argv`; spawning always passes a vector of arguments.
- `trace::elapsed` logs timing when `TALLBOY_TRACE` is enabled.

### Guarded jobs and single-item operations

`jobs::spawn(kind)` uses the environment policy. For current preferences use
`spawn_with_options(kind, guard::Policy::from_environment(), JobOptions {
no_usb_trash: config.no_usb_trash, start_paused: false })`. Test harnesses can
inject a locked policy with `spawn_with_policy`.

`JobKind` includes Copy, Move, Link, Trash, Delete, Restore, EmptyTrash,
SetAttrs and Count. URIs retain arbitrary filename bytes. `JobHandle` exposes
`events`, `pause`, `resume`, `cancel`. Events are Preparing, Prepared, Current,
Progress (at most 10/s), Ask and exactly one Finished. Keep the `Reply` while
showing its dialog and consume it via `reply.answer(answer)`; dropping it
unanswered cancels that question. Conflict metadata is boxed. Apply-to-all is
scoped separately to conflicts, errors and unsupported trash. A MountPoint
question offers EnterMount/Skip/Cancel for recursive deletion boundaries.
Finished retains counts, allocated bytes, skips, errors and metadata warnings.

The engine preflights the entire selection and destination before writing.
File copies use private GIO staging, preserve old destinations through failure
or cancellation, and publish only completed data. Never substitute an
OVERWRITE copy to a preexisting staging file: it adds GIO persistence work and
failed the throughput budget. Directories merge; mixed type replacement uses
backup/publication/rollback. Move tries native rename first; fallback deletes
only successfully published sources. Symlinks are never traversed. Link
requires the same backend/server for source and destination and derives an
absolute target path with raw bytes through GIO; actual support is backend-dependent.
Count is read-only, includes directories in item counts, excludes directory
entry sizes from logical bytes, and also reports allocated bytes.

`jobs::rename`, `create_folder`, `create_file` are synchronous guarded
single-item helpers; dispatch them on a worker and present their typed failures.
Rename uses GIO display-name semantics; creation retries exclusive name clashes.
`trash::is_empty` is async, and `offer_delete` implements removable-media policy.
Restore reads `trash::orig-path` without UTF-8 loss. Confirm permanent deletion
in the UI; only an explicit DeleteInstead answer permits trash fallback.

Default allowed roots are the fixture, build `target/tmp`, and system temp.
`TALLBOY_ALLOW_ROOTS` adds native paths or URI prefixes separated by semicolons
or newlines. Trash requires XDG_DATA_HOME beneath an allowed root. Isolated Goal B
runs must add both the original fixture and their isolated XDG tree explicitly,
since changing XDG_CACHE_HOME changes the default fixture root. Agents must
never set `TALLBOY_UNLOCKED`. Parent symlinks are resolved for guard checks;
user-file operations themselves all use GIO. The guard is a development safety
boundary, not a substitute for the Phase 7/10 data-loss reviews.

### System services and thumbnails

- `volumes::Model::new(Filters)`, `snapshot`, `set_filters`, `changes` give
  sorted plain `Entry` snapshots. `volume(id)` / `mounted(id)` resolve operation
  objects. `inventory()` is unfiltered and must feed automount decisions:
  hiding unmounted Places rows must not suppress automount.
  Mount/unmount/eject/eject_volume are async and take the caller's MountOperation,
  never FORCE. The original GIO error is preserved, including Busy.
  `locations_inside(root, locations)` returns affected tab indices.
- `volumes::autorun::automount_plan(entries, config, startup)`,
  `content_types(mount)`, `applications(type)`, `choice`, `remember`,
  `prompt_enabled`. Compare inventories for newly added volumes and mount
  eligible ones explicitly; internal disks are excluded. Only execute autorun
  after UI policy/confirmation. The `*` choice codec is preserved, but Unix
  software media always loses remembered trust.
- `thumbs::Cache::new(cache_home)`, `Spec::from(&FileMeta)`,
  `Pool::new(cache)` and `request(spec, logical_pixels, scale_f64, &config)`.
  A Request exposes `result` and `cancel`; retain until delivery and cancel on
  unbind/folder change. Request only when `show_thumbnail` is enabled.
  Normal/large/x-large/xx-large size chooses the smallest class at least
  pixel size × fractional scale. Workers use GIO streams and built-in pixbuf
  loaders, apply orientation, write atomic mode-0600 PNGs with URI/MTime/Size/
  Mimetype chunks, and retain a 500-entry decoded LRU. Failed generation is
  cached by URI/mtime; remote, oversize and own-cache skip rules honor config.
  The installed PNG loader reads the metadata, so no custom chunk parser exists.
  `Decoded` is owned RGB/RGBA bytes plus dimensions and rowstride. On the UI
  create `glib::Bytes` and a `gdk::MemoryTexture` with R8g8b8 or R8g8b8a8 format,
  correct stride and unpremultiplied alpha. Retain a UI texture LRU as needed;
  no GTK object crosses the worker boundary.
- `import::read(pcmanfm_path, libfm_path)` returns Config, imported/ignored
  `Key` lists and warnings. libfm is applied first; PCManFM profile overrides.
  Originals are read-only and never recovered/renamed. Report quick_exec=1
  explicitly, then let the CLI select and save its isolated destination profile.
- `filemanager1::register(&connection).await` acquires the well-known name
  with DO_NOT_QUEUE, without replacement. `AlreadyOwned` is a normal result;
  continue as a directory handler without that service. Retain `Service` and
  consume ShowFolders, ShowItems and ShowItemProperties `Event`s with locations
  and startup ID. Only default-profile primary instances should register.
  Dropping it unregisters and sends ReleaseName without blocking. ShowItems
  containing-folder grouping/selection and activation belong to the UI.

## Goal B task map

Every Goal B task has an entry point or is explicitly application-owned.

| Tasks | Start here |
|---|---|
| 5.1 | This handoff and SCOPE; any backend patch needs a regression test and HANDOFF-B entry. |
| 5.2–5.6 | Application-owned isolation/actions/menus/shell; `invocation::parse_os` + `application_id`, terminal supervisor, guard environment above. |
| 5.7 | Config + settings_store; implement live Settings notifications/debounce in GTK. |
| 5.8 | Application first-frame calibration; `trace::elapsed`, SCOPE §11. |
| 5.9–5.12 | FileMeta → FileItem; FolderEvent batch model; `meta::compare`; core format/glob/enable. |
| 5.13–5.16 | Application-owned smoke, gesture spike, rendered benchmark and QA; fsls provides the headless baseline. |
| 6.1–6.6 | Config toolbar; complete; Location breadcrumbs; History; application tabs and focus/shortcut policy. |
| 6.7–6.11 | Config view/columns/sort; meta::compare; glob/helpers; DirectoryStore; GTK interactions. |
| 6.12–6.14 | open::classify; apps grouping/launch contexts/Open With; GIO target and mount metadata. |
| 6.15–6.17 | terminal; invocation; application lifetime and owner QA. |
| 7.1 | Review jobs and tests/jobs.rs, tests/data_loss.rs, tests/trash.rs; scope's permitted backend-patch rule. |
| 7.2–7.8 | JobHandle/events/replies + naming/enable; worker-dispatched create/rename helpers. |
| 7.9–7.11 | transfer payloads/drop policy; GTK clipboard and gestures feed guarded jobs. |
| 7.12–7.13 | jobbench baseline; application progress/throughput and owner QA. |
| 8.1–8.2 | enable/config/format; trash::is_empty; application menu and fixed-row models. |
| 8.3–8.6 | volume snapshots/operations; bookmarks Store; complete::Children; pane Config. |
| 8.7 | Restore/EmptyTrash jobs, raw trash metadata, enable's trash restrictions. |
| 8.8–8.9 | autorun/automount inventory; locations_inside; Server::location; GIO URI enumeration. |
| 8.10–8.11 | Count/SetAttrs jobs; FileMeta; apps defaults; naming/perms controls. |
| 8.12 | Owner device/remote QA, allowed scratch roots, isolated trash. |
| 9.1 | thumbs Pool/Request/Decoded → UI texture, bind/unbind cancellation. |
| 9.2–9.4 | Application pane/window/lifetime/counter work; every file action resolves the active pane before submitting a job. |
| 9.5–9.6 | Config codecs/live Settings; import report and profile save. |
| 9.7 | FileManager1 Service/events; selection after containing folder has loaded. |
| 9.8–9.11 | Application-owned data/install/README/X11; data terminal format above, exact deviations in SCOPE §10. |
| 9.12–9.13 | HANDOFF-B, validation and owner QA; list any backend patches and outstanding manual checks. |

## Departures from PROMPT §7

1. `Location` currently has only Uri. Search/AppMenu variants and producers
   are omitted under SCOPE's deferred v1 features; their schemes return typed
   not-in-this-version errors. `admin://` is always rejected.
2. GLib filename keys remain opaque inside FileMeta because their safe API has
   no byte accessor. Core accepts a name-comparator closure; fs supplies the
   cached locale keys. FileMeta's file type is the widget-free GIO enum.
3. Job Conflict fields are boxed to keep events small. Answers carry explicit
   apply_to_all fields; Reply has a consuming `answer` method. MountPoint /
   EnterMount implement S6. JobSummary adds allocated bytes, skips and warnings;
   JobOptions and policy-injection entry points support preferences/isolation.
4. Create/rename are synchronous guarded worker helpers rather than extra
   JobKind variants. Their UI dispatch must follow the worker rule above.
5. Thumbnail workers return decoded pixels rather than textures, as required
   by S8. Fractional scales are accepted directly; GTK constructs textures.
6. Settings notification/debounce remains in the application; Config is flat,
   typed, and storage preserves unknown keys. Per-folder persistence is a
   separate DirectoryStore. No new notification framework was added to core.
7. Pure date formatting uses UTC; local-time formatting is available to the UI
   through GLib. Runtime module entry points are qualified rather than root
   reexports. `parse_os` adds a lossless native argv boundary to the text parser.

## Evidence and remaining validation

Headless checks cover settings/unknown keys/corrupt recovery, raw metadata,
5,000-item async load/cancellation, monitor coalescing, completion, bookmark
interoperability, isolated application grouping, argv safety, all guarded job
entry points, conflict choices, pause/retry/cancellation, data-loss fixtures,
real tmpfs→btrfs move and failure preservation, private-GVFS trash/restore/empty,
volume filters/autorun/affected tabs/Busy preservation, installed PNG metadata,
500 image requests, pre-start cancellation and orientation, actual-owner config
copies, and private-bus gdbus calls for all FileManager1 methods and ownership.

Release measurements: first 50,000-item batch 17.3 ms; load 625.2 ms; sort
28.3 ms; total 653.4 ms. Cross-filesystem 1 GiB + 128 small files: median job
145.768 ms / cp 144.983 ms = 1.005, within the 10% gate. Same-btrfs copy
380.715 ms, shared extents confirmed. These are backend warm-cache figures;
rendering, main-loop stalls and memory budgets still need task 5.15 measurement.

No physical device was mounted, unmounted or ejected during Goal A. Busy error
preservation and mount boundaries are tested; actual busy-device behavior,
credentials/prompts, USB automount and remote backend capabilities belong to
8.12. SMB/MTP need the scoped GVFS packages; SFTP/FTP/WebDAV manual gaps must
remain explicit in QA. No screenshots or owner QA were required for Goal A.
The UI still needs monitor-to-selection integration, launch focus, thumbnail
textures, import CLI wiring, D-Bus selection, all dialogs and installation.
There are no known headless acceptance failures at this handoff.
