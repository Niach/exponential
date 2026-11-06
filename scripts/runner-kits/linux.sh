#!/usr/bin/env bash
# The linux runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
#
#   <kit>/<arch>/exponential-runner          the desktop runner (crate eengine-runner, profile runner)
#   <kit>/<arch>/appimage-runtime            the AppImage type2 runtime, pinned + sha256-checked
#   <kit>/appimage-runtime.NOTICES.txt       its notices (inside every AppImage)
#   <kit>/NOTICES.txt                        the runner's third-party notices
#
# x86_64 and aarch64: each arch builds when it is the host's, or when its rust
# target and a linker (aarch64-linux-gnu-gcc) are installed; otherwise it is
# skipped. EXP_RUNNER_BIN_<ARCH> (e.g. EXP_RUNNER_BIN_X86_64) takes a prebuilt
# runner instead (the release asset); EXP_RUNNER_NOTICES another notices file.
set -euo pipefail
kit="$1"
root="$(cd "$(dirname "$0")/../.." && pwd)"
manifest="$root/apps/desktop/Cargo.toml"
notices="${EXP_RUNNER_NOTICES:-$root/apps/desktop/crates/eengine-runner/NOTICES.txt}"
[ -f "$notices" ] || { echo "linux: no runner notices at $notices" >&2; exit 1; }

# The pinned AppImage runtime (https://github.com/AppImage/type2-runtime, MIT).
runtime_tag=20251108
declare -A runtime_sha=(
  [x86_64]=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
  [aarch64]=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
)

target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$manifest" | sed -E 's/.*"target_directory":"([^"]*)".*/\1/')"
host="$(uname -m)"
built=0
for arch in x86_64 aarch64; do
  var="EXP_RUNNER_BIN_${arch^^}"
  triple="$arch-unknown-linux-gnu"
  bin=""
  if [ -n "${!var:-}" ]; then
    bin="${!var}"
  elif [ "$arch" = "$host" ]; then
    cargo build --locked --manifest-path "$manifest" -p eengine-runner --profile runner
    bin="$target_dir/runner/exponential-runner"
  elif rustup target list --installed 2>/dev/null | grep -qx "$triple" && command -v "$arch-linux-gnu-gcc" >/dev/null; then
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$arch-linux-gnu-gcc" \
      cargo build --locked --manifest-path "$manifest" -p eengine-runner --profile runner --target "$triple"
    bin="$target_dir/$triple/runner/exponential-runner"
  else
    echo "linux: skipping $arch (no $triple target + $arch-linux-gnu-gcc, and no $var)" >&2
    continue
  fi
  mkdir -p "$kit/$arch"
  install -m 0755 "$bin" "$kit/$arch/exponential-runner"
  curl -fsSL -o "$kit/$arch/appimage-runtime" "https://github.com/AppImage/type2-runtime/releases/download/$runtime_tag/runtime-$arch"
  echo "${runtime_sha[$arch]}  $kit/$arch/appimage-runtime" | sha256sum -c --quiet - || { echo "linux: the AppImage runtime for $arch does not match its pinned sha256" >&2; exit 1; }
  built=$((built + 1))
done
[ "$built" -gt 0 ] || { echo "linux: no runner was built" >&2; exit 1; }

cp "$notices" "$kit/NOTICES.txt"
cat > "$kit/appimage-runtime.NOTICES.txt" <<NOTICE
AppImage type2 runtime ($runtime_tag), https://github.com/AppImage/type2-runtime
MIT License, Copyright (c) 2004-23 probonopd and contributors. It links squashfuse
(BSD-2-Clause), libfuse 3 (LGPL-2.1; source: https://github.com/libfuse/libfuse),
zstd (BSD-3-Clause) and zlib (Zlib), statically, built from the sources at that tag.

Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
NOTICE
