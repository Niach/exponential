#!/usr/bin/env bash
# The ios runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
#
# iOS is the one target compiled per app (ITMS-90683: only the declared
# capability modules are linked), so the kit is the runner TEMPLATE, laid out
# at its workspace paths so make-project.sh and the Package.swift path
# dependencies resolve inside the kit unchanged:
#
#   apps/runner/ios/                  App/, Sources/RunnerKit, Tests/, Package.swift, make-project.sh
#   apps/desktop/crates/eengine-ffi/  Package.swift, bindings/swift, swift/, out/ExponentialEngineFFI.xcframework
#   packages/exponential-ui-swift/    Package.swift, Sources/, Tests/, Binaries/ExponentialUIFFI.xcframework,
#                                     Example/KitchenSink/LucideIcons.swift (the icon registry)
#   packages/eengine/fixtures/bundle.json   the scope → module + purpose mapping make-project.sh reads
#   NOTICES.txt                       apps/runner/ios/NOTICES.txt (or RUNNER_KIT_NOTICES=<file>)
#
# The two xcframeworks need the device (aarch64-apple-ios) and simulator
# (aarch64-apple-ios-sim) slices; they are built (build-ios.sh) when either is
# missing, or always with RUNNER_REBUILD_FFI=1. Needs a Mac (Xcode, rustup).
set -euo pipefail
kit="$1"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
engine="$repo/apps/desktop/crates/eengine-ffi"
eui="$repo/packages/exponential-ui-swift"
notices="${RUNNER_KIT_NOTICES:-$repo/apps/runner/ios/NOTICES.txt}"  # the override: a stub until the notices lane lands
[ "$(uname -s)" = Darwin ] || { echo "the ios kit builds on a Mac (Xcode)" >&2; exit 1; }
[ -f "$notices" ] || { echo "no $notices (the iOS runner's third-party notices)" >&2; exit 1; }

targets="aarch64-apple-ios aarch64-apple-ios-sim"
# #PERF-11: the Exponential app's link stubs (exponential-mobile-ffi/build-ios.sh,
# marked LINK_STUB) carry no code: rebuild the real ones.
slices() { [ ! -e "$1/LINK_STUB" ] && [ -d "$1/ios-arm64" ] && ls -d "$1"/ios-*-simulator >/dev/null 2>&1; }
if [ "${RUNNER_REBUILD_FFI:-}" = 1 ] || ! slices "$engine/out/ExponentialEngineFFI.xcframework"; then
  EENGINE_APPLE_TARGETS="$targets" bash "$engine/build-ios.sh"
fi
if [ "${RUNNER_REBUILD_FFI:-}" = 1 ] || ! slices "$eui/Binaries/ExponentialUIFFI.xcframework"; then
  EXPONENTIAL_UI_APPLE_TARGETS="$targets" bash "$repo/apps/desktop/crates/exponential-ui-ffi/build-ios.sh"
fi

copy() { # <repo-relative path>...
  for p in "$@"; do
    mkdir -p "$kit/$(dirname "$p")"
    cp -R "$repo/$p" "$kit/$p"
  done
}
copy apps/runner/ios/App apps/runner/ios/Sources apps/runner/ios/Tests apps/runner/ios/Package.swift apps/runner/ios/make-project.sh
copy apps/desktop/crates/eengine-ffi/Package.swift apps/desktop/crates/eengine-ffi/bindings/swift apps/desktop/crates/eengine-ffi/swift \
  apps/desktop/crates/eengine-ffi/out/ExponentialEngineFFI.xcframework
copy packages/exponential-ui-swift/Package.swift packages/exponential-ui-swift/Sources packages/exponential-ui-swift/Tests \
  packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework packages/exponential-ui-swift/Example/KitchenSink/LucideIcons.swift
copy packages/eengine/fixtures/bundle.json
cp "$notices" "$kit/NOTICES.txt"
echo "ios kit: $(du -sh "$kit" | cut -f1)"
