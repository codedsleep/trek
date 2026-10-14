# Properties and Connect to Server (SCOPE 8.9–8.11). Run with:
#   scripts/agent-run.sh -- ~/.cache/trek-fixture/names < scripts/agent/checks/properties.sh
P=$(mktemp -d "$HOME/.cache/trek-agent/props-XXXXXX")
mkdir -p "$P/folder/inner"; printf 'hello\n' >"$P/a.txt"; printf 'x\n' >"$P/b.txt"
head -c 3000 /dev/zero >"$P/folder/inner/data"; printf 'y\n' >"$P/folder/top"
chmod 644 "$P/a.txt"; chmod 600 "$P/b.txt"; chmod 755 "$P/folder" "$P/folder/inner"; chmod 644 "$P/folder/top"
col() { echo $((222 + 141 * $1)); }
mode() { stat -c %a "$1"; }
act go-to "'$P'"; settle 0.8
# Order: folder a.txt b.txt

echo "--- one file"
vin move "$(col 1)" 145 click; vin key alt+Return; settle 0.8; shot props-general
check "Alt+Return opens File Properties" has "$(state)" 'dialogs=\["File Properties"\]'
check "the name has the keyboard, base name selected" has "$(state)" 'toplevels=2'
vin type "renamed"; vin key Return; settle 1
check "changing the name renames the file on OK" test -f "$P/renamed.txt" -a ! -e "$P/a.txt"
check "and the cursor is on the renamed file" has "$(state)" 'cursor="renamed.txt"'

echo "--- a folder: live count"
vin move "$(col 0)" 145 click; act properties; settle 1.2; shot props-folder
vin key Escape; settle 0.4
check "Esc closes Properties" has "$(state)" 'dialogs=\[\]'

echo "--- permissions on a mixed selection (b.txt 600, renamed.txt 644)"
vin move "$(col 1)" 145 click; vin mods ctrl move "$(col 2)" 145 click mods none; settle 0.3
check "two files are selected" has "$(state)" 'selected=2'
act properties; settle 0.8
vin key alt+p; settle 0.4; shot props-permissions
# View content is mixed (owner only / anyone), so "(no change)" is selected.
# Open the list, jump to its first entry, "Nobody", take it, OK.
vin key alt+w; settle 0.2; vin key space; settle 0.5; vin key Home; vin key Return; settle 0.4
shot props-permissions-set
vin key alt+o; settle 1.2
check "View content = Nobody clears the read bits of both files and nothing else" test "$(mode "$P/b.txt")$(mode "$P/renamed.txt")" = 200200

echo "--- a folder asks about recursion"
vin key Escape; settle 0.2; vin move "$(col 0)" 145 click; act properties; settle 1
vin type "renamed-folder"
vin key alt+p; settle 0.4
# Change content: jump to the last entry, "Anyone".
vin key alt+h; settle 0.2; vin key space; settle 0.5; vin key End; vin key Return; settle 0.4
vin key alt+o; settle 0.8; shot props-recursive
check "changing a folder asks whether to recurse" has "$(state)" 'dialogs=\["Confirm"\]|dialogs=\[""\]'
check "the folder keeps its name until its permission changes finish" test -d "$P/folder" -a ! -e "$P/renamed-folder"
vin key Tab; vin key space; settle 1.5       # Yes
check "Yes applies to the folder and then renames it" test "$(mode "$P/renamed-folder")" = 777
check "and to everything inside it" test "$(mode "$P/renamed-folder/inner")$(mode "$P/renamed-folder/top")$(mode "$P/renamed-folder/inner/data")" = 777666666

echo "--- Connect to Server"
act connect-server; settle 0.6; shot connect
check "the dialog opens" has "$(state)" 'dialogs=\["Connect to remote server"\]'
vin key Return; settle 0.4
check "an empty host keeps it open" has "$(state)" 'dialogs=\["Connect to remote server"\]'
vin key Escape; settle 0.4
