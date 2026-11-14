#!/bin/sh
# #VAPP-268: THE MAC BUILD LANE of the iOS runner: one exported app's Xcode
# project + xcodebuild (apps/runner/ios/make-project.sh) under the `ios`
# lane lock, building the two xcframeworks first when the slice is missing.
# E7's `exponential bundle --target ios` calls this, here or on a remote Mac.
#
#   scripts/lanes/runner-ios.sh <owner> --bundle <app dir> --out <dir> \
#     [--capabilities photos,share] [--build simulator|device|none]
#
# The lock is held for the whole build (cargo + xcodebuild) and released
# through release_lock whatever the exit (lane.sh); never rm a lock.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
[ $# -ge 1 ] || { echo "usage: $0 <owner> --bundle <app dir> --out <dir> [--capabilities …] [--build …]" >&2; exit 2; }
owner=$1
shift

# The rust targets the build needs: the simulator slice or the device one.
targets="aarch64-apple-ios-sim"
prev=""
for arg in "$@"; do
  [ "$prev" = "--build" ] && [ "$arg" = "device" ] && targets="aarch64-apple-ios"
  prev=$arg
done
slice='ios-*-simulator'
[ "$targets" = "aarch64-apple-ios" ] && slice=ios-arm64

exec "$here/lane.sh" ios "$owner" -- sh -c '
  set -eu
  repo=$1 targets=$2 slice=$3
  shift 3
  engine="$repo/apps/desktop/crates/eengine-ffi/out/ExponentialEngineFFI.xcframework"
  eui="$repo/packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework"
  # RUNNER_REBUILD_FFI=1 rebuilds them anyway (after an Engine or genUI change).
  # #PERF-11: the Exponential app's link stubs (exponential-mobile-ffi/build-ios.sh,
  # marked LINK_STUB) carry no code: the runner rebuilds the real ones.
  has() { [ -e "$1/LINK_STUB" ] && return 1; for d in $1/$slice; do [ -d "$d" ] && return 0; done; return 1; }
  if [ "${RUNNER_REBUILD_FFI:-}" = 1 ] || ! has "$engine"; then EENGINE_APPLE_TARGETS="$targets" bash "$repo/apps/desktop/crates/eengine-ffi/build-ios.sh"; fi
  if [ "${RUNNER_REBUILD_FFI:-}" = 1 ] || ! has "$eui"; then EXPONENTIAL_UI_APPLE_TARGETS="$targets" bash "$repo/apps/desktop/crates/exponential-ui-ffi/build-ios.sh"; fi
  bash "$repo/apps/runner/ios/make-project.sh" "$@"
' sh "$repo" "$targets" "$slice" "$@"
