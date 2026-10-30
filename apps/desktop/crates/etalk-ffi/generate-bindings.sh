#!/usr/bin/env bash
# #EXP-1441: regenerate the COMMITTED Swift + Kotlin binding sources from the
# host library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/etalk-ffi/generate-bindings.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libetalk_ffi.$EXT"
BINDINGS="$HERE/bindings"

cargo build -p etalk-ffi --release --features cli
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p etalk-ffi --release --features cli --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG" --config "$HERE/uniffi.toml"
done
mkdir -p "$BINDINGS/swift" "$BINDINGS/kotlin"
cp "$HERE/out/swift/ExponentialTalkFFI.swift" "$BINDINGS/swift/ExponentialTalkFFI.swift"
cp "$HERE/out/swift/ExponentialTalkFFIFFI.h" "$BINDINGS/swift/ExponentialTalkFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialTalkFFIFFI.modulemap" "$BINDINGS/swift/module.modulemap"
rm -rf "$BINDINGS/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$BINDINGS/kotlin/"
echo "bindings regenerated from $LIB into $BINDINGS"
