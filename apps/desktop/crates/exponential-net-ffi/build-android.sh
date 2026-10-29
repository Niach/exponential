#!/usr/bin/env bash
# Build the eNet UniFFI facade for arm64-v8a + armeabi-v7a (+ x86_64 for the
# emulator) with cargo-ndk and regenerate the Kotlin bindings (JNA).
#   bash apps/desktop/crates/exponential-net-ffi/build-android.sh
# Outputs (.so gitignored; the Kotlin binding source is COMMITTED under bindings/kotlin):
#   apps/desktop/crates/exponential-net-ffi/out/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libexponential_net_ffi.so
#   apps/desktop/crates/exponential-net-ffi/bindings/kotlin/at/exponential/net/ffi/exponential_net_ffi.kt
# Knobs:
#   ABIS="arm64-v8a"   the ABIs to build (default: all three)
#   SKIP_BINDINGS=1    keep the committed Kotlin binding; skip the host build + bindgen
#   ENET_FEATURES=""   `--no-default-features --features <list>` (empty = no fs-store)
# The .so is built as ONE crate type (`cargo rustc --crate-type cdylib`) so the
# `mobile` profile's LTO applies; `strip = true` on the profile strips it.
# ring (iroh's TLS crypto) compiles C/asm: cargo-ndk points CC at the NDK clang.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
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
  echo "== bindings: skipped (SKIP_BINDINGS=1, the committed binding stays)"
else
  echo "== bindings"
  bash "$HERE/generate-bindings.sh"
fi

echo "== android .so (profile mobile: LTO, stripped)"
FEATURE_ARGS=()
if [ -n "${ENET_FEATURES+x}" ]; then FEATURE_ARGS=(--no-default-features --features "$ENET_FEATURES"); fi
cargo ndk "${NDK_TARGETS[@]}" -o "$HERE/out/jniLibs" rustc -p exponential-net-ffi --profile mobile --crate-type cdylib ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"}
for abi in ${ABIS//,/ }; do ls -la "$HERE/out/jniLibs/$abi/"; done
