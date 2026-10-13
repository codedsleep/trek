# drop_default_action=ask (SCOPE 7.11). The setting has no control until the
# Preferences dialog exists, so seed the profile and keep it:
#   C=~/.cache/tallboy-agent/config/tallboy/agent; mkdir -p $C
#   printf '[config]\ndrop_default_action=ask\n' >$C/tallboy.conf
#   scripts/agent-run.sh --keep -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/dnd-ask.sh
D=$(mktemp -d "$HOME/.cache/tallboy-agent/dnd-XXXXXX")
mkdir "$D/dest1"; echo a >"$D/a.txt"; echo b >"$D/b.txt"
act go-to "'$D'"; settle 0.8
# Order: dest1 a.txt b.txt
vin move 363 145 down sleep 250 drag 222 145 sleep 400 up; settle 0.8; shot ask
check "the drop asks instead of acting" test -f "$D/a.txt" -a ! -e "$D/dest1/a.txt"
vin move 222 226 click; settle 1          # Move Here
check "Move Here moves" test -f "$D/dest1/a.txt" -a ! -e "$D/a.txt"
vin move 363 145 down sleep 250 mods ctrl drag 222 145 sleep 400 up mods none; settle 1
check "a modifier decides without asking" test -f "$D/dest1/b.txt" -a -f "$D/b.txt"
