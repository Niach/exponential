#!/usr/bin/env bash
# Builds libpeer_ffi.so for arm64-v8a / armeabi-v7a / x86_64, records sizes (only once peer-core is
# real: a stub core is LTO'd away and its size means nothing), generates the UniFFI Kotlin bindings
# and copies .so + bindings + DevTickets.kt into PeerHello. Idempotent.
#   spike/vapp-3/android/build-so.sh [--features full,rust-crypto|minimal,rust-crypto] [--no-record]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
SPIKE="$ROOT/spike/vapp-3"
FEATURES="full,rust-crypto"; RECORD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --features) FEATURES="$2"; shift 2 ;;
    --no-record) RECORD=0; shift ;;
    *) echo "unknown arg $1" >&2; exit 2 ;;
  esac
done
VARIANT="${FEATURES//,/-}"
export CARGO_TARGET_DIR="$SPIKE/rust/target"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/28.2.13676358}"
MAN="$SPIKE/rust/Cargo.toml"
OUT="$HERE/out/jniLibs"
rm -rf "$OUT"; mkdir -p "$OUT"
cd "$SPIKE/rust"   # cargo-ndk reads Cargo.toml from the cwd
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -P 26 -o "$OUT" \
  build --release --manifest-path "$MAN" -p peer-ffi --no-default-features --features "$FEATURES"
# Keep a copy per variant (size comparisons).
mkdir -p "$HERE/out/variants/$VARIANT"; rm -rf "$HERE/out/variants/$VARIANT/jniLibs"
cp -R "$OUT" "$HERE/out/variants/$VARIANT/jniLibs"

READELF="$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-readelf | head -1)"
CORE_STATE="stub"
if [ "$(grep -c NotImplemented "$SPIKE/rust/peer-core/src/link.rs" || true)" -le 2 ]; then CORE_STATE="core"; fi
for so in "$OUT"/*/libpeer_ffi.so; do
  abi="$(basename "$(dirname "$so")")"
  bytes=$(stat -f%z "$so")
  align=$("$READELF" -lW "$so" | awk '/LOAD/{print $NF}' | sort -u | tr '\n' ' ')
  echo "$abi $bytes bytes, LOAD align: $align"
  if [ "$RECORD" = 1 ] && [ "$CORE_STATE" = core ]; then
    printf '{"kind":"size","at":"%s","platform":"android","artifact":"libpeer_ffi.so %s","variant":"%s","bytes":%s,"notes":"cargo-ndk stripped, opt-z fat LTO, API 26; LOAD align %s"}\n' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$abi" "$VARIANT" "$bytes" "$(echo $align)" >> "$SPIKE/results/c-size.jsonl"
  fi
done
[ "$CORE_STATE" = stub ] && echo "peer-core is still the stub: sizes NOT recorded (LTO strips str0m)"

# UniFFI Kotlin. cargo-ndk STRIPS the .so, which drops the UniFFI metadata, so bindgen reads the
# host cdylib (same crate, same metadata). uniffi-kotlin.toml renames PeerLink.close -> closeLink
# (it clashes with the AutoCloseable.close every generated Kotlin object has).
KOUT="$HERE/out/kotlin"; rm -rf "$KOUT"; mkdir -p "$KOUT"
cargo build -q --release --manifest-path "$MAN" -p peer-ffi --no-default-features --features "$FEATURES"
cargo run -q --manifest-path "$MAN" -p peer-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$CARGO_TARGET_DIR/release/libpeer_ffi.dylib" --language kotlin --out-dir "$KOUT" \
  --no-format --config "$HERE/uniffi-kotlin.toml"

# Into PeerHello.
APP="$HERE/PeerHello/app/src/main"
rm -rf "$APP/jniLibs" "$APP/java/uniffi"; mkdir -p "$APP/jniLibs" "$APP/java"
cp -R "$OUT"/. "$APP/jniLibs/"
cp -R "$KOUT/uniffi" "$APP/java/uniffi"
"$HERE/write-dev-tickets.sh"
echo "done: $OUT, $KOUT"
