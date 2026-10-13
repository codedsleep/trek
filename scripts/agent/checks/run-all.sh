#!/usr/bin/env bash
# Runs every saved UI check in the headless agent session and prints one
# line per check. Each check's own header says what it covers.
#
#   scripts/agent/checks/run-all.sh [NAME...]     # default: all of them
#
# Exit status 1 if any check failed or produced no result.
set -uo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
checks="$root/scripts/agent/checks"
run="$root/scripts/agent-run.sh"
fixture="$HOME/.cache/tallboy-fixture"
agent="$HOME/.cache/tallboy-agent"
conf="$agent/config/tallboy/agent/tallboy.conf"
artifacts="$root/target/agent/checks"

run_session() {
    local out="$artifacts/$name-$session_index" status=0
    session_index=$((session_index + 1))
    "$run" --out "$out" --env G_DEBUG=fatal-criticals "$@" || status=$?
    if [[ $status != 0 ]]; then
        echo "FAIL session exited with status $status (logs: $out)"
    fi
    if rg -n '(Gtk|Gdk|GLib|GObject)(-[[:alnum:]]+)*-(WARNING|CRITICAL)|assertion .+ failed' \
        "$out/tallboy.log" "$out/session.log" >"$out/toolkit-diagnostics.log"; then
        echo "FAIL toolkit warning or critical (logs: $out/toolkit-diagnostics.log)"
        cat "$out/toolkit-diagnostics.log"
        status=1
    fi
    return "$status"
}

# Starts the profile from defaults, or from the given key-file text.
seed() {
    mkdir -p "$(dirname "$conf")"
    rm -f -- "$conf"
    [[ $# -gt 0 ]] && printf '%b' "$1" >"$conf"
    return 0
}

one() {
    local name=$1
    case $name in
        navigation) seed; run_session -- "$fixture/deep" <"$checks/navigation.sh" ;;
        opening)
            seed
            run_session --env "TERMINAL=$agent/opening/bin/fake-term" -- "$fixture/names" <"$checks/opening.sh"
            ;;
        single-click)
            seed '[config]\nsingle_click=1\nauto_selection_delay=300\n'
            run_session --keep -- "$fixture" <"$checks/single-click.sh"
            ;;
        dnd-ask)
            seed '[config]\ndrop_default_action=ask\n'
            run_session --keep -- "$fixture/names" <"$checks/dnd-ask.sh"
            ;;
        progress) seed; run_session --release -- "$fixture" <"$checks/progress.sh" ;;
        prefs) seed; run_session --size 1280x900 --timeout 120 -- "$fixture/names" <"$checks/prefs.sh" ;;
        leaks|session)
            seed
            run_session --env TALLBOY_TRACE=1 -- --daemon-mode "$fixture/deep" <"$checks/$name.sh"
            ;;
        filemanager1)
            seed
            run_session --env TALLBOY_FILEMANAGER1=1 -- --daemon-mode <"$checks/filemanager1.sh"
            EXPECT_OWNED=0 run_session -- --daemon-mode <"$checks/filemanager1.sh"
            ;;
        *) seed; run_session -- "$fixture/names" <"$checks/$name.sh" ;;
    esac
}

names=("$@")
if [[ ${#names[@]} -eq 0 ]]; then
    names=(navigation opening single-click fileops dnd dnd-ask progress menus sidepane
        properties dual-pane prefs leaks filemanager1 session)
fi
failed=0
for name in "${names[@]}"; do
    session_index=1
    status=0
    out=$(one "$name" 2>&1) || status=$?
    pass=$(grep -c '^PASS' <<<"$out")
    fail=$(grep -c '^FAIL' <<<"$out")
    printf '%-13s %3d passed, %d failed\n' "$name" "$pass" "$fail"
    grep '^FAIL' <<<"$out" | sed 's/^/    /'
    [[ $status != 0 || $fail -gt 0 || $pass -eq 0 ]] && failed=1
done
seed
exit $failed
