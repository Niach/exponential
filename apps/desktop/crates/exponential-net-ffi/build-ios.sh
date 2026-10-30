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
#       into out/push/ExponentialNetFFI.xcframework (what apps/ios links).
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
if [ "${ENET_PUSH_ONLY:-}" = "1" ]; then OUT="$HERE/out/push"; BINDINGS="$HERE/bindings/push"; ENET_FEATURES=""; fi
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
  cp "$TARGET/$t/mobile/libexponential_net_ffi.a" "$THIN/$t/libexponential_net_ffi.a"
  strip -S -x "$THIN/$t/libexponential_net_ffi.a"
done
lib() { echo "$THIN/$1/libexponential_net_ffi.a"; }

echo "== xcframework"
HEADERS="$OUT/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$BINDINGS/swift/ExponentialNetFFIFFI.h" "$HEADERS/"
cp "$BINDINGS/swift/module.modulemap" "$HEADERS/module.modulemap"
FAT="$OUT/fat"
rm -rf "$FAT"
built() { [ -f "$(lib "$1")" ]; }
slice() {
  local name="$1"; shift
  local libs=()
  for t in "$@"; do built "$t" && libs+=("$(lib "$t")"); done
  [ ${#libs[@]} -eq 0 ] && return 0
  mkdir -p "$FAT/$name"
  if [ ${#libs[@]} -eq 1 ]; then cp "${libs[0]}" "$FAT/$name/libexponential_net_ffi.a"
  else lipo -create "${libs[@]}" -output "$FAT/$name/libexponential_net_ffi.a"; fi
  XCF_ARGS+=(-library "$FAT/$name/libexponential_net_ffi.a" -headers "$HEADERS")
}
XCF_ARGS=()
slice ios aarch64-apple-ios
slice ios-simulator aarch64-apple-ios-sim x86_64-apple-ios
slice macos aarch64-apple-darwin x86_64-apple-darwin
XCF="$OUT/ExponentialNetFFI.xcframework"
rm -rf "$XCF"
xcodebuild -create-xcframework "${XCF_ARGS[@]}" -output "$XCF" | tail -2
rm -rf "$FAT" "$THIN"
for s in "$XCF"/*/libexponential_net_ffi.a; do
  echo "$(basename "$(dirname "$s")"): $(lipo -archs "$s") $(du -h "$s" | cut -f1)"
done
du -sh "$XCF"
