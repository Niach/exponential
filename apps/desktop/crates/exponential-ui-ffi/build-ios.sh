#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for the iOS simulator + device, generate the
# Swift bindings, and assemble ExponentialUIFFI.xcframework for the Swift
# package (VAPP-88 ships it inside `ExponentialUI`).
#   bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   (from anywhere)
# Outputs (the xcframework is gitignored; the Swift binding source is COMMITTED
# under bindings/swift so the contract is reviewable without a toolchain):
#   apps/desktop/crates/exponential-ui-ffi/out/ExponentialUIFFI.xcframework
#   apps/desktop/crates/exponential-ui-ffi/bindings/swift/ExponentialUIFFI.swift
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null

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

echo "== ios targets (profile mobile: LTO, stripped)"
cargo build -p exponential-ui-ffi --profile mobile --target aarch64-apple-ios-sim
cargo build -p exponential-ui-ffi --profile mobile --target aarch64-apple-ios

echo "== xcframework"
HEADERS="$HERE/out/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$HERE/bindings/swift/ExponentialUIFFIFFI.h" "$HEADERS/"
cp "$HERE/bindings/swift/module.modulemap" "$HEADERS/module.modulemap"
rm -rf "$HERE/out/ExponentialUIFFI.xcframework"
xcodebuild -create-xcframework \
  -library "$TARGET/aarch64-apple-ios-sim/mobile/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -library "$TARGET/aarch64-apple-ios/mobile/libexponential_ui_ffi.a" -headers "$HEADERS" \
  -output "$HERE/out/ExponentialUIFFI.xcframework" | tail -2
du -sh "$TARGET/aarch64-apple-ios/mobile/libexponential_ui_ffi.a" "$HERE/out/ExponentialUIFFI.xcframework"
