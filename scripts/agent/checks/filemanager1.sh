# org.freedesktop.FileManager1 (SCOPE 9.7), on the private session bus. Run with:
#   scripts/agent-run.sh --env TALLBOY_FILEMANAGER1=1 -- --daemon-mode < scripts/agent/checks/filemanager1.sh
# Without the variable the agent profile must not own the name:
#   EXPECT_OWNED=0 scripts/agent-run.sh -- --daemon-mode < scripts/agent/checks/filemanager1.sh
F="$HOME/.cache/tallboy-fixture"
fm() {
    gdbus call --session --dest org.freedesktop.FileManager1 \
        --object-path /org/freedesktop/FileManager1 \
        --method "org.freedesktop.FileManager1.$1" "$2" "''" 2>&1
}
owner() {
    gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.NameHasOwner org.freedesktop.FileManager1
}
settle 0.8
if [ "${EXPECT_OWNED:-1}" = 0 ]; then
    check "a profile other than default does not take the name" has "$(owner)" 'false'
    exit $FAILED
fi
check "the name is owned" has "$(owner)" 'true'

echo "--- ShowItems with no window open"
fm ShowItems "['file://$F/deep/level-00/level-01/item.txt']" >/dev/null; settle 1.5; shot fm1-item
S=$(state)
check "a window opens on the item's folder" has "$S" "location=file://$F/deep/level-00/level-01 "
check "with the item selected" has "$S" 'selected=1 names=\["item.txt"\]'

echo "--- ShowItems in a large folder: the item is scrolled to"
fm ShowItems "['file://$F/big/file-25000.txt']" >/dev/null; settle 3; shot fm1-big
S=$(state)
check "a tab opens in the same window" has "$S" 'tabs=2 '
check "the item is selected" has "$S" 'selected=1 names=\["file-25000.txt"\]'
check "and the view has scrolled to it" test "$(echo "$S" | grep -o 'scroll=[0-9]*' | cut -d= -f2)" -gt 1000

echo "--- ShowFolders"
fm ShowFolders "['file://$F/names']" >/dev/null; settle 1
check "the folder opens in a tab" has "$(state)" "location=file://$F/names .*tabs=3 "

echo "--- ShowItemProperties"
fm ShowItemProperties "['file://$F/names/two%20words']" >/dev/null; settle 1; shot fm1-props
check "Properties opens" has "$(state)" 'dialogs=\["File Properties"\]'
vin key Escape; settle 0.4

echo "--- bad input"
check "a relative path is refused" has "$(fm ShowItems "['names/two words']")" 'InvalidArgs'
