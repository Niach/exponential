#!/usr/bin/env bash
# Cold start of the REAL app (at.exponential) on the connected device/emulator.
#   coldstart-android.sh <apk> <baseline|with-core> [runs=10]
# Installs the APK, force-stops between runs, drops one warm-up launch, parses TotalTime from
# `am start -W`, appends a `coldstart` line to results/c-coldstart.jsonl.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APK="$1"; VARIANT="$2"; RUNS="${3:-10}"
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
PKG=at.exponential; ACT=com.exponential.app.MainActivity
"$ADB" uninstall "$PKG" >/dev/null 2>&1 || true
# --no-incremental: an .idsig next to the APK triggers an incremental (lazy-loaded) install that
# slows cold starts.
"$ADB" install --no-incremental -r "$APK" >/dev/null
# The app asks for POST_NOTIFICATIONS on first launch; the dialog would become the launched
# activity and `am start -W` reports no TotalTime. Pre-grant it.
"$ADB" shell pm grant "$PKG" android.permission.POST_NOTIFICATIONS || true
EMU=$("$ADB" shell getprop ro.kernel.qemu | tr -d '\r'); PLATFORM=android-device; [ "$EMU" = 1 ] && PLATFORM=android-emu
SAMPLES=()
for i in $(seq 0 "$RUNS"); do
  "$ADB" shell am force-stop "$PKG"
  "$ADB" shell 'echo 3 > /proc/sys/vm/drop_caches' >/dev/null 2>&1 || true
  timeout 3 tail -f /dev/null || true   # let the process die fully
  t=""
  for attempt in 1 2 3; do
    o=$("$ADB" shell am start -W -n "$PKG/$ACT" 2>&1 | tr -d '\r' || true)
    t=$(echo "$o" | awk -F': ' '/TotalTime/{print $2}')
    [ -z "$t" ] && echo "no TotalTime (attempt $attempt): $(echo "$o" | tr '\n' ' ')" >&2
    case "$t" in ''|*[!0-9]*) "$ADB" shell am force-stop "$PKG"; timeout 3 tail -f /dev/null || true ;; *) break ;; esac
  done
  echo "run $i: ${t} ms"
  if [ "$i" -gt 0 ] && [ -n "$t" ]; then SAMPLES+=("$t"); fi
  timeout 4 tail -f /dev/null || true   # let the first frame + app init settle
done
"$ADB" shell am force-stop "$PKG"
LINE=$(python3 - "$PLATFORM" "$VARIANT" "${SAMPLES[@]}" <<'PY'
import json, statistics, sys, datetime
plat, var, *s = sys.argv[1:]
s = [int(x) for x in s]
print(json.dumps({"kind":"coldstart","at":datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "platform":plat,"variant":var,"samplesMs":s,"medianMs":statistics.median(s),"method":"am start -W TotalTime"}))
PY
)
echo "$LINE"; echo "$LINE" >> "$HERE/../results/c-coldstart.jsonl"
