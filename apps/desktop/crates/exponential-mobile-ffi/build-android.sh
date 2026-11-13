#!/usr/bin/env bash
# #PERF-11: the Android app's ONE native library: genUI's, the Engine's and
# P2P's UniFFI facades linked into one `libexponential_mobile_ffi.so` per ABI
# (one Rust std, one copy of what they share), built with cargo-ndk.
#   bash apps/desktop/crates/exponential-mobile-ffi/build-android.sh
# Output (gitignored):
#   apps/desktop/crates/exponential-mobile-ffi/out/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libexponential_mobile_ffi.so
# The Kotlin bindings stay the facades' own COMMITTED sources (regenerate them
# with each facade's build-android.sh / generate-bindings.sh); the app points
# them at this library (MobileNativeLibrary.kt). apps/android runs this
# through its `buildMobileFfi` gradle task before merge*JniLibFolders.
# Knob: ABIS="arm64-v8a" builds only those ABIs (default: all three).
# Built as ONE crate type (`cargo rustc --crate-type cdylib`) so the `mobile`
# profile's LTO applies; `strip = true` on the profile strips it.
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

echo "== android .so (profile mobile: LTO, stripped)"
cargo ndk "${NDK_TARGETS[@]}" -o "$HERE/out/jniLibs" rustc -p exponential-mobile-ffi --profile mobile --crate-type cdylib
# cargo-ndk copies EVERY .so in the (possibly shared) target profile dir: keep
# only this library, or stray dylibs (and an older run's facades) ship in the APK.
for abi in ${ABIS//,/ }; do
  find "$HERE/out/jniLibs/$abi" -name '*.so' ! -name 'libexponential_mobile_ffi.so' -delete
  ls -la "$HERE/out/jniLibs/$abi/"
done
