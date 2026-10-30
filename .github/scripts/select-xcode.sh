#!/usr/bin/env bash
# The ONE Xcode + simulator every Apple job uses: release Xcode 27.1 (the
# iOS 27 SDK) and an iPhone 17 Pro Max on the iOS 27 runtime. Never a beta
# Xcode, never another iOS runtime. The app's deployment target is separate
# (apps/ios/Project.swift) and does not move with this.
#
# GitHub-hosted (`xcode-27` image): /Applications/Xcode_27.1.app.
# Self-hosted (the MacBook, no sudo): /Applications/Xcode.app, selected for
# the job through DEVELOPER_DIR (the runner's .env sets it too).
#
# Exports to the job: DEVELOPER_DIR, IOS_SIM_OS (e.g. 27.1), IOS_SIM_NAME,
# IOS_SIM_UDID. `--simulator` also picks (or creates) the device.
set -euo pipefail

XCODE_VERSION=27.1
IOS_MAJOR=27
SIM_NAME="iPhone 17 Pro Max"

app=""
for candidate in /Applications/Xcode_${XCODE_VERSION}.app /Applications/Xcode.app /Applications/Xcode_${XCODE_VERSION}*.app; do
  [ -d "$candidate" ] || continue
  version="$(defaults read "$candidate/Contents/Info" CFBundleShortVersionString 2>/dev/null || true)"
  [ "$version" = "$XCODE_VERSION" ] && { app="$candidate"; break; }
done
if [ -z "$app" ]; then
  for candidate in /Applications/Xcode*.app; do
    echo "$candidate: $(defaults read "$candidate/Contents/Info" CFBundleShortVersionString 2>/dev/null || echo '?')"
  done
  echo "::error::No release Xcode $XCODE_VERSION on this machine" >&2
  exit 1
fi

export DEVELOPER_DIR="$app/Contents/Developer"
build="$(xcodebuild -version | awk '/Build version/ {print $3}')"
# Apple's beta builds end in a letter (27A5194q); a release build never does.
if [[ "$build" =~ [a-z]$ ]]; then
  echo "::error::$app is a beta Xcode ($build)" >&2
  exit 1
fi
xcodebuild -showsdks | grep -q "iphonesimulator${IOS_MAJOR}\." \
  || { echo "::error::$app ships no iOS $IOS_MAJOR simulator SDK" >&2; exit 1; }
echo "Xcode $XCODE_VERSION ($build) at $app"
echo "DEVELOPER_DIR=$DEVELOPER_DIR" >> "${GITHUB_ENV:-/dev/null}"
# GitHub-hosted has passwordless sudo: select it machine-wide too.
if sudo -n true 2>/dev/null; then sudo xcode-select -s "$app"; fi

[ "${1:-}" = "--simulator" ] || exit 0

# The newest iOS 27.x runtime that runs the device. iOS 27.1 ships only for
# iPhone Duo (its profile's restrictedToDeviceTypes), so on Xcode 27.1 the
# iPhone 17 Pro Max runs on iOS 27.0 (24A434).
runtime="$(xcrun simctl list runtimes available -j | jq -r --arg m "$IOS_MAJOR" --arg n "$SIM_NAME" '
  [.runtimes[] | select(.platform == "iOS" and (.version | startswith($m + "."))
                        and any(.supportedDeviceTypes[]?; .name == $n))]
  | sort_by(.version | split(".") | map(tonumber)) | last // empty | "\(.identifier) \(.version)"')"
if [ -z "$runtime" ]; then
  xcrun simctl list runtimes
  echo "::error::No iOS $IOS_MAJOR simulator runtime that runs $SIM_NAME" >&2
  exit 1
fi
runtime_id="${runtime% *}"
os="${runtime#* }"

udid="$(xcrun simctl list devices available -j | jq -r --arg r "$runtime_id" --arg n "$SIM_NAME" '
  .devices[$r] // [] | map(select(.name == $n)) | .[0].udid // empty')"
if [ -z "$udid" ]; then
  type="$(xcrun simctl list devicetypes -j | jq -r --arg n "$SIM_NAME" '.devicetypes[] | select(.name == $n) | .identifier')"
  udid="$(xcrun simctl create "$SIM_NAME" "$type" "$runtime_id")"
fi
echo "Simulator: $SIM_NAME, iOS $os ($udid)"
{
  echo "IOS_SIM_OS=$os"
  echo "IOS_SIM_NAME=$SIM_NAME"
  echo "IOS_SIM_UDID=$udid"
} >> "${GITHUB_ENV:-/dev/null}"
