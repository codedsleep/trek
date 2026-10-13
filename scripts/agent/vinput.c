/* vinput — keyboard and pointer injection for headless agent runs.
 *
 * Talks to the compositor started by scripts/agent-run.sh (cage, wlroots)
 * through virtual-keyboard-unstable-v1 and wlr-virtual-pointer-unstable-v1.
 * It only ever connects to $WAYLAND_DISPLAY, which inside agent-run.sh is the
 * private headless session, never the owner's desktop.
 *
 * Commands are executed in order:
 *   key COMBO      press and release, e.g. Delete, ctrl+c, shift+Delete, alt+Left
 *   mods LIST      hold modifiers for the following pointer commands, e.g.
 *                  "mods ctrl", "mods ctrl+shift"; "mods none" releases them
 *   type TEXT      type ASCII text
 *   move X Y       absolute pointer position in output pixels
 *   down [BTN]     press left (default), right or middle
 *   up [BTN]       release it
 *   click [BTN]    down + up
 *   dblclick       two left clicks
 *   drag X Y       move there in steps with the button already held
 *   scroll DY      vertical wheel, positive scrolls down
 *   sleep MS
 */
#define _GNU_SOURCE
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

#include "virtual-keyboard-unstable-v1-client-protocol.h"
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

static const char KEYMAP[] =
    "xkb_keymap {\n"
    "  xkb_keycodes { include \"evdev+aliases(qwerty)\" };\n"
    "  xkb_types { include \"complete\" };\n"
    "  xkb_compat { include \"complete\" };\n"
    "  xkb_symbols { include \"pc+us+inet(evdev)\" };\n"
    "};\n";

enum { MOD_SHIFT = 1, MOD_CTRL = 4, MOD_ALT = 8 };

struct named_key {
    const char *name;
    int code;
};

/* evdev key codes */
static const struct named_key KEYS[] = {
    {"Escape", 1},     {"BackSpace", 14}, {"Tab", 15},      {"Return", 28},
    {"space", 57},     {"F1", 59},        {"F2", 60},       {"F3", 61},
    {"F4", 62},        {"F5", 63},        {"F6", 64},       {"F7", 65},
    {"F8", 66},        {"F9", 67},        {"F10", 68},      {"F11", 87},
    {"F12", 88},       {"Home", 102},     {"Up", 103},      {"Page_Up", 104},
    {"Left", 105},     {"Right", 106},    {"End", 107},     {"Down", 108},
    {"Page_Down", 109}, {"Insert", 110},  {"Delete", 111},  {"Menu", 127},
    {"minus", 12},     {"equal", 13},     {"slash", 53},    {"period", 52},
    {"comma", 51},     {"KP_Add", 78},    {"KP_Subtract", 74},
    {NULL, 0},
};

static const char ROW_DIGITS[] = "1234567890";
static const char SHIFT_DIGITS[] = "!@#$%^&*()";
static const char *ROWS[] = {"qwertyuiop", "asdfghjkl", "zxcvbnm"};
static const int ROW_BASE[] = {16, 30, 44};

static struct wl_display *display;
static struct wl_seat *seat;
static struct zwp_virtual_keyboard_manager_v1 *kbd_manager;
static struct zwlr_virtual_pointer_manager_v1 *ptr_manager;
static struct zwp_virtual_keyboard_v1 *kbd;
static struct zwlr_virtual_pointer_v1 *ptr;
static uint32_t extent_w = 1280, extent_h = 720;
static double cur_x, cur_y;

static uint32_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static void pause_ms(int ms) {
    wl_display_roundtrip(display);
    usleep((useconds_t)ms * 1000);
}

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
                            const char *interface, uint32_t version) {
    (void)data;
    (void)version;
    if (!strcmp(interface, wl_seat_interface.name) && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    else if (!strcmp(interface, zwp_virtual_keyboard_manager_v1_interface.name))
        kbd_manager = wl_registry_bind(registry, name,
                                       &zwp_virtual_keyboard_manager_v1_interface, 1);
    else if (!strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name))
        ptr_manager = wl_registry_bind(registry, name,
                                       &zwlr_virtual_pointer_manager_v1_interface, 1);
}

static void registry_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {registry_global, registry_remove};

/* Returns the evdev code for one ASCII character; sets *shift when needed. */
static int char_key(char c, int *shift) {
    *shift = 0;
    if (isupper((unsigned char)c)) {
        *shift = 1;
        c = (char)tolower((unsigned char)c);
    }
    for (int r = 0; r < 3; r++) {
        const char *p = strchr(ROWS[r], c);
        if (p && c)
            return ROW_BASE[r] + (int)(p - ROWS[r]);
    }
    const char *d = strchr(ROW_DIGITS, c);
    if (d && c)
        return 2 + (int)(d - ROW_DIGITS);
    d = strchr(SHIFT_DIGITS, c);
    if (d && c) {
        *shift = 1;
        return 2 + (int)(d - SHIFT_DIGITS);
    }
    switch (c) {
    case ' ': return 57;
    case '-': return 12;
    case '_': *shift = 1; return 12;
    case '=': return 13;
    case '+': *shift = 1; return 13;
    case '/': return 53;
    case '?': *shift = 1; return 53;
    case '.': return 52;
    case ',': return 51;
    case ';': return 39;
    case ':': *shift = 1; return 39;
    case '~': *shift = 1; return 41;
    case '\'': return 40;
    case '[': return 26;
    case ']': return 27;
    case '{': *shift = 1; return 26;
    case '}': *shift = 1; return 27;
    case '\n': return 28;
    default: return -1;
    }
}

static void send_mods(uint32_t mods) {
    zwp_virtual_keyboard_v1_modifiers(kbd, mods, 0, 0, 0);
}

static void tap(int code, uint32_t mods) {
    static const struct { uint32_t mask; int code; } MODKEYS[] = {
        {MOD_CTRL, 29}, {MOD_SHIFT, 42}, {MOD_ALT, 56}};
    for (int i = 0; i < 3; i++)
        if (mods & MODKEYS[i].mask)
            zwp_virtual_keyboard_v1_key(kbd, now_ms(), (uint32_t)MODKEYS[i].code, 1);
    send_mods(mods);
    zwp_virtual_keyboard_v1_key(kbd, now_ms(), (uint32_t)code, 1);
    pause_ms(15);
    zwp_virtual_keyboard_v1_key(kbd, now_ms(), (uint32_t)code, 0);
    for (int i = 2; i >= 0; i--)
        if (mods & MODKEYS[i].mask)
            zwp_virtual_keyboard_v1_key(kbd, now_ms(), (uint32_t)MODKEYS[i].code, 0);
    send_mods(0);
    pause_ms(25);
}

static uint32_t held_mods;

/* Holds or releases modifier keys so that pointer events carry them. */
static int hold_mods(const char *list) {
    static const struct { const char *name; uint32_t mask; int code; } MODS[] = {
        {"ctrl", MOD_CTRL, 29}, {"shift", MOD_SHIFT, 42}, {"alt", MOD_ALT, 56}};
    uint32_t wanted = 0;
    if (strcmp(list, "none")) {
        char buf[64];
        snprintf(buf, sizeof buf, "%s", list);
        for (char *name = strtok(buf, "+"); name; name = strtok(NULL, "+")) {
            int found = 0;
            for (int i = 0; i < 3; i++)
                if (!strcmp(name, MODS[i].name)) {
                    wanted |= MODS[i].mask;
                    found = 1;
                }
            if (!found) {
                fprintf(stderr, "vinput: unknown modifier %s\n", name);
                return 1;
            }
        }
    }
    for (int i = 0; i < 3; i++) {
        uint32_t mask = MODS[i].mask;
        if ((wanted & mask) != (held_mods & mask))
            zwp_virtual_keyboard_v1_key(kbd, now_ms(), (uint32_t)MODS[i].code,
                                        (wanted & mask) ? 1 : 0);
    }
    held_mods = wanted;
    send_mods(held_mods);
    pause_ms(30);
    return 0;
}

static int key_combo(const char *combo) {
    char buf[64];
    snprintf(buf, sizeof buf, "%s", combo);
    uint32_t mods = 0;
    char *name = buf;
    for (char *plus; (plus = strchr(name, '+')) && plus[1];) {
        *plus = 0;
        if (!strcmp(name, "ctrl")) mods |= MOD_CTRL;
        else if (!strcmp(name, "shift")) mods |= MOD_SHIFT;
        else if (!strcmp(name, "alt")) mods |= MOD_ALT;
        else {
            fprintf(stderr, "vinput: unknown modifier %s\n", name);
            return 1;
        }
        name = plus + 1;
    }
    for (const struct named_key *k = KEYS; k->name; k++)
        if (!strcmp(k->name, name)) {
            tap(k->code, mods);
            return 0;
        }
    if (strlen(name) == 1) {
        int shift;
        int code = char_key(name[0], &shift);
        if (code >= 0) {
            tap(code, mods | (shift ? MOD_SHIFT : 0));
            return 0;
        }
    }
    fprintf(stderr, "vinput: unknown key %s\n", name);
    return 1;
}

static void pointer_to(double x, double y) {
    cur_x = x;
    cur_y = y;
    zwlr_virtual_pointer_v1_motion_absolute(ptr, now_ms(), (uint32_t)x, (uint32_t)y,
                                            extent_w, extent_h);
    zwlr_virtual_pointer_v1_frame(ptr);
    pause_ms(12);
}

static uint32_t button_code(const char *name) {
    if (name && !strcmp(name, "right")) return 0x111;
    if (name && !strcmp(name, "middle")) return 0x112;
    return 0x110;
}

static void button(uint32_t code, int pressed) {
    zwlr_virtual_pointer_v1_button(ptr, now_ms(), code,
                                   pressed ? WL_POINTER_BUTTON_STATE_PRESSED
                                           : WL_POINTER_BUTTON_STATE_RELEASED);
    zwlr_virtual_pointer_v1_frame(ptr);
    pause_ms(30);
}

static int is_button(const char *s) {
    return s && (!strcmp(s, "left") || !strcmp(s, "right") || !strcmp(s, "middle"));
}

int main(int argc, char **argv) {
    const char *env = getenv("VINPUT_EXTENT");
    if (env)
        sscanf(env, "%ux%u", &extent_w, &extent_h);
    display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "vinput: no Wayland display\n");
        return 1;
    }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!seat || !kbd_manager || !ptr_manager) {
        fprintf(stderr, "vinput: compositor lacks virtual keyboard or pointer support\n");
        return 1;
    }
    kbd = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(kbd_manager, seat);
    ptr = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(ptr_manager, seat);
    int fd = memfd_create("vinput-keymap", MFD_CLOEXEC);
    if (fd < 0 || write(fd, KEYMAP, sizeof KEYMAP) != (ssize_t)sizeof KEYMAP) {
        perror("vinput: keymap");
        return 1;
    }
    zwp_virtual_keyboard_v1_keymap(kbd, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, sizeof KEYMAP);
    pause_ms(60);

    int status = 0;
    for (int i = 1; i < argc && !status; i++) {
        const char *cmd = argv[i];
        const char *a = i + 1 < argc ? argv[i + 1] : NULL;
        const char *b = i + 2 < argc ? argv[i + 2] : NULL;
        if (!strcmp(cmd, "key") && a) {
            status = key_combo(a);
            i++;
        } else if (!strcmp(cmd, "mods") && a) {
            status = hold_mods(a);
            i++;
        } else if (!strcmp(cmd, "type") && a) {
            for (const char *c = a; *c && !status; c++) {
                int shift;
                int code = char_key(*c, &shift);
                if (code < 0) {
                    fprintf(stderr, "vinput: cannot type '%c'\n", *c);
                    status = 1;
                } else {
                    tap(code, shift ? MOD_SHIFT : 0);
                }
            }
            i++;
        } else if (!strcmp(cmd, "move") && a && b) {
            pointer_to(atof(a), atof(b));
            i += 2;
        } else if (!strcmp(cmd, "drag") && a && b) {
            double tx = atof(a), ty = atof(b), sx = cur_x, sy = cur_y;
            for (int step = 1; step <= 12; step++)
                pointer_to(sx + (tx - sx) * step / 12.0, sy + (ty - sy) * step / 12.0);
            i += 2;
        } else if (!strcmp(cmd, "down") || !strcmp(cmd, "up") || !strcmp(cmd, "click")) {
            uint32_t code = button_code(is_button(a) ? a : NULL);
            if (strcmp(cmd, "up")) button(code, 1);
            if (strcmp(cmd, "down")) button(code, 0);
            if (is_button(a)) i++;
        } else if (!strcmp(cmd, "dblclick")) {
            for (int n = 0; n < 2; n++) {
                button(0x110, 1);
                button(0x110, 0);
            }
        } else if (!strcmp(cmd, "scroll") && a) {
            zwlr_virtual_pointer_v1_axis(ptr, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL,
                                         wl_fixed_from_double(atof(a) * 15.0));
            zwlr_virtual_pointer_v1_frame(ptr);
            pause_ms(30);
            i++;
        } else if (!strcmp(cmd, "sleep") && a) {
            pause_ms(atoi(a));
            i++;
        } else {
            fprintf(stderr, "vinput: bad command %s\n", cmd);
            status = 1;
        }
    }
    if (held_mods)
        hold_mods("none");
    pause_ms(40);
    zwp_virtual_keyboard_v1_destroy(kbd);
    zwlr_virtual_pointer_v1_destroy(ptr);
    wl_display_roundtrip(display);
    wl_display_disconnect(display);
    return status;
}
