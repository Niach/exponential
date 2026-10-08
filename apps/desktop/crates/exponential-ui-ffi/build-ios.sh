#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for the iOS simulator + device (+ the macOS
# host, VAPP-88: the Swift package builds and tests on a Mac too), generate the
# Swift bindings, and assemble ExponentialUIFFI.xcframework for the Swift
# package `ExponentialUI` (packages/exponential-ui-swift).
#   bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   (from anywhere)
# Outputs (the xcframework is gitignored; the Swift binding source is COMMITTED
# under bindings/swift AND inside the Swift package so the contract is
# reviewable without a toolchain):
#   apps/desktop/crates/exponential-ui-ffi/out/ExponentialUIFFI.xcframework
#   packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework (a copy)
#   apps/desktop/crates/exponential-ui-ffi/bindings/swift/ExponentialUIFFI.swift
#   packages/exponential-ui-swift/Sources/ExponentialUICore/ExponentialUIFFI.swift (a copy)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null

echo "== host build (bindgen reads the dylib's metadata)"
cargo build -p exponential-ui-ffi --release --features cli
echo "== swift bindings"
rm -rf "$HERE/out/swift" && mkdir -p "$HERE/out/swift" "$HERE/bindings/swift"
cargo run -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library "$TARGET/release/libexponential_ui_ffi.dylib" --language swift --out-dir "$HERE/out/swift" --config "$HERE/uniffi.toml"
cp "$HERE/out/swift/ExponentialUIFFI.swift" "$HERE/bindings/swift/ExponentialUIFFI.swift"
cp "$HERE/out/swift/ExponentialUIFFIFFI.h" "$HERE/bindings/swift/ExponentialUIFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialUIFFIFFI.modulemap" "$HERE/bindings/swift/module.modulemap"
SWIFT_PKG="$(cd "$HERE/../../../../packages/exponential-ui-swift" && pwd)"
mkdir -p "$SWIFT_PKG/Sources/ExponentialUICore"
cp "$HERE/bindings/swift/ExponentialUIFFI.swift" "$SWIFT_PKG/Sources/ExponentialUICore/ExponentialUIFFI.swift"

echo "== ios targets (profile mobile: LTO, stripped)"
cargo build -p exponential-ui-ffi --profile mobile --target aarch64-apple-ios-sim
cargo build -p exponential-ui-ffi --profile mobile --target aarch64-apple-ios

echo "== xcframework"
HEADERS="$HERE/out/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$HERE/bindings/swift/ExponentialUIFFIFFI.h" "$HEADERS/"
cp "$HERE/bindings/swift/module.modulemap" "$HEADERS/module.modulemap"
rm -rf "$HERE/out/ExponentialUIFFI.xcframework"
# The macOS slice is the host's release staticlib (built above for bindgen):
# it lets `swift test` and a macOS embedder link the same package.
xcodebuild -create-xcframework \
  -library "$TARGET/aarch64-apple-ios-sim/mobile/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -library "$TARGET/aarch64-apple-ios/mobile/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -library "$TARGET/release/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -output "$HERE/out/ExponentialUIFFI.xcframework" | tail -2
rm -rf "$SWIFT_PKG/Binaries/ExponentialUIFFI.xcframework" && mkdir -p "$SWIFT_PKG/Binaries"
cp -R "$HERE/out/ExponentialUIFFI.xcframework" "$SWIFT_PKG/Binaries/ExponentialUIFFI.xcframework"
du -sh "$TARGET/aarch64-apple-ios/mobile/libexponential_ui_ffi.a" "$HERE/out/ExponentialUIFFI.xcframework"
