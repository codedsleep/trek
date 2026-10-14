# Opening files (SCOPE 6.12–6.16). Run with:
#   scripts/agent-run.sh --env TERMINAL=$HOME/.cache/tallboy-agent/opening/bin/fake-term \
#       -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/opening.sh
# Applications are stand-ins that append their arguments to $OUT/launch.log.
F=$HOME/.cache/tallboy-fixture
work=$HOME/.cache/tallboy-agent/opening
rm -rf "$work"; mkdir -p "$work/bin" "$work/files" "$XDG_DATA_HOME/applications"
: >"$OUT/launch.log"
cat >"$work/bin/record" <<SH
#!/bin/sh
{ printf 'app=%s argc=%s cwd=%s\n' "\$TB_APP" "\$#" "\$PWD"; for a in "\$@"; do printf '  arg=[%s]\n' "\$a"; done; } >>"$OUT/launch.log"
SH
chmod +x "$work/bin/record"
for app in text image; do
    cat >"$XDG_DATA_HOME/applications/tb-$app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Stand-in $app
Exec=env TB_APP=$app $work/bin/record %F
MimeType=$([ $app = text ] && echo 'text/plain;application/x-shellscript;' || echo 'image/png;')
DESKTOP
done
mkdir -p "$XDG_CONFIG_HOME"
printf '[Default Applications]\ntext/plain=tb-text.desktop\napplication/x-shellscript=tb-text.desktop\nimage/png=tb-image.desktop\n' >"$XDG_CONFIG_HOME/mimeapps.list"
printf 'a\n' >"$work/files/one.txt"; printf 'b\n' >"$work/files/two.txt"
cp "$F/images/image-000.png" "$work/files/pic.png"
printf 'x\n' >"$work/files/; touch pwned"
printf '#!/bin/sh\ntouch "%s/script-ran"\n' "$work" >"$work/files/script.sh"; chmod +x "$work/files/script.sh"
# Bytes no content test takes for a known type. Random ones are, now and
# then: 64 from /dev/urandom have come out as text/x-matlab.
printf '\0\1\2\3tallboy\0\377\376\0\0' >"$work/files/data.tbunknown"
printf '[Desktop Entry]\nType=Application\nName=Loose launcher\nExec=touch %s/launcher-ran\n' "$work" >"$work/files/loose.desktop"
log() { cat "$OUT/launch.log"; }
loc() { state | grep -o 'location=[^ ]*'; }

only() { act filter; settle 0.4; vin key ctrl+a; vin type "$1"; vin key Return; settle 0.5; act select-all; }
act go-to "'$work/files'"; settle 0.8
check "focus is in the view" has "$(state)" 'focus=GtkListItemWidget'
echo "--- one file, by activation"
only "*pwned"; vin key Return; settle 1.2
check "a name with shell characters reaches the application as one argument" has "$(log)" "arg=\[$work/files/; touch pwned\]"
check "nothing in the name was executed" test ! -e "$work/files/pwned"

echo "--- three files of two types make two launches"
: >"$OUT/launch.log"
# one.txt, two.txt and pic.png together.
only "*.[tp][xn][tg]"; act open; settle 1.2
check "the text application got both text files in one launch" has "$(log)" 'app=text argc=2'
check "the image application got the picture" has "$(log)" 'app=image argc=1'
only "*"

echo "--- no default application leads to Open With"
: >"$OUT/launch.log"
only "*.tbunknown"; act open; settle 0.8; shot open-with
check "the Open With dialog is open" has "$(state)" 'toplevels=2'
vin key Escape; settle 0.6
check "Esc closes the Open With dialog" has "$(state)" 'toplevels=1'
check "and opens nothing" test ! -s "$OUT/launch.log"

echo "--- executable script asks first"
only "script*"; act open; settle 0.8; shot ask-script
check "nothing ran before the answer" test ! -e "$work/script-ran"
vin key Escape; settle 0.4
check "Cancel runs nothing" test ! -e "$work/script-ran"
only "loose*"; act open; settle 0.8; shot ask-launcher
vin key Escape; settle 0.5
check "an untrusted launcher does not run on Cancel" test ! -e "$work/launcher-ran"
only "script*"; act open; settle 0.8; vin move 900 424 click; settle 1   # the Execute button
check "Execute runs the script" test -e "$work/script-ran"
only "*"

echo "--- command line: a file and a folder, relative to the caller"
: >"$OUT/launch.log"
(cd "$work" && tallboy files/one.txt files) ; settle 1.2
check "the file went to its application" has "$(log)" "arg=\[$work/files/one.txt\]"
check "the folder opened as a second tab" has "$(state)" 'tabs=2'
vin key ctrl+w; settle 0.3
(cd "$work" && tallboy -n files); settle 1
check "-n opened a second window" test "$(WIN=2 state | grep -c location=)" = 1

echo "--- Open With sets the default application"
WIN=2 act close-window; settle 0.4
only "*.tbunknown"; act open; settle 0.8
vin type "stand-in t"; settle 0.4; vin key alt+s; settle 0.2; vin key Return; settle 1.2
check "the chosen application opened the file" has "$(log)" "arg=\[$work/files/data.tbunknown\]"
check "it is now the default for the type" grep -q 'application/octet-stream=tb-text.desktop' "$XDG_CONFIG_HOME/mimeapps.list"
check "gio agrees" has "$(gio mime application/octet-stream)" 'tb-text.desktop'
only "*"; vin key Escape

echo "--- terminal and Run a Command (needs --env TERMINAL=$work/bin/fake-term)"
printf '#!/bin/sh\npwd >"%s/terminal-cwd"\n' "$work" >"$work/bin/fake-term"; chmod +x "$work/bin/fake-term"
vin move 700 400 click; vin key F4; settle 1
check "F4 starts the terminal in the folder" test "$(cat "$work/terminal-cwd" 2>/dev/null)" = "$work/files"
act run-command; settle 0.5; vin type "touch ran-here"; vin key Return; settle 1
check "Run a Command runs in the current folder" test -e "$work/files/ran-here"
act run-command; settle 0.5; vin type "no-such-program-tb"; vin key Return; settle 0.6; shot run-error; vin key Escape; settle 0.3
