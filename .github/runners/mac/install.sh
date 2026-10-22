#!/usr/bin/env bash
# EXP-1364: the MacBook's self-hosted runner (label exp-mac), native under the
# logged-in user (Xcode + simulators need it). Idempotent; rerun to upgrade.
#   bash .github/runners/mac/install.sh
# Needs: gh logged in as a repo admin, Xcode, Homebrew tuist at the CI pin.
set -euo pipefail

REPO=Niach/exponential
DIR="$HOME/actions-runner"
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
    --name macbook --labels exp-mac --work _work
fi

# Jobs skip Swatinem/rust-cache on self-hosted: sccache replaces it. Only the
# runner's environment, never the login shell's.
cat > .env <<ENV
RUSTC_WRAPPER=$(command -v sccache)
SCCACHE_DIR=$HOME/Library/Caches/sccache-ci
SCCACHE_CACHE_SIZE=40G
CARGO_INCREMENTAL=0
ENV

ls "$HOME"/Library/LaunchAgents/actions.runner.*.plist >/dev/null 2>&1 || ./svc.sh install
PLIST="$(ls "$HOME"/Library/LaunchAgents/actions.runner.*.plist | head -1)"
# Background QoS + nice: builds yield to whatever you are doing.
plutil -replace ProcessType -string Background "$PLIST"
plutil -replace Nice -integer 10 "$PLIST"
./svc.sh stop >/dev/null 2>&1 || true
./svc.sh start
