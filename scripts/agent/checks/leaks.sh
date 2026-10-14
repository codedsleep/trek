# Leak counters (SCOPE 9.4). Tabs, panes, the directory tree and windows are
# opened and closed; live tab pages, watched folders and the folder listings
# tabs keep for going back to must return to zero.
# Run with:
#   scripts/agent-run.sh --env TALLBOY_TRACE=1 -- --daemon-mode ~/.cache/tallboy-fixture/deep < scripts/agent/checks/leaks.sh
live() { state | grep -o 'live_tabs=[0-9-]* live_folders=[0-9-]*'; }
settle 1
check "one tab, one folder" has "$(live)" 'live_tabs=1 live_folders=1'

echo "--- tabs"
vin key ctrl+t; vin key ctrl+t; settle 0.8
check "three tabs watch three folders" has "$(live)" 'live_tabs=3 live_folders=3'
vin key ctrl+w; vin key ctrl+w; settle 0.8
check "closing two returns to one" has "$(live)" 'live_tabs=1 live_folders=1'

echo "--- navigation replaces the folder"
act go-to "'$HOME/.cache/tallboy-fixture/names'"; settle 0.8
act go-to "'$HOME/.cache/tallboy-fixture/deep'"; settle 0.8
check "still one folder after going somewhere and back" has "$(live)" 'live_tabs=1 live_folders=1'
check "and the folder left behind is kept for going back to" has "$(state)" 'live_listings=[2-9]'

echo "--- dual pane"
vin key F3; settle 0.8
check "the second pane adds a tab and a folder" has "$(live)" 'live_tabs=2 live_folders=2'
vin key F3; settle 0.8
check "and gives them back" has "$(live)" 'live_tabs=1 live_folders=1'

echo "--- directory tree"
act sidepane-mode "'dirtree'"; settle 1.5
TREE=$(live); echo "    with the tree open: $TREE"
check "the tree watches the folders it has expanded" test "${TREE##*=}" -gt 1
act sidepane-mode "'places'"; settle 0.5
act go-to "'/usr/share'"; settle 1
act sidepane-mode "'dirtree'"; settle 1.5
act go-to "'$HOME/.cache/tallboy-fixture/deep'"; settle 1.5
echo "    after moving the tree about: $(live)"

echo "--- a second window, then both closed"
vin key ctrl+n; settle 1
WIN=2; check "the new window has its own tab" has "$(live)" 'live_tabs=2'
vin key ctrl+q; settle 0.8
WIN=1; vin key ctrl+q; settle 1.5
check "the process is still running (daemon mode)" test -n "$(pgrep -f 'tallboy --profile agent --daemon-mode')"
check "nothing is alive after the last window closed" has "$(grep 'after a window closed' "$OUT/tallboy.log" | tail -1)" 'live_tabs=0 live_folders=0'
check "nor is any listing a tab had kept" has "$(grep 'after a window closed' "$OUT/tallboy.log" | tail -1)" 'live_listings=0$'
