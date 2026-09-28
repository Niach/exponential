#!/usr/bin/env bash
# Cold start of the REAL Exponential app, baseline vs with the Rust peer core, via
# ExponentialUITests/LaunchMetricTests (XCTApplicationLaunchMetric, 5 iterations, Release).
#   coldstart-ios.sh <baseline|with-core> [destination] [--no-results]
# destination default: the iPhone 17 Pro simulator. For a device: 'id=00008150-00161C411130401C'
# (needs signing: drop nothing, run from Xcode once to trust the developer profile).
set -euo pipefail
VARIANT="$1"; DEST="${2:-platform=iOS Simulator,name=iPhone 17 Pro}"; WRITE="${3:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
IOS="$HERE/../../../apps/ios"
LOG=/tmp/vapp3-cold-$VARIANT.log
cd "$IOS"
if [ "$VARIANT" = with-core ]; then TUIST_PEER_SPIKE=1 tuist generate --no-open >/dev/null; else tuist generate --no-open >/dev/null; fi
rm -rf "/tmp/vapp3-cold-$VARIANT.xcresult"
TEST_RUNNER_VAPP3_LAUNCH_METRIC=1 xcodebuild test -workspace Exponential.xcworkspace -scheme Exponential \
  -configuration Release -destination "$DEST" -derivedDataPath /tmp/vapp3-dd-simtest ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
  -resultBundlePath "/tmp/vapp3-cold-$VARIANT.xcresult" \
  -only-testing:ExponentialUITests/LaunchMetricTests > "$LOG" 2>&1 || { echo "test failed, see $LOG"; grep -E 'error:' "$LOG" | head; exit 1; }
LINE=$(grep -m1 'measured \[Duration (AppLaunch), s\]' "$LOG")
VALUES=$(echo "$LINE" | sed -E 's/.*values: \[([^]]*)\].*/\1/')
PLATFORM=ios-sim; case "$DEST" in *Simulator*) ;; *) PLATFORM=ios-device ;; esac
JSON=$(python3 - "$VALUES" "$VARIANT" "$PLATFORM" "$DEST" <<'PY'
import json, sys, statistics, datetime
vals = [round(float(v) * 1000, 1) for v in sys.argv[1].split(",")]
print(json.dumps({"kind": "coldstart", "at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "platform": sys.argv[3], "variant": sys.argv[2], "samplesMs": vals, "medianMs": statistics.median(vals),
  "method": "XCTApplicationLaunchMetric",
  "notes": f"Exponential.app Release, 5 iterations, {sys.argv[4]}; Xcode 27.0 beta; signed-out launch (sign-in screen); the core is linked into ExpCore but not called at launch"}, separators=(",", ":")))
PY
)
echo "$JSON"
[ "$WRITE" = "--no-results" ] || echo "$JSON" >> "$HERE/../results/b-coldstart.jsonl"
