# Phase 10 — review and hardening handoff

2026-10-13, Codex. SCOPE 10.1–10.9 complete. The locked build is ready for owner review; 10.10 (guard removal and `v1.0.0`) awaits owner sign-off. No release tag, real installation, default-handler change or hardware mount was performed.

## Review result

[CODE-REVIEW.md](CODE-REVIEW.md) records thirteen findings: two High, eight Medium, three Low. Ten are fixed with regression coverage; three behavior decisions remain below. All ten HANDOFF-B backend patches were kept with reasons, with stronger bounded-cache, destination-publication and sparse-metadata handling. GTK ownership, scheduling, unbind cleanup and every UI-to-job launch were reviewed. All 26 DISCOVERY §7 security items have evidence or an explicit limitation. README and SCOPE §10 contain the same 27 deviations.

## Fixes

Each change is in an atomic `phase 10:` commit; fixes did not remove the guard.

| Fix | Regression evidence |
|---|---|
| A file overwrite cannot delete a non-empty folder created while the question is pending; publication revalidates the actual backup and rolls back | `overwrite_cannot_delete_a_folder_created_after_the_conflict_question` failed before the fix and passes now |
| Execution trust queries the activated target filesystem, including symlinks and CLI items; unknown trust is refused | `execution_trust_queries_the_target_filesystem_and_refuses_unknown_trust`; core execution decision table |
| Password-bearing location userinfo is rejected before it can enter logs/history/bookmarks | `embedded_passwords_never_enter_stored_or_logged_locations` covers raw/escaped passwords and username-only IPv6 |
| Shared strings/icons are bounded at 1,024/256; decoded thumbnails at 500 and 96 MiB | Three cache eviction/lifetime regressions and the 500-image generation/reuse test |
| Properties completes successful attributes before rename | Extended Properties UI check reproduced old-path failure before the fix; 13 assertions now pass |
| Asynchronous tree probes cannot alter recycled cells; unbind clears the row reference | Actual GTK expander regression in isolated cage, plus sidepane check |
| Conflict suggestions tolerate missing edit-name | Sparse FileInfo fallback test under fatal criticals; conflict-answer integration tests |
| FsInfo obtains filesystem identity through a file query | `folder_capacity_includes_the_file_filesystem_identity` |
| UI runner rejects failed sessions and toolkit warnings, retains both FileManager1 session logs | Fake-runner harness in check.sh and an actual headless executable returning 7 after PASS |
| Symlink emblem CSS stays within nonnegative minimum sizes | All four view modes exercised by drag check; screenshot inspected; diagnostic gate passes |

A new `session.sh` check opens all seven menubar menus, Create and View submenus, switches all four factories, opens six dialogs, closes the last daemon window, and verifies zero live tabs/folders and no toolkit diagnostics.

## Behavior decisions for the owner

Left unfixed under SCOPE 10.5. Accept them for v1, or request the corresponding design change before release.

1. **Preparation and child-error recovery (deviation 16).** A preparation error aborts the whole job before its execution Error question. During execution, a child error reaches the top-level source: Skip abandons its remaining subtree and Retry repeats completed entries/conflicts. Decide whether preparation should continue best-effort and execution should ask per entry. Changing only one side leaves recovery inconsistent.
2. **Recursive permission repair (deviation 26).** Preparation must enter the tree before attributes are applied, and children-first chmod cannot repair an inaccessible subtree by granting access to its parent. A separate repair ordering/strategy needs a decision; globally reversing chmod breaks permission removal.
3. **Partial listing error presentation (deviation 27).** An error after the first initial batch leaves the partial model in Loading. Choose retained results plus an error banner, or the failure placeholder with Retry. The initial error must then be made terminal without treating later monitor errors as load failures.

File references and recommendations are in CODE-REVIEW. Existing GIO path-swap races and crash-left hidden staging files remain documented in README.

## Verification

| Check | Result and scope |
|---|---|
| `scripts/check.sh` | Green after all code changes: fmt, clippy all targets, workspace tests with fatal GLib criticals, layering, runner regression, GTK smoke and explicit recycled-tree test |
| `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps` | Green |
| `TALLBOY_TEST_GVFS=1 G_DEBUG=fatal-criticals cargo test -p tallboy-fs --test trash` | Three tests pass, actual private-bus GVFS trash/restore/empty exercised |
| Original full 14-script UI suite | 200 functional assertions passed. Strengthened diagnostic gate correctly rejected a negative-size GTK warning in drag; fixed afterward |
| Final changed-path UI run | Properties 13, drag 17, sidepane 15, menu/session 11: 56 assertions pass, no toolkit warnings, zero live counters. Other unchanged scripts passed the full run. The current default suite contains 216 assertions; a single final 216-assertion run is not claimed |
| Release benchmarks | SCOPE §11 records calibrated startup, shown-window large listing/sort, memory, copy and thumbnails with misses/limits |
| Screenshots inspected | Menu/submenus, compact symlink emblem, freshly generated thumbnails at scale 1.5 |
| README/SCOPE deviation comparison | Identical 27 numbered descriptions, in order |

Local logs are under `target/phase10-*.log`, `target/agent/checks/<check>-<session>/`, `target/agent/review-*` and `target/agent/tree-probe-test/`; build artifacts are not committed. Saved scripts and regressions reproduce the checks.

## Performance limits

Home first frame is 184.905 ms against a calibrated 263.100 ms limit. A shown 50,000-file view paints its first batch in 69.813–98.590 ms, loads/sorts in 548.313–591.682 ms, and stalls at most 42.3 ms. Re-sort is 42.401–44.849 ms. Memory growth is 10.3 MiB/10k (13.5 MiB/10k including initial view cost). Cross-filesystem copy is 0.991× `cp -a`; native btrfs copies share extents as D7 requires.

- `/usr/bin` still misses 150 ms: 286.740 ms shown-window load/sort, 228.097 ms backend load. Executable content sniffing dominates; MIME-policy changes were not introduced during hardening.
- RSS after closing the loaded view misses the within-a-few-MB requirement: about 59 MiB stays above baseline, including after the last daemon window closes. A second large visit reuses that memory and live counters are zero. Allocator high-water retention fits these observations but is not proven; unbounded caches were corrected, and unsafe allocator trimming was not added.
- Supplementary warm native throughput is 32.029 ms versus cp 4.793 ms (6.683×). When cloning costs almost nothing, preparation, guard checks, staging and metadata dominate. D7 makes extent cloning the native acceptance gate; the original PROMPT throughput comparison is still recorded transparently.
- Fresh thumbnails generated 417 visible-cell cache files, with a 57.360 ms first view and a 70.306 ms second visit; backend tests generate/reuse all 500. Loading stall measurements do not cover every later decode callback. Live GPU scrolling/appearance remains owner QA.

## Owner sign-off

These boxes belong to the owner, not an agent.

- [ ] Read the ten fixes and three remaining behavior decisions; accept the remaining v1 behavior or request changes.
- [ ] Accept the recorded performance misses and measurement limits, or request further work.
- [ ] Complete or explicitly waive the outstanding owner checkpoints 1–5 in QA.md: live appearance/focus, cross-application clipboard/drag, hardware/remote mounts and credentials, thumbnails, importer and installation. Agent tests do not substitute for those checks.
- [ ] Approve SCOPE 10.10: delete `fs::guard` and its call sites/override, update documentation, rerun checks, then create and push `v1.0.0`.

After approval, guard deletion/release must be a separate atomic commit and verified before the tag. Installation into the owner's `~/.local` and changing the folder handler remain the owner's actions unless separately requested.
