#!/usr/bin/env bash
# The macos runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
#
#   <kit>/exponential-runner   the desktop runner (crate eengine-runner, profile `runner`):
#                              universal (lipo arm64 + x86_64) when both rust targets
#                              are installed, else the host arch
#   <kit>/NOTICES.txt          apps/desktop/crates/eengine-runner/NOTICES.txt
#                              ($EXP_RUNNER_NOTICES overrides the path)
set -euo pipefail
kit="$1"
root="$(cd "$(dirname "$0")/../.." && pwd)"
desktop="$root/apps/desktop"
notices="${EXP_RUNNER_NOTICES:-$desktop/crates/eengine-runner/NOTICES.txt}"
[ -f "$notices" ] || { echo "macos: no runner notices at $notices" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || { echo "macos: the runner kit builds on macOS" >&2; exit 1; }

# The .app floor (Info.plist LSMinimumSystemVersion), as release-eengine.yml builds it.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
target_dir="${CARGO_TARGET_DIR:-$desktop/target}"
installed="$(rustup target list --installed 2>/dev/null || true)"
targets=()
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  grep -qx "$t" <<<"$installed" && targets+=("$t")
done
host="$(rustc -vV | sed -n 's/^host: //p')"
[ ${#targets[@]} -eq 2 ] || targets=("$host")

bins=()
for t in "${targets[@]}"; do
  (cd "$desktop" && cargo build --locked -p eengine-runner --profile runner --target "$t")
  bins+=("$target_dir/$t/runner/exponential-runner")
done
if [ ${#bins[@]} -gt 1 ]; then
  lipo -create -output "$kit/exponential-runner" "${bins[@]}"
else
  cp "${bins[0]}" "$kit/exponential-runner"
fi
chmod 755 "$kit/exponential-runner"
lipo -info "$kit/exponential-runner"
cp "$notices" "$kit/NOTICES.txt"
