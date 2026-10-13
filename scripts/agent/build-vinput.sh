#!/usr/bin/env bash
# Builds the agent input helper into target/agent/vinput. Needs cc,
# wayland-scanner and the wayland-client headers (pulled in by gtk4-devel).
set -euo pipefail
src="$(cd "$(dirname "$0")" && pwd)"
out="$src/../../target/agent"
mkdir -p "$out"
if [[ -x $out/vinput && $out/vinput -nt $src/vinput.c ]]; then exit 0; fi
for proto in virtual-keyboard-unstable-v1 wlr-virtual-pointer-unstable-v1; do
    wayland-scanner client-header "$src/$proto.xml" "$out/$proto-client-protocol.h"
    wayland-scanner private-code "$src/$proto.xml" "$out/$proto-protocol.c"
done
# shellcheck disable=SC2046
cc -O1 -Wall -Wextra -I"$out" -o "$out/vinput" "$src/vinput.c" \
    "$out"/virtual-keyboard-unstable-v1-protocol.c \
    "$out"/wlr-virtual-pointer-unstable-v1-protocol.c \
    $(pkg-config --cflags --libs wayland-client)
