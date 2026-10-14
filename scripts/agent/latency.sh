#!/usr/bin/env bash
# Compares latency with the installed PCManFM, measured from outside and the
# same way for both: each application runs with WAYLAND_DEBUG=1 in the headless
# cage session and scripts/agent/latency.awk reads, from the client's own
# protocol log, how long after an input event (or after exec) it committed a
# frame. Release build, 3840x2160 at scale 1.5 like the owner's monitor.
#
#   scripts/agent/latency.sh                    tallboy, pcmanfm, pcmanfm-list
#   scripts/agent/latency.sh tallboy            one of them
#   LATENCY_ONLY=nav scripts/agent/latency.sh   only the scenarios of one group
#                                               (start, mid, big or nav)
#
# pcmanfm is PCManFM as installed with its stock settings (icon view);
# pcmanfm-list is the same in its detailed list view, the nearer relative of
# tallboy's lines. Folders are only read. Rows land in
# target/agent/latency/rows.tsv, the table is printed.
#
# What the figures are not: cage has no fractional-scale protocol, so GTK 4
# draws 5120x2880 here where the owner's compositor asks it for 3840x2160
# (GTK 3 draws 5120x2880 in both), and a commit is the application handing its
# frame over, not light leaving the monitor.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/target/agent/latency"
apps=${*:-tallboy pcmanfm pcmanfm-list}
command -v pcmanfm >/dev/null || { echo "latency.sh: pcmanfm is not installed" >&2; exit 2; }
cargo build --release -q -p tallboy
# Two small folders to walk into and out of.
tree="$HOME/.cache/tallboy-agent/latency/nav"
if [[ ! -d $tree ]]; then
    for d in a b; do
        mkdir -p "$tree/$d"
        (cd "$tree/$d" && seq -f 'file-%02g.txt' 1 30 | xargs touch)
    done
fi
rm -rf "$out"

LATENCY_APPS=$apps LATENCY_ONLY=${LATENCY_ONLY:-} "$root/scripts/agent-run.sh" --release \
    --size 3840x2160 --scale 1.5 --app "sleep 3000" --out "$out" --timeout 2700 <<'DRIVER'
bin="$root/target/release/tallboy"
fixture="$HOME/.cache/tallboy-fixture"
tree="$HOME/.cache/tallboy-agent/latency/nav"
# PCManFM finds a running instance through a socket in XDG_RUNTIME_DIR. It gets
# a runtime directory of its own, so that it can never hand its folder to a
# PCManFM on the owner's desktop, and settings of its own, written before every
# launch (it saves them on exit).
runtime=$(mktemp -d "$HOME/.cache/tallboy-agent/latency/runtime-XXXXXX")
chmod 700 "$runtime"
settings="$HOME/.cache/tallboy-agent/latency/pcmanfm-config"
display="$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
serial=0

ticks() { awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo gone; }
# Returns once PID has drawn something and then used no CPU for half a second.
idle() {
    local last="" now quiet=0 spent=0
    while ((spent++ < 1200)); do
        sleep 0.05
        now=$(ticks "$1")
        if [[ $now == "$last" ]] && grep -Fq '.commit()' "$log"; then
            ((++quiet < 10)) || return 0
        else
            quiet=0 last=$now
        fi
    done
    echo "latency: $app never went quiet" >&2
}
mark() { printf 'MARK %s\n' "$*" >>"$log"; }
# launch LABEL FOLDER: starts $app there, timed from just before exec as LABEL.
launch() {
    log="$OUT/$app.$((++serial)).wl"
    case $app in
        tallboy)
            rm -rf "$XDG_CONFIG_HOME/tallboy/agent"
            printf 'T0 %s %s\n' "$(date -u +%T.%6N)" "$1" >"$log"
            WAYLAND_DEBUG=1 "$bin" --profile agent "$2" >/dev/null 2>>"$log" &
            ;;
        pcmanfm*)
            rm -rf "$settings"
            mkdir -p "$settings/pcmanfm/default"
            {
                printf '[volume]\nmount_on_startup=0\nmount_removable=0\nautorun=0\n'
                [[ $app != pcmanfm-list ]] || printf '[ui]\nview_mode=list\n'
            } >"$settings/pcmanfm/default/pcmanfm.conf"
            printf 'T0 %s %s\n' "$(date -u +%T.%6N)" "$1" >"$log"
            env XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$display" XDG_CONFIG_HOME="$settings" \
                WAYLAND_DEBUG=1 pcmanfm --no-desktop "$2" >/dev/null 2>>"$log" &
            ;;
    esac
    pid=$!
    idle "$pid"
    mark
}
quit() {
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}
# keys LABELS COUNT SPACING KEY...: presses the keys COUNT times over, one
# every SPACING ms (40 at least), labelled in turn by LABELS.
keys() {
    local labels=$1 count=$2 gap=$(($3 - 40)) args=(sleep 300) key
    shift 3
    for ((n = 0; n < count; n++)); do
        for key in "$@"; do args+=(key "$key" sleep "$gap"); done
    done
    mark "$labels"
    vin "${args[@]}"
    idle "$pid"
    mark
}
wanted() { [[ -z $LATENCY_ONLY || $LATENCY_ONLY == "$1" ]]; }

for app in $LATENCY_APPS; do
    case $app in
        tallboy) down=j bottom=G page=ctrl+f enter=l parent=h tab=t invert=ctrl+r first=() ;;
        # Home puts PCManFM's cursor on the first folder; it is not measured.
        *) down=Down bottom=End page=Page_Down enter=Return parent=BackSpace tab=ctrl+t invert=ctrl+i first=(Home) ;;
    esac
    skip=${first:+- }
    launch - "$fixture/names"   # the first start of a session also starts the portals
    vin move 1280 12            # park the pointer on the title bar
    quit

    if wanted start; then
        for start in "small $fixture/names" "mid /usr/bin" "big $fixture/big"; do
            for _ in 1 2 3 4 5; do
                launch "start.${start%% *}" "${start#* }"
                quit
            done
        done
    fi

    if wanted mid; then
        launch - /usr/bin
        keys mid.down 40 150 "$down"
        keys mid.page 15 200 "$page"
        keys "mid.bottom mid.top" 6 400 "$bottom" Home
        keys mid.held 100 40 "$down"
        shot "$app-mid"
        quit
    fi

    if wanted big; then
        launch - "$fixture/big"
        keys big.down 40 150 "$down"
        keys big.page 15 200 "$page"
        keys "big.bottom big.top" 5 1000 "$bottom" Home
        keys big.held 100 40 "$down"
        keys big.select-all 1 2000 ctrl+a
        keys big.invert 4 2000 "$invert"
        mark big.wheel
        wheel=(move 1000 700 sleep 300)
        for _ in $(seq 30); do wheel+=(scroll 3 sleep 100); done
        vin "${wheel[@]}" move 1280 12
        idle "$pid"
        mark
        shot "$app-big"
        quit
    fi

    if wanted nav; then
        launch - "$tree"
        keys "${skip}nav.enter-small nav.leave-small" 10 500 "${first[@]}" "$enter" "$parent"
        keys tab.new 5 600 "$tab"
        quit
        launch - /usr           # bin is its first folder
        keys "${skip}nav.enter-mid nav.leave-mid" 5 1500 "${first[@]}" "$enter" "$parent"
        quit
        launch - "$fixture"     # and big the first here
        keys "${skip}nav.enter-big nav.leave-big" 4 3000 "${first[@]}" "$enter" "$parent"
        shot "$app-nav"
        quit
    fi
done
rm -rf "$runtime"

for log in "$OUT"/*.wl; do
    app=$(basename "$log")
    awk -f "$root/scripts/agent/latency.awk" "$log" | sed "s/^/${app%%.*}\t/"
done | grep -v -P '^[^\t]+\t-\t' >"$OUT/rows.tsv"
DRIVER

# Per scenario and application: median, 95th percentile and worst time to the
# first frame, median and worst time to the last, and events that drew nothing.
awk -F'\t' '
function at(list, n, share,    i) { i = int((n - 1) * share + 1.5); return list[i > n ? n : i] }
{
    key = $2 SUBSEP $1
    if (!($2 in seen)) { seen[$2] = 1; order[++scenarios] = $2 }
    if (!($1 in known)) { known[$1] = 1; apps[++napps] = $1 }
    events[key]++
    if ($5 == 0) { silent[key]++; next }
    drawn[key]++
    first[key][drawn[key]] = $3 + 0
    last[key][drawn[key]] = $4 + 0
}
END {
    printf "%-17s %-13s %4s %9s %8s %8s %10s %9s %7s\n", "scenario", "application", "n",
        "first med", "p95", "worst", "last med", "worst", "nothing"
    for (s = 1; s <= scenarios; s++) {
        for (a = 1; a <= napps; a++) {
            key = order[s] SUBSEP apps[a]
            if (!(key in events)) continue
            n = drawn[key] + 0
            if (!n) { printf "%-17s %-13s %4d %56d\n", order[s], apps[a], events[key], silent[key]; continue }
            asort(first[key]); asort(last[key])
            printf "%-17s %-13s %4d %9.1f %8.1f %8.1f %10.1f %9.1f %7d\n", order[s], apps[a], events[key],
                at(first[key], n, 0.5), at(first[key], n, 0.95), first[key][n],
                at(last[key], n, 0.5), last[key][n], silent[key] + 0
        }
        print ""
    }
}' "$out/rows.tsv"
