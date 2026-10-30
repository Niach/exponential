#!/usr/bin/env bash
# Build the eNet UniFFI facade for iOS device (arm64), iOS simulator
# (arm64 + x86_64) and macOS (arm64 + x86_64), regenerate the Swift
# bindings, and assemble out/ExponentialNetFFI.xcframework.
#   bash apps/desktop/crates/exponential-net-ffi/build-ios.sh   (from anywhere)
# Each slice = profile `mobile` (LTO, stripped), built as ONE crate type
# (`cargo rustc --crate-type staticlib`): with `[lib] crate-type` listing an
# rlib too, cargo skips the profile's LTO (VAPP-106, same as exponential-ui-ffi).
# Knob: ENET_APPLE_TARGETS="aarch64-apple-ios" builds only those rust targets.
# Knob: ENET_FEATURES="" builds with `--no-default-features --features <list>`
#       (an empty list drops `fs-store`: in-memory blobs only, smaller binary).
# Knob: ENET_PUSH_ONLY=1 = the push-only facade (no iroh, no tokio: the
#       Notification Service Extension's memory cap) with bindings/push/swift,
#       into out/push/ExponentialNetPush.xcframework (module
#       ExponentialNetPushFFI, libexponential_net_push.a): named apart from the
#       full build, so the iOS app links both (VAPP-40: ExpCore the node, the
#       extension this one) without two xcframeworks writing one product.
# Outputs (out/ is gitignored; the Swift binding source is COMMITTED under bindings/swift):
#   apps/desktop/crates/exponential-net-ffi/out/ExponentialNetFFI.xcframework
#   apps/desktop/crates/exponential-net-ffi/bindings/swift/ExponentialNetFFI.swift
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
OUT="$HERE/out"
BINDINGS="$HERE/bindings"
MODULE=ExponentialNetFFI
LIBNAME=libexponential_net_ffi.a
XCF_NAME=ExponentialNetFFI
if [ "${ENET_PUSH_ONLY:-}" = "1" ]; then
  OUT="$HERE/out/push"; BINDINGS="$HERE/bindings/push"; ENET_FEATURES=""
  MODULE=ExponentialNetPush; LIBNAME=libexponential_net_push.a; XCF_NAME=ExponentialNetPush
fi
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin >/dev/null

echo "== bindings (host build; bindgen reads the dylib's metadata)"
bash "$HERE/generate-bindings.sh"

echo "== apple targets (profile mobile: LTO, stripped; no \`cli\` feature)"
APPLE_TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin)
if [ -n "${ENET_APPLE_TARGETS:-}" ]; then read -r -a APPLE_TARGETS <<<"$ENET_APPLE_TARGETS"; fi
FEATURE_ARGS=()
if [ -n "${ENET_FEATURES+x}" ]; then FEATURE_ARGS=(--no-default-features --features "$ENET_FEATURES"); fi
THIN="$OUT/thin"
rm -rf "$THIN"
for t in "${APPLE_TARGETS[@]}"; do
  cargo rustc -p exponential-net-ffi --profile mobile --target "$t" --crate-type staticlib ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"}
  mkdir -p "$THIN/$t"
  cp "$TARGET/$t/mobile/libexponential_net_ffi.a" "$THIN/$t/$LIBNAME"
  strip -S -x "$THIN/$t/$LIBNAME"
done
lib() { echo "$THIN/$1/$LIBNAME"; }

echo "== xcframework"
HEADERS="$OUT/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS/${MODULE}FFI"
# The headers sit in a directory named after the clang module, not at the
# root: an archive copies every static xcframework's headers into ONE
# BuildProductsPath/include, so two root-level module.modulemap files (this
# one and eUI's) fail with "Multiple commands produce include/module.modulemap".
# Clang finds `ExponentialNetFFIFFI` at include/ExponentialNetFFIFFI/module.modulemap
# (the push build: `ExponentialNetPushFFI`).
cp "$BINDINGS/swift/${MODULE}FFI.h" "$HEADERS/${MODULE}FFI/"
cp "$BINDINGS/swift/module.modulemap" "$HEADERS/${MODULE}FFI/module.modulemap"
FAT="$OUT/fat"
rm -rf "$FAT"
built() { [ -f "$(lib "$1")" ]; }
slice() {
  local name="$1"; shift
  local libs=()
  for t in "$@"; do built "$t" && libs+=("$(lib "$t")"); done
  [ ${#libs[@]} -eq 0 ] && return 0
  mkdir -p "$FAT/$name"
  if [ ${#libs[@]} -eq 1 ]; then cp "${libs[0]}" "$FAT/$name/$LIBNAME"
  else lipo -create "${libs[@]}" -output "$FAT/$name/$LIBNAME"; fi
  XCF_ARGS+=(-library "$FAT/$name/$LIBNAME" -headers "$HEADERS")
}
XCF_ARGS=()
slice ios aarch64-apple-ios
slice ios-simulator aarch64-apple-ios-sim x86_64-apple-ios
slice macos aarch64-apple-darwin x86_64-apple-darwin
XCF="$OUT/$XCF_NAME.xcframework"
rm -rf "$XCF"
xcodebuild -create-xcframework "${XCF_ARGS[@]}" -output "$XCF" | tail -2
rm -rf "$FAT" "$THIN"
for s in "$XCF"/*/"$LIBNAME"; do
  echo "$(basename "$(dirname "$s")"): $(lipo -archs "$s") $(du -h "$s" | cut -f1)"
done
du -sh "$XCF"
