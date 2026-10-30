#!/usr/bin/env bash
# Regenerate the COMMITTED Swift + Kotlin binding sources from the host
# library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/exponential-net-ffi/generate-bindings.sh
# Knob: ENET_PUSH_ONLY=1 = the push-only facade (`--no-default-features`: no
#       iroh, no tokio) into bindings/push/ (what the iOS Notification Service
#       Extension and the Android app link, EXP-1435).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
LIB="$TARGET/release/libexponential_net_ffi.$EXT"

FEATURE_ARGS=(--features cli)
BINDINGS="$HERE/bindings"
if [ "${ENET_PUSH_ONLY:-}" = "1" ]; then
  FEATURE_ARGS=(--no-default-features --features cli)
  BINDINGS="$HERE/bindings/push"
fi
cargo build -p exponential-net-ffi --release "${FEATURE_ARGS[@]}"
for LANG in swift kotlin; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p exponential-net-ffi --release "${FEATURE_ARGS[@]}" --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG" --config "$HERE/uniffi.toml"
done
mkdir -p "$BINDINGS/swift" "$BINDINGS/kotlin"
cp "$HERE/out/swift/ExponentialNetFFI.swift" "$BINDINGS/swift/ExponentialNetFFI.swift"
cp "$HERE/out/swift/ExponentialNetFFIFFI.h" "$BINDINGS/swift/ExponentialNetFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialNetFFIFFI.modulemap" "$BINDINGS/swift/module.modulemap"
rm -rf "$BINDINGS/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$BINDINGS/kotlin/"
echo "bindings regenerated from $LIB into $BINDINGS"
