#!/usr/bin/env bash
# Builds trek in release mode and installs it for this user, beside
# PCManFM (DISCOVERY D21, D27).
#
#   scripts/install.sh                 binary, launcher, icon, man page
#   scripts/install.sh --set-default   also: open folders with trek, and
#                                      answer "Show in folder" (FileManager1)
#   scripts/install.sh --uninstall     remove what was installed
#   scripts/install.sh --prefix DIR    install under DIR instead of ~/.local
#
# Without --set-default nothing about the desktop's defaults changes:
# `xdg-mime query default inode/directory` answers as before.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
prefix="$HOME/.local"
set_default=0
uninstall=0
while [[ $# -gt 0 ]]; do
    case $1 in
        --set-default) set_default=1; shift ;;
        --uninstall) uninstall=1; shift ;;
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        -h | --help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "install.sh: unknown option $1" >&2; exit 2 ;;
    esac
done

id=io.github.codedsleep.Trek
bin="$prefix/bin/trek"
launcher="$prefix/share/applications/$id.desktop"
icon="$prefix/share/icons/hicolor/scalable/apps/$id.svg"
manual="$prefix/share/man/man1/trek.1"
service="$prefix/share/dbus-1/services/org.freedesktop.FileManager1.service"

refresh() {
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$prefix/share/applications" || true
    fi
}

if [[ $uninstall == 1 ]]; then
    for file in "$bin" "$launcher" "$icon" "$manual" "$service"; do
        if [[ -e $file ]]; then
            rm -f -- "$file"
            echo "removed $file"
        fi
    done
    refresh
    if command -v xdg-mime >/dev/null \
        && [[ $(xdg-mime query default inode/directory) == "$id.desktop" ]]; then
        echo "trek is still named as the handler for folders; choose another with:"
        echo "  xdg-mime default pcmanfm.desktop inode/directory"
    fi
    exit 0
fi

cargo build --manifest-path "$root/Cargo.toml" --release --locked -p trek

install -Dm755 "$root/target/release/trek" "$bin"
# The launcher names the binary by its full path, so that it works even when
# the prefix's bin directory is not on the desktop session's PATH.
install -d "$(dirname "$launcher")"
sed -e "s|^TryExec=.*|TryExec=$bin|" -e "s|^Exec=trek|Exec=$bin|" \
    "$root/data/$id.desktop" >"$launcher"
chmod 644 "$launcher"
install -Dm644 "$root/data/trek.svg" "$icon"
install -Dm644 "$root/data/trek.1" "$manual"
refresh
echo "installed $bin"
echo "installed $launcher"
echo "installed $icon"
echo "installed $manual"

if [[ $set_default == 1 ]]; then
    before=$(xdg-mime query default inode/directory || true)
    install -d "$(dirname "$service")"
    sed "s|@BINDIR@|$prefix/bin|" "$root/data/org.freedesktop.FileManager1.service.in" >"$service"
    chmod 644 "$service"
    echo "installed $service"
    xdg-mime default "$id.desktop" inode/directory
    echo "folders now open with trek (before: ${before:-nothing})"
    if [[ -n $before && $before != "$id.desktop" ]]; then
        echo "to go back:  xdg-mime default $before inode/directory"
    fi
else
    echo "defaults unchanged; run with --set-default to open folders with trek"
fi
