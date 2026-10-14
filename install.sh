#!/bin/sh
# trek installer — downloads a prebuilt trek and installs it for this user.
#
#   curl -fsSL https://raw.githubusercontent.com/codedsleep/trek/master/install.sh | sh
#
# Options (pass after `-s --` when piping: `... | sh -s -- --set-default`):
#   --prefix DIR     install under DIR instead of ~/.local
#   --version TAG    install a specific release (default: the latest)
#   --set-default    also open folders with trek and answer "Show in folder"
#   -y, --yes        never prompt; take the default answer
#   --uninstall      remove an installed trek
#   -h, --help       print this and exit
#
# Installs the binary, a launcher, the icon and the man page. The binary links
# against the system's GTK 4 (4.14 or later) and glibc (2.39 or later).

set -eu

OWNER_REPO="codedsleep/trek"
BIN_NAME="trek"
APP_ID="io.github.codedsleep.Trek"
VERSION="latest"
PREFIX="$HOME/.local"
ASSUME_YES=0
UNINSTALL=0
SET_DEFAULT=0
WORKDIR=""

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    B=$(printf '\033[1m'); DIM=$(printf '\033[2m'); RED=$(printf '\033[31m')
    YLW=$(printf '\033[33m'); GRN=$(printf '\033[32m'); R=$(printf '\033[0m')
else
    B=''; DIM=''; RED=''; YLW=''; GRN=''; R=''
fi

say()  { printf '%s==>%s %s\n' "$B" "$R" "$1"; }
warn() { printf '%swarning:%s %s\n' "$YLW" "$R" "$1" >&2; }
die()  { printf '%serror:%s %s\n' "$RED" "$R" "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

cleanup() { [ -n "$WORKDIR" ] && rm -rf "$WORKDIR"; return 0; }
trap cleanup EXIT INT TERM

usage() { sed -n '/^# trek installer/,/^$/p' "$0" | sed 's/^#\{1,\} \{0,1\}//'; }

# Ask a yes/no question on the terminal rather than stdin, so this still works
# when the script itself arrived on stdin through a pipe.
confirm() {
    [ "$ASSUME_YES" -eq 1 ] && return 0
    if [ -r /dev/tty ]; then
        printf '%s [y/N] ' "$1" > /dev/tty
        read -r reply < /dev/tty || reply=""
    else
        return 1
    fi
    case "$reply" in [Yy]|[Yy][Ee][Ss]) return 0 ;; *) return 1 ;; esac
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)  [ $# -ge 2 ] || die "--prefix needs a directory"; PREFIX="$2"; shift 2 ;;
        --prefix=*) PREFIX="${1#--prefix=}"; shift ;;
        --version) [ $# -ge 2 ] || die "--version needs a tag"; VERSION="$2"; shift 2 ;;
        --version=*) VERSION="${1#--version=}"; shift ;;
        --set-default) SET_DEFAULT=1; shift ;;
        -y|--yes)  ASSUME_YES=1; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
done

BIN="$PREFIX/bin/$BIN_NAME"
LAUNCHER="$PREFIX/share/applications/$APP_ID.desktop"
ICON="$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg"
MANUAL="$PREFIX/share/man/man1/$BIN_NAME.1"
SERVICE="$PREFIX/share/dbus-1/services/org.freedesktop.FileManager1.service"

refresh() {
    if have update-desktop-database; then
        update-desktop-database -q "$PREFIX/share/applications" || true
    fi
}

# ------------------------------------------------------------- uninstall ----

if [ "$UNINSTALL" -eq 1 ]; then
    found=0
    for f in "$BIN" "$LAUNCHER" "$ICON" "$MANUAL" "$SERVICE"; do
        if [ -e "$f" ]; then
            rm -f "$f"
            say "removed $f"
            found=1
        fi
    done
    [ "$found" -eq 1 ] || die "no $BIN_NAME found under $PREFIX."
    refresh
    if have xdg-mime && [ "$(xdg-mime query default inode/directory 2>/dev/null)" = "$APP_ID.desktop" ]; then
        warn "$BIN_NAME is still named as the handler for folders; choose another with:"
        printf '\n    xdg-mime default pcmanfm.desktop inode/directory\n'
    fi
    exit 0
fi

# ------------------------------------------------------------------ fetch ---

if have curl; then
    dl() { curl -fsSL --retry 3 -o "$2" "$1"; }
elif have wget; then
    dl() { wget -qO "$2" "$1"; }
else
    die "need curl or wget to download."
fi

# --------------------------------------------------------------- platform ---

[ "$(uname -s)" = "Linux" ] || die "trek is Linux only; this is $(uname -s)."

case "$(uname -m)" in
    x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
    *) die "no prebuilt binary for $(uname -m). Build from source:
    https://github.com/$OWNER_REPO#from-source" ;;
esac

if have ldconfig && ! ldconfig -p 2>/dev/null | grep -q 'libgtk-4\.so\.1'; then
    warn "GTK 4 was not found; trek needs it to start (Fedora: gtk4, Debian/Ubuntu: libgtk-4-1, Arch: gtk4)."
fi

ASSET="$BIN_NAME-$TARGET.tar.gz"
if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$OWNER_REPO/releases/latest/download"
else
    BASE="https://github.com/$OWNER_REPO/releases/download/$VERSION"
fi

# -------------------------------------------------------------- download ----

WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/trek-install.XXXXXX")

say "Downloading $ASSET ($VERSION)"
dl "$BASE/$ASSET" "$WORKDIR/$ASSET" || die "no release asset $ASSET.
Check https://github.com/$OWNER_REPO/releases for available versions."

# Checksums are published as one SHA256SUMS covering every asset.
if dl "$BASE/SHA256SUMS" "$WORKDIR/SHA256SUMS" 2>/dev/null; then
    if have sha256sum; then
        want=$(awk -v a="$ASSET" '$2 == a || $2 == "*"a {print $1}' "$WORKDIR/SHA256SUMS" | head -1)
        got=$(sha256sum "$WORKDIR/$ASSET" | awk '{print $1}')
        if [ -z "$want" ]; then
            warn "SHA256SUMS has no entry for $ASSET; skipping verification."
        elif [ "$want" != "$got" ]; then
            die "checksum mismatch for $ASSET.
  expected $want
  got      $got"
        else
            say "Checksum verified"
        fi
    else
        warn "sha256sum not found; skipping checksum verification."
    fi
else
    warn "no SHA256SUMS published for this release; skipping verification."
fi

tar -xzf "$WORKDIR/$ASSET" -C "$WORKDIR"
dist="$WORKDIR/$BIN_NAME-$TARGET"
[ -x "$dist/$BIN_NAME" ] || die "the archive did not contain a $BIN_NAME binary."

# --------------------------------------------------------------- install ----

if [ -e "$BIN" ]; then
    confirm "$BIN already exists. Overwrite?" || die "left the existing $BIN_NAME alone."
fi

say "Installing under $PREFIX"
install -Dm755 "$dist/$BIN_NAME" "$BIN"
# The launcher names the binary by its full path, so that it works even when
# the prefix's bin directory is not on the desktop session's PATH.
mkdir -p "$(dirname "$LAUNCHER")"
sed -e "s|^TryExec=.*|TryExec=$BIN|" -e "s|^Exec=$BIN_NAME|Exec=$BIN|" \
    "$dist/$APP_ID.desktop" > "$LAUNCHER"
chmod 644 "$LAUNCHER"
install -Dm644 "$dist/$BIN_NAME.svg" "$ICON"
install -Dm644 "$dist/$BIN_NAME.1" "$MANUAL"
refresh
for f in "$BIN" "$LAUNCHER" "$ICON" "$MANUAL"; do
    printf '    %s%s%s\n' "$DIM" "$f" "$R"
done

if [ "$SET_DEFAULT" -eq 1 ]; then
    have xdg-mime || die "--set-default needs xdg-mime (xdg-utils)."
    before=$(xdg-mime query default inode/directory 2>/dev/null || true)
    mkdir -p "$(dirname "$SERVICE")"
    sed "s|@BINDIR@|$PREFIX/bin|" "$dist/org.freedesktop.FileManager1.service.in" > "$SERVICE"
    chmod 644 "$SERVICE"
    xdg-mime default "$APP_ID.desktop" inode/directory
    say "Folders now open with $BIN_NAME (before: ${before:-nothing})"
    if [ -n "$before" ] && [ "$before" != "$APP_ID.desktop" ]; then
        printf '    to go back:  xdg-mime default %s inode/directory\n' "$before"
    fi
fi

# ------------------------------------------------------------------ PATH ----

on_path=0
case ":$PATH:" in *":$PREFIX/bin:"*) on_path=1 ;; esac

printf '\n%s%s installed to %s%s\n' "$GRN" "$BIN_NAME" "$BIN" "$R"

if [ "$on_path" -eq 0 ]; then
    printf '\n'
    warn "$PREFIX/bin is not on your PATH. The launcher works anyway; for the shell, add it:"
    case "$(basename "${SHELL:-sh}")" in
        fish) printf '\n    fish_add_path %s/bin\n' "$PREFIX" ;;
        zsh)  printf '\n    echo '\''export PATH="%s/bin:$PATH"'\'' >> ~/.zshrc\n' "$PREFIX" ;;
        *)    printf '\n    echo '\''export PATH="%s/bin:$PATH"'\'' >> ~/.profile\n' "$PREFIX" ;;
    esac
else
    printf 'Run %s%s%s, or start it from your launcher.\n' "$B" "$BIN_NAME" "$R"
fi
