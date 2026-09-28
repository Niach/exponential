#!/usr/bin/env bash
# VAPP-4 spike: build the UniFFI facade for arm64-v8a with cargo-ndk, generate
# the Kotlin bindings, drop the .so into the app's jniLibs.
#   bash spikes/vapp-ffi/build-android.sh        (from the repo root)
# Outputs (.so gitignored, Kotlin source committed):
#   apps/android/app/src/main/jniLibs/arm64-v8a/libvapp_spike_ffi.so
#   apps/android/app/src/main/java/uniffi/vapp_spike_ffi/vapp_spike_ffi.kt
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/spikes/vapp-ffi"
export CARGO_TARGET_DIR="$PWD/target"
NDK_DIR="${ANDROID_NDK_HOME:-$(ls -d "$HOME"/Library/Android/sdk/ndk/* 2>/dev/null | sort | tail -1)}"
export ANDROID_NDK_HOME="$NDK_DIR"
echo "NDK: $ANDROID_NDK_HOME"
rustup target add aarch64-linux-android >/dev/null
command -v cargo-ndk >/dev/null || cargo install cargo-ndk

echo "== host build (bindgen reads the dylib's metadata)"
cargo build --release --features cli
echo "== kotlin bindings"
rm -rf out/kotlin && mkdir -p out/kotlin
cargo run --release --features cli --bin uniffi-bindgen -- generate \
  --library target/release/libvapp_spike_ffi.dylib --language kotlin --out-dir out/kotlin
APP="$ROOT/apps/android/app/src/main"
mkdir -p "$APP/java/uniffi"
rm -rf "$APP/java/uniffi/vapp_spike_ffi"
cp -R out/kotlin/uniffi/vapp_spike_ffi "$APP/java/uniffi/"

echo "== arm64-v8a .so"
cargo ndk -t arm64-v8a -o "$APP/jniLibs" build --release
ls -la "$APP/jniLibs/arm64-v8a/"
