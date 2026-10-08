#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for arm64-v8a (+ x86_64 for the emulator)
# with cargo-ndk, generate the Kotlin bindings (JNA), drop the .so files
# where the Compose package (VAPP-89, `at.exponential:ui-compose`) picks them up.
#   bash apps/desktop/crates/exponential-ui-ffi/build-android.sh
# Outputs (.so gitignored; the Kotlin binding source is COMMITTED under bindings/kotlin):
#   apps/desktop/crates/exponential-ui-ffi/out/jniLibs/{arm64-v8a,x86_64}/libexponential_ui_ffi.so
#   apps/desktop/crates/exponential-ui-ffi/bindings/kotlin/at/exponential/ui/ffi/exponential_ui_ffi.kt
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
NDK_DIR="${ANDROID_NDK_HOME:-$(ls -d "$HOME"/Library/Android/sdk/ndk/* 2>/dev/null | sort | tail -1)}"
export ANDROID_NDK_HOME="$NDK_DIR"
echo "NDK: $ANDROID_NDK_HOME"
rustup target add aarch64-linux-android x86_64-linux-android >/dev/null
command -v cargo-ndk >/dev/null || cargo install cargo-ndk

echo "== host build (bindgen reads the dylib's metadata)"
cargo build -p exponential-ui-ffi --release --features cli
echo "== kotlin bindings"
rm -rf "$HERE/out/kotlin" && mkdir -p "$HERE/out/kotlin" "$HERE/bindings/kotlin"
cargo run -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library "$TARGET/release/libexponential_ui_ffi.dylib" --language kotlin --out-dir "$HERE/out/kotlin" --config "$HERE/uniffi.toml"
rm -rf "$HERE/bindings/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$HERE/bindings/kotlin/"

echo "== android .so (profile mobile)"
cargo ndk -t arm64-v8a -t x86_64 -o "$HERE/out/jniLibs" build -p exponential-ui-ffi --profile mobile
ls -la "$HERE/out/jniLibs/"*/
