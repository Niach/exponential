#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for iOS device (arm64), iOS simulator
# (arm64 + x86_64) and macOS (arm64 + x86_64; VAPP-88: the Swift package builds
# and tests on a Mac too), generate the Swift bindings, and assemble
# ExponentialUIFFI.xcframework for the Swift package `ExponentialUI`
# (packages/exponential-ui-swift). Every slice = profile `mobile`, no `cli`.
#   bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   (from anywhere)
# Outputs (the xcframework is gitignored; the Swift binding source is COMMITTED
# under bindings/swift AND inside the Swift package so the contract is
# reviewable without a toolchain):
#   packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework (~200 MB:
#     five ~42 MB static slices; the release zip is what SwiftPM downloads)
#   apps/desktop/crates/exponential-ui-ffi/bindings/swift/ExponentialUIFFI.swift
#   packages/exponential-ui-swift/Sources/ExponentialUICore/ExponentialUIFFI.swift (a copy)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin >/dev/null

echo "== host build (bindgen reads the dylib's metadata)"
cargo build -p exponential-ui-ffi --release --features cli
echo "== swift bindings"
rm -rf "$HERE/out/swift" && mkdir -p "$HERE/out/swift" "$HERE/bindings/swift"
cargo run -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library "$TARGET/release/libexponential_ui_ffi.$EXT" --language swift --out-dir "$HERE/out/swift" --config "$HERE/uniffi.toml"
cp "$HERE/out/swift/ExponentialUIFFI.swift" "$HERE/bindings/swift/ExponentialUIFFI.swift"
cp "$HERE/out/swift/ExponentialUIFFIFFI.h" "$HERE/bindings/swift/ExponentialUIFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialUIFFIFFI.modulemap" "$HERE/bindings/swift/module.modulemap"
SWIFT_PKG="$(cd "$HERE/../../../../packages/exponential-ui-swift" && pwd)"
mkdir -p "$SWIFT_PKG/Sources/ExponentialUICore"
cp "$HERE/bindings/swift/ExponentialUIFFI.swift" "$SWIFT_PKG/Sources/ExponentialUICore/ExponentialUIFFI.swift"

echo "== apple targets (profile mobile: LTO, stripped; no \`cli\` feature)"
# One fat slice per platform: device arm64; simulator arm64 + x86_64 (Intel
# Macs, Rosetta CI runners); macOS arm64 + x86_64.
APPLE_TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin)
for t in "${APPLE_TARGETS[@]}"; do
  cargo build -p exponential-ui-ffi --profile mobile --target "$t"
done
lib() { echo "$TARGET/$1/mobile/libexponential_ui_ffi.a"; }

echo "== xcframework"
HEADERS="$HERE/out/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$HERE/bindings/swift/ExponentialUIFFIFFI.h" "$HEADERS/"
cp "$HERE/bindings/swift/module.modulemap" "$HEADERS/module.modulemap"
FAT="$HERE/out/fat"
rm -rf "$FAT" && mkdir -p "$FAT/ios-simulator" "$FAT/macos"
lipo -create "$(lib aarch64-apple-ios-sim)" "$(lib x86_64-apple-ios)" -output "$FAT/ios-simulator/libexponential_ui_ffi.a"
lipo -create "$(lib aarch64-apple-darwin)" "$(lib x86_64-apple-darwin)" -output "$FAT/macos/libexponential_ui_ffi.a"
XCF="$SWIFT_PKG/Binaries/ExponentialUIFFI.xcframework"
rm -rf "$HERE/out/ExponentialUIFFI.xcframework" "$XCF" && mkdir -p "$SWIFT_PKG/Binaries"
xcodebuild -create-xcframework \
  -library "$(lib aarch64-apple-ios)" -headers "$HEADERS" \
  -library "$FAT/ios-simulator/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -library "$FAT/macos/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -output "$XCF" | tail -2
rm -rf "$FAT"
for slice in "$XCF"/*/libexponential_ui_ffi.a; do
  echo "$(basename "$(dirname "$slice")"): $(lipo -archs "$slice")"
done
du -sh "$XCF"
