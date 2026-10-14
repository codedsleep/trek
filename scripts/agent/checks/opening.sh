# Opening files (SCOPE 6.12–6.16). Run with:
#   scripts/agent-run.sh --env TERMINAL=$HOME/.cache/tallboy-agent/opening/bin/fake-term \
#       --env VISUAL=$HOME/.cache/tallboy-agent/opening/bin/fake-edit \
#       --env TALLBOY_HOST_XDG_CONFIG_HOME=$HOME/.cache/tallboy-agent/opening/host-config \
#       -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/opening.sh
# Applications, the terminal and the editor are stand-ins; the applications
# and the terminal append their arguments to $OUT/launch.log.
F=$HOME/.cache/tallboy-fixture
work=$HOME/.cache/tallboy-agent/opening
rm -rf "$work"; mkdir -p "$work/bin" "$work/files" "$XDG_DATA_HOME/applications"
: >"$OUT/launch.log"
cat >"$work/bin/record" <<SH
#!/bin/sh
{ printf 'app=%s argc=%s cwd=%s\n' "\$TB_APP" "\$#" "\$PWD"; for a in "\$@"; do printf '  arg=[%s]\n' "\$a"; done; } >>"$OUT/launch.log"
SH
# What a started program finds in its environment, apart from the launch log.
cat >>"$work/bin/record" <<SH
printf '%s config=%s host=%s\n' "\$TB_APP" "\$XDG_CONFIG_HOME" "\${TALLBOY_HOST_XDG_CONFIG_HOME-unset}" >>"$work/environment"
SH
# The terminal records what it was asked to run, and where it was started.
cat >"$work/bin/fake-term" <<SH
#!/bin/sh
pwd >"$work/terminal-cwd"
TB_APP=term exec "$work/bin/record" "\$@"
SH
printf '#!/bin/sh\n' >"$work/bin/fake-edit"
chmod +x "$work/bin/record" "$work/bin/fake-term" "$work/bin/fake-edit"
for app in text image; do
    cat >"$XDG_DATA_HOME/applications/tb-$app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Stand-in $app
Exec=env TB_APP=$app $work/bin/record %F
MimeType=$([ $app = text ] && echo 'text/plain;application/x-shellscript;' || echo 'image/png;image/svg+xml;')
DESKTOP
done
cat >"$XDG_DATA_HOME/applications/tb-console.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Console program
Exec=env TB_APP=console $work/bin/record %F
Terminal=true
DESKTOP
mkdir -p "$XDG_CONFIG_HOME"
printf '[Default Applications]\ntext/plain=tb-text.desktop\napplication/x-shellscript=tb-text.desktop\nimage/png=tb-image.desktop\nimage/svg+xml=tb-image.desktop\n' >"$XDG_CONFIG_HOME/mimeapps.list"
printf 'a\n' >"$work/files/one.txt"; printf 'b\n' >"$work/files/two.txt"
cp "$F/images/image-000.png" "$work/files/pic.png"
printf 'x\n' >"$work/files/; touch pwned"
printf '#!/bin/sh\ntouch "%s/script-ran"\n' "$work" >"$work/files/script.sh"; chmod +x "$work/files/script.sh"
printf '# notes\n' >"$work/files/notes.md"; printf 'export const a = 1;\n' >"$work/files/mod.ts"; : >"$work/files/blank"
printf '<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>\n' >"$work/files/drawing.svg"
printf '#include <fcntl.h>\nint main(void) { return creat("program-ran", 0644) < 0; }\n' | cc -x c - -o "$work/files/program"
# Bytes no content test takes for a known type. Random ones are, now and
# then: 64 from /dev/urandom have come out as text/x-matlab.
printf '\0\1\2\3tallboy\0\377\376\0\0' >"$work/files/data.tbunknown"
printf '[Desktop Entry]\nType=Application\nName=Loose launcher\nExec=touch %s/launcher-ran\n' "$work" >"$work/files/loose.desktop"
log() { cat "$OUT/launch.log"; }
loc() { state | grep -o 'location=[^ ]*'; }

only() { act filter; settle 0.4; vin key ctrl+a; vin type "$1"; vin key Return; settle 0.5; act select-all; }
act go-to "'$work/files'"; settle 0.8
check "focus is in the view" has "$(state)" 'focus=GtkListItemWidget'
echo "--- one text file, by activation, goes to the editor in the terminal"
only "*pwned"; vin key Return; settle 1.2
check "the terminal was asked to run the editor" has "$(log | tr -d '\n')" "app=term argc=3 cwd=$work/files  arg=\[-e\]  arg=\[$work/bin/fake-edit\]"
check "a name with shell characters reaches the editor as one argument" has "$(log)" "arg=\[$work/files/; touch pwned\]"
check "nothing in the name was executed" test ! -e "$work/files/pwned"
check "the default application for text was not asked" test "$(log | grep -c app=text)" = 0
check "the terminal got the settings folder from before the isolation" grep -qx "term config=$work/host-config host=unset" "$work/environment"

echo "--- three files of two kinds make two launches"
: >"$OUT/launch.log"
# one.txt, two.txt and pic.png together.
only "*.[tp][xn][tg]"; act open; settle 1.2
check "the editor got both text files in one terminal" has "$(log)" 'app=term argc=4'
check "the image application got the picture" has "$(log)" 'app=image argc=1'
check "so did the application" grep -qx "image config=$work/host-config host=unset" "$work/environment"

echo "--- what counts as text"
: >"$OUT/launch.log"
# notes.md, mod.ts and the empty file blank.
only "[nmb][ola]*"; act open; settle 1.2
check "Markdown, TypeScript and an empty file are edited" has "$(log)" 'app=term argc=5'
: >"$OUT/launch.log"
only "*.svg"; act open; settle 1.2
check "a picture written in XML goes to its application" has "$(log)" 'app=image argc=1'
check "and not to the editor" test "$(log | grep -c app=term)" = 0

echo "--- an application that needs a terminal gets tallboy's"
: >"$OUT/launch.log"
only "pic*"; act open-with-app "'tb-console.desktop'"; settle 1.2
check "the terminal was asked to run it with the file" has "$(log | tr -d '\n')" "app=term argc=5 .*arg=\[-e\]  arg=\[env\]  arg=\[TB_APP=console\]  arg=\[$work/bin/record\]  arg=\[$work/files/pic.png\]"
only "*"

echo "--- no default application leads to Open With"
: >"$OUT/launch.log"
only "*.tbunknown"; act open; settle 0.8; shot open-with
check "the Open With dialog is open" has "$(state)" 'toplevels=2'
vin key Escape; settle 0.6
check "Esc closes the Open With dialog" has "$(state)" 'toplevels=1'
check "and opens nothing" test ! -s "$OUT/launch.log"

echo "--- an executable script is edited, an executable program asks first"
: >"$OUT/launch.log"
only "script*"; act open; settle 1.2
check "the script went to the editor" has "$(log)" "arg=\[$work/files/script.sh\]"
check "without a question" has "$(state)" 'toplevels=1'
check "and did not run" test ! -e "$work/script-ran"
only "program"; act open; settle 0.8; shot ask-program
check "nothing ran before the answer" test ! -e "$work/files/program-ran"
vin key Escape; settle 0.4
check "Cancel runs nothing" test ! -e "$work/files/program-ran"
only "loose*"; act open; settle 0.8; shot ask-launcher
vin key Escape; settle 0.5
check "an untrusted launcher does not run on Cancel" test ! -e "$work/launcher-ran"
only "program"; act open; settle 0.8; vin move 816 415 click; settle 1   # the Execute button
check "Execute runs the program" test -e "$work/files/program-ran"
rm -f "$work/files/program-ran"
only "*"

echo "--- command line: a file and a folder, relative to the caller"
: >"$OUT/launch.log"
(cd "$work" && tallboy files/one.txt files) ; settle 1.2
check "the file went to the editor" has "$(log)" "arg=\[$work/files/one.txt\]"
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
rm -f "$work/terminal-cwd"
vin move 700 400 click; vin key F4; settle 1
check "F4 starts the terminal in the folder" test "$(cat "$work/terminal-cwd" 2>/dev/null)" = "$work/files"
act run-command; settle 0.5; vin type "touch ran-here"; vin key Return; settle 1
check "Run a Command runs in the current folder" test -e "$work/files/ran-here"
act run-command; settle 0.5; vin type "no-such-program-tb"; vin key Return; settle 0.6; shot run-error; vin key Escape; settle 0.3
