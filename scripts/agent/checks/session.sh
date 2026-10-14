# Phase 10 session cleanliness: all menubar menus, their submenus, view
# factories, dialogs and the final tab/folder counters. Run through run-all.sh session.
settle 0.8
for key in f e v g b t h; do
    vin key "alt+$key"; settle 0.2; shot "menubar-$key"
    vin key Escape; settle 0.1
done
# Create New and every View submenu. Open without activating their items.
vin key alt+f; vin key r; settle 0.2; shot create-submenu
vin key Escape; vin key Escape
for key in s l t h p; do
    vin key alt+v; vin key "$key"; settle 0.2; shot "view-submenu-$key"
    vin key Escape; vin key Escape
done
for mode in list compact thumbnail icon; do
    act view-mode "'$mode'"; settle 0.4
    check "$mode view remains usable after opening menus" has "$(state)" "mode=$mode"
done
for dialog in about edit-bookmarks folder-properties connect-server run-command preferences; do
    act "$dialog"; settle 0.4; shot "dialog-$dialog"
    vin key Escape; settle 0.3
    check "$dialog closes without leaving a toplevel" has "$(state)" 'toplevels=1'
done
vin key ctrl+q; settle 1.2
check 'closing the session releases every tab and folder' has \
    "$(rg 'after a window closed' "$OUT/trek.log" | tail -1)" 'live_tabs=0 live_folders=0'
