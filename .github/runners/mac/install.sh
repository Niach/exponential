#!/usr/bin/env bash
# EXP-1364: the MacBook's self-hosted runner (label exp-mac), native under the
# logged-in user (Xcode + simulators need it). Idempotent; rerun to upgrade.
#   bash .github/runners/mac/install.sh              # macbook   in ~/actions-runner
#   bash .github/runners/mac/install.sh macbook-2    # macbook-2 in ~/actions-runner-2
# Every instance gets its own work dir, launchd service, TMPDIR, sccache
# server port, simulator and tuist + SwiftPM caches, so two Mac jobs run side by side; touching one
# instance never restarts another.
# Needs: gh logged in as a repo admin, the newest release Xcode 27.x at /Applications/Xcode.app
# with the iOS 27 simulator runtime (.github/scripts/select-xcode.sh), Homebrew
# tuist at the CI pin.
set -euo pipefail

REPO=Niach/exponential
NAME="${1:-macbook}"
case "$NAME" in
  macbook) N=1 ;;
  macbook-[2-9]) N="${NAME#macbook-}" ;;
  *) echo "usage: $0 [macbook|macbook-<2-9>]" >&2; exit 2 ;;
esac
DIR="$HOME/actions-runner${NAME#macbook}"
VERSION="$(gh api repos/actions/runner/releases/latest -q .tag_name | sed s/^v//)"

command -v sccache >/dev/null || brew install sccache

mkdir -p "$DIR" && cd "$DIR"
if [ ! -x ./config.sh ] || [ "$(cat .version 2>/dev/null)" != "$VERSION" ]; then
  curl -fsSL "https://github.com/actions/runner/releases/download/v$VERSION/actions-runner-osx-arm64-$VERSION.tar.gz" | tar -xz
  echo "$VERSION" > .version
fi

if [ ! -f .runner ]; then
  TOKEN="$(gh api -X POST "repos/$REPO/actions/runners/registration-token" -q .token)"
  ./config.sh --unattended --replace --url "https://github.com/$REPO" --token "$TOKEN" \
    --name "$NAME" --labels exp-mac --work _work
fi

# Jobs skip Swatinem/rust-cache on self-hosted: sccache replaces it. Only the
# runner's environment, never the login shell's.
cat > .env <<ENV
RUSTC_WRAPPER=$(command -v sccache)
SCCACHE_DIR=$HOME/Library/Caches/sccache-ci
SCCACHE_CACHE_SIZE=15G
CARGO_INCREMENTAL=0
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
ENV
# One sccache server per instance (the cache dir stays shared): a job's disk
# guard stopping its server never kills the other instance's build.
echo "SCCACHE_SERVER_PORT=$((4225 + N))" >> .env
if [ "$N" -gt 1 ]; then
  # Never the user's TMPDIR: tools park fixed paths there (the swift-ios bench's
  # DerivedData). Never the first instance's simulator: swift-ios shuts down
  # the device it booted (.github/scripts/select-xcode.sh reads the name).
  mkdir -p "$DIR/_tmp"
  echo "TMPDIR=$DIR/_tmp/" >> .env
  echo "IOS_SIM_DEVICE_NAME=iPhone 17 Pro Max ($NAME)" >> .env
  # Never the first instance's tuist cache (~/.cache/tuist) or SwiftPM cache
  # (~/Library/Caches/org.swift.swiftpm): parallel `tuist install`s in one
  # lose Firebase's xcframeworks. test-ios, ios-core and eui-swift pass
  # EXP_SWIFTPM_CACHE as --cache-path; DerivedData is per job ($RUNNER_TEMP).
  mkdir -p "$DIR/_cache/swiftpm"
  echo "TUIST_XDG_CACHE_HOME=$DIR/_cache" >> .env
  echo "EXP_SWIFTPM_CACHE=$DIR/_cache/swiftpm" >> .env
fi
# EXP-1396: the disk guard also drops the dev box's shared cargo incremental dirs.
[ -n "${CARGO_TARGET_DIR:-}" ] && echo "DISK_GUARD_CARGO_TARGETS=$CARGO_TARGET_DIR" >> .env

PLIST="$HOME/Library/LaunchAgents/actions.runner.${REPO/\//-}.$NAME.plist"
[ -f "$PLIST" ] || ./svc.sh install
# Nice only: builds yield to whatever you are doing. Never Background QoS:
# it pins the jobs to the efficiency cores (a 40-minute xcframework build).
plutil -replace ProcessType -string Standard "$PLIST"
plutil -replace Nice -integer 10 "$PLIST"
./svc.sh stop >/dev/null 2>&1 || true
./svc.sh start
