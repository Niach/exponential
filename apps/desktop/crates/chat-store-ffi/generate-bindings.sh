#!/usr/bin/env bash
# Regenerate the COMMITTED Swift + Kotlin binding sources from the host
# library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/chat-store-ffi/generate-bindings.sh
# The phones link the code through exponential-mobile-ffi (its build-ios.sh /
# build-android.sh); these sources are what ExpCore and the Android app compile.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libchat_store_ffi.$EXT"
MODULE=ExponentialChatStoreFFI
cargo build -p chat-store-ffi --release --features cli
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p chat-store-ffi --release --features cli --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG"
done
mkdir -p "$HERE/bindings/swift" "$HERE/bindings/kotlin"
rm -f "$HERE"/bindings/swift/*.swift "$HERE"/bindings/swift/*.h
cp "$HERE/out/swift/$MODULE.swift" "$HERE/bindings/swift/$MODULE.swift"
cp "$HERE/out/swift/${MODULE}FFI.h" "$HERE/bindings/swift/${MODULE}FFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/${MODULE}FFI.modulemap" "$HERE/bindings/swift/module.modulemap"
rm -rf "$HERE/bindings/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$HERE/bindings/kotlin/"
rm -rf "$HERE/out"
echo "bindings regenerated from $LIB into $HERE/bindings"
