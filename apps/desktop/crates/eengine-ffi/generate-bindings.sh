#!/usr/bin/env bash
# #VAPP-263: regenerate the COMMITTED Swift + Kotlin binding sources from the
# host library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/eengine-ffi/generate-bindings.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libeengine_ffi.$EXT"
BINDINGS="$HERE/bindings"

# EENGINE_FFI_FEATURES="sqlite,hub" (or "" for none) replaces the default
# features (`sqlite`); the shipped products keep the defaults.
FEATURES=()
if [ -n "${EENGINE_FFI_FEATURES+x}" ]; then FEATURES=(--no-default-features --features "$EENGINE_FFI_FEATURES"); fi
cargo build -p eengine-ffi --release --features cli ${FEATURES[@]+"${FEATURES[@]}"}
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p eengine-ffi --release --features cli ${FEATURES[@]+"${FEATURES[@]}"} --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG"
done
mkdir -p "$BINDINGS/swift" "$BINDINGS/kotlin"
cp "$HERE/out/swift/ExponentialEngineFFI.swift" "$BINDINGS/swift/ExponentialEngineFFI.swift"
cp "$HERE/out/swift/ExponentialEngineFFIFFI.h" "$BINDINGS/swift/ExponentialEngineFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialEngineFFIFFI.modulemap" "$BINDINGS/swift/module.modulemap"
rm -rf "$BINDINGS/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$BINDINGS/kotlin/"
echo "bindings regenerated from $LIB into $BINDINGS"
