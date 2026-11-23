#!/usr/bin/env bash
# #AXC-12 (VAPP-296): builds the Express compiler's WASM (crate
# `a2ui-express-wasm` of Niach/a2ui-express-compiler-rust, public) into
# packages/exponential-ui/wasm/express/ (COMMITTED: the web renderers
# lazy-load it through `@exponential_app/eui/express`, and CI's gate has no
# wasm toolchain). It builds the SAME rev eui-core pins
# (apps/desktop/crates/eui-core/Cargo.toml `a2ui-express`), so the TS and the
# Rust ExpressDecoder compile with the same compiler; SOURCE_HASH = that rev,
# and src/express/express-wasm.test.ts fails when the pin moves: rerun this.
#
# The full build (compile, stream, decompile, generatePrompt): ~228 KB gzipped.
# Needs: wasm-pack (it fetches the wasm-bindgen CLI the crate pins), the
# wasm32-unknown-unknown target, git.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packages/exponential-ui/wasm/express"
toml="$root/apps/desktop/crates/eui-core/Cargo.toml"
rev="$(grep -E '^a2ui-express = ' "$toml" | sed -E 's/.*rev = "([0-9a-f]+)".*/\1/')"
[[ "$rev" =~ ^[0-9a-f]{7,40}$ ]] || { echo "no a2ui-express rev in $toml" >&2; exit 1; }

src="$(mktemp -d)"
trap 'rm -rf "$src"' EXIT
git -C "$src" init -q
git -C "$src" fetch -q --depth 1 https://github.com/Niach/a2ui-express-compiler-rust "$rev"
git -C "$src" checkout -q FETCH_HEAD

# A RUSTFLAGS variable would replace the size flags of the repo's .cargo/config.toml.
unset RUSTFLAGS
# The repo's own target dir: the shared CARGO_TARGET_DIR would mix in another
# workspace's artifacts.
export CARGO_TARGET_DIR="$src/target"
rm -rf "$out"
wasm-pack build "$src/crates/a2ui-express-wasm" --target web --release --no-pack --out-dir "$out" --out-name a2ui_express
rm -f "$out/.gitignore" "$out/package.json" "$out/README.md"
echo "$rev" > "$out/SOURCE_HASH"
wasm="$out/a2ui_express_bg.wasm"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -n -c "$wasm" | wc -c | tr -d ' ')
echo "express wasm ($rev): $raw bytes, $gz gzipped -> $out"
