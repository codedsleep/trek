# Preferences (SCOPE 9.5), keyboard only. Run with:
#   scripts/agent-run.sh --size 1280x900 -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/prefs.sh
CONF="$XDG_CONFIG_HOME/tallboy/agent/tallboy.conf"
saved() { sleep 1.6; grep -qx "$1" "$CONF"; }
settle 0.8

echo "--- General"
act preferences; settle 0.8
check "Edit > Preferences opens the dialog" has "$(state)" 'dialogs=\["Preferences"\]'
vin key alt+o; settle 0.3
check "a check box is saved: single_click" saved 'single_click=1'
vin key alt+d; settle 0.2; vin key space; settle 0.4; vin key End; vin key Return; settle 0.3
check "a dropdown is saved: drop_default_action" saved 'drop_default_action=ask'
vin key alt+s; settle 0.2; vin key Up; vin key Up; vin key Tab; settle 0.3
check "a number is saved in its stored unit: auto_selection_delay" saved 'auto_selection_delay=800'

echo "--- Display"
tallboy --show-pref=2; settle 0.6
check "--show-pref reuses the open dialog" has "$(state)" 'toplevels=2'
vin key alt+b; settle 0.2; vin key space; settle 0.4; vin key End; vin key Return; settle 0.8
shot prefs-big-icons
check "big_icon_size is saved" saved 'big_icon_size=256'
vin key alt+l; settle 0.2; vin key Up; vin key Tab; settle 0.3
check "max_tab_chars is saved" saved 'max_tab_chars=33'

echo "--- Layout, applied live"
tallboy --show-pref=3; settle 0.6
check "Filesystem Root is not in Places yet" test -z "$(state | grep -o '"Filesystem Root", "Devices"')"
vin key alt+f; settle 0.6
check "ticking it adds the row at once" has "$(state)" '"Trash Can", "Filesystem Root", "Devices"'
vin key alt+h; settle 0.6
check "unticking Home removes its row at once" has "$(state)" 'places=\["Desktop"'
vin key alt+h; settle 0.4
vin key alt+p; settle 0.2; vin key space; settle 0.4; vin key End; vin key Return; settle 0.6
check "path bar mode switches at once" has "$(state)" 'page=crumbs'
vin key alt+t; settle 0.5; shot prefs-layout-live
check "pathbar_mode_buttons and always_show_tabs are saved" saved 'always_show_tabs=1'

echo "--- a change made elsewhere shows in the dialog"
act pathbar-mode "'location'"; settle 0.5; shot prefs-follows
check "the window is back in location mode" has "$(state)" 'page=entry'

echo "--- Advanced"
tallboy --show-pref=5; settle 0.6
vin key alt+t; settle 0.2; vin type "foot --title x"; vin key Tab; settle 0.3
check "a text field is saved when it loses the keyboard" saved 'terminal=foot --title x'
vin key alt+h; settle 0.2; vin type "$HOME/.cache/tallboy-fixture"; vin key Return; settle 0.6
check "home_path changes where Home Folder points" saved "home_path=$HOME/.cache/tallboy-fixture"
vin key Escape; settle 0.5
check "Esc closes the dialog" has "$(state)" 'toplevels=1'
act home; settle 0.8
check "Go > Home follows the override" has "$(state)" "location=file://$HOME/.cache/tallboy-fixture "
check "nothing was logged as a warning" test -z "$(grep -E 'WARNING|CRITICAL' "$OUT/tallboy.log")"
