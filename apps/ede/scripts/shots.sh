#!/usr/bin/env bash
# One eDE shot (#EDE-11): the headless compositor draws the factory
# documents through the supervisor and the shell, exponential-ctl captures the
# output as the compositor composes it. The shots lane (`bun run shots --
# --platform ede`) runs this once per view; by hand:
#
#   apps/ede/scripts/shots.sh --out /tmp/panel.png [--popup NAME] [--open NAME] [--size 1280x720] [--settle 1500] [--no-build]
#
# `--popup NAME` opens a popup document (the menu, the notification centre)
# before the capture; `--open NAME` an on-demand layer document (the
# switcher, the OSD, the toasts).
# The shell runs with EDE_SHELL_DEMO=1: fixed sample apps and windows, so
# the pictures never depend on the machine.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$here/target}"
# The shell is a member of apps/desktop (#EDE-51): its own target/ unless CARGO_TARGET_DIR is shared.
desktop="$(dirname "$here")/desktop"
shell_target="${CARGO_TARGET_DIR:-$desktop/target}"
out=""; popup=(); open=(); size="1280x720"; settle=1500; build=1
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --popup) popup+=(--popup "$2"); shift 2 ;;
    --open) open+=(--open "$2"); shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --settle) settle="$2"; shift 2 ;;
    --no-build) build=0; shift ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "$out" ] || { echo "--out FILE is required" >&2; exit 2; }
if [ "$build" = 1 ]; then
  (cd "$here" && cargo build -q -p ede-compositor -p ede-ui -p ede-ctl)
  (cd "$desktop" && cargo build -q -p ede-shell)
fi
# A runtime dir of its own: the socket and token never collide with a
# desktop's, and a CI runner has no /run/user.
scratch="$(mktemp -d /tmp/ede-shot.XXXXXX)"
export XDG_RUNTIME_DIR="$scratch/run"; mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
export EDE_FACTORY_DIR="$here/ui"
# A golden never ticks.
export EDE_SHELL_CLOCK="${EDE_SHELL_CLOCK:-12:00}"
# Nor turns the page: the calendar applet draws this day, not the runner's.
export EDE_SHELL_DATE="${EDE_SHELL_DATE:-2026-11-08}"
export EDE_SHELL_DEMO=1
export RUST_LOG="${RUST_LOG:-info,smithay=warn,wgpu_core=error,wgpu_hal=error}"
log="$scratch/run.log"
"$target/debug/exponential-compositor" --headless "$size" --no-xwayland -- \
  "$target/debug/exponential-ui" --dir "$scratch/ui" --factory "$here/ui" supervise -- "$shell_target/debug/exponential-shell" "${popup[@]}" "${open[@]}" > "$log" 2>&1 &
pid=$!
cleanup() { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; rm -rf "$scratch"; }
trap cleanup EXIT
sock=""
for _ in $(seq 1 100); do
  sock="$(grep -o 'listening on wayland-[0-9]*' "$log" 2>/dev/null | head -1 | awk '{print $3}' || true)"
  [ -n "$sock" ] && break
  sleep 0.2
done
[ -n "$sock" ] || { echo "the compositor did not come up:" >&2; tail -20 "$log" >&2; exit 1; }
# The shell's first frame (the supervisor says Started, the shell says "surface opened"), then the settle.
for _ in $(seq 1 150); do
  grep -q 'surface opened' "$log" 2>/dev/null && break
  sleep 0.2
done
grep -q 'surface opened' "$log" || { echo "the shell drew nothing:" >&2; grep -v '^\s*$' "$log" | tail -30 >&2; exit 1; }
sleep "$(LC_ALL=C awk "BEGIN{printf \"%.3f\", $settle/1000}")"
mkdir -p "$(dirname "$out")"
WAYLAND_DISPLAY="$sock" "$target/debug/exponential-ctl" capture --png "$out" --expect-drawn
