#!/bin/sh
# #EXP-2022: the ONE way a Linux screen capture is taken on a dev machine.
#
#   scripts/lanes/capture.sh [--window <id>] <display> <out.png> [<WxH>]
#
# On 2026-11-19 an `import -window root` ran with the session's inherited
# DISPLAY=:0 and photographed the person's live desktop. This script makes that
# impossible instead of relying on discipline. It refuses (exit 2, writes
# nothing) unless <display>:
#   - is a local X display `:N` with N > 0 (never empty, `:0`, `wayland-0`, a
#     host:N, a screen suffix);
#   - is not the caller's inherited $DISPLAY or $WAYLAND_DISPLAY (that one is
#     the person's screen; pass a private display you started yourself);
#   - is served by an Xvfb process (its /tmp/.X<N>-lock pid runs `Xvfb`), and
#     answers `xdpyinfo -display <display>` with DISPLAY/WAYLAND_DISPLAY unset.
# It captures with the inherited variables unset (`import -display <display>`),
# into a hidden temp file next to <out>, and compares the picture's size with
# the expected geometry: <WxH> if given, else the screen's `xdpyinfo`
# dimensions (or, with --window, the window's `xwininfo` size). A mismatch
# deletes the file unviewed and exits 3; only a matching picture lands at <out>.
#
# Headless Wayland compositors (eDE's headless backend) are NOT captured here:
# use the compositor's own capture (`exponential-ctl capture`). If you must use
# grim, the matching rule is: name the socket explicitly, never `wayland-0`,
# never the inherited one, and check it is your compositor's socket first:
#   [ "$sock" != wayland-0 ] && [ "$sock" != "${WAYLAND_DISPLAY:-}" ] \
#     && [ -S "$XDG_RUNTIME_DIR/$sock" ] \
#     && env -u DISPLAY WAYLAND_DISPLAY="$sock" grim "$out"
set -u

die() { echo "capture.sh: $2" >&2; exit "$1"; }

window=
if [ "${1:-}" = --window ]; then
  window=${2:-}
  case $window in ''|*[!0-9a-fA-Fx]*) die 2 "--window wants an X window id" ;; esac
  shift 2
fi
[ $# -ge 2 ] && [ $# -le 3 ] || die 2 "usage: capture.sh [--window <id>] <display> <out.png> [<WxH>]"
display=$1
out=$2
expected=${3:-}

# 1. The display itself: refuse anything that could be the person's screen.
case $display in
  '') die 2 "refusing an empty display" ;;
  wayland-*|*/*) die 2 "refusing '$display': a Wayland socket (use the compositor's capture, see the header)" ;;
  :0|:0.*) die 2 "refusing ':0': that is the person's live display" ;;
esac
case $display in
  :[1-9]|:[1-9][0-9]|:[1-9][0-9][0-9]|:[1-9][0-9][0-9][0-9]) ;;
  *) die 2 "refusing '$display': only a local private X display ':N' (N > 0) is captured" ;;
esac
num=${display#:}
for inherited in "${DISPLAY:-}" "${WAYLAND_DISPLAY:-}"; do
  [ -n "$inherited" ] || continue
  bare=${inherited%%.*}
  [ "$display" = "$inherited" ] || [ "$display" = "$bare" ] || [ "$display" = ":${bare##*:}" ] \
    && die 2 "refusing '$display': it is the inherited display, i.e. the screen this session runs on"
done
[ -n "$expected" ] && { echo "$expected" | grep -Eq '^[1-9][0-9]*x[1-9][0-9]*$' || die 2 "expected geometry '$expected' is not WxH"; }

# 2. Who serves it: only an Xvfb we can see, answering with the inherited
# variables unset.
lock=/tmp/.X$num-lock
[ -r "$lock" ] || die 2 "refusing '$display': no $lock, so no X server of ours is listening there"
pid=$(tr -d ' \n' < "$lock")
case $pid in ''|*[!0-9]*) die 2 "refusing '$display': unreadable pid in $lock" ;; esac
comm=$(cat "/proc/$pid/comm" 2>/dev/null || true)
[ "$comm" = Xvfb ] || die 2 "refusing '$display': served by '${comm:-nothing}' (pid $pid), not a private Xvfb"
info=$(env -u DISPLAY -u WAYLAND_DISPLAY xdpyinfo -display "$display" 2>/dev/null) \
  || die 2 "refusing '$display': no X server answers there"

if [ -z "$expected" ]; then
  if [ -n "$window" ]; then
    expected=$(env -u DISPLAY -u WAYLAND_DISPLAY xwininfo -display "$display" -id "$window" 2>/dev/null \
      | awk '/^ *Width:/ { w = $2 } /^ *Height:/ { h = $2 } END { if (w && h) print w "x" h }')
  else
    expected=$(echo "$info" | awk '/dimensions:/ { print $2; exit }')
  fi
  [ -n "$expected" ] || die 2 "could not read the expected geometry of '$display'"
fi

# 3. Capture into a hidden temp file; only a picture of the expected size
# becomes <out>.
dir=$(dirname "$out")
[ -d "$dir" ] || die 2 "no directory $dir"
base=$(basename "$out")
tmp="$dir/.capture-$$-$base"
rm -f "$tmp"
trap 'rm -f "$tmp"' EXIT
trap 'exit 130' INT TERM
env -u DISPLAY -u WAYLAND_DISPLAY import -display "$display" -window "${window:-root}" "$tmp" \
  || die 1 "import failed on '$display'"
got=$(identify -format '%wx%h' "$tmp" 2>/dev/null | head -1)
if [ "$got" != "$expected" ]; then
  rm -f "$tmp"
  die 3 "deleted unviewed: the capture is ${got:-unreadable}, the test display is $expected"
fi
mv -f "$tmp" "$out" || die 1 "could not move the capture to $out"
echo "$out"
