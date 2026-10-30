#!/usr/bin/env bash
# VAPP-86: build the UniFFI facade for iOS device (arm64), iOS simulator
# (arm64 + x86_64) and macOS (arm64 + x86_64; VAPP-88: the Swift package builds
# and tests on a Mac too), generate the Swift bindings, and assemble
# ExponentialUIFFI.xcframework for the Swift package `ExponentialUI`
# (packages/exponential-ui-swift). Every slice = profile `mobile`, no `cli`.
#   bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   (from anywhere)
# VAPP-106: each slice is built as ONE crate type (`cargo rustc --crate-type
# staticlib`): with the facade's `[lib] crate-type` listing an rlib too, cargo
# skips the profile's LTO, which left every std object (with its debug info)
# in the archive, ~44 MB a slice. LTO + `strip -S -x` on a copy = ~6 MB.
# Knob: EXPONENTIAL_UI_APPLE_TARGETS="aarch64-apple-ios" builds only those
# rust targets and only the slices they fill (EXP-1364: the App Store archive
# needs the device slice alone; five LTO builds were 16 of its 29 minutes).
# Knob: EXPONENTIAL_UI_FEATURES="descriptions" builds the facade with
# `--no-default-features --features <list>` (an empty list drops the
# non-default themes; see crates/eui-core/Cargo.toml).
# Outputs (the xcframework is gitignored; the Swift binding source is COMMITTED
# under bindings/swift AND inside the Swift package so the contract is
# reviewable without a toolchain):
#   packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework
#     (release/sizes.json has the slice sizes; the release zip is what SwiftPM downloads)
#   apps/desktop/crates/exponential-ui-ffi/bindings/swift/ExponentialUIFFI.swift
#   packages/exponential-ui-swift/Sources/ExponentialUICore/ExponentialUIFFI.swift (a copy)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin >/dev/null
# VAPP-165: two runs at once broke each other: both used out/thin + out/fat
# (one's `rm -rf` deleted the other's slices: "strip: can't stat", "lipo:
# open() failed"), and with a shared CARGO_TARGET_DIR another worktree's
# build can rewrite $TARGET/<t>/mobile/libexponential_ui_ffi.a between our
# cargo and our cp. So: ONE run per target dir at a time (a mkdir lock that
# a dead holder's pid frees), and this run's slices live in its own scratch dir.
LOCK="$TARGET/.exponential-ui-build-ios.lock"
mkdir -p "$TARGET"
until mkdir "$LOCK" 2>/dev/null; do
  holder="$(cat "$LOCK/pid" 2>/dev/null || true)"
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then rm -rf "$LOCK"; continue; fi
  echo "waiting for build-ios.sh pid ${holder:-?} ($LOCK)"; sleep 5
done
echo $$ >"$LOCK/pid"
mkdir -p "$HERE/out"
WORK="$(mktemp -d "$HERE/out/build-ios.XXXXXX")"
trap 'rm -rf "$WORK" "$LOCK"' EXIT

echo "== host build (bindgen reads the dylib's metadata)"
cargo build -p exponential-ui-ffi --release --features cli
echo "== swift bindings"
rm -rf "$HERE/out/swift" && mkdir -p "$HERE/out/swift" "$HERE/bindings/swift"
cargo run -p exponential-ui-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library "$TARGET/release/libexponential_ui_ffi.$EXT" --language swift --out-dir "$HERE/out/swift" --config "$HERE/uniffi.toml"
# VAPP-131: the readers load in place (patch-swift-binding.pl).
perl "$HERE/patch-swift-binding.pl" "$HERE/out/swift/ExponentialUIFFI.swift"
cp "$HERE/out/swift/ExponentialUIFFI.swift" "$HERE/bindings/swift/ExponentialUIFFI.swift"
cp "$HERE/out/swift/ExponentialUIFFIFFI.h" "$HERE/bindings/swift/ExponentialUIFFIFFI.h"
# uniffi names it <module>FFI.modulemap; an xcframework needs module.modulemap.
cp "$HERE/out/swift/ExponentialUIFFIFFI.modulemap" "$HERE/bindings/swift/module.modulemap"
SWIFT_PKG="$(cd "$HERE/../../../../packages/exponential-ui-swift" && pwd)"
mkdir -p "$SWIFT_PKG/Sources/ExponentialUICore"
cp "$HERE/bindings/swift/ExponentialUIFFI.swift" "$SWIFT_PKG/Sources/ExponentialUICore/ExponentialUIFFI.swift"

echo "== apple targets (profile mobile: LTO, stripped; no \`cli\` feature)"
# One fat slice per platform: device arm64; simulator arm64 + x86_64 (Intel
# Macs, Rosetta CI runners); macOS arm64 + x86_64.
APPLE_TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin)
if [ -n "${EXPONENTIAL_UI_APPLE_TARGETS:-}" ]; then read -r -a APPLE_TARGETS <<<"$EXPONENTIAL_UI_APPLE_TARGETS"; fi
FEATURE_ARGS=()
if [ -n "${EXPONENTIAL_UI_FEATURES+x}" ]; then FEATURE_ARGS=(--no-default-features --features "$EXPONENTIAL_UI_FEATURES"); fi
THIN="$WORK/thin"
for t in "${APPLE_TARGETS[@]}"; do
  cargo rustc -p exponential-ui-ffi --profile mobile --target "$t" --crate-type staticlib ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"}
  # Strip a COPY (debug info + local symbols of the std objects cargo cannot
  # strip from an archive); cargo's own output stays as it was built.
  mkdir -p "$THIN/$t"
  cp "$TARGET/$t/mobile/libexponential_ui_ffi.a" "$THIN/$t/libexponential_ui_ffi.a"
  strip -S -x "$THIN/$t/libexponential_ui_ffi.a"
done
lib() { echo "$THIN/$1/libexponential_ui_ffi.a"; }

echo "== xcframework"
HEADERS="$WORK/headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$HERE/bindings/swift/ExponentialUIFFIFFI.h" "$HEADERS/"
cp "$HERE/bindings/swift/module.modulemap" "$HEADERS/module.modulemap"
FAT="$WORK/fat"
built() { [ -f "$(lib "$1")" ]; }
# One -library per platform the built targets fill; two archs = one lipo'd slice.
slice() {
  local name="$1"; shift
  local libs=()
  for t in "$@"; do built "$t" && libs+=("$(lib "$t")"); done
  [ ${#libs[@]} -eq 0 ] && return 0
  mkdir -p "$FAT/$name"
  if [ ${#libs[@]} -eq 1 ]; then cp "${libs[0]}" "$FAT/$name/libexponential_ui_ffi.a"
  else lipo -create "${libs[@]}" -output "$FAT/$name/libexponential_ui_ffi.a"; fi
  XCF_ARGS+=(-library "$FAT/$name/libexponential_ui_ffi.a" -headers "$HEADERS")
}
XCF_ARGS=()
slice ios aarch64-apple-ios
slice ios-simulator aarch64-apple-ios-sim x86_64-apple-ios
slice macos aarch64-apple-darwin x86_64-apple-darwin
XCF="$SWIFT_PKG/Binaries/ExponentialUIFFI.xcframework"
rm -rf "$HERE/out/ExponentialUIFFI.xcframework" "$XCF" && mkdir -p "$SWIFT_PKG/Binaries"
xcodebuild -create-xcframework "${XCF_ARGS[@]}" -output "$XCF" | tail -2
for slice in "$XCF"/*/libexponential_ui_ffi.a; do
  echo "$(basename "$(dirname "$slice")"): $(lipo -archs "$slice") $(du -h "$slice" | cut -f1)"
done
du -sh "$XCF"
