#!/bin/sh
# #EXP-2022: the capture guard (`bun run test:lanes`). Every run of capture.sh
# here has DISPLAY and WAYLAND_DISPLAY unset (or set to a fake/private value
# on purpose), so this test can never photograph the person's screen.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(mktemp -d "${TMPDIR:-/tmp}/exp-capture-test.XXXXXX")
xpid=
fakelock=
cleanup() {
  [ -n "$xpid" ] && kill "$xpid" 2>/dev/null && wait "$xpid" 2>/dev/null
  [ -n "$fakelock" ] && rm -f "$fakelock"
  rm -rf "$root"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
fails=0
ok() { echo "ok - $1"; }
no() { echo "not ok - $1"; fails=$((fails + 1)); }
c="$here/capture.sh"
cap() { env -u DISPLAY -u WAYLAND_DISPLAY sh "$c" "$@" 2>/dev/null; }
nothing() { [ -z "$(ls -A "$root/out")" ]; }
mkdir "$root/out"

refused() { # <name> <capture args...>
  name=$1; shift
  if cap "$@"; then no "$name"; elif nothing; then ok "$name"; else no "$name (wrote a file)"; fi
}
refused "refuses an empty display" "" "$root/out/a.png"
refused "refuses :0" :0 "$root/out/a.png"
refused "refuses :0.0" :0.0 "$root/out/a.png"
refused "refuses wayland-0" wayland-0 "$root/out/a.png"
refused "refuses a remote host:N" localhost:5 "$root/out/a.png"
refused "refuses a display nobody serves" :4999 "$root/out/a.png"
if env -u WAYLAND_DISPLAY DISPLAY=:77 sh "$c" :77 "$root/out/a.png" 2>/dev/null; then no "refuses the inherited DISPLAY"; elif nothing; then ok "refuses the inherited DISPLAY"; else no "refuses the inherited DISPLAY (wrote a file)"; fi
if env -u WAYLAND_DISPLAY DISPLAY=:77.0 sh "$c" :77 "$root/out/a.png" 2>/dev/null; then no "refuses the inherited DISPLAY with a screen"; else ok "refuses the inherited DISPLAY with a screen"; fi
if env -u DISPLAY WAYLAND_DISPLAY=wayland-3 sh "$c" wayland-3 "$root/out/a.png" 2>/dev/null; then no "refuses the inherited WAYLAND_DISPLAY"; else ok "refuses the inherited WAYLAND_DISPLAY"; fi

free_display() {
  n=4100
  while [ $n -lt 4200 ]; do
    [ -e "/tmp/.X$n-lock" ] || [ -e "/tmp/.X11-unix/X$n" ] || { echo $n; return 0; }
    n=$((n + 1))
  done
  return 1
}

# A lock whose pid is not an Xvfb (this shell) is refused.
if n=$(free_display); then
  fakelock=/tmp/.X$n-lock
  printf '%10d\n' $$ > "$fakelock"
  refused "refuses a display not served by Xvfb" ":$n" "$root/out/a.png"
  rm -f "$fakelock"; fakelock=
fi

if ! command -v Xvfb >/dev/null || ! command -v xdpyinfo >/dev/null || ! command -v import >/dev/null || ! command -v identify >/dev/null; then
  echo "ok - # SKIP private Xvfb cases: Xvfb, xdpyinfo or ImageMagick missing"
else
  n=$(free_display) || { no "a free display number"; n=; }
  if [ -n "$n" ]; then
    env -u DISPLAY -u WAYLAND_DISPLAY Xvfb ":$n" -screen 0 320x200x24 -nolisten tcp >/dev/null 2>&1 &
    xpid=$!
    i=0
    while [ $i -lt 50 ] && ! env -u DISPLAY -u WAYLAND_DISPLAY xdpyinfo -display ":$n" >/dev/null 2>&1; do sleep 0.1; i=$((i + 1)); done
    cap ":$n" "$root/out/ok.png" >/dev/null && [ "$(identify -format '%wx%h' "$root/out/ok.png")" = 320x200 ] \
      && ok "captures a private Xvfb at its own geometry" || no "captures a private Xvfb at its own geometry"
    cap ":$n" "$root/out/ok2.png" 320x200 >/dev/null && [ -s "$root/out/ok2.png" ] \
      && ok "captures with the expected geometry passed in" || no "captures with the expected geometry passed in"
    rm -f "$root/out/ok.png" "$root/out/ok2.png"
    if cap ":$n" "$root/out/bad.png" 1920x1080; then no "a size mismatch fails"; else ok "a size mismatch fails"; fi
    nothing && ok "a size mismatch deletes the capture" || no "a size mismatch deletes the capture ($(ls -A "$root/out"))"
    if env -u WAYLAND_DISPLAY DISPLAY=":$n" sh "$c" ":$n" "$root/out/a.png" 2>/dev/null; then no "an inherited private display is still refused"; else ok "an inherited private display is still refused"; fi
    kill "$xpid" 2>/dev/null; wait "$xpid" 2>/dev/null; xpid=
  fi
fi

[ $fails -eq 0 ] && echo "capture.test.sh: all passed" || { echo "capture.test.sh: $fails failed"; exit 1; }
