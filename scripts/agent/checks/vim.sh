# The vim-motion key bindings of the file view, through real key presses. Run with:
#   scripts/agent-run.sh -- ~/.cache/trek-fixture/names < scripts/agent/checks/vim.sh
# Everything that changes files happens in a folder made here, inside the
# allowed roots.
V=$(mktemp -d "$HOME/.cache/trek-agent/vim-XXXXXX")
mkdir -p "$V/sub/inner" "$V/zeta"
for n in alpha beta gamma delta; do printf '%s\n' "$n" >"$V/$n.txt"; done
printf 'x\n' >"$V/Archive.tar.gz"; printf 'h\n' >"$V/.hidden"
ln -s sub/inner "$V/link"
cur() { has "$(state)" " cursor=\"$1\" "; }
sel() { has "$(state)" " selected=$1 "; }
at() { has "$(state)" "location=file://$1 "; }
act go-to "'$V'"; settle 0.8

echo "--- hopping"
# Folders first (a link to a folder is one): link, sub, zeta; then alpha.txt,
# Archive.tar.gz, beta.txt, delta.txt, gamma.txt. `.hidden` is not shown.
check "the cursor starts on the first file, nothing selected" has "$(state)" 'cursor="link" at=0 .* selected=0 '
vin key j; settle 0.2
check "j moves down" cur sub
vin key k key k; settle 0.2
check "k moves up and wraps to the last file" cur gamma.txt
vin key j; settle 0.2
check "j wraps to the first" cur link
vin type "G"; settle 0.2
check "G goes to the bottom" cur gamma.txt
vin type "g"; settle 0.3
check "g starts a sequence and lists its continuations" has "$(state)" 'pending="g" which_key=true'
vin type "g"; settle 0.2
check "gg goes to the top" has "$(state)" 'cursor="link" .* pending="" which_key=false'
vin type "g"; settle 0.2; vin key Escape; settle 0.2
check "Esc drops a started sequence" has "$(state)" 'pending="" which_key=false'
vin type "g"; settle 0.2; vin type "x"; settle 0.2
check "a key that continues nothing drops it too, and does nothing" has "$(state)" 'cursor="link" .* selected=0 .* pending=""'
vin key Down key Down; settle 0.2
check "Down moves like j" cur zeta
vin key Home; settle 0.2
check "Home goes to the top" cur link
check "moving the cursor selects nothing" sel 0

echo "--- navigation"
vin key j; vin key l; settle 0.6
check "l enters the folder under the cursor" at "$V/sub"
vin key l; settle 0.6
check "and again" at "$V/sub/inner"
vin key h; settle 0.6
check "h goes to the parent" at "$V/sub"
check "with the cursor on the folder that was left" cur inner
vin key h; settle 0.6
check "h again" has "$(state)" "location=file://$V .* cursor=\"sub\" "
vin type "H"; settle 0.6
check "H goes back in the history" at "$V/sub"
vin type "L"; settle 0.6
check "L goes forward" at "$V"
vin type "G"; settle 0.2; vin key l; settle 0.4
check "l on a file does nothing" has "$(state)" "location=file://$V .* cursor=\"gamma.txt\" .* toplevels=1 "
vin type "gh"; settle 0.6
check "gh goes home" at "$HOME"
vin type "H"; settle 0.6

echo "--- selection and visual mode"
vin type "gg"; vin key space; settle 0.3
check "Space selects the file under the cursor and steps down" has "$(state)" 'cursor="sub" .* selected=1 names=\["link"\]'
vin key space; settle 0.2; vin key k; vin key space; settle 0.3
check "Space again deselects" has "$(state)" 'cursor="zeta" .* selected=1 names=\["link"\]'
vin key Escape; settle 0.2
check "Esc clears the selection" sel 0
vin key ctrl+a; settle 0.3
check "Ctrl+A selects everything" sel 8
vin key ctrl+r; settle 0.3
check "Ctrl+R inverts" sel 0
vin type "gg"; vin type "v"; vin key j key j; settle 0.3
check "v starts visual mode and movements select" has "$(state)" 'vim=SEL .* selected=3 names=\["link", "sub", "zeta"\]'
vin key k; settle 0.2
check "moving back shrinks it" sel 2
vin key Escape; settle 0.2
check "Esc leaves visual mode and keeps the selection" has "$(state)" 'vim=NOR .* selected=2 '
vin type "V"; vin key k; settle 0.3
check "V deselects as it moves" has "$(state)" 'vim=UNS .* selected=0 '
vin key Escape; vin key ctrl+a; vin type "gg"; vin type "V"; vin key j; settle 0.3
check "and only what it moves over" has "$(state)" 'vim=UNS .* selected=6 '
vin key Escape; settle 0.2; vin key Escape; settle 0.2

echo "--- find and filter"
vin type "gg"; vin type "/"; settle 0.3
check "/ opens the input line" has "$(state)" 'input="Find next:":"" '
vin type "ta"; settle 0.4
check "the cursor follows the query as it is typed" cur zeta
vin key Return; settle 0.3
check "Enter closes the line and gives the files the keyboard" has "$(state)" 'cursor="zeta" .* focus=GtkListItemWidget input=none'
vin type "n"; settle 0.2
check "n goes to the next match" cur Archive.tar.gz
vin type "n"; settle 0.2
check "and on" cur beta.txt
vin type "N"; settle 0.2
check "N goes back" cur Archive.tar.gz
vin type "gg"; vin type "/"; settle 0.2; vin type "ARCH"; settle 0.3; vin key Return; settle 0.2
check "a query with capitals is matched as written, so finds nothing" has "$(state)" 'cursor="link" .* notice="No match for \\"ARCH\\""'
vin type "/"; settle 0.2; vin type "arch"; settle 0.3
check "one without is matched without regard to case" cur Archive.tar.gz
vin key Escape; settle 0.3
check "Esc puts the cursor back" cur link
vin type "?"; settle 0.2; vin type "txt"; settle 0.3; vin key Return; settle 0.2
check "? finds upwards, wrapping to the end" cur gamma.txt
vin type "f"; settle 0.3; vin type "TA"; settle 0.3
check "f filters as it is typed, capitals as written" has "$(state)" 'needle="TA" .* visible=0 '
vin key BackSpace key BackSpace; vin type "ta"; settle 0.4
check "folders are filtered too" has "$(state)" 'needle="ta" .* visible=4 '
vin key Return; settle 0.3
check "Enter keeps the filter" has "$(state)" 'needle="ta" .* visible=4 .* input=none'
vin key Escape; settle 0.3
check "Esc in the view clears it" has "$(state)" 'needle="" .* visible=8 '
vin type "."; settle 0.4
check ". shows hidden files" has "$(state)" ' visible=9 '
vin type "."; settle 0.4
check ". hides them again" has "$(state)" ' visible=8 '

echo "--- linemode and sorting"
vin type "ms"; settle 0.3
check "ms shows sizes" has "$(state)" 'linemode=size'
vin type ",M"; settle 0.4
check ",M sorts by modified time, newest first, and shows it" has "$(state)" 'linemode=mtime .* sort=mtime;descending;'
vin type ",s"; settle 0.3
check ",s sorts by size" has "$(state)" 'linemode=size .* sort=size;ascending;'
vin type ",E"; settle 0.3
check ",E sorts by extension, reversed, and leaves the linemode" has "$(state)" 'linemode=size .* sort=ext;descending;'
vin type ",r"; settle 0.3
check ",r says that random order is not available" has "$(state)" 'sort=ext;descending;.* notice="Random order is not available"'
vin type ",a"; vin type "mn"; settle 0.3
check ",a and mn restore the defaults" has "$(state)" 'linemode=none .* sort=name;ascending;'

echo "--- create and rename"
vin type "a"; settle 0.3; vin type "made.txt"; vin key Return; settle 0.8
check "a creates a file" test -f "$V/made.txt"
check "and puts the cursor on it" cur made.txt
vin type "a"; settle 0.3; vin type "newdir/"; vin key Return; settle 0.8
check "a name ending in a slash makes a folder" test -d "$V/newdir"
vin type "a"; settle 0.3; vin type "a//b"; vin key Return; settle 0.5
check "an invalid path keeps the line open" has "$(state)" 'input="Create '
vin key Escape; settle 0.3
check "and creates nothing" test ! -e "$V/a"
vin type "a"; settle 0.3; vin type "deep/.planning/"; vin key Return; settle 1
check "a path makes the folders it names" test -d "$V/deep/.planning"
check "and shows the new folder with the cursor on it" has "$(state)" "location=file://$V/deep .* cursor=\".planning\" "
check "showing hidden files for a dot name" has "$(state)" 'notice="Showing hidden files'
vin key h; settle 0.6
check "h goes back to where it started" has "$(state)" "location=file://$V .* cursor=\"deep\" "
# Later checks click on fixed rows; take the folder away again.
rm -rf "$V/deep"; settle 0.6
vin type "."; settle 0.4
check ". hides them again" has "$(state)" ' hidden=[1-9]'
vin type "/"; vin type "made"; vin key Return; settle 0.3
vin type "r"; settle 0.4
check "r opens the line with the name" has "$(state)" 'input="Rename:":"made.txt"'
vin type "-over"; vin key Return; settle 0.8
check "typing goes before the extension" test -f "$V/made-over.txt"
check "the cursor follows the renamed file" cur made-over.txt
vin key ctrl+a; settle 0.2; vin type "r"; settle 0.3
check "r with several files selected says it cannot" has "$(state)" 'input=none .* notice="Renaming several files'
vin key Escape; settle 0.2

echo "--- yank, paste, link"
vin type "/"; vin type "alpha"; vin key Return; settle 0.3
vin type "y"; settle 0.4
check "y yanks the file under the cursor" has "$(state)" 'yanked=Some\(\(1, false\)\)'
check "onto the clipboard other programs read" has "$(state)" 'clipboard=\[[^]]*x-special/gnome-copied-files'
vin type "/"; vin type "newdir"; vin key Return; vin key l; settle 0.6
vin type "p"; settle 1
check "p pastes into the folder shown" test -f "$V/newdir/alpha.txt"
check "the original stays" test -f "$V/alpha.txt"
printf 'changed\n' >"$V/newdir/alpha.txt"
vin type "P"; settle 1
check "P replaces an existing file without asking" test "$(cat "$V/newdir/alpha.txt")" = alpha
check "and no question was put" has "$(state)" 'toplevels=1 '
vin key h; settle 0.6
vin type "/"; vin type "zeta"; vin key Return; vin key l; settle 0.6
vin type "-"; settle 1
check "- makes a symbolic link to the yanked file" test -L "$V/zeta/alpha.txt"
vin type "_"; settle 0.3
check "_ says relative links are not available" has "$(state)" 'notice="Relative links'
vin key ctrl+minus; settle 0.3
check "Ctrl+- says hard links are not available" has "$(state)" 'notice="Hard links'
vin type "Y"; settle 0.4
check "Y unyanks" has "$(state)" 'yanked=None'
vin key h; settle 0.6
vin type "/"; vin type "beta"; vin key Return; vin key space; vin type "/"; vin type "delta"; vin key Return; vin key space; settle 0.3
vin type "x"; settle 0.4
check "x cuts the selected files and leaves nothing selected" has "$(state)" 'selected=0 .* yanked=Some\(\(2, true\)\)'
vin type "/"; vin type "zeta"; vin key Return; vin key l; settle 0.6; vin type "p"; settle 1
check "p moves what was cut" test -f "$V/zeta/beta.txt" -a -f "$V/zeta/delta.txt" -a ! -e "$V/beta.txt"
check "and nothing is yanked any more" has "$(state)" 'yanked=None'
vin key h; settle 0.6

echo "--- trash and delete"
vin type "/"; vin type "gamma"; vin key Return; settle 0.3
vin type "d"; settle 0.4; shot confirm
check "d asks before trashing" has "$(state)" 'confirm="Move this file to the trash can\?"'
vin key j; settle 0.2
check "the question takes the keys" cur gamma.txt
vin type "n"; settle 0.3
check "n says no" has "$(state)" 'confirm=none'
check "and the file stays" test -f "$V/gamma.txt"
vin type "d"; settle 0.3; vin type "y"; settle 1
check "y says yes: the file is trashed" test ! -e "$V/gamma.txt"
vin type "/"; vin type "made-over"; vin key Return; settle 0.3
vin type "D"; settle 0.4
check "D asks before deleting" has "$(state)" 'confirm="Delete this file permanently\?"'
vin key Escape; settle 0.3
check "Esc says no" test -f "$V/made-over.txt"
vin type "D"; settle 0.3; vin key Return; settle 1
check "Enter says yes: the file is deleted" test ! -e "$V/made-over.txt"
vin type "/"; vin type "alpha"; vin key Return; vin type "D"; settle 0.3
act go-to "'$V/sub'"; settle 0.6
check "going elsewhere withdraws the question" has "$(state)" 'confirm=none'
vin type "y"; settle 0.6; act go-to "'$V'"; settle 0.6
check "and a late yes deletes nothing" test -f "$V/alpha.txt"

echo "--- copying text, links, tabs"
vin type "/"; vin type "archive"; vin key Return; settle 0.3
vin type "cc"; settle 0.3
check "cc copies the path" has "$(state)" "notice=\"Copied: $V/Archive.tar.gz\""
vin type "cn"; settle 0.3
check "cn copies the name without its extension" has "$(state)" 'notice="Copied: Archive"'
vin type "cd"; settle 0.3
check "cd copies the folder's path" has "$(state)" "notice=\"Copied: $V\""
vin type "/"; vin type "link"; vin key Return; settle 0.3; vin type "gf"; settle 0.8
check "gf follows the link under the cursor" has "$(state)" "location=file://$V/sub .* cursor=\"inner\" "
vin key h; settle 0.6
vin type "tt"; settle 0.5
check "tt opens a tab on the same folder" has "$(state)" "location=file://$V .* tabs=2 "
vin type "tr"; settle 0.3; vin type "mine"; vin key Return; settle 0.4
check "tr names the tab" has "$(state)" 'title="mine"'
vin type "1"; settle 0.3
check "1 switches to the first tab" has "$(state)" "title=\"$(basename "$V")\""
vin type "]"; settle 0.3
check "] goes to the next" has "$(state)" 'title="mine"'
vin type "{"; settle 0.3; vin type "2"; settle 0.3
check "{ swaps the tab with the one before" has "$(state)" "title=\"$(basename "$V")\""
vin type "["; settle 0.3; vin key ctrl+c; settle 0.4
check "Ctrl+C closes the tab" has "$(state)" 'tabs=1 '

echo "--- columns and help"
vin type "gg"; settle 0.5
check "the parent column shows the folder above" has "$(state)" "parent=$(dirname "$V") "
check "the preview shows the folder under the cursor" has "$(state)" 'preview=folder:link '
vin type "/"; vin type "alpha"; vin key Return; settle 0.5
check "a text file is previewed as text" has "$(state)" 'preview=text:alpha.txt '
vin type "/"; vin type "archive"; vin key Return; settle 0.5
check "a file that is not text gets a card, and is not read" has "$(state)" 'preview=info:Archive.tar.gz '
vin key F8; settle 0.4
check "F8 hides the preview" has "$(state)" 'preview=off '
vin key F8 key F7; settle 0.4
check "F7 hides the parent column" has "$(state)" 'parent=off preview=info'
vin key F7; settle 0.3
vin type "~"; settle 0.6; shot help
check "~ opens the key list" has "$(state)" 'dialogs=\["Keys"\]'
vin key Escape; settle 0.4
check "Esc closes it" has "$(state)" 'toplevels=1 '
vin type "w"; settle 0.3
check "w with nothing running says so" has "$(state)" 'notice="No file operation is running"'

echo "--- the pointer"
# One vin call per gesture: modifiers are held only while it runs.
vin type "gg"; vin key Escape; vin move 500 154 click; settle 0.4
check "a click puts the cursor on the file and selects it" has "$(state)" 'cursor="sub" at=2 .* selected=1 names=\["sub"\]'
vin key j; settle 0.3
check "moving on from the keyboard drops a click's selection" has "$(state)" 'cursor="zeta" .* selected=0 '
vin move 500 94 click mods ctrl move 500 154 click mods none; settle 0.4
check "Ctrl+click builds a selection" has "$(state)" 'cursor="sub" .* selected=2 '
vin key j; settle 0.3
check "which moving on keeps" has "$(state)" 'cursor="zeta" .* selected=2 '
vin key Escape; settle 0.2

echo "--- typing in an entry is typing"
vin key ctrl+l; settle 0.3; vin type "jjGd"; settle 0.3
check "keys typed into the location entry are text" has "$(state)" 'focus=GtkText .* confirm=none .* entry="[^"]*jjGd"'
vin key Escape; settle 0.3
check "Esc returns to the files" has "$(state)" 'focus=GtkListItemWidget'
rm -rf "$V"
