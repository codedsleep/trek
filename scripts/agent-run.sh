#!/usr/bin/env bash
# Runs tallboy for an agent: headless cage compositor, private D-Bus session,
# isolated XDG directories, --profile agent. Nothing reaches the owner's
# monitors, bus, config, trash or thumbnail cache (DISCOVERY D14, D25; SCOPE S5).
#
#   scripts/agent-run.sh [options] [-- tallboy arguments] < driver.sh
#
# The driver is a bash script read from stdin. It runs inside the session once
# the application has started, with these helpers:
#   shot NAME            screenshot to $OUT/NAME.png
#   act NAME [VARIANT]   activate win.NAME on window $WIN (default 1)
#   app_act NAME [VARIANT]
#   describe NAME        print (enabled, parameter type, state) of win.NAME
#   vin ...              keyboard and pointer input, see scripts/agent/vinput.c
#   state                dump and print the active tab's state line
#   wait_log REGEX [S]   wait until the log matches (default 10 s)
#   settle [S]           sleep (default 0.4)
#   tallboy ARGS...      a further invocation with the agent profile
#   check DESC CMD...    print PASS or FAIL for CMD; any FAIL fails the run
#   has TEXT REGEX       true when TEXT matches (for use with check)
# With no driver the application runs until --timeout.
#
# Options:
#   --size WxH     output size in pixels (default 1280x720)
#   --scale N      output scale (default 1)
#   --release      use the release build
#   --app CMD      run CMD instead of tallboy (reference shots of pcmanfm)
#   --env K=V      set a variable for the application only (repeatable)
#   --smoke        shorthand for --env TALLBOY_SMOKE=1 --env G_DEBUG=fatal-criticals
#   --keep         keep the agent profile's config from the previous run
#   --out DIR      artefact directory (default target/agent/out)
#   --timeout S    hard limit for the whole session (default 90)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
agent_home="$HOME/.cache/tallboy-agent"

if [[ ${1:-} != --inside ]]; then
    size=1280x720 scale=1 build=debug app="" keep=0 out="$root/target/agent/out" limit=90
    app_env=()
    while [[ $# -gt 0 ]]; do
        case $1 in
            --size) size=$2; shift 2 ;;
            --scale) scale=$2; shift 2 ;;
            --release) build=release; shift ;;
            --app) app=$2; shift 2 ;;
            --env) app_env+=("$2"); shift 2 ;;
            --smoke) app_env+=(TALLBOY_SMOKE=1 G_DEBUG=fatal-criticals); shift ;;
            --keep) keep=1; shift ;;
            --out) out=$2; shift 2 ;;
            --timeout) limit=$2; shift 2 ;;
            --) shift; break ;;
            *) echo "agent-run.sh: unknown option $1" >&2; exit 2 ;;
        esac
    done
    for tool in cage grim dbus-run-session gdbus wlr-randr; do
        command -v "$tool" >/dev/null || { echo "agent-run.sh: $tool is not installed" >&2; exit 2; }
    done
    "$root/scripts/agent/build-vinput.sh"
    if [[ -z $app ]]; then
        if [[ $build == release ]]; then cargo build --release -q -p tallboy; else cargo build -q -p tallboy; fi
    fi
    mkdir -p "$out" "$agent_home"/{config,cache,data,state}
    [[ $keep == 1 ]] || rm -rf "$agent_home/config/tallboy/agent"
    driver="$out/driver.sh"
    if [[ -t 0 ]]; then : >"$driver"; else cat >"$driver"; fi
    unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE TALLBOY_UNLOCKED
    export XDG_CONFIG_HOME="$agent_home/config" XDG_CACHE_HOME="$agent_home/cache"
    export XDG_DATA_HOME="$agent_home/data" XDG_STATE_HOME="$agent_home/state"
    export TALLBOY_ALLOW_ROOTS="$HOME/.cache/tallboy-fixture;$agent_home${TALLBOY_ALLOW_ROOTS:+;$TALLBOY_ALLOW_ROOTS}"
    export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 GTK_A11Y=none NO_AT_BRIDGE=1
    export AGENT_SIZE=$size AGENT_SCALE=$scale AGENT_BUILD=$build AGENT_APP=$app OUT=$out
    export AGENT_DRIVER=$driver AGENT_APP_ENV="${app_env[*]}"
    exec timeout --kill-after=5 "$limit" dbus-run-session -- \
        cage -- "$root/scripts/agent-run.sh" --inside "$@" 2>"$out/session.log"
fi

# ---- inside the headless session ----
shift
wlr-randr --output HEADLESS-1 --custom-mode "$AGENT_SIZE" --scale "$AGENT_SCALE" >/dev/null
width=${AGENT_SIZE%x*} height=${AGENT_SIZE#*x}
export VINPUT_EXTENT="$(awk -v w="$width" -v h="$height" -v s="$AGENT_SCALE" 'BEGIN{printf "%dx%d", w/s, h/s}')"
app_id=io.github.codedsleep.Tallboy.p_agent
app_path=/io/github/codedsleep/Tallboy/p_agent
log="$OUT/tallboy.log"
WIN=1

shot() { grim "$OUT/$1.png"; }
act() {
    local param="[]"
    [[ $# -gt 1 ]] && param="[<$2>]"
    gdbus call --session --dest "$app_id" --object-path "$app_path/window/$WIN" \
        --method org.gtk.Actions.Activate "$1" "$param" "{}" >/dev/null
}
app_act() {
    local param="[]"
    [[ $# -gt 1 ]] && param="[<$2>]"
    gdbus call --session --dest "$app_id" --object-path "$app_path" \
        --method org.gtk.Actions.Activate "$1" "$param" "{}" >/dev/null
}
describe() {
    gdbus call --session --dest "$app_id" --object-path "$app_path/window/$WIN" \
        --method org.gtk.Actions.Describe "$1"
}
vin() { "$root/target/agent/vinput" "$@"; }
tallboy() { "$root/target/$AGENT_BUILD/tallboy" --profile agent "$@"; }
FAILED=0
check() {
    local description=$1
    shift
    if "$@"; then echo "PASS $description"; else echo "FAIL $description"; FAILED=1; fi
}
has() { grep -Eq -- "$2" <<<"$1"; }
settle() { sleep "${1:-0.4}"; }
wait_log() {
    local deadline=$((SECONDS + ${2:-10}))
    until grep -Eq "$1" "$log" 2>/dev/null; do
        if ((SECONDS >= deadline)); then echo "wait_log: timed out on $1" >&2; return 1; fi
        sleep 0.05
    done
}
state() {
    local before
    before=$(grep -c '^tallboy state:' "$log" 2>/dev/null || true)
    act debug-dump
    for _ in $(seq 100); do
        [[ $(grep -c '^tallboy state:' "$log" 2>/dev/null || true) -gt ${before:-0} ]] && break
        sleep 0.02
    done
    grep '^tallboy state:' "$log" | tail -1
}

# Emptied here, not by the background job's redirection: otherwise wait_log
# can match "tallboy ready" left in the log by the previous run.
: >"$log"
if [[ -n $AGENT_APP ]]; then
    # shellcheck disable=SC2086
    env $AGENT_APP_ENV $AGENT_APP "$@" >>"$log" 2>&1 &
else
    # shellcheck disable=SC2086
    env $AGENT_APP_ENV TALLBOY_T0="$(date +%s%N)" \
        "$root/target/$AGENT_BUILD/tallboy" --profile agent "$@" >>"$log" 2>&1 &
fi
app_pid=$!
status=0
if [[ -s $AGENT_DRIVER ]]; then
    [[ -n $AGENT_APP ]] && sleep 1.5 || wait_log '^tallboy ready' 15 || true
    set +e
    # shellcheck disable=SC1090
    (set -e; source "$AGENT_DRIVER"; exit "$FAILED")
    status=$?
    set -e
else
    wait "$app_pid" || status=$?
fi
if kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
else
    # A driver can print passing assertions before the application crashes.
    app_status=0
    wait "$app_pid" || app_status=$?
    [[ $status != 0 ]] || status=$app_status
fi
exit "$status"
