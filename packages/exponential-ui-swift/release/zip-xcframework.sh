#!/usr/bin/env bash
# VAPP-91: zip Binaries/ExponentialUIFFI.xcframework for the SwiftPM binary
# target and print its checksum.
#
#   bash release/zip-xcframework.sh <outdir>
#   → <outdir>/ExponentialUIFFI.xcframework.zip + the `swift package
#     compute-checksum` line make-release-package.sh takes.
#
# Build the xcframework first: bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <outdir>" >&2
  exit 2
fi

here="$(cd "$(dirname "$0")/.." && pwd)"
xcf="$here/Binaries/ExponentialUIFFI.xcframework"
[ -d "$xcf" ] || { echo "missing $xcf (run apps/desktop/crates/exponential-ui-ffi/build-ios.sh)" >&2; exit 1; }

mkdir -p "$1"
out="$(cd "$1" && pwd)/ExponentialUIFFI.xcframework.zip"
rm -f "$out"
# --keepParent: the archive holds ExponentialUIFFI.xcframework/ at its root,
# the layout SwiftPM expects of a binary target zip.
ditto -c -k --sequesterRsrc --keepParent "$xcf" "$out"
checksum="$(cd "$here" && swift package compute-checksum "$out")"
echo "$out"
echo "checksum $checksum"
