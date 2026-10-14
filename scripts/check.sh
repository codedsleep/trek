#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z ${RUSTFMT:-} && -x /usr/bin/rustfmt ]]; then export RUSTFMT=/usr/bin/rustfmt; fi
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
G_DEBUG=fatal-criticals cargo test --workspace
core_tree=$(cargo tree -p trek-core)
fs_tree=$(cargo tree -p trek-fs)
if rg -q '\b(glib|gio|gtk4|gdk4) v' <<<"$core_tree"; then
    echo 'ERROR: core depends on the GObject stack' >&2; exit 1
fi
if rg -q '\b(gtk4|gdk4) v' <<<"$fs_tree"; then
    echo 'ERROR: fs depends on GTK/GDK' >&2; exit 1
fi
scripts/agent/test-harness.sh
# GTK smoke: load a fixture folder in an isolated headless session and exit.
# A GTK critical aborts the application and fails the run.
if command -v cage >/dev/null && command -v dbus-run-session >/dev/null; then
    scripts/mkfixture.sh >/dev/null
    scripts/agent-run.sh --smoke --timeout 60 -- "$HOME/.cache/trek-fixture/names" </dev/null
    scripts/agent-run.sh --out target/agent/tree-probe-test --timeout 60 \
        --env G_DEBUG=fatal-criticals \
        --app 'cargo test -p trek stale_directory_probe -- --ignored --test-threads=1' </dev/null
    echo 'GTK smoke: passed'
else
    echo 'GTK smoke: SKIPPED (cage and dbus-run-session are needed for the headless run)'
fi
