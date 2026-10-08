#!/usr/bin/env bash
# Round 1: regenerate the COMMITTED Swift + Kotlin binding sources from the
# host library (macOS: .dylib, Linux: .so), without building device targets,
# plus the Swift package's copy (packages/exponential-ui-swift).
#   bash apps/desktop/crates/exponential-ui-ffi/generate-bindings.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libexponential_ui_ffi.$EXT"

cargo build -p exponential-ui-ffi --release --features cli
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG" --config "$HERE/uniffi.toml"
done
cp "$HERE/out/swift/ExponentialUIFFI.swift" "$HERE/bindings/swift/ExponentialUIFFI.swift"
cp "$HERE/out/swift/ExponentialUIFFIFFI.h" "$HERE/bindings/swift/ExponentialUIFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialUIFFIFFI.modulemap" "$HERE/bindings/swift/module.modulemap"
rm -rf "$HERE/bindings/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$HERE/bindings/kotlin/"
# The Swift painter package compiles its own copy (VAPP-88; build-ios.sh too).
SWIFT_PKG="$(cd "$HERE/../../../../packages/exponential-ui-swift" && pwd)"
mkdir -p "$SWIFT_PKG/Sources/ExponentialUICore"
cp "$HERE/bindings/swift/ExponentialUIFFI.swift" "$SWIFT_PKG/Sources/ExponentialUICore/ExponentialUIFFI.swift"
echo "bindings regenerated from $LIB"
