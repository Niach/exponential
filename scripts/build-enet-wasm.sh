#!/usr/bin/env bash
# EXP-1457: builds the browser's eNet node (apps/desktop/crates/exponential-net-wasm)
# into apps/web/public/enet/ (gitignored, like public/widget/). The web app
# loads it lazily after sign-in (apps/web/src/lib/enet/), never in the initial
# bundle. CI builds it before the image build (build-web.yml); the Dockerfile
# needs no Rust, `COPY . .` picks the artifact up.
#
# Needs: the wasm32-unknown-unknown target, rustup's llvm-tools (Apple `ar`
# writes EMPTY archives for ring's wasm objects, so the link fails on
# `ring_core_*` symbols), and wasm-bindgen-cli at EXACTLY the wasm-bindgen
# version in apps/desktop/Cargo.lock:
#   rustup target add wasm32-unknown-unknown && rustup component add llvm-tools
#   cargo install wasm-bindgen-cli --version <lock version> --locked
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
desktop="$root/apps/desktop"
out="$root/apps/web/public/enet"
cd "$desktop"

want="$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$have" != "$want" ]]; then
  echo "wasm-bindgen-cli $want required (found ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

sysroot="$(rustc --print sysroot)"
host="$(rustc -vV | awk '/^host:/{print $2}')"
llvm_ar="$sysroot/lib/rustlib/$host/bin/llvm-ar"
if [[ ! -x "$llvm_ar" ]]; then
  echo "llvm-ar missing: rustup component add llvm-tools" >&2
  exit 1
fi
export AR_wasm32_unknown_unknown="$llvm_ar"

cargo build -p exponential-net-wasm --target wasm32-unknown-unknown --profile wasm

target_dir="${CARGO_TARGET_DIR:-$desktop/target}"
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" \
  "$target_dir/wasm32-unknown-unknown/wasm/exponential_net_wasm.wasm"

wasm="$out/exponential_net_wasm_bg.wasm"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "enet wasm: $raw bytes, $gz gzipped -> $out"
