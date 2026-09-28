#!/usr/bin/env bash
# Measure an unsigned Release arm64 Exponential.app build and append a size line.
#   size-ios.sh <derivedDataPath> <variant baseline|full-apple-crypto|...> [--no-results]
set -euo pipefail
DD="$1"; VARIANT="$2"; WRITE="${3:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$DD/Build/Products/Release-iphoneos/Exponential.app"
[ -d "$APP" ] || { echo "no app at $APP" >&2; exit 1; }
APP_KB=$(du -sk "$APP" | cut -f1)
APP_BYTES=$(find "$APP" -type f -exec stat -f %z {} + | awk '{s+=$1} END {print s}')
MAIN=$(stat -f %z "$APP/Exponential")
CORE=$(stat -f %z "$APP/Frameworks/ExpCore.framework/ExpCore")
seg() { xcrun size -m "$1" | awk '/^Segment __TEXT:/ {t=$3} /^Segment __DATA/ {d+=$3} /^Segment __LINKEDIT:/ {l=$3} END {printf "__TEXT %d __DATA* %d __LINKEDIT %d", t, d, l}'; }
CORE_SEG=$(seg "$APP/Frameworks/ExpCore.framework/ExpCore")
MAIN_SEG=$(seg "$APP/Exponential")
# App Store "download size" approximation: zip of the .app (Apple compresses the thinned IPA).
ZIP=/tmp/vapp3-size-$VARIANT.zip; rm -f "$ZIP"
(cd "$(dirname "$APP")" && ditto -c -k --keepParent Exponential.app "$ZIP")
ZIP_BYTES=$(stat -f %z "$ZIP"); rm -f "$ZIP"
# `xcodebuild build` does not strip (archive does, DEPLOYMENT_POSTPROCESSING). Strip a copy the way
# an archive would (-S -x: debug + local symbols) to get the shipped numbers.
STRIPPED=/tmp/vapp3-size-stripped-$VARIANT; rm -rf "$STRIPPED"; mkdir -p "$STRIPPED"
ditto "$APP" "$STRIPPED/Exponential.app"
find "$STRIPPED/Exponential.app" -type f | while read -r f; do
  if file -b "$f" | grep -q 'Mach-O'; then xcrun strip -S -x "$f" 2>/dev/null || true; fi
done
S_BYTES=$(find "$STRIPPED/Exponential.app" -type f -exec stat -f %z {} + | awk '{s+=$1} END {print s}')
S_MAIN=$(stat -f %z "$STRIPPED/Exponential.app/Exponential")
S_CORE=$(stat -f %z "$STRIPPED/Exponential.app/Frameworks/ExpCore.framework/ExpCore")
(cd "$STRIPPED" && ditto -c -k --keepParent Exponential.app "$STRIPPED.zip")
S_ZIP=$(stat -f %z "$STRIPPED.zip"); rm -rf "$STRIPPED" "$STRIPPED.zip"
echo "stripped: app=$S_BYTES zip=$S_ZIP main=$S_MAIN ExpCore=$S_CORE"
echo "app=${APP_BYTES}B (du ${APP_KB}KiB) zip=$ZIP_BYTES main=$MAIN [$MAIN_SEG] ExpCore=$CORE [$CORE_SEG]"
if [ "$WRITE" != "--no-results" ]; then
  AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '{"kind":"size","at":"%s","platform":"ios","artifact":"Exponential.app (Release, arm64, unsigned, archive-style strip -S -x)","variant":"%s","bytes":%d,"notes":"zipped (download-size proxy) %d; main %d; ExpCore %d. The arm64 device slice IS what App Store thinning ships to a device, so this delta ~= the install delta and the zip delta ~= the download delta."}\n' \
    "$AT" "$VARIANT" "$S_BYTES" "$S_ZIP" "$S_MAIN" "$S_CORE" >> "$HERE/../results/b-size.jsonl"
  printf '{"kind":"size","at":"%s","platform":"ios","artifact":"Exponential.app (Release, arm64, unsigned, xcodebuild build = unstripped)","variant":"%s","bytes":%d,"notes":"sum of file bytes (du %d KiB); zipped (download-size proxy) %d; main binary %d [%s]; ExpCore.framework/ExpCore %d [%s]. The arm64 device slice IS what App Store thinning ships to a device, so the .app delta ~= the install delta and the zip delta ~= the download delta."}\n' \
    "$AT" "$VARIANT" "$APP_BYTES" "$APP_KB" "$ZIP_BYTES" "$MAIN" "$MAIN_SEG" "$CORE" "$CORE_SEG" >> "$HERE/../results/b-size.jsonl"
fi
