#!/usr/bin/env bash
# EXP-1603: builds eUI Motion's WASM (apps/desktop/crates/eui-motion-wasm)
# into packages/exponential-ui/wasm/motion/ (COMMITTED: the server validates
# Motion sources in exponential_show and the web renderers lazy-load it, and
# CI's gate has no wasm toolchain). SOURCE_HASH pins the crate sources the
# binary was built from; packages/exponential-ui's motion-wasm test fails
# when they drift: rerun this script.
#
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli at the
# Cargo.lock version; wasm-opt (binaryen) is used when on PATH or via bunx.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
desktop="$root/apps/desktop"
out="$root/packages/exponential-ui/wasm/motion"
cd "$desktop"

want="$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$have" != "$want" ]]; then
  echo "wasm-bindgen-cli $want required (found ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

cargo build -p eui-motion-wasm --target wasm32-unknown-unknown --profile motion-wasm
target_dir="${CARGO_TARGET_DIR:-$desktop/target}"
rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --out-dir "$out" "$target_dir/wasm32-unknown-unknown/motion-wasm/eui_motion_wasm.wasm"
wasm="$out/eui_motion_wasm_bg.wasm"
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --strip-debug --strip-producers "$wasm" -o "$wasm"
else
  bunx --bun -p binaryen wasm-opt -Oz --strip-debug --strip-producers "$wasm" -o "$wasm"
fi
bun "$root/packages/exponential-ui/scripts/motion-source-hash.ts" > "$out/SOURCE_HASH"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "motion wasm: $raw bytes, $gz gzipped -> $out"
