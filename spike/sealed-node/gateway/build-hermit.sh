#!/bin/bash
# Build the gateway as a Hermit unikernel image (x86_64).
#
# Needs: rustup, the pinned nightly + rust-src (installed on demand), git.
# The Hermit kernel is built from a LOCAL checkout of hermit-rs at tag
# hermit-0.13.2 with ../hermit/*.patch applied (vsock fixes, see RESULTS.md).
#
# usage: build-hermit.sh <work-dir>   -> <work-dir>/target/x86_64-unknown-hermit/release/sealed-gateway
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(readlink -f "${1:-/tmp/sealed}")
toolchain=nightly-2026-02-01   # = hermit-rs' rust-toolchain.toml at hermit-0.13.2
rustup toolchain install $toolchain --profile minimal --component rust-src,llvm-tools >/dev/null

src=$work/dl/hermit-rs
if [ ! -d "$src" ]; then
  git clone -q --depth 1 --branch hermit-0.13.2 https://github.com/hermit-os/hermit-rs.git "$src"
  git -C "$src" submodule update --init --depth 1 kernel
  for p in "$here"/../hermit/*.patch; do git -C "$src/kernel" apply "$p"; done
fi

cd "$here"
# LTO off: the prebuilt kernel archive carries no LLVM bitcode.
CARGO_TARGET_DIR=$work/target CARGO_PROFILE_RELEASE_LTO=false HERMIT_MANIFEST_DIR=$src/kernel \
  cargo +$toolchain build -Zbuild-std=std,panic_abort --target x86_64-unknown-hermit --release
ls -l "$work/target/x86_64-unknown-hermit/release/sealed-gateway"
