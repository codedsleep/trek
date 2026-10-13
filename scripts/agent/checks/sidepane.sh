# The side pane: Places, bookmarks and the Directory Tree (SCOPE 8.2–8.6).
#   scripts/agent-run.sh -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/sidepane.sh
# The bookmarks file is the agent profile's own ($XDG_CONFIG_HOME/gtk-3.0).
F=$HOME/.cache/tallboy-fixture
B=$XDG_CONFIG_HOME/gtk-3.0/bookmarks
mkdir -p "$(dirname "$B")"; : >"$B"
at() { has "$(state)" "location=file://$1 "; }
settle 1; shot places
echo "--- fixed rows follow the [places] keys"
check "Home, Desktop, Trash and Devices are listed by default" has "$(state)" 'places=\["Home Folder", "Desktop", "Trash Can", "Devices"'
check "Filesystem Root is not, by default" bash -c '! grep -q "Filesystem Root" <<<"$1"' _ "$(state | grep -o 'places=\[[^]]*\]')"

echo "--- bookmarks"
act go-to "'$F/deep'"; settle 0.5
act add-bookmark; settle 0.5; shot add-bookmark; vin key Return; settle 0.8
check "Add to Bookmarks writes the shared GTK file" grep -q "file://$F/deep deep" "$B"
check "and the row appears in Places" has "$(state)" 'places=\[[^]]*"deep"\]'
check "the bookmark for the current folder is highlighted" bash -c '[[ "$1" != *"place_selected=-1"* ]]' _ "$(state)"
printf 'file://%s/images Pictures here\n' "$F" >>"$B"; settle 1.5
check "a bookmark added by another program appears without a restart" has "$(state)" 'places=\[[^]]*"deep", "Pictures here"\]'
act go-to "'$F/names'"; settle 0.5
act open-bookmark "'file://$F/images'"; settle 0.8
check "the Bookmarks menu opens a bookmark in the current tab" at "$F/images"
shot bookmarks
act edit-bookmarks; settle 0.6; shot edit-bookmarks
vin key alt+d; settle 0.5       # Move Down: deep goes below Pictures here
check "Edit Bookmarks moves an entry" bash -c 'head -1 "$1" | grep -q "images Pictures here"' _ "$B"
vin key Escape; settle 0.4

echo "--- clicking a place"
# Rows are 38 px apart from y=165: Home Folder, Desktop, Trash Can, Devices.
vin move 70 165 click; settle 0.8
check "a click on Home Folder goes Home" at "$HOME"
vin move 70 241 click; settle 1
check "a click on Trash Can opens the trash" has "$(state)" 'location=trash:///'

echo "--- directory tree"
act sidepane-mode "'dirtree'"; settle 0.6
act go-to "'$F/deep/level-00/level-01'"; settle 2; shot tree
check "the tree expands down to the folder, through a hidden one" has "$(state)" 'tree=\[[^]]*">.cache"[^]]*">>tallboy-fixture"[^]]*">>>deep", ">>>>level-00", ">>>>>level-01"'
check "a row is selected" bash -c '[[ "$1" != *"tree_selected=-1"* ]]' _ "$(state)"
mkdir -p "$F/deep/level-00/zz-new"; settle 1.5
check "a folder created outside appears under its expanded parent" has "$(state)" '">>>>>zz-new"'
rmdir "$F/deep/level-00/zz-new"; settle 1
check "and goes again when it is removed" bash -c '[[ "$1" != *"zz-new"* ]]' _ "$(state)"
act sidepane-show; settle 0.4
check "F9's action hides the pane" has "$(describe sidepane-show)" 'false'
act sidepane-show; act sidepane-mode "'places'"; settle 0.4
