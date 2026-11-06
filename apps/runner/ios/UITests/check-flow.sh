#!/usr/bin/env bash
# #VAPP-271: THE Photobook flow on a simulator of its own, driven by XCUITest
# (PhotobookFlowTests.swift; never the real screen). Boots a fresh iPhone,
# installs the bundle's simulator build (`EENGINE_BUNDLE_IOS_SIMULATOR=1
# exponential bundle … --target ios`), adds 3 photos (`simctl addmedia`),
# runs the UI test (Albums → Permissions → new album → Add photos through the
# consent sheet and PHPicker → a page of thumbnails → Export PDF), then checks
# a valid PDF (%PDF-) in the app's container and that the app's process held
# no internet socket. Shuts the simulator down and deletes it.
#   bash apps/runner/ios/UITests/check-flow.sh <Photobook-…-simulator.app> [shots dir]
# Takes the `ios` lane lock (scripts/lanes/lock.sh) as $FLOW_OWNER (default runner-flow-$$).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
APP="$(cd "$1" && pwd)"
SHOTS="${2:-}"
BUNDLE_ID="$(plutil -extract CFBundleIdentifier raw "$APP/Info.plist")"
EXE="$(plutil -extract CFBundleExecutable raw "$APP/Info.plist")"
CONFIG_ID="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["id"])' "$APP/app/runner.json")"
WORK="$(mktemp -d /tmp/runner-flow.XXXXXX)"
OWNER="${FLOW_OWNER:-runner-flow-$$}"
[ -z "$SHOTS" ] || { mkdir -p "$SHOTS"; SHOTS="$(cd "$SHOTS" && pwd)"; }

. "$REPO/scripts/lanes/lock.sh"
LOCK="$(lane_lock_dir ios)"
take_lock "$LOCK" "$OWNER"
udid=""
cleanup() {
  [ -z "$udid" ] || { xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true; xcrun simctl delete "$udid" >/dev/null 2>&1 || true; }
  rm -rf "$WORK"
  release_lock "$LOCK" "$OWNER" || true
}
trap cleanup EXIT

runtime="$(xcrun simctl list runtimes -j | python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["isAvailable"] and x["platform"]=="iOS"]; print(r[-1]["identifier"])')"
dtype="$(xcrun simctl list devicetypes -j | python3 -c 'import json,sys; import re; d=[x for x in json.load(sys.stdin)["devicetypes"] if re.fullmatch(r"iPhone \d+ Pro", x["name"])]; print(max(d, key=lambda x: int(x["name"].split()[1]))["identifier"])')"
udid="$(xcrun simctl create "runner-flow-$OWNER" "$dtype" "$runtime")"
xcrun simctl boot "$udid"
xcrun simctl bootstatus "$udid" >/dev/null
xcrun simctl install "$udid" "$APP"

# Three photos (plain PNGs, three colours) into the simulator's library.
python3 -I - "$WORK" <<'PY'
import struct, sys, zlib
def png(path, rgb, w=600, h=400):
    raw = b"".join(b"\0" + bytes(rgb) * w for _ in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
for i, c in enumerate([(220, 80, 60), (60, 140, 220), (80, 180, 100)]):
    png(f"{sys.argv[1]}/photo-{i}.png", c)
PY
xcrun simctl addmedia "$udid" "$WORK"/photo-*.png

# The UI test runner (no host app: it drives the installed bundle by id).
cp "$HERE/project.yml" "$HERE/PhotobookFlowTests.swift" "$WORK/"
(cd "$WORK" && xcodegen generate --quiet --spec project.yml)
# Sockets of the app's process, sampled while the test runs (network = off: none).
(
  while sleep 2; do
    pid="$(pgrep -f "$udid/.*\.app/$EXE\$" | head -1 || true)"
    [ -z "$pid" ] || { lsof -nP -a -p "$pid" -i 2>/dev/null || true; } | tail -n +2 >>"$WORK/sockets.txt"
    echo "$pid" >>"$WORK/pids.txt"
  done
) &
watcher=$!
status=0
TEST_RUNNER_FLOW_DEBUG="${FLOW_DEBUG:-}" TEST_RUNNER_FLOW_SHOTS="$SHOTS" TEST_RUNNER_FLOW_BUNDLE_ID="$BUNDLE_ID" xcodebuild test -project "$WORK/RunnerFlow.xcodeproj" -scheme RunnerFlow \
  -destination "id=$udid" -derivedDataPath "$WORK/DerivedData" CODE_SIGNING_ALLOWED=NO >"$WORK/test.log" 2>&1 || status=$?
kill "$watcher" 2>/dev/null || true; wait "$watcher" 2>/dev/null || true
if [ "$status" != 0 ]; then
  cp "$WORK/test.log" /tmp/runner-flow-test.log 2>/dev/null || true
  { grep -E "error:|failed|XCTAssert" "$WORK/test.log" || true; } | head -40
  echo "FAIL: the UI test failed (log: /tmp/runner-flow-test.log)"; exit 1
fi
echo "ui test: ok"

fail=0
data="$(xcrun simctl get_app_container "$udid" "$BUNDLE_ID" data)"
store="$data/Library/Application Support/$CONFIG_ID"
pdfs=0
while IFS= read -r -d '' f; do
  [ "$(head -c 5 "$f")" = "%PDF-" ] || continue
  pdfs=$((pdfs + 1))
  echo "pdf: ${f#"$data/"} ($(wc -c <"$f" | tr -d ' ') bytes, $(grep -ac '/Type */Page[^s]' "$f" || true) page(s), $(grep -ac '/Subtype */Image' "$f" || true) image(s))"
  [ -z "${FLOW_PDF_OUT:-}" ] || cp "$f" "$FLOW_PDF_OUT"
done < <(find "$store" -type f -print0)
[ "$pdfs" -gt 0 ] || { echo "FAIL: no %PDF- file under $store"; fail=1; }
seen="$(grep -c . "$WORK/pids.txt" || true)"
if [ -s "$WORK/sockets.txt" ]; then echo "FAIL: the app held sockets:"; sort -u "$WORK/sockets.txt"; fail=1
else echo "network: no internet socket on the app's process (${seen} pid sample(s))"; fi
[ "$fail" = 0 ] && echo "photobook flow: ok"
exit "$fail"
