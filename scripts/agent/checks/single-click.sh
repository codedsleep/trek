# Single-click mode (SCOPE 6.11). The setting has no control until the
# Preferences dialog exists, so seed the profile and keep it:
#   C=~/.cache/trek-agent/config/trek/agent; rm -rf $C; mkdir -p $C
#   printf '[config]\nsingle_click=1\nauto_selection_delay=300\n' >$C/trek.conf
#   scripts/agent-run.sh --keep -- ~/.cache/trek-fixture < scripts/agent/checks/single-click.sh
# vinput must stay connected while hovering: when it exits, its pointer
# leaves and the pending hover is cancelled, as it should be.
settle 0.8
vin move 222 145 sleep 700
check "hovering selects after the delay" has "$(state)" 'selected=1 names=\["big"\]'
vin move 360 145 sleep 700
check "hover moves the selection" has "$(state)" 'names=\["deep"\]'
vin move 360 145 click; settle 0.8
check "a single click opens the folder" has "$(state)" 'location=file://[^ ]*/trek-fixture/deep '
act back; settle 0.6
vin move 222 145 sleep 600 mods ctrl move 500 145 click mods none; settle 0.5
check "Ctrl+click still builds a selection" has "$(state)" 'selected=2'
