#!/usr/bin/env bash
# The ONE Xcode + simulator every Apple job uses: the newest RELEASE Xcode 27.x
# (the iOS 27 SDK; 27.2 today) and an iPhone 17 Pro Max on an iOS 27 runtime.
# Never a beta Xcode, never another iOS major. The app's deployment target is
# separate (apps/ios/Project.swift) and does not move with this.
#
# "Newest 27.x", not one exact version: the MacBook's Xcode.app comes from the
# App Store, which updates it in place (27.1 -> 27.2 on 2026-10-30).
# GitHub-hosted (`xcode-27` image): /Applications/Xcode_27.*.app.
# Self-hosted (the MacBook, no sudo): /Applications/Xcode.app, selected for
# the job through DEVELOPER_DIR (the runner's .env sets it too).
#
# Exports to the job: DEVELOPER_DIR, IOS_SIM_OS (e.g. 27.0), IOS_SIM_NAME,
# IOS_SIM_UDID. `--simulator` also picks (or creates) the device.
set -euo pipefail

XCODE_MAJOR=27
IOS_MAJOR=27
SIM_NAME="iPhone 17 Pro Max"

# Apple's beta builds end in a letter (27A5194q); a release build never does.
app="" best=""
for candidate in /Applications/Xcode*.app; do
  version="$(defaults read "$candidate/Contents/Info" CFBundleShortVersionString 2>/dev/null || true)"
  build="$(defaults read "$candidate/Contents/version" ProductBuildVersion 2>/dev/null || true)"
  echo "$candidate: ${version:-?} (${build:-?})"
  [[ "$version" == "$XCODE_MAJOR".* && -n "$build" && ! "$build" =~ [a-z]$ ]] || continue
  if [ -z "$best" ] || [ "$(printf '%s\n%s\n' "$best" "$version" | sort -V | tail -1)" = "$version" ]; then
    app="$candidate" best="$version"
  fi
done
if [ -z "$app" ]; then
  echo "::error::No release Xcode $XCODE_MAJOR.x on this machine" >&2
  exit 1
fi

export DEVELOPER_DIR="$app/Contents/Developer"
build="$(xcodebuild -version | awk '/Build version/ {print $3}')"
xcodebuild -showsdks | grep -q "iphonesimulator${IOS_MAJOR}\." \
  || { echo "::error::$app ships no iOS $IOS_MAJOR simulator SDK" >&2; exit 1; }
echo "Xcode $best ($build) at $app"
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
