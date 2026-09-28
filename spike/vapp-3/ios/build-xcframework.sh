#!/usr/bin/env bash
# VAPP-3 Lane B: build PeerFFI.xcframework (device + simulator staticlibs) + Swift bindings,
# record size lines. Idempotent. Usage:
#   spike/vapp-3/ios/build-xcframework.sh [--features full,apple-crypto|full,rust-crypto|minimal,apple-crypto] [--no-results] [--no-xcframework]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SPIKE="$(cd "$HERE/.." && pwd)"
RUST="$SPIKE/rust"
export CARGO_TARGET_DIR="$RUST/target"
FEATURES="full,apple-crypto"
WRITE_RESULTS=1
MAKE_XCF=1
while [ $# -gt 0 ]; do
  case "$1" in
    --features) FEATURES="$2"; shift 2 ;;
    --no-results) WRITE_RESULTS=0; shift ;;
    --no-xcframework) MAKE_XCF=0; shift ;;
    *) echo "unknown arg $1" >&2; exit 2 ;;
  esac
done
VARIANT="$(echo "$FEATURES" | tr ',' '-')"
OUT="$HERE/out"
VOUT="$OUT/variants/$VARIANT"
mkdir -p "$VOUT"
# The simulator slice can not share the device deployment target default; keep both explicit.
export IPHONEOS_DEPLOYMENT_TARGET=17.4

for T in aarch64-apple-ios aarch64-apple-ios-sim; do
  echo "== cargo build $T ($FEATURES)"
  # `cargo rustc --crate-type staticlib`: peer-ffi also declares cdylib+lib, and cargo skips
  # LTO when an rlib is among the crate types; overriding to staticlib alone gets the real fat-LTO build.
  cargo rustc --release --manifest-path "$RUST/Cargo.toml" -p peer-ffi \
    --no-default-features --features "$FEATURES" --target "$T" --lib --crate-type staticlib
  mkdir -p "$VOUT/$T"
  cp "$CARGO_TARGET_DIR/$T/release/libpeer_ffi.a" "$VOUT/$T/libpeer_ffi.a"
done

# --- Swift bindings (from a host cdylib: uniffi --library wants a dylib; metadata is identical).
BIND="$OUT/bindings"
HOST_FEATURES="$(echo "$FEATURES" | sed 's/apple-crypto/rust-crypto/')"
cargo build --release --manifest-path "$RUST/Cargo.toml" -p peer-ffi --no-default-features --features "$HOST_FEATURES" --lib
rm -rf "$BIND"; mkdir -p "$BIND"
( cd "$RUST" && cargo run --quiet --manifest-path "$RUST/Cargo.toml" -p peer-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$CARGO_TARGET_DIR/release/libpeer_ffi.dylib" --language swift --out-dir "$BIND" )
ls "$BIND"


# --- Size: the archive, its segment totals, and the dead-stripped linked contribution.
DEV_A="$VOUT/aarch64-apple-ios/libpeer_ffi.a"
A_BYTES=$(stat -f %z "$DEV_A")
SEG=$(xcrun size -arch arm64 "$DEV_A" 2>/dev/null | awk 'NR>1 {t+=$1; d+=$2} END {printf "%d %d", t, d}')
TEXT_SUM=${SEG% *}; DATA_SUM=${SEG#* }
# Link a trivial C main that references every exported uniffi symbol, dead-stripped, and diff it
# against the same main without the archive. This is what an app actually pays for (before
# its own usage prunes further).
LINK="$VOUT/link"; mkdir -p "$LINK"
# (Apple nm chokes on rustc's embedded LLVM-22 bitcode; take the exported names from the header.)
SYMS=$(grep -oE '\b(uniffi_peer_ffi_fn_[a-z0-9_]+|ffi_peer_ffi_[a-z0-9_]+)\(' "$BIND/peer_ffiFFI.h" | tr -d '(' | sort -u | sed 's/^/_/')
{
  echo '#include <stdint.h>'
  echo 'extern void keep(void*);'
  for s in $SYMS; do echo "extern void ${s#_}(void);"; done
  echo 'void keep(void* p) { __asm__ volatile("" :: "r"(p)); }'
  echo 'int main(void) {'
  for s in $SYMS; do echo "  keep((void*)&${s#_});"; done
  echo '  return 0; }'
} > "$LINK/main.c"
echo 'int main(void) { return 0; }' > "$LINK/empty.c"
SDK=$(xcrun --sdk iphoneos --show-sdk-path)
CC="xcrun --sdk iphoneos clang -arch arm64 -miphoneos-version-min=17.4 -isysroot $SDK -Os"
# apple-crypto pulls Swift objects (CryptoKit shims) from the archive: link through swiftc so the
# Swift runtime/compat libs resolve (the runtime itself is in the OS, it costs no bytes).
SWIFTC="xcrun --sdk iphoneos swiftc -target arm64-apple-ios17.4 -sdk $SDK"
$CC -c "$LINK/empty.c" -o "$LINK/empty.o"
$CC -c "$LINK/main.c" -o "$LINK/main.o"
$SWIFTC "$LINK/empty.o" -o "$LINK/empty" -Xlinker -dead_strip
$SWIFTC "$LINK/main.o" "$DEV_A" -o "$LINK/with" -Xlinker -dead_strip \
  -framework Security -framework Foundation -framework CryptoKit -framework SystemConfiguration -lresolv 2>"$LINK/link.log" || {
  echo "link failed, see $LINK/link.log"; tail -20 "$LINK/link.log"; exit 1; }
strip -x "$LINK/with" "$LINK/empty" 2>/dev/null || true
LINKED_DELTA=$(( $(stat -f %z "$LINK/with") - $(stat -f %z "$LINK/empty") ))
LSEG=$(xcrun size -m "$LINK/with" | awk '/^Segment __TEXT/ {t=$3} /^Segment __DATA/ {d+=$3} /^Segment __DATA_CONST/ {} END {printf "%d %d", t, d}')
echo "size: .a=$A_BYTES text=$TEXT_SUM data=$DATA_SUM linked-delta=$LINKED_DELTA ($LSEG)"

if [ "$MAKE_XCF" = 1 ]; then
  HDR="$OUT/headers"; rm -rf "$HDR"; mkdir -p "$HDR"
  cp "$BIND/peer_ffiFFI.h" "$HDR/"
  cp "$BIND/peer_ffiFFI.modulemap" "$HDR/module.modulemap"
  rm -rf "$OUT/PeerFFI.xcframework"
  xcodebuild -create-xcframework \
    -library "$VOUT/aarch64-apple-ios/libpeer_ffi.a" -headers "$HDR" \
    -library "$VOUT/aarch64-apple-ios-sim/libpeer_ffi.a" -headers "$HDR" \
    -output "$OUT/PeerFFI.xcframework"
  echo "$VARIANT" > "$OUT/PeerFFI.xcframework.variant"
  # Hand the bindings to PeerHello.
  GEN="$HERE/PeerHello/Sources/Generated"; mkdir -p "$GEN"
  cp "$BIND/peer_ffi.swift" "$GEN/peer_ffi.swift"
  "$HERE/write-dev-tickets.sh" || true
fi

if [ "$WRITE_RESULTS" = 1 ]; then
  CORE="stub"
  if [ "$(grep -c NotImplemented "$RUST/peer-core/src/link.rs")" -le 2 ]; then CORE="core"; fi
  AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '{"kind":"size","at":"%s","platform":"ios","artifact":"libpeer_ffi.a arm64 device","variant":"%s","bytes":%d,"notes":"peer-core=%s; release .a (opt-z, fat LTO via cargo rustc --crate-type staticlib, panic=abort; profile strip does not apply to staticlibs, objects carry embedded bitcode); __TEXT sum %d, __DATA sum %d over members; dead-stripped link of every exported uniffi fn into an empty iOS main adds %d bytes (linked __TEXT/__DATA %s)"}\n' \
    "$AT" "$VARIANT" "$A_BYTES" "$CORE" "$TEXT_SUM" "$DATA_SUM" "$LINKED_DELTA" "$LSEG" >> "$SPIKE/results/b-size.jsonl"
  printf '{"kind":"size","at":"%s","platform":"ios","artifact":"libpeer_ffi linked arm64 device (dead_strip, all exports kept)","variant":"%s","bytes":%d,"notes":"peer-core=%s; delta of a stripped arm64 executable linking the .a vs an empty main"}\n' \
    "$AT" "$VARIANT" "$LINKED_DELTA" "$CORE" >> "$SPIKE/results/b-size.jsonl"
fi
echo "done: $VARIANT"
