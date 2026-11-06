#!/usr/bin/env bash
# #VAPP-268: the build check of the generated runner project (CI: ios-runner.yml).
# Generates + builds the sample bundle for the simulator twice and checks the
# ITMS-90683 rule on the products: with `photos` the binary links Photos and
# PhotosUI and Info.plist carries NSPhotoLibraryUsageDescription; with only
# `share` neither Photos, PhotosUI nor UserNotifications is linked and no
# photo-library purpose string is declared but the add one. Both carry the bundle
# at <name>.app/app and the deep-link scheme from runner.json.
#   bash apps/runner/ios/check-build.sh [workdir]     (needs the simulator slices of both xcframeworks)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${1:-$(mktemp -d /tmp/runner-ios-check.XXXXXX)}"
mkdir -p "$WORK"
fail=0
bad() { echo "FAIL: $*"; fail=1; }

check() { # name capabilities expect-linked expect-absent expect-keys absent-keys
  local name="$1" caps="$2"
  local out="$WORK/$name"
  rm -rf "$out"
  local app
  app="$(bash "$HERE/make-project.sh" --bundle "$HERE/sample/app" --out "$out" --capabilities "$caps" --build simulator | tail -1)"
  local bin="$app/$(basename "$app" .app)"
  local plist="$app/Info.plist"
  [ -f "$app/app/runner.json" ] && [ -f "$app/app/package.app.json" ] || bad "$name: no bundle at <name>.app/app"
  [ "$(plutil -extract CFBundleIdentifier raw "$plist")" = "at.exponential.runner.sample" ] || bad "$name: bundle id"
  [ "$(plutil -extract CFBundleShortVersionString raw "$plist")" = "1.0.0" ] || bad "$name: version"
  [ "$(plutil -extract CFBundleURLTypes.0.CFBundleURLSchemes.0 raw "$plist")" = "at.exponential.runner.sample" ] || bad "$name: url scheme"
  local linked
  linked="$(otool -L "$bin")"
  for f in $3; do grep -q "/$f.framework/" <<<"$linked" || bad "$name: $f.framework is not linked"; done
  for f in $4; do grep -q "/$f.framework/" <<<"$linked" && bad "$name: $f.framework is linked"; done
  for k in $5; do plutil -extract "$k" raw "$plist" >/dev/null 2>&1 || bad "$name: no $k"; done
  for k in $6; do plutil -extract "$k" raw "$plist" >/dev/null 2>&1 && bad "$name: $k declared"; done
  echo "$name: $(du -sh "$app" | cut -f1) $(basename "$app")"
  rm -rf "$out/DerivedData/Build/Intermediates.noindex"
}

check photos photos "Photos PhotosUI" "UserNotifications LinkPresentation" "NSPhotoLibraryUsageDescription" "NSPhotoLibraryAddUsageDescription"
check share share "LinkPresentation" "Photos PhotosUI UserNotifications" "NSPhotoLibraryAddUsageDescription" "NSPhotoLibraryUsageDescription"
[ "$fail" = 0 ] && echo "runner build check: ok"
exit "$fail"
