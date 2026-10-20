# File operations through the UI (SCOPE 7.2–7.9, 7.12). Run with:
#   scripts/agent-run.sh -- ~/.cache/trek-fixture/names < scripts/agent/checks/fileops.sh
# Everything happens in ~/.cache/trek-agent/work, inside the allowed roots,
# except the confinement check, which must change nothing outside them.
W=$HOME/.cache/trek-agent/work
outside=$HOME/.cache/trek-outside-test
rm -rf "$W" "$outside"; mkdir -p "$W/sub" "$outside"
printf 'one\n' >"$W/alpha.txt"; printf 'two\n' >"$W/beta.txt"; printf 'three\n' >"$W/gamma.tar.gz"
enabled() { has "$(describe "$1")" '^\(\(true'; }
disabled() { ! enabled "$1"; }
# Puts the cursor on an item by finding part of its name.
pick() { vin key Escape; settle 0.2; vin type "/"; vin type "$1"; settle 0.3; vin key Return; settle 0.3; }
act go-to "'$W'"; settle 0.8

echo "--- create"
act create-folder; settle 0.5; shot create-dialog
vin type "made here"; vin key Return; settle 1
check "Create New Folder makes the folder" test -d "$W/made here"
check "and puts the cursor on it" has "$(state)" 'cursor="made here"'
act create-file; settle 0.5; vin key Return; settle 0.8
check "Create New Empty File with the default name" test -f "$W/New"
act create-file; settle 0.5; vin key Return; settle 0.8
check "a taken name gets a unique one, nothing is overwritten" test -f "$W/New (copy)"
act create-folder; settle 0.5; vin type "a//b"; vin key Return; settle 0.5; shot create-invalid
check "an invalid path keeps the input line open" has "$(state)" 'input="New folder:":"a//b"'
vin key Escape; settle 0.4
check "and creates nothing" test ! -e "$W/a"

echo "--- rename"
pick gamma; vin key F2; settle 0.5; shot rename-dialog
check "F2 puts the text cursor before the extension" has "$(state)" 'input="Rename:":"gamma.tar.gz"'
vin key ctrl+u; vin type "delta"; vin key Return; settle 1
check "Ctrl+U clears to the start, so typing replaces the base name and keeps the extension" test -f "$W/delta.tar.gz"
check "the cursor is on the renamed file" has "$(state)" 'cursor="delta.tar.gz"'

echo "--- copy and paste"
pick alpha.t; vin type "y"; settle 0.4
clip=$(state | grep -o 'clipboard=\[[^]]*\]')
for mime in x-special/gnome-copied-files text/uri-list text/plain; do
    check "the clipboard offers $mime" has "$clip" "$mime"
done
check "a copy does not offer the KDE cut marker" bash -c "! grep -q kde-cutselection <<<'$clip'"
check "Paste is enabled with files on the clipboard" enabled paste
vin type "p"; settle 1
check "pasting into the source folder makes a copy beside it" test -f "$W/alpha (copy).txt"
act go-to "'$W/sub'"; settle 0.6; vin type "p"; settle 1
check "Paste copies into the current folder" test "$(cat "$W/sub/alpha.txt" 2>/dev/null)" = one
check "and leaves the source" test -f "$W/alpha.txt"

echo "--- conflict dialog"
printf 'changed\n' >"$W/alpha.txt"
vin type "p"; settle 0.8; shot conflict
check "a second paste asks about the conflict" has "$(state)" 'dialogs=\[.*"Confirm to replace files"'
vin key alt+s; settle 0.8                      # Skip
check "Skip keeps the existing file" test "$(cat "$W/sub/alpha.txt")" = one
vin type "p"; settle 0.8; vin key alt+o; settle 0.8   # Overwrite
check "Overwrite replaces it" test "$(cat "$W/sub/alpha.txt")" = changed
vin type "p"; settle 0.8; vin key alt+r; settle 0.8   # Rename to the suggested name
check "Rename writes beside it under the suggested name" test -f "$W/sub/alpha (copy).txt"
vin type "p"; settle 0.8; vin key Escape; settle 0.8
check "Esc cancels the job and closes the dialog" has "$(state)" 'dialogs=\[\]'

echo "--- cut and paste"
act go-to "'$W'"; settle 0.6
pick beta; vin type "x"; settle 0.4
check "a cut offers the KDE cut marker" has "$(state)" 'application/x-kde-cutselection'
act go-to "'$W/sub'"; settle 0.6; vin type "p"; settle 1
check "Paste after Cut moves the file" test -f "$W/sub/beta.txt" -a ! -e "$W/beta.txt"
check "and empties the clipboard" bash -c '! grep -q gnome-copied-files <<<"$1"' _ "$(state | grep -o 'clipboard=\[[^]]*\]')"
check "Paste is disabled afterwards" disabled paste

echo "--- copy paths"
pick beta; act copy-path; settle 0.4
check "Copy Path(s) puts the native path on the clipboard" test "$(wl-paste -n 2>/dev/null)" = "$W/sub/beta.txt"

echo "--- trash and remove"
pick beta; vin key Delete; settle 0.6; shot confirm-trash
check "Delete asks before trashing" has "$(state)" 'confirm="Move this file to the trash can\?"'
vin key Escape; settle 0.5
check "No keeps the file" test -f "$W/sub/beta.txt"
vin key Delete; settle 0.6; vin key Return; settle 1.2
check "Yes moves it to the trash can" test ! -e "$W/sub/beta.txt" -a -n "$(ls "$XDG_DATA_HOME/Trash/files" 2>/dev/null | grep beta)"
pick "alpha ("; vin key shift+Delete; settle 0.6; shot confirm-delete
vin key Return; settle 1.2
check "Shift+Delete removes permanently after confirming" test ! -e "$W/sub/alpha (copy).txt"

echo "--- text entries are safe now that Delete and Ctrl+C do something (task 6.6)"
pick alpha; vin key ctrl+l; settle 0.3
vin key Delete key BackSpace key ctrl+c key ctrl+x; vin type "dDyxq"; settle 0.4
check "nothing typed in the location entry asked a question" has "$(state)" 'confirm=none .* tabs=1 '
vin key Escape; settle 0.4
check "or touched the file" test -f "$W/sub/alpha.txt"
check "or yanked it" has "$(state)" 'yanked=None'

echo "--- enablement"
vin key Escape; settle 0.3
check "nothing selected: Rename applies to the file under the cursor" enabled rename
mkdir -p "$W/readonly"; chmod 555 "$W/readonly"
act go-to "'$W/readonly'"; settle 0.8
check "an empty folder: Rename is disabled" disabled rename
check "an empty folder: Move to Trash is disabled" disabled trash
check "read-only folder: Create New is disabled" disabled create-folder
check "read-only folder: Paste is disabled" disabled paste
chmod 755 "$W/readonly"

echo "--- confinement refuses to write outside the allowed roots (task 7.12)"
act go-to "'$outside'"; settle 0.8
act create-folder; settle 0.5; vin key Return; settle 1; shot confined
check "nothing was created outside" test -z "$(ls -A "$outside")"
vin key Return; settle 0.4; vin key Escape; settle 0.4
rmdir "$outside"
