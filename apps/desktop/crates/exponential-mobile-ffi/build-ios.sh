#!/usr/bin/env bash
# #PERF-11: the iOS app's ONE native library: genUI's, the Engine's and P2P's
# UniFFI facades as one static library (one Rust std, one copy of what they
# share), linked into ONE image: ExpCore.framework.
#   bash apps/desktop/crates/exponential-mobile-ffi/build-ios.sh   (from anywhere)
# Outputs (gitignored):
#   apps/desktop/crates/exponential-mobile-ffi/out/ExponentialMobileFFI.xcframework
#     libexponential_mobile_ffi.a + the P2P C module (`ExponentialNetFFIFFI`,
#     the binding ExpCore compiles). ExpCore (a dynamic framework) links it,
#     so all three facades live in ExpCore and export their symbols from it.
#   packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework
#   apps/desktop/crates/eengine-ffi/out/ExponentialEngineFFI.xcframework
#     LINK STUBS: the facades' headers + an empty archive, where the SDK's
#     Swift packages (`ExponentialUI`, `ExponentialEngine`) look for their
#     xcframework. Their bindings compile against the headers and resolve
#     against ExpCore when the app links, so the code is not linked twice.
#     The facades' own build-ios.sh overwrite them with the standalone
#     libraries (SDK work: the Swift package tests, the iOS runner, which
#     treat a `LINK_STUB` marker as missing); rerun this script before
#     building the app again.
# The bindings stay the facades' own COMMITTED sources (their build-ios.sh /
# generate-bindings.sh regenerate them); nothing here generates any.
# Knob: MOBILE_APPLE_TARGETS="aarch64-apple-ios" builds only those rust
#       targets and only the slices they fill (the App Store archive needs
#       the device slice alone). Default: device + simulator (arm64, x86_64).
# Each slice = profile `mobile` (LTO, stripped), built as ONE crate type
# (`cargo rustc --crate-type staticlib`) so the profile's LTO applies.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
CRATES="$(cd "$HERE/.." && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$DESKTOP/../.." && pwd)"
cd "$DESKTOP"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
APPLE_TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
if [ -n "${MOBILE_APPLE_TARGETS:-}" ]; then read -r -a APPLE_TARGETS <<<"$MOBILE_APPLE_TARGETS"; fi
rustup target add "${APPLE_TARGETS[@]}" >/dev/null
# One run per target dir at a time (as exponential-ui-ffi's VAPP-165 lock):
# another worktree's build can rewrite $TARGET/<t>/mobile/*.a between our
# cargo and our cp.
LOCK="$TARGET/.exponential-mobile-ffi-build-ios.lock"
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

echo "== apple targets (profile mobile: LTO, stripped)"
THIN="$WORK/thin"
for t in "${APPLE_TARGETS[@]}"; do
  cargo rustc -p exponential-mobile-ffi --profile mobile --target "$t" --crate-type staticlib
  # Strip a COPY (debug info + local symbols cargo cannot strip from an archive).
  mkdir -p "$THIN/$t"
  cp "$TARGET/$t/mobile/libexponential_mobile_ffi.a" "$THIN/$t/libexponential_mobile_ffi.a"
  strip -S -x "$THIN/$t/libexponential_mobile_ffi.a"
done

# The empty archive a stub carries, per rust target (clang's triple: the
# xcframework reads the platform from the object's build version).
STUBS="$WORK/stubs"
stub_lib() {
  local t="$1" name="$2" triple sdk
  case "$t" in
    aarch64-apple-ios) triple=arm64-apple-ios17.0; sdk=iphoneos ;;
    aarch64-apple-ios-sim) triple=arm64-apple-ios17.0-simulator; sdk=iphonesimulator ;;
    x86_64-apple-ios) triple=x86_64-apple-ios17.0-simulator; sdk=iphonesimulator ;;
    *) echo "unknown target: $t" >&2; exit 2 ;;
  esac
  mkdir -p "$STUBS/$name/$t"
  echo "int exponential_mobile_ffi_stub_$name = 0;" >"$STUBS/$name/$t/stub.c"
  xcrun -sdk "$sdk" clang -target "$triple" -c "$STUBS/$name/$t/stub.c" -o "$STUBS/$name/$t/stub.o"
  libtool -static -o "$STUBS/$name/$t/lib.a" "$STUBS/$name/$t/stub.o" 2>/dev/null
  echo "$STUBS/$name/$t/lib.a"
}

# xcframework <output> <archive name> <headers dir> <lib-of-target fn>
xcframework() {
  local out="$1" archive="$2" headers="$3" libfn="$4" fat="$WORK/fat/$(basename "$1")"
  local args=()
  slice() {
    local name="$1"; shift
    local libs=()
    for t in "$@"; do
      for want in "${APPLE_TARGETS[@]}"; do [ "$want" = "$t" ] && libs+=("$($libfn "$t")"); done
    done
    [ ${#libs[@]} -eq 0 ] && return 0
    mkdir -p "$fat/$name"
    if [ ${#libs[@]} -eq 1 ]; then cp "${libs[0]}" "$fat/$name/$archive"
    else lipo -create "${libs[@]}" -output "$fat/$name/$archive"; fi
    args+=(-library "$fat/$name/$archive" -headers "$headers")
  }
  slice ios aarch64-apple-ios
  slice ios-simulator aarch64-apple-ios-sim x86_64-apple-ios
  rm -rf "$out" && mkdir -p "$(dirname "$out")"
  xcodebuild -create-xcframework "${args[@]}" -output "$out" | tail -1
  # A stub says so (the iOS runner's scripts rebuild the real library over it).
  if [ "$archive" != libexponential_mobile_ffi.a ]; then
    echo "#PERF-11 link stub (exponential-mobile-ffi/build-ios.sh): headers only, the code is in ExpCore." >"$out/LINK_STUB"
  fi
}

echo "== ExponentialMobileFFI.xcframework"
# Headers in a directory named after the clang module (an archive copies every
# static xcframework's headers into ONE BuildProductsPath/include).
NET="$CRATES/exponential-net-ffi/bindings/swift"
mkdir -p "$WORK/headers/mobile/ExponentialNetFFIFFI"
cp "$NET/ExponentialNetFFIFFI.h" "$NET/module.modulemap" "$WORK/headers/mobile/ExponentialNetFFIFFI/"
mobile_lib() { echo "$THIN/$1/libexponential_mobile_ffi.a"; }
xcframework "$HERE/out/ExponentialMobileFFI.xcframework" libexponential_mobile_ffi.a "$WORK/headers/mobile" mobile_lib

echo "== link stubs (the SDK packages' headers, no code)"
# genUI: header + module.modulemap at the root, as its own build-ios.sh lays them out.
UI="$CRATES/exponential-ui-ffi/bindings/swift"
mkdir -p "$WORK/headers/ui"
cp "$UI/ExponentialUIFFIFFI.h" "$UI/module.modulemap" "$WORK/headers/ui/"
ui_stub() { stub_lib "$1" ui; }
xcframework "$REPO/packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework" libexponential_ui_ffi.a "$WORK/headers/ui" ui_stub
# The Engine: in its clang module's directory, as eengine-ffi/build-ios.sh.
ENGINE="$CRATES/eengine-ffi/bindings/swift"
mkdir -p "$WORK/headers/engine/ExponentialEngineFFIFFI"
cp "$ENGINE/ExponentialEngineFFIFFI.h" "$ENGINE/module.modulemap" "$WORK/headers/engine/ExponentialEngineFFIFFI/"
engine_stub() { stub_lib "$1" engine; }
xcframework "$CRATES/eengine-ffi/out/ExponentialEngineFFI.xcframework" libeengine_ffi.a "$WORK/headers/engine" engine_stub

for s in "$HERE/out/ExponentialMobileFFI.xcframework"/*/libexponential_mobile_ffi.a; do
  echo "$(basename "$(dirname "$s")"): $(lipo -archs "$s") $(du -h "$s" | cut -f1)"
done
du -sh "$HERE/out/ExponentialMobileFFI.xcframework"
