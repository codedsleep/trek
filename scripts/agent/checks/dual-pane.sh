# Dual-pane mode (SCOPE 9.2). Run with:
#   scripts/agent-run.sh -- ~/.cache/trek-fixture/names < scripts/agent/checks/dual-pane.sh
P=$(mktemp -d "$HOME/.cache/trek-agent/dual-XXXXXX")
mkdir -p "$P/left" "$P/right"; printf 'one\n' >"$P/left/one.txt"; printf 'two\n' >"$P/left/two.txt"
loc() { state | grep -o 'location=[^ ]*'; }
act go-to "'$P/left'"; settle 0.8
check "one pane to begin with" has "$(state)" 'panes=1 active_pane=0'

echo "--- F3 opens a second pane on the same folder"
vin key F3; settle 1; shot dual-on
check "two panes, the first still active" has "$(state)" 'panes=2 active_pane=0'
check "three tabs do not appear: one per pane" has "$(state)" 'tabs=2 '

echo "--- clicking in the second pane makes it active"
vin move 1000 500 click; settle 0.4
check "the second pane is active" has "$(state)" 'active_pane=1'
act go-to "'$P/right'"; settle 0.8
check "Go acts on the active pane" has "$(loc)" "location=file://$P/right"
vin move 500 500 click; settle 0.4
check "clicking back activates the first, still on left" has "$(state)" "active_pane=0"
check "and its folder was not changed by the other pane" has "$(loc)" "location=file://$P/left"

echo "--- copy in one pane, F6, paste in the other"
vin key ctrl+a; vin type "y"; settle 0.3
vin key F6; settle 0.4
check "F6 moves to the second pane" has "$(state)" 'active_pane=1'
vin type "p"; settle 1.2; shot dual-copied
check "paste lands in the second pane's folder" test -f "$P/right/one.txt" -a -f "$P/right/two.txt"
check "and the originals stay" test -f "$P/left/one.txt"
vin key F6; settle 0.4
check "F6 again reaches the side pane's list" has "$(state)" 'focus=GtkListBoxRow'
vin key F6; settle 0.4
check "and once more the first pane" has "$(state)" 'active_pane=0'

echo "--- tabs belong to their pane"
vin key ctrl+t; settle 0.6
check "Ctrl+T adds a tab to the active pane" has "$(state)" 'tabs=3 '
vin key ctrl+w; settle 0.5
check "Ctrl+W closes it" has "$(state)" 'tabs=2 '

echo "--- F3 closes the second pane"
vin key F3; settle 0.8; shot dual-off
check "back to one pane and one tab" has "$(state)" 'panes=1 active_pane=0 tabs=1 '
check "live tabs match" has "$(state)" 'live_tabs=1'
vin key F3; settle 0.8
vin move 1000 500 click; settle 0.3; vin key ctrl+w; settle 0.6
check "closing the last tab of the second pane leaves dual-pane mode" has "$(state)" 'panes=1 active_pane=0 tabs=1 '
vin key F3; settle 0.8
vin move 500 500 click; settle 0.3; vin key ctrl+w; settle 0.6
check "closing the last tab of the first pane keeps the other" has "$(state)" 'panes=1 active_pane=0 tabs=1 live_tabs=1'
check "nothing was logged as a warning" test -z "$(grep -E 'WARNING|CRITICAL' "$OUT/trek.log")"

