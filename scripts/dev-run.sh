#!/usr/bin/env bash
# Runs tallboy on the live display for owner QA, away from the owner's real
# configuration: private D-Bus session, XDG directories under
# ~/.cache/vim-centric-fm-dev and the profile "dev" (DISCOVERY D14; SCOPE S5).
#
#   scripts/dev-run.sh [--debug] [--reset] [tallboy arguments]
#
# With no arguments it opens the fixture, ~/.cache/tallboy-fixture.
#
# The build is locked: copy, move, delete, trash, rename and create only work
# inside the fixture and the dev directories. To test a USB stick or one
# scratch folder, name it first:
#
#   TALLBOY_ALLOW_ROOTS=/run/media/$USER/STICK scripts/dev-run.sh
#
# Because the session is isolated, bookmarks, trash, thumbnails and default
# applications seen here are the dev profile's, not the real ones. What
# tallboy starts (the terminal, the editor, applications) gets the owner's
# own XDG directories and session bus back through TALLBOY_HOST_*, so nvim
# and kitty come up with their real configuration.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dev_home="$HOME/.cache/vim-centric-fm-dev"
fixture="$HOME/.cache/tallboy-fixture"
build=release
while [[ $# -gt 0 ]]; do
    case $1 in
        --debug) build=debug; shift ;;
        --reset) rm -rf "$dev_home"; shift ;;
        *) break ;;
    esac
done
command -v dbus-run-session >/dev/null || { echo "dev-run.sh: dbus-run-session is not installed (sudo dnf install dbus-daemon)" >&2; exit 2; }
if [[ $build == release ]]; then cargo build --release -p tallboy; else cargo build -p tallboy; fi
"$root/scripts/mkfixture.sh" >/dev/null
mkdir -p "$dev_home"/{config,cache,data,state}
unset TALLBOY_UNLOCKED
for name in XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME XDG_STATE_HOME DBUS_SESSION_BUS_ADDRESS; do
    export "TALLBOY_HOST_$name=${!name-}"
done
export XDG_CONFIG_HOME="$dev_home/config" XDG_CACHE_HOME="$dev_home/cache"
export XDG_DATA_HOME="$dev_home/data" XDG_STATE_HOME="$dev_home/state"
export TALLBOY_ALLOW_ROOTS="$fixture;$dev_home${TALLBOY_ALLOW_ROOTS:+;$TALLBOY_ALLOW_ROOTS}"
[[ $# -gt 0 ]] || set -- "$fixture"
exec dbus-run-session -- "$root/target/$build/tallboy" --profile dev "$@"
