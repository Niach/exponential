#!/usr/bin/env bash
# EDE-115: builds the web's chat store (apps/desktop/crates/chat-store-wasm)
# into apps/web/public/chat-store/ (gitignored, like public/enet/). The web
# loads it lazily inside its chat-store worker (apps/web/src/lib/chat-store/),
# never in the main bundle. CI builds it before the image build
# (build-web.yml, beside the eNet wasm); the Dockerfile needs no Rust.
#
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at EXACTLY the
# wasm-bindgen version in apps/desktop/Cargo.lock:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <lock version> --locked
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
desktop="$root/apps/desktop"
out="${1:-$root/apps/web/public/chat-store}"
cd "$desktop"

want="$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$have" != "$want" ]]; then
  echo "wasm-bindgen-cli $want required (found ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

cargo build --locked -p chat-store-wasm --target wasm32-unknown-unknown --profile wasm

target_dir="${CARGO_TARGET_DIR:-$desktop/target}"
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" \
  "$target_dir/wasm32-unknown-unknown/wasm/chat_store_wasm.wasm"

wasm="$out/chat_store_wasm_bg.wasm"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "chat-store wasm: $raw bytes, $gz gzipped -> $out"
