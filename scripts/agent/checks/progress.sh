# The progress window on a long copy (SCOPE 7.2). Run with:
#   scripts/agent-run.sh --release -- ~/.cache/trek-fixture < scripts/agent/checks/progress.sh
# Copies fixture/big (50,000 files) into a scratch folder in the agent tree.
F=$HOME/.cache/trek-fixture
P=$(mktemp -d "$HOME/.cache/trek-agent/progress-XXXXXX")
count() { find "$P/big" -type f 2>/dev/null | wc -l; }
pick() { vin key Escape; settle 0.2; vin type "/"; vin type "$1"; settle 0.3; vin key Return; settle 0.3; }
settle 0.6
pick big; vin type "y"; settle 0.3
act go-to "'$P'"; settle 0.6; vin type "p"; settle 1.5
shot progress
check "the progress window is up after half a second" has "$(state)" 'dialogs=\[.*"Copying files"'
started=$(date +%s%N); state >/dev/null; elapsed=$(( ($(date +%s%N) - started) / 1000000 ))
echo "    the window answered in ${elapsed} ms while copying"
check "the main window stays responsive during the copy" test "$elapsed" -lt 300
vin key alt+p; settle 0.8
a=$(count); settle 1; b=$(count)
check "Pause stops the copy ($a files, then $b)" test "$a" = "$b" -a "$a" -gt 0
shot paused
vin key alt+r; settle 1
c=$(count)
check "Resume continues it ($b, then $c)" test "$c" -gt "$b"
vin key alt+c; settle 1.5
d=$(count); settle 1; e=$(count)
check "Cancel stops it for good ($d, then $e, of 50000)" test "$d" = "$e" -a "$d" -lt 50000
check "and closes the window" has "$(state)" 'dialogs=\[\]'
check "no partial file is left behind" test -z "$(find "$P" -name '.trek-copy-*' | head -1)"
check "the source is untouched" test "$(find "$F/big" -type f | wc -l)" = 50000
