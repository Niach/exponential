#!/usr/bin/env bash
# Regenerate the COMMITTED Swift + Kotlin binding sources from the host
# library (macOS: .dylib, Linux: .so), without building device targets.
#   bash apps/desktop/crates/exponential-net-ffi/generate-bindings.sh
# Knob: ENET_PUSH_ONLY=1 = the push-only facade (`--no-default-features`: no
#       iroh, no tokio) into bindings/push/swift (what the iOS Notification
#       Service Extension links, EXP-1435; Android links the full node, VAPP-40).
#       Its Swift module is `ExponentialNetPush` (C module
#       `ExponentialNetPushFFI`, uniffi-push.toml) so the iOS app links it next
#       to the full build (VAPP-40).
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
CONFIG="$HERE/uniffi.toml"
MODULE=ExponentialNetFFI
LANGS="swift kotlin"
if [ "${ENET_PUSH_ONLY:-}" = "1" ]; then
  FEATURE_ARGS=(--no-default-features --features cli)
  BINDINGS="$HERE/bindings/push"
  CONFIG="$HERE/uniffi-push.toml"
  MODULE=ExponentialNetPush
  LANGS="swift"
fi
cargo build -p exponential-net-ffi --release "${FEATURE_ARGS[@]}"
for LANG in $LANGS; do
  rm -rf "$HERE/out/$LANG" && mkdir -p "$HERE/out/$LANG"
  cargo run -q -p exponential-net-ffi --release "${FEATURE_ARGS[@]}" --bin uniffi-bindgen -- generate \
    --library "$LIB" --language "$LANG" --out-dir "$HERE/out/$LANG" --config "$CONFIG"
done
mkdir -p "$BINDINGS/swift"
rm -f "$BINDINGS"/swift/*.swift "$BINDINGS"/swift/*.h
cp "$HERE/out/swift/$MODULE.swift" "$BINDINGS/swift/$MODULE.swift"
cp "$HERE/out/swift/${MODULE}FFI.h" "$BINDINGS/swift/${MODULE}FFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/${MODULE}FFI.modulemap" "$BINDINGS/swift/module.modulemap"
if [ "$LANGS" != "swift" ]; then
  mkdir -p "$BINDINGS/kotlin"
  rm -rf "$BINDINGS/kotlin/at"
  cp -R "$HERE/out/kotlin/at" "$BINDINGS/kotlin/"
fi
echo "bindings regenerated from $LIB into $BINDINGS"
