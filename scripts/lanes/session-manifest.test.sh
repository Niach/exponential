#!/bin/sh
# #EXP-1868: the session manifest writer (`bun run test:lanes`).
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(mktemp -d "${TMPDIR:-/tmp}/exp-manifest-test.XXXXXX")
trap 'rm -rf "$root"' EXIT
export EXP_SESSION_MANIFEST_DIR="$root/manifests"
fails=0
ok() { echo "ok - $1"; }
no() { echo "not ok - $1"; fails=$((fails + 1)); }
m="$here/session-manifest.sh"
s=81457d90-7766-4380-b5c3-117b07633c12

EXP_MCP_SESSION_ID=$s sh "$m" add sim A1B2C3D4-0000-4000-8000-00000000E868
EXP_MCP_SESSION_ID=$s sh "$m" add dir /tmp/s339-dd
EXP_MCP_SESSION_ID=$s sh "$m" add dir /tmp/s339-dd
EXP_MCP_SESSION_ID=$s sh "$m" add avd exp_s339
EXP_MCP_SESSION_ID=$s sh "$m" add compose cap9122
expected=$(printf 'sim\tA1B2C3D4-0000-4000-8000-00000000E868\ndir\t/tmp/s339-dd\navd\texp_s339\ncompose\tcap9122')
[ "$(cat "$root/manifests/$s")" = "$expected" ] && ok "each add lands once in the session's manifest" || no "each add lands once in the session's manifest ($(cat "$root/manifests/$s"))"
[ "$(EXP_MCP_SESSION_ID=$s sh "$m" path)" = "$root/manifests/$s" ] && ok "path names the session's file" || no "path names the session's file"
[ "$(EXP_MCP_SESSION_ID=$s sh "$m" list)" = "$expected" ] && ok "list prints it" || no "list prints it"

if EXP_MCP_SESSION_ID=$s sh "$m" add volume x 2>/dev/null; then no "an unknown kind is refused"; else ok "an unknown kind is refused"; fi
if EXP_MCP_SESSION_ID=$s sh "$m" add dir relative/x 2>/dev/null; then no "a relative dir is refused"; else ok "a relative dir is refused"; fi
env -u EXP_MCP_SESSION_ID sh "$m" add dir /tmp/x 2>/dev/null && ok "outside a session add succeeds" || no "outside a session add succeeds"
[ "$(ls "$root/manifests")" = "$s" ] && ok "outside a session nothing is recorded" || no "outside a session nothing is recorded"
if EXP_MCP_SESSION_ID=../etc sh "$m" add dir /tmp/x 2>/dev/null && [ -e "$root/etc" ]; then no "a session id is never a path"; else ok "a session id is never a path"; fi

# Sourced: the same function.
( . "$m"; EXP_MCP_SESSION_ID=other session_manifest_add dir /tmp/other-dd )
[ "$(cat "$root/manifests/other")" = "$(printf 'dir\t/tmp/other-dd')" ] && ok "sourced, session_manifest_add appends" || no "sourced, session_manifest_add appends"

# #FEED-306: lane.sh lists its pid while it runs and drops it at its end.
EXP_MCP_SESSION_ID=lanes sh "$m" add lane 4242
if EXP_MCP_SESSION_ID=lanes sh "$m" add lane 42x 2>/dev/null; then no "a lane is a pid"; else ok "a lane is a pid"; fi
( . "$m"; EXP_MCP_SESSION_ID=lanes session_manifest_add dir /tmp/lanes-dd; EXP_MCP_SESSION_ID=lanes session_manifest_remove lane 4242 )
[ "$(cat "$root/manifests/lanes")" = "$(printf 'dir\t/tmp/lanes-dd')" ] && ok "remove drops exactly its line" || no "remove drops exactly its line ($(cat "$root/manifests/lanes"))"

[ "$fails" -eq 0 ] && echo "all session manifest checks passed" || { echo "$fails session manifest check(s) failed"; exit 1; }
