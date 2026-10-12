#!/bin/bash
# Fetch the two prebuilt boot artifacts (no sudo needed):
#  - the Hermit multiboot loader (QEMU -kernel for the gateway unikernel)
#  - an Ubuntu generic kernel for the agent VM, matching the HOST's running
#    kernel so the host's vsock modules can be copied into the initramfs
#    (/boot/vmlinuz-* is root-only on Ubuntu/Mint; `apt download` is not).
# usage: fetch-deps.sh <work-dir>
set -euo pipefail
work=$(readlink -f "${1:-/tmp/sealed}")
mkdir -p "$work/dl/loader" "$work/kernel"
curl -fsSLo "$work/dl/loader/hermit-loader-x86_64-multiboot" \
  https://github.com/hermit-os/loader/releases/download/v0.5.7/hermit-loader-x86_64-multiboot
cd "$work/kernel"
apt download "linux-image-unsigned-$(uname -r)"
# Mint's apt wrapper may hand back a tarball instead of the .deb.
if [ -f "linux-image-unsigned-$(uname -r).tar.gz" ]; then tar xzf "linux-image-unsigned-$(uname -r).tar.gz"; fi
deb=$(find . -name "linux-image-unsigned-$(uname -r)_*.deb" | head -1)
dpkg-deb -x "$deb" x
cp "x/boot/vmlinuz-$(uname -r)" vmlinuz
ls -l "$work/dl/loader/hermit-loader-x86_64-multiboot" "$work/kernel/vmlinuz"
