#!/usr/bin/env bash
# Fetch a prebuilt GhosttyKit.xcframework + ghostty resources for the macOS app.
# Mirrors how github.com/thdxg/macterm sets up libghostty: rather than build
# ghostty with zig (zig 0.15.2 can't link on macOS 26), download a prebuilt
# xcframework from a release. Output (gitignored): apps/ios/vendor/.
#
# NOTE: this pulls from a third-party ghostty fork's releases. For a shipping
# build we should build + host our OWN GhosttyKit.xcframework (CI on a macOS-15
# runner, from our pinned ghostty) and point GHOSTTY_REPO at it.
set -euo pipefail

GHOSTTY_REPO="${GHOSTTY_REPO:-thdxg/ghostty}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="$(dirname "$SCRIPT_DIR")/vendor"
XCFRAMEWORK="$VENDOR_DIR/GhosttyKit.xcframework"
RESOURCES_MARKER="$VENDOR_DIR/ghostty-resources/terminfo"

mkdir -p "$VENDOR_DIR"
need_fw=true; [ -d "$XCFRAMEWORK" ] && need_fw=false
need_res=true; [ -d "$RESOURCES_MARKER" ] && need_res=false
if ! $need_fw && ! $need_res; then echo "GhosttyKit + resources already present"; exit 0; fi

command -v gh >/dev/null || { echo "missing required tool: gh" >&2; exit 1; }
TAG="$(gh release list --repo "$GHOSTTY_REPO" --limit 1 --json tagName -q '.[0].tagName')"
[ -n "$TAG" ] || { echo "no releases on $GHOSTTY_REPO" >&2; exit 1; }
echo "==> $GHOSTTY_REPO @ $TAG"

if $need_fw; then
  ( cd "$VENDOR_DIR" && gh release download "$TAG" --repo "$GHOSTTY_REPO" --pattern 'GhosttyKit.xcframework.tar.gz' --clobber \
    && tar xzf GhosttyKit.xcframework.tar.gz && rm -f GhosttyKit.xcframework.tar.gz )
fi
if $need_res; then
  ( cd "$VENDOR_DIR" && gh release download "$TAG" --repo "$GHOSTTY_REPO" --pattern 'ghostty-resources.tar.gz' --clobber \
    && rm -rf ghostty-resources && mkdir -p ghostty-resources \
    && tar xzf ghostty-resources.tar.gz -C ghostty-resources && rm -f ghostty-resources.tar.gz )
fi
echo "==> done:"
echo "  framework: $XCFRAMEWORK"
echo "  resources: $VENDOR_DIR/ghostty-resources"
