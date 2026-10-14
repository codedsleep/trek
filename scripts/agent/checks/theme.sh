# The desktop's GTK theme (SCOPE §10, deviation 10). A theme is read once, at
# start-up, so run-all.sh makes the themes and starts a session for each:
#   THEME_CASE=gtk3 scripts/agent-run.sh --env TALLBOY_GTK_THEME=TallboyThree \
#       -- ~/.cache/tallboy-fixture/names < scripts/agent/checks/theme.sh
# Colours are read off the screen: the empty part of the folder area, the
# cursor's row and the status line.
pixel() { grim -g "$1,$2 1x1" -t ppm - | tail -c 3 | od -An -tu1 | xargs; }
settle 0.8
area=$(pixel 500 500) cursor=$(pixel 500 93) status=$(pixel 640 708)
shot "theme-$THEME_CASE"
case $THEME_CASE in
    gtk3)
        echo "--- a theme with a GTK 3 part only: its colours, on GTK's own stylesheet"
        check "the folder area has the theme's base colour" test "$area" = "16 32 48"
        check "the window has the theme's background colour" test "$status" = "32 48 64"
        check "the cursor has the theme's selection colour" test "$cursor" = "192 128 64"
        act preferences; settle 0.8
        check "a dialog has the background colour too" test "$(pixel 700 700)" = "32 48 64"
        ;;
    gtk4)
        echo "--- a theme with a GTK 4 part and no dark variant: used as it is"
        check "the folder area is as the theme's stylesheet has it" test "$area" = "64 32 48"
        # A stylesheet of one rule has no padding: the row is higher up.
        check "the cursor has the colour the theme names" test "$(pixel 500 58)" = "64 128 192"
        ;;
    light)
        echo "--- a light GTK 3 theme with no dark variant: light"
        check "the folder area has the theme's base colour" test "$area" = "255 255 255"
        check "the window has the theme's background colour" test "$status" = "240 236 228"
        ;;
    unreadable)
        echo "--- a GTK 3 theme whose colours cannot be read: GTK's own dark look"
        check "the folder area is GTK's" test "$area" = "45 45 45"
        check "the cursor is GTK's" test "$cursor" = "21 83 158"
        ;;
    missing)
        echo "--- a theme that is not installed: GTK's own dark look"
        check "the folder area is GTK's" test "$area" = "45 45 45"
        check "the cursor is GTK's" test "$cursor" = "21 83 158"
        ;;
esac
check "nothing was logged as a warning" test -z "$(grep -E 'WARNING|CRITICAL' "$OUT/tallboy.log")"
