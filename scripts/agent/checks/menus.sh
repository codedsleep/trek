# Context menus and the trash can (SCOPE 8.1, 8.7). Run with:
#   scripts/agent-run.sh -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/menus.sh
M=$(mktemp -d "$HOME/.cache/tallboy-agent/menus-XXXXXX")
mkdir "$M/folder"; printf 'one\n' >"$M/a.txt"; printf 'two\n' >"$M/b.txt"; printf 'in\n' >"$M/folder/inner.txt"
col() { echo $((222 + 141 * $1)); }
gio trash --empty 2>/dev/null || true      # the agent profile's own trash
act go-to "'$M'"; settle 0.8
# Order: folder a.txt b.txt

echo "--- right-click rules"
vin move "$(col 1)" 145 click; vin mods ctrl move "$(col 2)" 145 click mods none; settle 0.3
vin move "$(col 1)" 145 click right; settle 0.5; shot menu-files
check "right-click on a selected item keeps the selection" has "$(state)" 'selected=2'
vin key Escape; settle 0.3
vin move "$(col 0)" 145 click right; settle 0.5; shot menu-folder-item
check "right-click on an unselected item selects only it" has "$(state)" 'selected=1 names=\["folder"\]'
vin key Escape; settle 0.3
vin move 700 500 click right; settle 0.5; shot menu-background
check "right-click on empty space clears the selection" has "$(state)" 'selected=0'
vin key Escape; settle 0.3

echo "--- the Menu key"
vin move "$(col 1)" 145 click; settle 0.2; vin key Menu; settle 0.5; shot menu-key
vin key Escape; settle 0.3

echo "--- trash, restore, empty"
vin move "$(col 1)" 145 click; vin key Delete; settle 0.5; vin key Return; settle 1
vin move "$(col 0)" 145 click; vin key Delete; settle 0.5; vin key Return; settle 1
check "a file and a folder went to the trash" test ! -e "$M/a.txt" -a ! -e "$M/folder"
act go-trash; settle 1.2; shot trash
check "the trash can lists them" has "$(state)" 'visible=2'
check "Create New is disabled in the trash" bash -c "! (gdbus call --session --dest io.github.codedsleep.Tallboy.p_agent --object-path /io/github/codedsleep/Tallboy/p_agent/window/1 --method org.gtk.Actions.Describe create-folder | grep -q '^((true')"
check "Move to Trash is not an action in the trash" bash -c "gdbus call --session --dest io.github.codedsleep.Tallboy.p_agent --object-path /io/github/codedsleep/Tallboy/p_agent/window/1 --method org.gtk.Actions.Describe trash 2>&1 | grep -qi 'error\|No such'"
vin move "$(col 0)" 145 click right; settle 0.5; shot menu-trash; vin key Escape; settle 0.3
act select-all; act restore; settle 1.5
check "Restore puts both back where they were, with contents" test -f "$M/a.txt" -a -f "$M/folder/inner.txt"
check "and the trash is empty again" has "$(state)" 'visible=0'
act go-to "'$M'"; settle 0.8
vin move "$(col 0)" 145 click; vin key Delete; settle 0.5; vin key Return; settle 1
act empty-trash; settle 0.5; shot confirm-empty; vin key Return; settle 1.5
check "Empty Trash Can removes a trashed folder with contents" test -z "$(ls -A "$XDG_DATA_HOME/Trash/files" 2>/dev/null)"
