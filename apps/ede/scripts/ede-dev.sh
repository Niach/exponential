#!/usr/bin/env bash
# eDE in a VM or on a dev box (#EDE-7): the compositor nested in the running
# desktop (or headless), the supervisor and the shell from this checkout's
# factory documents, with a scratch ui directory. Prints the socket an agent
# points `ede-ctl` at (captures, input) and keeps running until Ctrl-C.
#
#   apps/ede/scripts/ede-dev.sh                 # nested: a window on the desktop around it
#   apps/ede/scripts/ede-dev.sh --headless 1280x720
#   apps/ede/scripts/ede-dev.sh --dir ~/.config/ede/ui   # your own documents, not a scratch dir
#   WAYLAND_DISPLAY=wayland-1 target/debug/ede-ctl capture --png /tmp/panel.png
#
# QEMU/KVM (the `ede vm` of the plan) is not what runs here: inside the
# VirtualBox VM there is no /dev/kvm, so the harness is this nested or
# headless compositor plus ede-ctl; on a host with KVM, the same script runs
# inside the guest.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$here/target}"
headless=""
dir="${EDE_UI_DIR:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --headless) headless="$2"; shift 2 ;;
    --dir) dir="$2"; shift 2 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
if [ -z "$dir" ]; then
  dir="$(mktemp -d /tmp/ede-dev.XXXXXX)/ui"
  echo "ui directory: $dir (scratch; --dir keeps your own)"
fi
(cd "$here" && cargo build -q -p ede-compositor -p ede-ui -p ede-ctl)
(cd "$here/shell" && cargo build -q -p ede-shell)
args=(--no-xwayland)
[ -n "$headless" ] && args=(--headless "$headless" --no-xwayland)
export EDE_FACTORY_DIR="$here/ui"
export RUST_LOG="${RUST_LOG:-info,smithay=warn,wgpu_core=warn,wgpu_hal=warn}"
echo "ede-ctl: WAYLAND_DISPLAY=<the socket logged below> $target/debug/ede-ctl capture --png /tmp/panel.png"
exec "$target/debug/ede-compositor" "${args[@]}" -- "$target/debug/ede-ui" --dir "$dir" --factory "$here/ui" supervise -- "$target/debug/ede-shell"
