#!/usr/bin/env bash
# EXP-1420: builds a scratch crate OUTSIDE this workspace that depends on one
# library's crates the way another repo (spezl) does: `git` + a commit of this
# repo, the ztracing [patch] copied, the desktop Cargo.lock as the starting
# lock (zed floats in the manifests; the lock is the pin, eui-gpui README).
# Proves the git-tag policy in docs/libraries.md: no path escapes a checkout,
# no app crate, the include_str! fixtures resolve from the git checkout.
# EXP-1460: a lib built on other libs (etalk-gpui: eui-core/eui-gpui,
# epilot/epilot-screen) is pinned TOGETHER with their tags, as a consumer of
# both does, and the lock must hold ONE copy of every crate of this repo
# (`cargo tree -d`).
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

# The libs a consumer of this one also pins (`<lib>:<crates>`), each at the
# tag this lib's manifests name for it, else at that lib's newest tag.
case "$lib" in
  etalk) peers="eui:eui-core,eui-gpui epilot:epilot,epilot-screen" ;;
  *) peers="" ;;
esac
manifests="$(for c in $crates; do git -C "$repo" show "$rev:apps/desktop/crates/$c/Cargo.toml"; done)"
tagged=""
for p in $peers; do
  plib="${p%%:*}"
  tag="$(printf '%s\n' "$manifests" | grep -o "tag = \"$plib-v[^\"]*\"" | cut -d'"' -f2 | sort -u || true)"
  [ -n "$tag" ] || tag="$(git -C "$repo" tag -l "$plib-v*" --sort=-v:refname | head -1)"
  [ "$(printf '%s\n' "$tag" | wc -l)" = 1 ] || { echo "$lib names more than one $plib tag: $tag" >&2; exit 1; }
  for c in $(printf %s "${p#*:}" | tr , " "); do
    tagged+="$c = { git = \"https://github.com/Niach/exponential\", tag = \"$tag\" }"$'\n'
  done
done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
src="git = \"file://$repo\", rev = \"$rev\""
{
  printf '[package]\nname = "consumer-smoke"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[dependencies]\n'
  for c in $crates; do printf '%s = { %s }\n' "$c" "$src"; done
  printf '%s' "$tagged"
  printf '\n# A dependency'"'"'s [patch] never reaches its consumers: copied (docs/libraries.md).\n'
  printf '[patch."https://github.com/zed-industries/zed"]\nztracing = { %s }\nztracing_macro = { %s }\n' "$src" "$src"
} > "$tmp/Cargo.toml"
for c in $crates $(printf '%s' "$tagged" | cut -d' ' -f1); do printf 'pub use %s;\n' "${c//-/_}"; done > "$tmp/src/lib.rs"
cp "$repo/apps/desktop/Cargo.lock" "$tmp/Cargo.lock"
cp "$repo/apps/desktop/rust-toolchain.toml" "$tmp/" 2>/dev/null || true

echo "--- $lib @ $rev: $crates"
# rustup picks the toolchain from the working directory: the copied pin.
cd "$tmp"
cat "$tmp/Cargo.toml"
# One copy of every crate of this repo: two sources of eui-gpui (the etalk
# tag's checkout + the eui tag) would build, but their types never unify.
ours="$(cd "$repo/apps/desktop/crates" && ls -d */ | tr -d / | paste -sd'|')"
cargo tree --manifest-path "$tmp/Cargo.toml" --config 'net.git-fetch-with-cli=true' \
  -d --depth 0 --target all -e normal,build > "$tmp/dupes.txt"
dupes="$(grep -E "^($ours) v" "$tmp/dupes.txt" || true)"
if [ -n "$dupes" ]; then
  echo "a git consumer gets two copies of this repo's crates:" >&2
  echo "$dupes" >&2
  exit 1
fi
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/apps/desktop/target}/consumer-smoke" \
  cargo check --manifest-path "$tmp/Cargo.toml" --config 'net.git-fetch-with-cli=true'

# The lock kept the workspace's zed pin (a fresh resolve would float it).
want="$(grep -o 'zed-industries/zed#[0-9a-f]*' "$repo/apps/desktop/Cargo.lock" | sort -u || true)"
got="$(grep -o 'zed-industries/zed#[0-9a-f]*' "$tmp/Cargo.lock" | sort -u || true)"
if [ -n "$got" ] && [ "$got" != "$want" ]; then
  echo "the consumer's zed pin drifted: $got (workspace: $want)" >&2
  exit 1
fi
echo "--- $lib: a git consumer builds, one copy of each shared crate"
