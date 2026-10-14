#!/usr/bin/env bash
# The suite must reject a crashed session even after PASS output, and toolkit diagnostics.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/scripts/agent/checks" "$scratch/home"
cp "$root/scripts/agent/checks/run-all.sh" "$scratch/scripts/agent/checks/run-all.sh"
: >"$scratch/scripts/agent/checks/navigation.sh"
cat >"$scratch/scripts/agent-run.sh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
while [[ $# -gt 0 ]]; do
    case $1 in --out) out=$2; shift 2;; *) shift;; esac
done
mkdir -p "$out"
: >"$out/trek.log"
: >"$out/session.log"
[[ ${HARNESS_WARNING:-0} == 0 ]] || echo 'Gtk-WARNING **: simulated regression' >"$out/trek.log"
echo 'PASS assertion before session exit'
exit "${HARNESS_STATUS:-0}"
STUB
chmod +x "$scratch/scripts/agent-run.sh"
run() {
    HOME="$scratch/home" HARNESS_STATUS=$1 HARNESS_WARNING=$2 \
        bash "$scratch/scripts/agent/checks/run-all.sh" navigation >"$scratch/result"
}
run 0 0
if run 7 0; then echo 'FAIL: session status 7 was ignored' >&2; exit 1; fi
rg -q 'session exited with status 7' "$scratch/result"
if run 0 1; then echo 'FAIL: GTK warning was ignored' >&2; exit 1; fi
rg -q 'toolkit warning or critical' "$scratch/result"
echo 'Agent harness: clean session accepted; failed exit and GTK warning rejected'
