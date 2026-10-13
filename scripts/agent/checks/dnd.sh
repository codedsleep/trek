# Drag and drop inside one tallboy window (SCOPE 7.10, 7.11). Run with:
#   scripts/agent-run.sh -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/dnd.sh
# Positions assume the default 1280x720 output and icon view: items sit
# 141 px apart from x=222 at y=145. Drags between applications cannot be
# checked here; they are owner QA.
D=$(mktemp -d "$HOME/.cache/tallboy-agent/dnd-XXXXXX")
mkdir "$D/dest1" "$D/dest2"
for name in a b c d e; do printf '%s\n' "$name" >"$D/$name.txt"; done
col() { echo $((222 + 141 * $1)); }
# drag FROM_COLUMN TO_COLUMN [MODIFIERS]: press, hold past GTK's 100 ms, move, drop.
drag() {
    if [[ -n ${3:-} ]]; then
        vin move "$(col "$1")" 145 down sleep 250 mods "$3" drag "$(col "$2")" 145 sleep 400 up mods none
    else
        vin move "$(col "$1")" 145 down sleep 250 drag "$(col "$2")" 145 sleep 400 up
    fi
    settle 1
}
act go-to "'$D'"; settle 0.8
# Order: dest1 dest2 a.txt b.txt c.txt d.txt e.txt

echo "--- drop on a folder item"
drag 2 0
check "a plain drag onto a folder on the same filesystem moves" test -f "$D/dest1/a.txt" -a ! -e "$D/a.txt"
drag 2 0 ctrl
check "Ctrl copies" test -f "$D/dest1/b.txt" -a -f "$D/b.txt"
drag 3 0 ctrl+shift
check "Ctrl+Shift links" test -L "$D/dest1/c.txt" -a -f "$D/c.txt"
drag 3 1 shift
check "Shift moves" test -f "$D/dest2/c.txt" -a ! -e "$D/c.txt"

echo "--- the whole selection travels"
# Now: dest1 dest2 b.txt d.txt e.txt
vin move "$(col 2)" 145 click mods ctrl move "$(col 3)" 145 click mods none; settle 0.3
check "two files are selected" has "$(state)" 'selected=2'
drag 2 1
check "dragging one of them moves both" test -f "$D/dest2/b.txt" -a -f "$D/dest2/d.txt" -a ! -e "$D/b.txt" -a ! -e "$D/d.txt"

echo "--- drops that must do nothing"
# Now: dest1 dest2 e.txt
vin move "$(col 2)" 145 down sleep 250 drag 700 500 sleep 400 up; settle 1
check "a move onto the folder the file is already in does nothing" test -f "$D/e.txt"
check "and starts no job" has "$(state)" 'dialogs=\[\]'
vin move "$(col 0)" 145 click; vin key ctrl+c; settle 0.3
vin move "$(col 0)" 145 dblclick; settle 0.8; vin key ctrl+v; settle 0.8
check "a folder pasted into itself is refused" has "$(state)" 'dialogs=\["' 
vin key Return; settle 0.4
check "and nothing was copied" test ! -e "$D/dest1/dest1"
act up; settle 0.6

echo "--- drop on a tab label"
act new-tab; settle 0.5; act go-to "'$D/dest2'"; settle 0.6
vin key alt+1; settle 0.5
check "the first tab is showing" has "$(state)" "location=file://$D "
shot tabs-before
# The second tab's label is the right half of the tab bar. One vinput call
# for the whole drag: when vinput exits, its pointer and the held button go.
vin move "$(col 2)" 185 down sleep 250 drag 960 108 sleep 1000 drag 966 110 sleep 300 up; settle 1
check "a drag resting on a tab label switches to that tab" has "$(state)" "location=file://$D/dest2 "
check "dropping on the label moves the file into that tab's folder" test -f "$D/dest2/e.txt" -a ! -e "$D/e.txt"

# A symlink's emblem must also measure without warnings in every factory.
act go-to "'$D/dest1'"; settle 0.5
for mode in list compact thumbnail icon; do
    act view-mode "'$mode'"; settle 0.4; shot "symlink-$mode"
    check "symlink emblem renders in $mode view" has "$(state)" "mode=$mode"
done
