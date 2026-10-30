#!/usr/bin/env bash
# EXP-1420: builds a scratch crate OUTSIDE this workspace that depends on one
# library's crates the way another repo (spezl) does: `git` + a commit of this
# repo, the ztracing [patch] copied, the desktop Cargo.lock as the starting
# lock (zed floats in the manifests; the lock is the pin, eui-gpui README).
# Proves the git-tag policy in docs/libraries.md: no path escapes a checkout,
# no app crate, the include_str! fixtures resolve from the git checkout.
#
#   scripts/libs/git-consumer-smoke.sh <etalk|epilot|agent-runtime> [rev]
#   (rev defaults to HEAD, which must be committed; the release workflows
#   pass the tagged commit)
set -euo pipefail

lib="${1:?usage: git-consumer-smoke.sh <etalk|epilot|agent-runtime> [rev]}"
repo="$(git rev-parse --show-toplevel)"
rev="${2:-$(git -C "$repo" rev-parse HEAD)}"
case "$lib" in
  etalk) crates="etalk etalk-codec etalk-engine etalk-crispasr etalk-wakeword etalk-gpui" ;;
  epilot) crates="epilot epilot-screen" ;;
  agent-runtime) crates="exponential-agent-runtime" ;;
  *) echo "unknown library: $lib" >&2; exit 2 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
src="git = \"file://$repo\", rev = \"$rev\""
{
  printf '[package]\nname = "consumer-smoke"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[dependencies]\n'
  for c in $crates; do printf '%s = { %s }\n' "$c" "$src"; done
  printf '\n# A dependency'"'"'s [patch] never reaches its consumers: copied (docs/libraries.md).\n'
  printf '[patch."https://github.com/zed-industries/zed"]\nztracing = { %s }\nztracing_macro = { %s }\n' "$src" "$src"
} > "$tmp/Cargo.toml"
for c in $crates; do printf 'pub use %s;\n' "${c//-/_}"; done > "$tmp/src/lib.rs"
cp "$repo/apps/desktop/Cargo.lock" "$tmp/Cargo.lock"
cp "$repo/apps/desktop/rust-toolchain.toml" "$tmp/" 2>/dev/null || true

echo "--- $lib @ $rev: $crates"
cat "$tmp/Cargo.toml"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/apps/desktop/target}/consumer-smoke" \
  cargo check --manifest-path "$tmp/Cargo.toml" --config 'net.git-fetch-with-cli=true'

# The lock kept the workspace's zed pin (a fresh resolve would float it).
want="$(grep -o 'zed-industries/zed#[0-9a-f]*' "$repo/apps/desktop/Cargo.lock" | sort -u || true)"
got="$(grep -o 'zed-industries/zed#[0-9a-f]*' "$tmp/Cargo.lock" | sort -u || true)"
if [ -n "$got" ] && [ "$got" != "$want" ]; then
  echo "the consumer's zed pin drifted: $got (workspace: $want)" >&2
  exit 1
fi
echo "--- $lib: a git consumer builds"
