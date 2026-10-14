#!/usr/bin/env bash
# Runs trek on the live display for owner QA, away from the owner's real
# configuration: private D-Bus session, XDG directories under
# ~/.cache/trek-dev and the profile "dev" (DISCOVERY D14; SCOPE S5).
#
#   scripts/dev-run.sh [--debug] [--reset] [trek arguments]
#
# With no arguments it opens the fixture, ~/.cache/trek-fixture.
#
# The build is locked: copy, move, delete, trash, rename and create only work
# inside the fixture and the dev directories. To test a USB stick or one
# scratch folder, name it first:
#
#   TREK_ALLOW_ROOTS=/run/media/$USER/STICK scripts/dev-run.sh
#
# Because the session is isolated, bookmarks, trash, thumbnails and default
# applications seen here are the dev profile's, not the real ones. What
# trek starts (the terminal, the editor, applications) gets the owner's
# own XDG directories and session bus back through TREK_HOST_*, so nvim
# and kitty come up with their real configuration. The desktop's GTK theme
# is one more thing the private session cannot ask for; its name goes in as
# TREK_GTK_THEME (set it yourself to try another theme).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dev_home="$HOME/.cache/trek-dev"
fixture="$HOME/.cache/trek-fixture"
build=release
while [[ $# -gt 0 ]]; do
    case $1 in
        --debug) build=debug; shift ;;
        --reset) rm -rf "$dev_home"; shift ;;
        *) break ;;
    esac
done
command -v dbus-run-session >/dev/null || { echo "dev-run.sh: dbus-run-session is not installed (sudo dnf install dbus-daemon)" >&2; exit 2; }
if [[ $build == release ]]; then cargo build --release -p trek; else cargo build -p trek; fi
"$root/scripts/mkfixture.sh" >/dev/null
mkdir -p "$dev_home"/{config,cache,data,state}
unset TREK_UNLOCKED
if [[ -z ${TREK_GTK_THEME-} ]] && command -v gsettings >/dev/null; then
    theme=$(gsettings get org.gnome.desktop.interface gtk-theme 2>/dev/null) || theme=
    theme=${theme#\'} theme=${theme%\'}
    [[ -z $theme ]] || export TREK_GTK_THEME="$theme"
fi
for name in XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME XDG_STATE_HOME DBUS_SESSION_BUS_ADDRESS; do
    export "TREK_HOST_$name=${!name-}"
done
export XDG_CONFIG_HOME="$dev_home/config" XDG_CACHE_HOME="$dev_home/cache"
export XDG_DATA_HOME="$dev_home/data" XDG_STATE_HOME="$dev_home/state"
export TREK_ALLOW_ROOTS="$fixture;$dev_home${TREK_ALLOW_ROOTS:+;$TREK_ALLOW_ROOTS}"
[[ $# -gt 0 ]] || set -- "$fixture"
exec dbus-run-session -- "$root/target/$build/trek" --profile dev "$@"
