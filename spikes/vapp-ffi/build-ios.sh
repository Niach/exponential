#!/usr/bin/env bash
# VAPP-4 spike: build the UniFFI facade for the iOS simulator + device, generate
# the Swift bindings, and assemble VappSpikeFFI.xcframework for Tuist.
#   bash spikes/vapp-ffi/build-ios.sh            (from the repo root)
# Outputs (xcframework gitignored, Swift source committed):
#   apps/ios/VappSpikeKit/VappSpikeFFI.xcframework
#   apps/ios/VappSpikeKit/Sources/VappSpikeFFI.swift
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT/spikes/vapp-ffi"
export CARGO_TARGET_DIR="$PWD/target"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null

echo "== host build (bindgen reads the dylib's metadata)"
cargo build --release --features cli
echo "== swift bindings"
rm -rf out/swift && mkdir -p out/swift
cargo run --release --features cli --bin uniffi-bindgen -- generate \
  --library target/release/libvapp_spike_ffi.dylib --language swift --out-dir out/swift
echo "== ios targets"
cargo build --release --target aarch64-apple-ios-sim
cargo build --release --target aarch64-apple-ios

echo "== xcframework"
rm -rf headers && mkdir -p headers
cp out/swift/VappSpikeFFIFFI.h headers/
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp out/swift/VappSpikeFFIFFI.modulemap headers/module.modulemap
KIT="$ROOT/apps/ios/VappSpikeKit"
mkdir -p "$KIT/Sources"
rm -rf "$KIT/VappSpikeFFI.xcframework"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios-sim/release/libvapp_spike_ffi.a -headers headers \
  -library target/aarch64-apple-ios/release/libvapp_spike_ffi.a -headers headers \
  -output "$KIT/VappSpikeFFI.xcframework" | tail -2
cp out/swift/VappSpikeFFI.swift "$KIT/Sources/VappSpikeFFI.swift"
ls -la "$KIT" "$KIT/Sources"
du -sh target/aarch64-apple-ios/release/libvapp_spike_ffi.a
