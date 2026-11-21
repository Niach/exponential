#!/bin/sh
# #EXP-1514 (#EXP-2011): ePilot's live test of an agent space, through the
# `serve` example exactly as a host's run goes: the release suite's
# `epilot-space-live` job, and the flip's proof by hand on its VM.
#
#   space-live.sh <exponential-compositor> <the serve example>
#
# On a machine that is ready for agent spaces: this user's systemd user
# manager (lingering on, as the installed daemon has it), the space guard
# installed as the package installs it, polkit with the package's rule. A
# headless compositor runs on this user's runtime directory (its own
# Wayland socket; nothing of the person's session is used), then
# `serve --space` asks the offer (Offered), takes a screenshot (the
# space's, its background), launches a program (in a scope of the space's
# own slice) and lists the windows (the space's: none). Then the
# compositor ends as a session ends it, and nothing of its slice is left.
set -eu

compositor=$1
serve=$2
runtime=${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}
work=$(mktemp -d)
cpid=
spid=

fail() {
  echo "space-live: FAILED: $*" >&2
  for log in compositor.log serve.log serve.err; do
    [ -f "$work/$log" ] && { echo "--- $log" >&2; tail -n 60 "$work/$log" >&2; }
  done
  exit 1
}

cleanup() {
  [ -n "$spid" ] && kill "$spid" 2>/dev/null || true
  if [ -n "$cpid" ] && kill -0 "$cpid" 2>/dev/null; then
    kill -TERM "$cpid" 2>/dev/null || true
    sleep 3
    kill -KILL "$cpid" 2>/dev/null || true
    "$compositor" space-end "$cpid" >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

# Waits up to $1 seconds for the shell test $2.
within() {
  seconds=$1
  shift
  n=0
  while ! eval "$@"; do
    n=$((n + 1))
    [ "$n" -ge $((seconds * 10)) ] && return 1
    sleep 0.1
  done
}

[ -S "$runtime/systemd/private" ] || fail "no systemd user manager in $runtime"
[ -S /run/exponential-space-guard.sock ] || fail "the space guard is not installed (exponential-space-guard.socket)"
[ -f /usr/share/polkit-1/rules.d/00-00-exponential-agent-spaces.rules ] || [ -f /etc/polkit-1/rules.d/00-00-exponential-agent-spaces.rules ] || fail "polkit has no rule for agent spaces"
linger=$(loginctl show-user "$(id -un)" --property=Linger --value 2>/dev/null || echo unknown)
echo "space-live: user manager in $runtime, lingering $linger"

printf 'background = "#102030"\n' > "$work/compositor.toml"
# Its first program says where it listens, and lives as long as it does.
"$compositor" --headless 1280x800 --no-xwayland --config "$work/compositor.toml" \
  -- sh -c 'printf %s "$WAYLAND_DISPLAY" > "$0.tmp" && mv "$0.tmp" "$0"; while kill -0 $PPID 2>/dev/null; do sleep 1; done' "$work/display" \
  > "$work/compositor.log" 2>&1 &
cpid=$!
within 20 '[ -s "$work/display" ]' || fail "the compositor never opened its socket"
display=$(cat "$work/display")
echo "space-live: exponential-compositor $cpid on $display"

cat > "$work/launch.json" <<JSON
{"argv": ["sh", "-c", "echo \$\$ > $work/launched.tmp && mv $work/launched.tmp $work/launched; exec sleep 300"]}
JSON
XDG_CURRENT_DESKTOP=Exponential WAYLAND_DISPLAY=$display EXP_SHOT=$work/space.png \
  "$serve" --space screenshot launch_app "$(cat "$work/launch.json")" list_windows \
  > "$work/serve.log" 2> "$work/serve.err" &
spid=$!
within 120 'grep -q "^serving; Ctrl-C ends it" "$work/serve.log"' || fail "serve never finished its calls"

grep -q '^space offer: Offered$' "$work/serve.log" || fail "the desktop did not offer a space: $(grep '^space offer' "$work/serve.log")"
python3 - "$work" "$cpid" <<'PY' || fail "a reply is not what a space gives"
import json, re, struct, sys, zlib
work, compositor = sys.argv[1], sys.argv[2]
text = open(f"{work}/serve.log").read()
# Each call: "--- tool args (n ms)" and then its reply, pretty JSON.
parts = re.split(r"^--- (\S+) .*\(\d+ ms\)$", text, flags=re.M)
replies = {parts[i]: json.loads(parts[i + 1].split("\nserving;")[0]) for i in range(1, len(parts), 2)}
for tool in ("screenshot", "launch_app", "list_windows"):
    reply = replies.get(tool)
    assert reply is not None, f"no {tool} reply"
    assert not reply.get("isError") and "error" not in reply, f"{tool}: {reply}"
shot = replies["screenshot"]["structuredContent"]
assert (shot["width"], shot["height"]) == (1280, 800), shot
assert replies["list_windows"]["structuredContent"]["windows"] == [], "the person's windows were listed"
# The space shows its background, never the person's screen: the first pixel.
data = open(f"{work}/space.png", "rb").read()
assert data[:8] == b"\x89PNG\r\n\x1a\n"
pos, idat, ihdr = 8, b"", None
while pos < len(data):
    length, kind = struct.unpack(">I4s", data[pos:pos + 8])
    body = data[pos + 8:pos + 8 + length]
    if kind == b"IHDR":
        ihdr = struct.unpack(">IIBBBBB", body)
    elif kind == b"IDAT":
        idat += body
    pos += 12 + length
channels = {2: 3, 6: 4}[ihdr[3]]
raw = zlib.decompress(idat)
# Row 0's first pixel is its raw bytes whatever the filter (no neighbours).
assert tuple(raw[1:4]) == (0x10, 0x20, 0x30), f"the screenshot's first pixel is {tuple(raw[1:1 + channels])}"
print("space-live: offer, screenshot, launch and windows are the space's")
PY

within 10 '[ -s "$work/launched" ]' || fail "the launched program never ran"
pid=$(cat "$work/launched")
slice="app-exponential_p$cpid.slice"
grep -q "/$slice/app-exponential_p$cpid-s1.slice/" "/proc/$pid/cgroup" || fail "the launched program is not in its space's slice: $(cat "/proc/$pid/cgroup")"
echo "space-live: launched $pid in $(cut -d: -f3 "/proc/$pid/cgroup")"

# The end: serve goes (its grant is the space's for up to five minutes), the
# compositor ends as a session ends it; its own exit stops the slice.
kill "$spid" 2>/dev/null || true
spid=
kill -TERM "$cpid"
within 15 '! kill -0 "$cpid" 2>/dev/null' || fail "the compositor did not end on SIGTERM"
wait "$cpid" 2>/dev/null || true
within 10 '[ ! -e "/proc/$pid" ]' || fail "a process of the space outlived the compositor (lingering $linger)"
if systemctl --user is-active --quiet "$slice"; then
  fail "the slice of all spaces outlived the compositor"
fi
cpid=
echo "space-live: ok"
