#!/usr/bin/env bash
# Build an embeddable libghostty.dylib (+ ghostty.h) for macOS (Metal embedded
# apprt). Unlike Linux, macOS is the apprt's native embedded target -- no GLAD/GL
# shim, no patchelf. We build the same pinned ghostty the Linux app uses so the
# C ABI (ghostty.h) matches, with a LOCAL zig 0.15.2 (ghostty 1.3.1 needs it;
# the app links the result as a plain C-ABI dylib regardless of its own zig).
#
# Output: apps/ios/vendor/ghostty-install/{lib/libghostty.dylib,include/ghostty.h,share/terminfo}
set -euo pipefail

GHOSTTY_FORK_URL="https://github.com/douglas/ghostty.git"
GHOSTTY_COMMIT="c5028f99876a35188329f65742fddb45de3c5360"
ZIG_VERSION="0.15.2"
ARCH="$(uname -m)"
ZIG_ARCH="aarch64"; [ "$ARCH" = "x86_64" ] && ZIG_ARCH="x86_64"
ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/zig-${ZIG_ARCH}-macos-${ZIG_VERSION}.tar.xz"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"
VENDOR_DIR="$IOS_DIR/vendor"
GHOSTTY_DIR="$VENDOR_DIR/ghostty"
INSTALL_DIR="$VENDOR_DIR/ghostty-install"
TOOLCHAIN_DIR="$VENDOR_DIR/.toolchain"
ZIG_DIR="$TOOLCHAIN_DIR/zig-${ZIG_VERSION}"
ZIG="$ZIG_DIR/zig"

say() { printf '\033[1;35m==>\033[0m %s\n' "$*"; }

for tool in git curl tar; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done
mkdir -p "$VENDOR_DIR" "$TOOLCHAIN_DIR"

# --- 1. local zig 0.15.2 (build-only) ---------------------------------------
if [ ! -x "$ZIG" ]; then
  say "Fetching zig ${ZIG_VERSION} (${ZIG_ARCH}-macos, local)"
  tmp="$TOOLCHAIN_DIR/zig.tar.xz"
  curl -fsSL "$ZIG_URL" -o "$tmp"
  tar -xJf "$tmp" -C "$TOOLCHAIN_DIR"
  rm -rf "$ZIG_DIR"
  mv "$TOOLCHAIN_DIR/zig-${ZIG_ARCH}-macos-${ZIG_VERSION}" "$ZIG_DIR"
  rm -f "$tmp"
fi
say "zig: $("$ZIG" version)"

# --- 2. ghostty @ pinned commit ---------------------------------------------
if [ ! -f "$GHOSTTY_DIR/build.zig" ]; then
  say "Cloning ghostty"
  git clone --filter=blob:none "$GHOSTTY_FORK_URL" "$GHOSTTY_DIR"
fi
if [ "$(git -C "$GHOSTTY_DIR" rev-parse HEAD)" != "$GHOSTTY_COMMIT" ]; then
  say "Checking out pinned commit"
  git -C "$GHOSTTY_DIR" fetch --depth 1 origin "$GHOSTTY_COMMIT"
  git -C "$GHOSTTY_DIR" checkout --detach "$GHOSTTY_COMMIT"
fi

# --- 3. build libghostty (embedded runtime, Metal) --------------------------
say "Building libghostty (-Dapp-runtime=none, ReleaseFast); fetches deps + takes a few minutes"
( cd "$GHOSTTY_DIR" && "$ZIG" build \
    -Dapp-runtime=none \
    -Doptimize=ReleaseFast \
    -Demit-terminfo=true \
    --prefix "$INSTALL_DIR" )

LIB_DIR="$INSTALL_DIR/lib"
DYLIB="$LIB_DIR/libghostty.dylib"
[ -f "$DYLIB" ] || { echo "libghostty.dylib not produced -- check the build output" >&2; exit 1; }

# --- 4. normalize install name for local-dev linking -----------------------
install_name_tool -id "$DYLIB" "$DYLIB" 2>/dev/null || true

say "Done."
echo "  lib:     $DYLIB"
echo "  header:  $INSTALL_DIR/include/ghostty.h"
echo "  terminfo:$INSTALL_DIR/share/terminfo"
