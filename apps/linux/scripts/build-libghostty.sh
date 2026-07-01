#!/usr/bin/env bash
# Build an embeddable libghostty.so (+ ghostty.h, libglad.so) for GTK4/Linux.
#
# Upstream ghostty's embedded apprt only supports macOS/iOS; the Linux embedded
# platform (GHOSTTY_PLATFORM_LINUX) lives in the douglas/ghostty fork (the same
# one cmux-gtk uses). We pin that fork, build it with `-Dapp-runtime=none`
# (= libghostty, host provides the GTK GL surface), and wire GLAD.
#
# ghostty 1.3.1 needs zig 0.15.2; our app uses 0.16.0. That's fine — libghostty
# is a C-ABI .so, so we build it once with a LOCAL 0.15.2 (not your system zig)
# and link it from the 0.16 app. Idempotent: re-running skips finished steps.
#
# Output: apps/linux/vendor/ghostty-install/{lib/libghostty.so,lib/libglad.so,include/ghostty.h}
set -euo pipefail

GHOSTTY_FORK_URL="https://github.com/douglas/ghostty.git"
GHOSTTY_COMMIT="c5028f99876a35188329f65742fddb45de3c5360"
ZIG_VERSION="0.15.2"
ZIG_ARCH="$(uname -m)" # aarch64 or x86_64 — matches Zig's release tarball naming directly
ZIG_TARBALL_NAME="zig-${ZIG_ARCH}-linux-${ZIG_VERSION}"
ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/${ZIG_TARBALL_NAME}.tar.xz"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LINUX_DIR="$(dirname "$SCRIPT_DIR")"
VENDOR_DIR="$LINUX_DIR/vendor"
GHOSTTY_DIR="$VENDOR_DIR/ghostty"
INSTALL_DIR="$VENDOR_DIR/ghostty-install"
TOOLCHAIN_DIR="$VENDOR_DIR/.toolchain"
ZIG_DIR="$TOOLCHAIN_DIR/zig-${ZIG_VERSION}"
ZIG="$ZIG_DIR/zig"

say() { printf '\033[1;35m==>\033[0m %s\n' "$*"; }

# --- tool check -------------------------------------------------------------
for tool in git gcc patchelf tic curl tar; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done

mkdir -p "$VENDOR_DIR" "$TOOLCHAIN_DIR"

# --- 1. local zig 0.15.2 ----------------------------------------------------
if [ ! -x "$ZIG" ]; then
  say "Fetching zig ${ZIG_VERSION} (local, build-only)…"
  tmp="$TOOLCHAIN_DIR/zig.tar.xz"
  curl -fsSL "$ZIG_URL" -o "$tmp"
  tar -xJf "$tmp" -C "$TOOLCHAIN_DIR"
  mv "$TOOLCHAIN_DIR/${ZIG_TARBALL_NAME}" "$ZIG_DIR"
  rm -f "$tmp"
fi
say "zig: $("$ZIG" version)"

# --- 2. ghostty fork @ pinned commit ----------------------------------------
if [ ! -f "$GHOSTTY_DIR/build.zig" ]; then
  say "Cloning douglas/ghostty fork…"
  git clone --filter=blob:none "$GHOSTTY_FORK_URL" "$GHOSTTY_DIR"
fi
if [ "$(git -C "$GHOSTTY_DIR" rev-parse HEAD)" != "$GHOSTTY_COMMIT" ]; then
  say "Checking out $GHOSTTY_COMMIT…"
  git -C "$GHOSTTY_DIR" fetch --depth 1 origin "$GHOSTTY_COMMIT"
  git -C "$GHOSTTY_DIR" checkout --detach "$GHOSTTY_COMMIT"
fi

# --- 3. build libghostty (embedded runtime) ---------------------------------
say "Building libghostty (-Dapp-runtime=none, ReleaseFast)… this fetches deps + takes a few minutes"
( cd "$GHOSTTY_DIR" && "$ZIG" build \
    -Dapp-runtime=none \
    -Doptimize=ReleaseFast \
    -Demit-terminfo=true \
    --prefix "$INSTALL_DIR" )

LIB_DIR="$INSTALL_DIR/lib"
[ -f "$LIB_DIR/libghostty.so" ] || { echo "libghostty.so not produced — check the build output" >&2; exit 1; }

# --- 4. GLAD (OpenGL loader) ------------------------------------------------
# ghostty excludes GLAD, expecting the host to provide it. Build as .so so the
# dynamic linker resolves gladLoaderLoad/UnloadGLContext at runtime.
GLAD_DIR="$GHOSTTY_DIR/vendor/glad"
if [ ! -f "$LIB_DIR/libglad.so" ]; then
  say "Building libglad.so…"
  gcc -shared -fPIC -o "$LIB_DIR/libglad.so" \
    -I "$GLAD_DIR/include" "$GLAD_DIR/src/gl.c"
fi

# --- 5. wire GLAD into libghostty.so ----------------------------------------
if ! patchelf --print-needed "$LIB_DIR/libghostty.so" | grep -q '^libglad\.so$'; then
  say "patchelf: add-needed libglad.so + rpath \$ORIGIN…"
  patchelf --add-needed libglad.so --set-rpath '$ORIGIN' "$LIB_DIR/libghostty.so"
fi

# --- 6. terminfo bundle (TERM=xterm-ghostty) --------------------------------
# app-runtime=none doesn't install resources; generate the ghostty terminfo so
# spawned shells/agents get a correct TERM. Non-fatal if it fails.
TERMINFO_DIR="$INSTALL_DIR/share/terminfo"
if [ ! -d "$TERMINFO_DIR/x/xterm-ghostty" ] && [ ! -f "$TERMINFO_DIR/x/xterm-ghostty" ]; then
  say "Generating ghostty terminfo…"
  mkdir -p "$TERMINFO_DIR"
  helper="$TOOLCHAIN_DIR/ghostty-terminfo.zig"
  cat > "$helper" <<'ZIG'
const std = @import("std");
const ghostty = @import("ghostty_terminfo").ghostty;
pub fn main() !void {
    var buffer: [1024]u8 = undefined;
    var stdout_writer = std.fs.File.stdout().writer(&buffer);
    const writer = &stdout_writer.interface;
    try ghostty.encode(writer);
    try stdout_writer.end();
}
ZIG
  helper_exe="$TOOLCHAIN_DIR/ghostty-terminfo"
  src_ti="$TOOLCHAIN_DIR/ghostty.terminfo"
  if "$ZIG" build-exe \
        --dep ghostty_terminfo \
        "-Mroot=$helper" \
        "-Mghostty_terminfo=$GHOSTTY_DIR/src/terminfo/ghostty.zig" \
        -O ReleaseFast \
        "-femit-bin=$helper_exe" \
     && "$helper_exe" +terminfo > "$src_ti" \
     && tic -x -o "$TERMINFO_DIR" "$src_ti"; then
    say "terminfo installed → $TERMINFO_DIR"
  else
    echo "warning: terminfo generation failed (non-fatal; set TERM=xterm-256color)" >&2
  fi
fi

say "Done."
echo "  lib:     $LIB_DIR/libghostty.so"
echo "  glad:    $LIB_DIR/libglad.so"
echo "  header:  $INSTALL_DIR/include/ghostty.h"
echo "  terminfo:$TERMINFO_DIR"
