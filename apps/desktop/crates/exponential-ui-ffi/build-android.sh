#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for arm64-v8a + armeabi-v7a (+ x86_64 for the emulator)
# with cargo-ndk, generate the Kotlin bindings (JNA), drop the .so files
# where the Compose package (VAPP-89, `at.exponential:ui-compose`) picks them up.
#   bash apps/desktop/crates/exponential-ui-ffi/build-android.sh
# Outputs (.so gitignored; the Kotlin binding source is COMMITTED under bindings/kotlin):
#   apps/desktop/crates/exponential-ui-ffi/out/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libexponential_ui_ffi.so
#   apps/desktop/crates/exponential-ui-ffi/bindings/kotlin/at/exponential/ui/ffi/exponential_ui_ffi.kt
# Knobs (EXP-1264, the shots `package` lane builds only what the device needs):
#   ABIS="arm64-v8a"   the ABIs to build (default: all three, what the AAR ships)
#   SKIP_BINDINGS=1    keep the committed Kotlin binding; skip the host build + bindgen
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
SDK_DIR="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
NDK_DIR="${ANDROID_NDK_HOME:-$(ls -d "$SDK_DIR"/ndk/* 2>/dev/null | sort | tail -1)}"
export ANDROID_NDK_HOME="$NDK_DIR"
echo "NDK: $ANDROID_NDK_HOME"
ABIS="${ABIS:-arm64-v8a armeabi-v7a x86_64}"
NDK_TARGETS=()
RUST_TARGETS=()
for abi in ${ABIS//,/ }; do
  case "$abi" in
    arm64-v8a) RUST_TARGETS+=(aarch64-linux-android) ;;
    armeabi-v7a) RUST_TARGETS+=(armv7-linux-androideabi) ;;
    x86_64) RUST_TARGETS+=(x86_64-linux-android) ;;
    *) echo "unknown ABI: $abi (arm64-v8a, armeabi-v7a, x86_64)" >&2; exit 2 ;;
  esac
  NDK_TARGETS+=(-t "$abi")
done
rustup target add "${RUST_TARGETS[@]}" >/dev/null
command -v cargo-ndk >/dev/null || cargo install cargo-ndk

if [ "${SKIP_BINDINGS:-}" = "1" ]; then
  echo "== kotlin bindings: skipped (SKIP_BINDINGS=1, the committed binding stays)"
else
echo "== host build (bindgen reads the dylib's metadata)"
cargo build -p exponential-ui-ffi --release --features cli
echo "== kotlin bindings"
rm -rf "$HERE/out/kotlin" && mkdir -p "$HERE/out/kotlin" "$HERE/bindings/kotlin"
cargo run -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library "$TARGET/release/libexponential_ui_ffi.$EXT" --language kotlin --out-dir "$HERE/out/kotlin" --config "$HERE/uniffi.toml"
rm -rf "$HERE/bindings/kotlin/at"
cp -R "$HERE/out/kotlin/at" "$HERE/bindings/kotlin/"
fi

echo "== android .so (profile mobile)"
cargo ndk "${NDK_TARGETS[@]}" -o "$HERE/out/jniLibs" build -p exponential-ui-ffi --profile mobile
for abi in ${ABIS//,/ }; do ls -la "$HERE/out/jniLibs/$abi/"; done
