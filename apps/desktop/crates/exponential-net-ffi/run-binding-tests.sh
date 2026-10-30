#!/usr/bin/env bash
# Run the Swift binding suite on the HOST against the release dylib (what
# build-ios.sh wraps for devices). Needs Xcode's swiftc.
#   bash apps/desktop/crates/exponential-net-ffi/run-binding-tests.sh
# (The Kotlin binding is generated and committed; a JVM suite like
# exponential-ui-ffi's is not written yet.)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
OUT="$HERE/out/tests"
mkdir -p "$OUT"
(cd "$DESKTOP" && cargo build -p exponential-net-ffi --release --features cli 2>&1 | tail -1)
DYLIB="$TARGET/release/libexponential_net_ffi.$EXT"
[ -f "$DYLIB" ] || { echo "no $DYLIB"; exit 1; }
echo "== swift"
swiftc -O -module-name ExponentialNetFFISuite -Xcc -fmodule-map-file="$HERE/bindings/swift/module.modulemap" -I "$HERE/bindings/swift" \
  -L "$TARGET/release" -lexponential_net_ffi \
  "$HERE/bindings/swift/ExponentialNetFFI.swift" "$HERE/tests/swift/main.swift" -o "$OUT/swift-suite"
DYLD_LIBRARY_PATH="$TARGET/release" "$OUT/swift-suite"
