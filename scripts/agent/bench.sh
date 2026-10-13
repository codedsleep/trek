#!/usr/bin/env bash
# Measures the SCOPE §11 figures with the release build in a headless cage
# session sized like the owner's monitor (3840x2160 at scale 1.5).
#
#   scripts/agent/bench.sh            all measurements
#   scripts/agent/bench.sh frames     first-frame baseline and $HOME only
#
# Times include exec and dynamic linking (TALLBOY_T0). Each launch uses the
# isolated agent profile; folders are only read.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
what=${1:-all}
out="$root/target/agent/bench"
fixture="$HOME/.cache/tallboy-fixture"
cargo build --release -q -p tallboy
mkdir -p "$fixture/empty"
# 10,000 files beside the 50,000 of fixture/big, for the memory slope.
ten="$HOME/.cache/tallboy-agent/bench-10k"
if [[ ! -d $ten ]]; then
    mkdir -p "$ten"
    (cd "$ten" && seq -f 'file-%05g.txt' 1 10000 | xargs touch)
fi

AGENT_BENCH_WHAT=$what "$root/scripts/agent-run.sh" --release --size 3840x2160 --scale 1.5 \
    --app "sleep 900" --out "$out" --timeout 600 <<'DRIVER'
bin="$root/target/release/tallboy"
export TALLBOY_TRACE=1
fixture="$HOME/.cache/tallboy-fixture"
launch() { # launch ENV... -- ARGS...
    local envs=()
    while [[ $1 != -- ]]; do envs+=("$1"); shift; done
    shift
    env "${envs[@]}" TALLBOY_T0="$(date +%s%N)" "$bin" --profile agent "$@" 2>&1
}
median() { sort -n | awk '{a[NR]=$1} END {print (NR%2) ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2}'; }
field() { grep -F "$1" | head -1 | sed -E 's/.*: ([0-9.]+) ms.*/\1/'; }
repeat() { # repeat LABEL PATTERN ENV... -- ARGS...
    local label=$1 pattern=$2; shift 2
    local values=()
    for _ in $(seq 10); do values+=("$(launch "$@" | field "$pattern")"); done
    printf '%s: median %s ms of 10 (%s)\n' "$label" "$(printf '%s\n' "${values[@]}" | median)" "${values[*]}"
}
launch TALLBOY_BASELINE=1 -- >/dev/null   # warm the caches
repeat "empty GTK 4 window, first frame" "first frame" TALLBOY_BASELINE=1 --
repeat "\$HOME window, first frame" "first frame" TALLBOY_SMOKE=1 -- "$HOME"
[[ $AGENT_BENCH_WHAT == frames ]] && exit 0
repeat "/usr/bin ($(ls /usr/bin | wc -l) entries), load finished" "folder loaded:" TALLBOY_SMOKE=1 -- /usr/bin
echo "--- fixture/big, three runs"
for _ in 1 2 3; do
    launch TALLBOY_SMOKE=1 -- "$fixture/big" |
        grep -E 'first batch painted|folder loaded|stall|rss' | sed 's/^tallboy trace: /  /' | paste -sd';'
done
echo "--- memory: resident set after loading 0, 10,000 and 50,000 files"
rss() { launch TALLBOY_SMOKE=1 -- "$1" | grep -F 'rss:' | tail -1 | sed -E 's/.*rss: ([0-9]+) KiB/\1/'; }
r0=$(rss "$fixture/empty") r10=$(rss "$HOME/.cache/tallboy-agent/bench-10k") r50=$(rss "$fixture/big")
awk -v a="$r0" -v b="$r10" -v c="$r50" 'BEGIN {
    printf "  empty %.1f MiB, 10k %.1f MiB, 50k %.1f MiB; per 10,000 files: %.1f MiB (10k to 50k slope), %.1f MiB (empty to 50k)\n",
        a/1024, b/1024, c/1024, (c-b)/4/1024, (c-a)/5/1024 }'
echo "--- in-window navigation and re-sort (window already shown)"
"$bin" --profile agent "$fixture/empty" 2>"$OUT/nav.log" &
nav=$!
sleep 1.5
for _ in 1 2 3; do act go-to "'$fixture/big'"; sleep 2.5; act go-to "'$fixture/empty'"; sleep 0.6; done
act go-to "'$fixture/big'"; sleep 2.5
act sort-order "'descending'"; sleep 1.5
act sort-order "'ascending'"; sleep 1.5
kill $nav; wait $nav 2>/dev/null || true
grep -E 'first batch painted|loaded and sorted|longest|re-sort' "$OUT/nav.log" | grep -v ' 0\.[0-9] ms' |
    sed 's/^tallboy trace: /  /' | paste -sd';' | sed 's/;  first batch painted/\n  first batch painted/g'
DRIVER
