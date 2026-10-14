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

# The layout the pointer checks were written against: icon view, side pane,
# toolbar buttons and menu bar shown, no parent or preview column. All of it
# is still on offer, so these checks keep it working; `vim` and `navigation`
# run with the defaults.
classic='[ui]\nview_mode=icon\nside_pane_mode=places\nmiller_parent=0\nmiller_preview=0\nshow_menubar=1\npathbar_mode_buttons=0\ntoolbar=newtab;navigation;home;\n'

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
        vim) seed; run_session --size 1400x800 --timeout 240 -- "$fixture/names" <"$checks/vim.sh" ;;
        finder) seed; run_session --size 1400x800 --timeout 120 -- "$fixture/names" <"$checks/finder.sh" ;;
        navigation)
            seed '[ui]\npathbar_mode_buttons=0\n'
            run_session --keep -- "$fixture/deep" <"$checks/navigation.sh"
            ;;
        opening)
            seed "$classic"
            run_session --keep --env "TERMINAL=$agent/opening/bin/fake-term" \
                --env "VISUAL=$agent/opening/bin/fake-edit" \
                --env "TALLBOY_HOST_XDG_CONFIG_HOME=$agent/opening/host-config" \
                -- "$fixture/names" <"$checks/opening.sh"
            ;;
        single-click)
            seed "[config]\nsingle_click=1\nauto_selection_delay=300\n$classic"
            run_session --keep -- "$fixture" <"$checks/single-click.sh"
            ;;
        dnd-ask)
            seed "[config]\ndrop_default_action=ask\n$classic"
            run_session --keep -- "$fixture/names" <"$checks/dnd-ask.sh"
            ;;
        progress) seed "$classic"; run_session --keep --release -- "$fixture" <"$checks/progress.sh" ;;
        prefs)
            seed "$classic"
            run_session --keep --size 1280x900 --timeout 120 -- "$fixture/names" <"$checks/prefs.sh"
            ;;
        leaks|session)
            seed "$classic"
            run_session --keep --env TALLBOY_TRACE=1 -- --daemon-mode "$fixture/deep" <"$checks/$name.sh"
            ;;
        filemanager1)
            seed
            run_session --env TALLBOY_FILEMANAGER1=1 -- --daemon-mode <"$checks/filemanager1.sh"
            EXPECT_OWNED=0 run_session -- --daemon-mode <"$checks/filemanager1.sh"
            ;;
        theme)
            # Read at start-up, so the themes are there before the session.
            themes="$agent/data/themes"
            rm -rf -- "$themes"
            mkdir -p "$themes"/Tallboy{Three,Light,Unreadable}/gtk-3.0 "$themes/TallboyFour/gtk-4.0"
            printf '@define-color %s;\n' 'theme_bg_color #203040' 'theme_base_color #102030' \
                'theme_fg_color #d0d8e0' 'theme_text_color @theme_fg_color' \
                'theme_selected_bg_color #c08040' 'theme_selected_fg_color rgba(0, 0, 0, 0.87)' \
                >"$themes/TallboyThree/gtk-3.0/gtk.css"
            printf '@define-color %s;\n' 'theme_bg_color #f0ece4' 'theme_base_color #ffffff' \
                'theme_fg_color #303030' 'theme_selected_bg_color #a05020' \
                >"$themes/TallboyLight/gtk-3.0/gtk.css"
            printf '@define-color %s;\n' 'theme_bg_color shade(#203040, 1.2)' 'theme_fg_color #d0d8e0' \
                >"$themes/TallboyUnreadable/gtk-3.0/gtk.css"
            printf '%s\n' '@define-color theme_selected_bg_color #4080c0;' \
                '@define-color theme_selected_fg_color white;' '@define-color theme_fg_color white;' \
                '@define-color success_color green;' '@define-color error_color red;' \
                '.view { background-color: #402030; color: white; }' \
                >"$themes/TallboyFour/gtk-4.0/gtk.css"
            for theme in gtk3:Three gtk4:Four light:Light unreadable:Unreadable missing:Missing; do
                seed
                THEME_CASE=${theme%:*} run_session --env "TALLBOY_GTK_THEME=Tallboy${theme#*:}" \
                    -- "$fixture/names" <"$checks/theme.sh"
            done
            rm -rf -- "$themes"
            ;;
        *) seed "$classic"; run_session --keep -- "$fixture/names" <"$checks/$name.sh" ;;
    esac
}

names=("$@")
if [[ ${#names[@]} -eq 0 ]]; then
    names=(vim finder navigation opening single-click fileops dnd dnd-ask progress menus sidepane
        properties dual-pane prefs theme leaks filemanager1 session)
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
