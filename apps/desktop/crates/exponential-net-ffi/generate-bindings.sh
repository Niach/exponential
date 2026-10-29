#!/usr/bin/env bash
# Regenerate the COMMITTED Swift + Kotlin binding sources from the host
# library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/exponential-net-ffi/generate-bindings.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libexponential_net_ffi.$EXT"

cargo build -p exponential-net-ffi --release --features cli
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p exponential-net-ffi --release --features cli --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG" --config "$HERE/uniffi.toml"
done
mkdir -p "$HERE/bindings/swift" "$HERE/bindings/kotlin"
cp "$HERE/out/swift/ExponentialNetFFI.swift" "$HERE/bindings/swift/ExponentialNetFFI.swift"
cp "$HERE/out/swift/ExponentialNetFFIFFI.h" "$HERE/bindings/swift/ExponentialNetFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialNetFFIFFI.modulemap" "$HERE/bindings/swift/module.modulemap"
rm -rf "$HERE/bindings/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$HERE/bindings/kotlin/"
echo "bindings regenerated from $LIB"
