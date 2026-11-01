#!/usr/bin/env bash
# EXP-1509: builds eAgent's orchestration reducer (apps/desktop/crates/
# eagent-hub-core-wasm) into packages/eagent/wasm/ (gitignored): `reduce` and
# `hydrate`, JSON in, JSON out. `packages/eagent/scripts/wasm-replay.test.ts`
# replays every fixture through it (release-suite.yml `eagent-wasm-replay`).
#
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at EXACTLY the
# wasm-bindgen version in apps/desktop/Cargo.lock:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <lock version> --locked
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
desktop="$root/apps/desktop"
out="${1:-$root/packages/eagent/wasm}"
cd "$desktop"

want="$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$have" != "$want" ]]; then
  echo "wasm-bindgen-cli $want required (found ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

cargo build --locked -p eagent-hub-core-wasm --target wasm32-unknown-unknown --profile wasm

target_dir="${CARGO_TARGET_DIR:-$desktop/target}"
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" \
  "$target_dir/wasm32-unknown-unknown/wasm/eagent_hub_core_wasm.wasm"

wasm="$out/eagent_hub_core_wasm_bg.wasm"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "eagent-hub-core wasm: $raw bytes, $gz gzipped -> $out"
