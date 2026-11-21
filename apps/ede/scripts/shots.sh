#!/usr/bin/env bash
# One eDE shot (#EDE-11): the headless compositor draws the factory
# documents through the supervisor and the shell, exponential-ctl captures the
# output as the compositor composes it. The shots lane (`bun run shots --
# --platform ede`) runs this once per view; by hand:
#
#   apps/ede/scripts/shots.sh --out /tmp/panel.png [--popup NAME] [--open NAME] [--size 1280x720] [--settle 1500] [--ready-timeout 120] [--no-build]
#
# `--popup NAME` opens a popup document (the menu, the notification centre)
# before the capture; `--open NAME` an on-demand layer document (the
# switcher, the OSD, the toasts). The capture waits until the shell drew
# them and every surface it opened (#EDE-106), then the settle.
# The shell runs with EDE_SHELL_DEMO=1: fixed sample apps and windows, so
# the pictures never depend on the machine.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$here/target}"
# The shell is a member of apps/desktop (#EDE-51): its own target/ unless CARGO_TARGET_DIR is shared.
desktop="$(dirname "$here")/desktop"
shell_target="${CARGO_TARGET_DIR:-$desktop/target}"
out=""; popup=(); open=(); size="1280x720"; settle=1500; ready_timeout=120; build=1
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --popup) popup+=(--popup "$2"); shift 2 ;;
    --open) open+=(--open "$2"); shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --settle) settle="$2"; shift 2 ;;
    --ready-timeout) ready_timeout="$2"; shift 2 ;;
    --no-build) build=0; shift ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
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
# Nor reads the machine's fonts (#EDE-140): the shell draws with the fonts it
# embeds, and fontconfig is shown DejaVu alone, the family every runner image
# has (gpui's system-font fallback, a menu's section label). A system Inter
# (fonts-inter, another build of it) took the embedded one's place on a
# developer's machine: 0.03 to 0.2 % of the pixels, at glyph edges.
fonts="${EDE_SHOTS_FONTS:-/usr/share/fonts/truetype/dejavu}"
[ -d "$fonts" ] || { echo "the goldens' fallback font is missing: $fonts (apt install fonts-dejavu-core, or EDE_SHOTS_FONTS=<the directory of DejaVu Sans>)" >&2; exit 1; }
printf '<?xml version="1.0"?>\n<fontconfig><dir>%s</dir><cachedir>%s</cachedir></fontconfig>\n' "$fonts" "$scratch/fontcache" > "$scratch/fonts.conf"
export FONTCONFIG_FILE="$scratch/fonts.conf"
export RUST_LOG="${RUST_LOG:-info,smithay=warn,wgpu_core=error,wgpu_hal=error}"
log="$scratch/run.log"
# #EXP-2024: the pinned space view is black in every redacted capture of the
# person's output; its golden (the view's env) asks for the unredacted one.
debug=()
[ "${EDE_SHELL_SHOT_UNREDACTED:-}" = 1 ] && debug=(--debug-captures)
"$target/debug/exponential-compositor" --headless "$size" --no-xwayland "${debug[@]}" -- \
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
# #EDE-106: wait for what the view needs to be drawn, never for time (a loaded
# runner captured the empty desktop): every surface the shell opened and every
# document asked for logs "drawn" once its first frame is presented. Nothing
# within --ready-timeout fails the shot, it never captures a half-drawn desktop.
need=()
for ((i = 1; i < ${#popup[@]}; i += 2)); do [ "${popup[i]}" = all ] || need+=("${popup[i]}"); done
for ((i = 1; i < ${#open[@]}; i += 2)); do need+=("${open[i]}"); done
undrawn() {
  local text names n
  text="$(sed 's/\x1b\[[0-9;]*m//g' "$log" 2>/dev/null || true)"
  names="$(grep -o 'surface opened document=[^ ]*' <<<"$text" | sed 's/.*=//' || true)"
  [ -n "$names" ] || { echo "(no surface yet)"; return; }
  for n in $names "${need[@]}"; do
    grep -qE "drawn document=$n( |\$)" <<<"$text" || echo "$n"
  done | sort -u | tr '\n' ' '
}
deadline=$((SECONDS + ready_timeout))
while :; do
  missing="$(undrawn)"
  [ -z "$missing" ] && break
  kill -0 "$pid" 2>/dev/null || { echo "the compositor exited before the shell drew: $missing" >&2; grep -v '^\s*$' "$log" | tail -30 >&2; exit 1; }
  [ "$SECONDS" -lt "$deadline" ] || { echo "not drawn within ${ready_timeout}s: $missing" >&2; grep -v '^\s*$' "$log" | tail -30 >&2; exit 1; }
  sleep 0.2
done
sleep "$(LC_ALL=C awk "BEGIN{printf \"%.3f\", $settle/1000}")"
mkdir -p "$(dirname "$out")"
WAYLAND_DISPLAY="$sock" "$target/debug/exponential-ctl" capture --png "$out" --expect-drawn
