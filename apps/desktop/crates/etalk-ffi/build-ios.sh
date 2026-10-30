#!/usr/bin/env bash
# #EXP-1441: build the eTalk UniFFI facade for iOS device (arm64), iOS
# simulator (arm64 + x86_64) and macOS (arm64 + x86_64), regenerate the Swift
# bindings, and assemble out/ExponentialTalkFFI.xcframework.
#   bash apps/desktop/crates/etalk-ffi/build-ios.sh   (from anywhere)
# Each slice = profile `mobile` (LTO, stripped), built as ONE crate type
# (`cargo rustc --crate-type staticlib`): with `[lib] crate-type` listing an
# rlib too, cargo skips the profile's LTO (VAPP-106, same as exponential-ui-ffi).
# Knob: ETALK_APPLE_TARGETS="aarch64-apple-ios" builds only those rust targets
#       and only the slices they fill.
# Outputs (out/ is gitignored; the Swift binding source is COMMITTED under bindings/swift):
#   apps/desktop/crates/etalk-ffi/out/ExponentialTalkFFI.xcframework
#   apps/desktop/crates/etalk-ffi/bindings/swift/ExponentialTalkFFI.swift
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin >/dev/null
# VAPP-165 (exponential-ui-ffi): with a shared CARGO_TARGET_DIR another
# worktree's build can rewrite $TARGET/<t>/mobile/libetalk_ffi.a between our
# cargo and our cp. So: ONE run per target dir at a time (a mkdir lock that
# a dead holder's pid frees), and this run's slices live in its own scratch dir.
LOCK="$TARGET/.etalk-ffi-build-ios.lock"
mkdir -p "$TARGET"
until mkdir "$LOCK" 2>/dev/null; do
  holder="$(cat "$LOCK/pid" 2>/dev/null || true)"
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then rm -rf "$LOCK"; continue; fi
  echo "waiting for build-ios.sh pid ${holder:-?} ($LOCK)"; sleep 5
done
echo $$ >"$LOCK/pid"
mkdir -p "$HERE/out"
WORK="$(mktemp -d "$HERE/out/build-ios.XXXXXX")"
trap 'rm -rf "$WORK" "$LOCK"' EXIT

echo "== bindings (host build; bindgen reads the dylib's metadata)"
bash "$HERE/generate-bindings.sh"

echo "== apple targets (profile mobile: LTO, stripped; no \`cli\` feature)"
APPLE_TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin)
if [ -n "${ETALK_APPLE_TARGETS:-}" ]; then read -r -a APPLE_TARGETS <<<"$ETALK_APPLE_TARGETS"; fi
THIN="$WORK/thin"
for t in "${APPLE_TARGETS[@]}"; do
  cargo rustc -p etalk-ffi --profile mobile --target "$t" --crate-type staticlib
  # Strip a COPY (debug info + local symbols cargo cannot strip from an archive).
  mkdir -p "$THIN/$t"
  cp "$TARGET/$t/mobile/libetalk_ffi.a" "$THIN/$t/libetalk_ffi.a"
  strip -S -x "$THIN/$t/libetalk_ffi.a"
done
lib() { echo "$THIN/$1/libetalk_ffi.a"; }

echo "== xcframework"
HEADERS="$WORK/headers"
mkdir -p "$HEADERS/ExponentialTalkFFIFFI"
# The headers sit in a directory named after the clang module (as in
# exponential-net-ffi): an archive copies every static xcframework's headers
# into ONE BuildProductsPath/include, so two root-level module.modulemap files fail.
cp "$HERE/bindings/swift/ExponentialTalkFFIFFI.h" "$HEADERS/ExponentialTalkFFIFFI/"
cp "$HERE/bindings/swift/module.modulemap" "$HEADERS/ExponentialTalkFFIFFI/module.modulemap"
FAT="$WORK/fat"
built() { [ -f "$(lib "$1")" ]; }
slice() {
  local name="$1"; shift
  local libs=()
  for t in "$@"; do built "$t" && libs+=("$(lib "$t")"); done
  [ ${#libs[@]} -eq 0 ] && return 0
  mkdir -p "$FAT/$name"
  if [ ${#libs[@]} -eq 1 ]; then cp "${libs[0]}" "$FAT/$name/libetalk_ffi.a"
  else lipo -create "${libs[@]}" -output "$FAT/$name/libetalk_ffi.a"; fi
  XCF_ARGS+=(-library "$FAT/$name/libetalk_ffi.a" -headers "$HEADERS")
}
XCF_ARGS=()
slice ios aarch64-apple-ios
slice ios-simulator aarch64-apple-ios-sim x86_64-apple-ios
slice macos aarch64-apple-darwin x86_64-apple-darwin
XCF="$HERE/out/ExponentialTalkFFI.xcframework"
rm -rf "$XCF"
xcodebuild -create-xcframework "${XCF_ARGS[@]}" -output "$XCF" | tail -2
for s in "$XCF"/*/libetalk_ffi.a; do
  echo "$(basename "$(dirname "$s")"): $(lipo -archs "$s") $(du -h "$s" | cut -f1)"
done
du -sh "$XCF"
