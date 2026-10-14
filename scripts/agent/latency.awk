#!/usr/bin/awk -f
# Turns a WAYLAND_DEBUG=1 client log into one row per input event:
#
#   LABEL <tab> ms to the first frame <tab> ms to the last frame <tab> frames
#
# A frame is a commit of the application's first toplevel surface with a new
# buffer attached, so it counts the same thing for GTK 3 and GTK 4. An input
# event is a key press (modifier keys aside) or a wheel step, and its frames
# are those committed before the next input event or MARK line.
#
# scripts/agent/latency.sh adds two kinds of line to the log:
#   T0 HH:MM:SS.micro LABEL   the moment before exec; opens an event of its own
#   MARK [LABEL...]           labels for the following input events, in turn
#                             and repeating; a bare MARK labels nothing

# Milliseconds since midnight of "[HH:MM:SS.micro]", carried across midnight.
function clock(stamp,    part, t) {
    split(stamp, part, /[\[\]:.]/)
    t = ((part[2] * 60 + part[3]) * 60 + part[4]) * 1000 + part[5] / 1000 + day
    if (t + 43200000 < latest) {
        day += 86400000
        t += 86400000
    }
    if (t > latest)
        latest = t
    return t
}

# The digits that follow `key` in `line`.
function id_after(line, key,    at, rest) {
    at = index(line, key)
    if (!at)
        return ""
    rest = substr(line, at + length(key))
    sub(/[^0-9].*/, "", rest)
    return rest
}

function close_event() {
    if (!open)
        return
    if (frames)
        printf "%s\t%.1f\t%.1f\t%d\n", label, first - began, last - began, frames
    else
        printf "%s\t-\t-\t0\n", label
    open = 0
}

function open_event(name, t) {
    close_event()
    open = 1
    label = name
    began = t
    frames = 0
}

$1 == "T0" {
    open_event($3, clock("[" $2 "]"))
    next
}

$1 == "MARK" {
    close_event()
    count = NF - 1
    for (i = 2; i <= NF; i++)
        names[i - 2] = $i
    turn = 0
    next
}

!/^\[/ { next }

{ t = clock($1) }

index($0, ".get_xdg_surface(") {
    surface_of[id_after($0, "new id xdg_surface#")] = id_after($0, ", wl_surface#")
    next
}

main == "" && index($0, ".get_toplevel(") {
    main = surface_of[id_after($0, "-> xdg_surface#")]
    next
}

main != "" && index($0, "-> wl_surface#" main ".attach(wl_buffer#") {
    attached = 1
    next
}

main != "" && attached && index($0, "-> wl_surface#" main ".commit()") {
    attached = 0
    if (open) {
        if (!frames)
            first = t
        last = t
        frames++
    }
    next
}

# wl_keyboard#N.key(serial, time, key, state): presses only; 29, 42 and 56 are
# Ctrl, Shift and Alt.
count && / wl_keyboard#[0-9]+\.key\(/ {
    split(substr($0, index($0, ".key(") + 5), arg, /[, )]+/)
    if (arg[4] == 1 && arg[3] != 29 && arg[3] != 42 && arg[3] != 56)
        open_event(names[turn++ % count], t)
    next
}

# One wheel step arrives as several axis events within a millisecond.
count && / wl_pointer#[0-9]+\.axis\(/ {
    if (t - wheel > 20)
        open_event(names[turn++ % count], t)
    wheel = t
}

END { close_event() }
