# Navigation chrome and shortcut safety (SCOPE 6.1–6.6). Run with:
#   scripts/agent-run.sh -- ~/.cache/tallboy-fixture/deep < scripts/agent/checks/navigation.sh
F=$HOME/.cache/tallboy-fixture
at() { has "$(state)" "location=file://$F$1 "; }
settle 0.6
echo "--- tabs"
act new-tab; act new-tab; settle 0.4
check "New Tab twice gives three tabs" has "$(state)" 'tabs=3'
vin key alt+1; settle 0.2; act go-to "'$F/names'"; settle 0.4
vin key ctrl+Tab; settle 0.2
check "Ctrl+Tab goes to the next tab" at /deep
vin key ctrl+shift+Tab; settle 0.2
check "Ctrl+Shift+Tab goes back" at /names
vin key ctrl+w; settle 0.3
check "Ctrl+W closes a tab" has "$(state)" 'tabs=2'

echo "--- location entry and completion"
vin key ctrl+l; settle 0.3
check "Ctrl+L focuses the entry" has "$(state)" 'focus=GtkText'
vin type "$F/"; settle 0.8
check "typing a folder lists its subfolders" has "$(state)" 'completions=8 completion_open=true'
vin type "d"; settle 0.6; vin key Tab; settle 0.6
check "Tab completes the only match" has "$(state)" "entry=\"$F/deep/\""
vin type "l"; settle 0.5; vin key Down; settle 0.2; vin key Return; settle 0.6
check "Down then Enter takes a row" has "$(state)" "entry=\"$F/deep/level-00/\""
vin key Return; settle 0.6
check "Enter navigates" at /deep/level-00
check "and the view has the keyboard again" has "$(state)" 'focus=GtkListItemWidget'

echo "--- file shortcuts typed into the entry (task 6.6)"
vin key Home; settle 0.2
before=$(state | grep -o 'location=[^ ]* \|selected=[0-9]* names=[^]]*]')
vin key ctrl+l; settle 0.2; vin key BackSpace key Delete key ctrl+a key ctrl+c key ctrl+x key ctrl+v; settle 0.3
after=$(state | grep -o 'location=[^ ]* \|selected=[0-9]* names=[^]]*]')
check "BackSpace, Delete, Ctrl+A, Ctrl+C, Ctrl+X, Ctrl+V leave folder and selection alone" test "$before" = "$after"
check "the entry kept the keyboard" has "$(state)" 'focus=GtkText'
vin key Escape; settle 0.3
check "Esc restores the text and returns to the view" has "$(state)" "entry=\"$F/deep/level-00\".*page=entry"
check "Esc gives the keyboard back" has "$(state)" 'focus=GtkListItemWidget'
vin key BackSpace; settle 0.5
check "BackSpace in the view goes to the parent" at /deep

echo "--- history and breadcrumbs"
act pathbar-mode "'buttons'"; settle 0.4
act go-to "'$F/deep/level-00/level-01/level-02'"; settle 0.4; act up; settle 0.4
check "Up goes to the parent" at /deep/level-00/level-01
check "with the cursor on the folder left" has "$(state)" 'cursor="level-02"'
act back; settle 0.3
check "Back returns" at /deep/level-00/level-01/level-02
act history-go "'0'"; settle 0.4
check "the history menu jumps to its first entry" at /deep
vin key ctrl+l; settle 0.4
check "Ctrl+L borrows the breadcrumb bar for the entry" has "$(state)" 'page=entry'
vin key Escape; settle 0.4
check "Esc gives it back" has "$(state)" 'page=crumbs'
