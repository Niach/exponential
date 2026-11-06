#!/usr/bin/env bash
# #VAPP-270 (E7): `exponential bundle --target ios` end to end on a Mac. Builds
# the ios runner kit (scripts/runner-kits.sh), bundles the album sample
# (photos.read) to {name}-{version}.ipa and checks it: Payload/<name>.app for
# the device (arm64, platform iOS), unsigned, carrying app/, NOTICES.txt and
# the AppIcon; the binary links Photos + PhotosUI and nothing undeclared
# (otool -L); Info.plist has EXACTLY the purpose strings bundle.json maps the
# scopes to. Also writes the simulator build (EENGINE_BUNDLE_IOS_SIMULATOR=1).
#   bash apps/runner/ios/check-bundle.sh [workdir] [--launch <screenshot.png>]
# --launch boots a simulator of its own, installs + launches the simulator
# build, takes a screenshot, then shuts the simulator down and deletes it
# (run it under the ios lane: scripts/lanes/lane.sh ios <owner> -- …).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
WORK="" SHOT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --launch) SHOT="$2"; shift 2 ;;
    *) WORK="$1"; shift ;;
  esac
done
WORK="${WORK:-$(mktemp -d /tmp/runner-ios-bundle.XXXXXX)}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
fail=0
bad() { echo "FAIL: $*"; fail=1; }

# The committed notices, or a stub until the notices lane lands them.
if [ ! -f "$HERE/NOTICES.txt" ]; then
  echo "stub notices (apps/runner/ios/NOTICES.txt is not committed yet)" >"$WORK/NOTICES.txt"
  export RUNNER_KIT_NOTICES="$WORK/NOTICES.txt"
fi
bash "$REPO/scripts/runner-kits.sh" --out "$WORK/kits" ios
cat >"$WORK/bundle.toml" <<'TOML'
id = "at.exponential.samples.album_check"
name = "Album"
version = "1.2.0"
network = "off"
TOML
cat >"$WORK/icon.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="8" y="8" width="48" height="48" rx="10" fill="#fafafa"/><circle cx="24" cy="26" r="6" fill="#09090b"/><path d="M14 50 L30 32 L40 42 L46 36 L52 50 Z" fill="#09090b"/></svg>
SVG
# The CLI, copied out at once: worktrees sharing a CARGO_TARGET_DIR share the artifacts
# of a path crate (hashed by its workspace-relative path), so another worktree's build of
# eengine-bundle can look fresh here: touch both to rebuild this tree's.
touch "$REPO/apps/desktop/crates/cli/src/main.rs" "$REPO/apps/desktop/crates/eengine-bundle/src/lib.rs"
built="$(cd "$REPO/apps/desktop" && cargo build -q -p exp-cli --bin exponential --message-format=json-render-diagnostics \
  | python3 -c 'import json,sys; print([m["executable"] for m in map(json.loads, sys.stdin) if m.get("reason")=="compiler-artifact" and m.get("executable")][-1])')"
cp "$built" "$WORK/exponential"
BIN="$WORK/exponential"
rm -rf "$WORK/dist"
EENGINE_BUNDLE_IOS_SIMULATOR=1 "$BIN" bundle "$REPO/apps/runner/web/samples/album.app.json" --icon "$WORK/icon.svg" --config "$WORK/bundle.toml" \
  --target ios --runners "$WORK/kits" --out "$WORK/dist" --allow-unsigned

IPA="$WORK/dist/Album-1.2.0.ipa"
[ -f "$IPA" ] || { echo "FAIL: no $IPA"; exit 1; }
rm -rf "$WORK/unzipped" && mkdir -p "$WORK/unzipped" && ditto -x -k "$IPA" "$WORK/unzipped"
apps=("$WORK"/unzipped/Payload/*.app)
[ ${#apps[@]} = 1 ] && [ -d "${apps[0]}" ] || { echo "FAIL: the ipa is not Payload/<name>.app"; exit 1; }
check_app() { # <app> <platform: IOS|IOSSIMULATOR>
  local app="$1" bin plist
  plist="$app/Info.plist"
  bin="$app/$(plutil -extract CFBundleExecutable raw "$plist")"
  for f in app/runner.json app/package.app.json NOTICES.txt; do [ -f "$app/$f" ] || bad "$(basename "$app"): no $f"; done
  cmp -s "$app/app/package.app.json" "$REPO/apps/runner/web/samples/album.app.json" || bad "package.app.json is not byte for byte"
  [ "$(plutil -extract CFBundleIdentifier raw "$plist")" = "at.exponential.samples.album-check" ] || bad "bundle id"
  [ "$(plutil -extract CFBundleShortVersionString raw "$plist")" = "1.2.0" ] || bad "version"
  [ "$(plutil -extract CFBundleURLTypes.0.CFBundleURLSchemes.0 raw "$plist")" = "at.exponential.samples.album_check" ] || bad "url scheme"
  [ "$(plutil -extract CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName raw "$plist" 2>/dev/null)" = AppIcon ] || bad "no AppIcon"
  [ -f "$app/Assets.car" ] || bad "no Assets.car"
  vtool -show-build "$bin" | grep -q "platform $2\$" || bad "$bin is not built for $2"
  lipo -archs "$bin" | grep -qx arm64 || bad "$bin is not arm64 only"
  local linked
  linked="$(otool -L "$bin")"
  for f in Photos PhotosUI; do grep -q "/$f.framework/" <<<"$linked" || bad "$f.framework is not linked"; done
  for f in UserNotifications LinkPresentation; do grep -q "/$f.framework/" <<<"$linked" && bad "$f.framework is linked"; done
  # Exactly the purpose strings of the declared scopes (photos.read and its `why`).
  local keys
  keys="$(plutil -convert json -o - "$plist" | python3 -c 'import json,sys; print(" ".join(sorted(k for k in json.load(sys.stdin) if k.endswith("UsageDescription"))))')"
  [ "$keys" = NSPhotoLibraryUsageDescription ] || bad "purpose strings: $keys"
  [ "$(plutil -extract NSPhotoLibraryUsageDescription raw "$plist")" = "Pick the photos for an album." ] || bad "the purpose is not the package's why"
  echo "$(basename "$app") ($2): $(du -sh "$app" | cut -f1), links: $(grep -o '/[A-Za-z]*\.framework/' <<<"$linked" | tr -d / | sed 's/\.framework//' | sort -u | tr '\n' ' ')"
}
check_app "${apps[0]}" IOS
# Unsigned: no signature or profile of a team (the linker's ad-hoc signature of an arm64 binary is all there is).
[ ! -e "${apps[0]}/_CodeSignature" ] && [ ! -e "${apps[0]}/embedded.mobileprovision" ] || bad "the device ipa is signed without a team"
SIM="$WORK/dist/Album-1.2.0-simulator.app"
check_app "$SIM" IOSSIMULATOR

if [ -n "$SHOT" ]; then
  runtime="$(xcrun simctl list runtimes -j | python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["isAvailable"] and x["platform"]=="iOS"]; print(r[-1]["identifier"])')"
  dtype="$(xcrun simctl list devicetypes -j | python3 -c 'import json,sys; d=[x for x in json.load(sys.stdin)["devicetypes"] if x["name"].startswith("iPhone") and "Pro" in x["name"]]; print(d[-1]["identifier"])')"
  udid="$(xcrun simctl create "runner-bundle-check" "$dtype" "$runtime")"
  trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true; xcrun simctl delete "$udid" >/dev/null 2>&1 || true' EXIT
  xcrun simctl boot "$udid"
  xcrun simctl bootstatus "$udid" >/dev/null
  xcrun simctl install "$udid" "$SIM"
  xcrun simctl launch "$udid" at.exponential.samples.album-check
  sleep 8
  xcrun simctl io "$udid" screenshot "$SHOT"
  echo "screenshot $SHOT"
fi
rm -rf "$WORK/dist/.work" "$WORK/unzipped" "$WORK/kits"
[ "$fail" = 0 ] && echo "ios bundle check: ok"
exit "$fail"
