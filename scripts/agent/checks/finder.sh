# The fuzzy finder (`z`), through real key presses. Run with:
#   scripts/agent-run.sh -- ~/.cache/trek-fixture/names < scripts/agent/checks/finder.sh
# It only reads; the tree it searches is made here, inside the allowed roots.
F=$(mktemp -d "$HOME/.cache/trek-agent/finder-XXXXXX")
mkdir -p "$F/src/window" "$F/src/view" "$F/docs" "$F/.cache" "$F/build/out" "$F/many"
for f in src/main.rs src/window/mod.rs src/window/vim.rs src/view/list.rs docs/guide.md \
    README.md Makefile .cache/secret.txt build/out/app.bin; do printf 'x\n' >"$F/$f"; done
for n in $(seq -w 1 300); do : >"$F/many/file-$n.txt"; done
# Outside a repository a .gitignore is only a file; .ignore always counts.
printf 'build/\n' >"$F/.ignore"
fz() { has "$(state)" " finder=$1 "; }
at() { has "$(state)" "location=file://$1 "; }
cur() { has "$(state)" " cursor=\"$1\" "; }
act go-to "'$F'"; settle 0.8

echo "--- opening and closing"
vin type "z"; settle 0.6; shot finder-empty
# 5 folders, 7 files and the 300 of many/; not .cache, .ignore or build/.
check "z opens the finder on everything below the folder" fz '"":312/312:settled:"docs/"'
check "and gives it the keyboard" has "$(state)" 'focus=GtkText '
vin type "jk"; settle 0.4
check "keys are the query's, not commands" has "$(state)" 'cursor="build" .* finder="jk":'
vin key Escape; settle 0.3
check "Esc closes it and gives the files the keyboard" has "$(state)" "location=file://$F .* cursor=\"build\" .* focus=GtkListItemWidget input=none finder=none "

echo "--- finding"
vin type "z"; settle 0.4; vin type "wvim"; settle 0.5; shot finder-query
check "a query matches letters in order, anywhere in the path" fz '"wvim":1/312:settled:"src/window/vim.rs"'
vin key Return; settle 0.8
check "Enter goes to the file's folder" at "$F/src/window"
check "with the cursor on the file" cur vim.rs
check "and the finder is closed" has "$(state)" 'focus=GtkListItemWidget input=none finder=none '
vin type "z"; settle 0.4; vin type "mod"; settle 0.4; vin key Return; settle 0.5
check "a file in the folder shown just gets the cursor" has "$(state)" "location=file://$F/src/window .* cursor=\"mod.rs\" "
act go-to "'$F'"; settle 0.8
vin type "z"; settle 0.4; vin type "view/"; settle 0.4
check "folders are listed with a slash" fz '"view/":2/312:settled:"src/view/"'
vin key Return; settle 0.8
check "Enter on a folder enters it" at "$F/src/view"
act go-to "'$F'"; settle 0.8
vin type "z"; settle 0.4; vin type "MAKE"; settle 0.4
check "capitals are matched as written" fz '"MAKE":0/312:settled:""'
vin key Return; settle 0.3
check "Enter without a match does nothing" fz '"MAKE":0/312:settled:""'
vin key ctrl+u; vin type "make"; settle 0.4
check "Ctrl+U empties the query; without capitals case is ignored" fz '"make":1/312:settled:"Makefile"'
vin key ctrl+u; vin type "rs\$ !vim win"; settle 0.4
check "words combine in any order, with fzf's end\$ and !not" fz '"rs\$ !vim win":1/312:settled:"src/window/mod.rs"'
vin key Escape; settle 0.3

echo "--- the list"
vin type "z"; settle 0.4; vin type "'file-00"; settle 0.4
check "a quoted word is matched exactly" fz "\"'file-00\":9/312:settled:\"many/file-001.txt\""
vin key Down key Down; settle 0.3
check "Down moves the cursor" fz '[^ ]*:"many/file-003.txt"'
vin key ctrl+p; settle 0.3
check "Ctrl+P moves it up" fz '[^ ]*:"many/file-002.txt"'
vin key ctrl+j key ctrl+n key ctrl+k; settle 0.3
check "Ctrl+J, Ctrl+N and Ctrl+K move it too" fz '[^ ]*:"many/file-003.txt"'
vin key Up key Up key Up key Up; settle 0.3
check "it stops at the top" fz '[^ ]*:"many/file-001.txt"'
vin key ctrl+n; vin key Return; settle 0.8
check "Enter goes to the row under the cursor" has "$(state)" "location=file://$F/many .* cursor=\"file-002.txt\" "
act go-to "'$F'"; settle 0.8
vin type "z"; settle 0.4; vin key Page_Down key Page_Down; settle 0.4; shot finder-scrolled
check "Page Down scrolls past what fits" fz '"":312/312:settled:"many/file-027.txt"'
vin key Page_Up key Page_Up key Page_Up; settle 0.3
check "Page Up scrolls back" fz '"":312/312:settled:"docs/"'
vin key Escape; settle 0.3

echo "--- hidden files, filters, other places"
vin type "."; settle 0.5; vin type "z"; settle 0.5
check "with hidden files shown they are searched too" fz '"":315/315:settled:".cache/"'
vin type "secret"; settle 0.4
check "inside hidden folders as well" fz '"secret":1/315:settled:".cache/secret.txt"'
vin key Escape; settle 0.3; vin type "."; settle 0.5
vin type "f"; settle 0.3; vin type "read"; vin key Return; settle 0.4
vin type "z"; settle 0.4; vin type "makef"; settle 0.4; vin key Return; settle 0.6
check "a file the f filter hides is shown again when jumped to" has "$(state)" 'cursor="Makefile" .* needle="" .* visible=6 '
vin type "z"; settle 0.4; vin move 250 154 click; settle 0.4
check "a click outside it closes the finder" has "$(state)" 'cursor="many" .* finder=none '
vin key Escape; settle 0.3
vin type "z"; settle 0.4; vin type "src"; settle 0.4; shot finder-rows
vin move 700 248 click; settle 0.8
check "a click on a row goes there" has "$(state)" "location=file://$F/src .* cursor=\"main.rs\" .* finder=none "
act go-to "'$F'"; settle 0.8
act go-to "'trash:///'"; settle 0.8
vin type "z"; settle 0.4
check "a place that is not a local folder says so" has "$(state)" 'finder=none .* notice="Only folders on this computer can be searched"'
vin type "H"; settle 0.6
vin type "z"; settle 0.4; act go-to "'$F/docs'"; settle 0.8
check "going elsewhere closes the finder" has "$(state)" "location=file://$F/docs .* finder=none "
rm -rf "$F"
