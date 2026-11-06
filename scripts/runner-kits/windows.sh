#!/usr/bin/env bash
# The windows runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
#
#   <kit>/exponential-runner.exe   the desktop runner (crate eengine-runner, profile runner), x86_64
#   <kit>/NOTICES.txt              the runner's third-party notices
#
# Built natively on Windows (Git Bash; CI: check-eengine-runner-windows.yml):
# gpui's Windows build compiles its HLSL shaders with the Windows SDK's fxc.exe,
# so it does not cross-compile from Linux. EXP_RUNNER_EXE takes a prebuilt
# runner instead (the release asset exponential-runner-windows-x86_64.exe);
# EXP_RUNNER_NOTICES another notices file.
set -euo pipefail
kit="$1"
root="$(cd "$(dirname "$0")/../.." && pwd)"
manifest="$root/apps/desktop/Cargo.toml"
notices="${EXP_RUNNER_NOTICES:-$root/apps/desktop/crates/eengine-runner/NOTICES.txt}"
[ -f "$notices" ] || { echo "windows: no runner notices at $notices" >&2; exit 1; }

if [ -n "${EXP_RUNNER_EXE:-}" ]; then
  exe="$EXP_RUNNER_EXE"
else
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) ;;
    *) echo "windows: build this kit on Windows (gpui needs fxc.exe), or pass EXP_RUNNER_EXE=<exponential-runner.exe>" >&2; exit 1 ;;
  esac
  cargo build --locked --manifest-path "$manifest" -p eengine-runner --profile runner
  target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$manifest" | sed -E 's/.*"target_directory":"([^"]*)".*/\1/; s/\\\\/\//g')"
  exe="$target_dir/runner/exponential-runner.exe"
fi
cp "$exe" "$kit/exponential-runner.exe"
cp "$notices" "$kit/NOTICES.txt"
