#!/usr/bin/env bash
# VAPP-86: run the Swift and/or Kotlin binding suites on the HOST against the
# release dylib (what `build-ios.sh` / `build-android.sh` wrap for devices).
#   bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh [swift|kotlin|all]
# Swift needs Xcode's swiftc. Kotlin needs a JDK plus, from the gradle cache,
# kotlin-compiler-embeddable (+ stdlib, script-runtime, daemon-embeddable,
# reflect, annotations, kotlinx-coroutines) and JNA — or set KOTLINC to a
# kotlinc binary and JNA_JAR to a jna jar.
set -euo pipefail
WHICH="${1:-all}"
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
FIXTURES="$(cd "$HERE/../../../../packages/exponential-ui/fixtures" && pwd)"
OUT="$HERE/out/tests"
mkdir -p "$OUT"

(cd "$DESKTOP" && cargo build -p exponential-ui-ffi --release --features cli 2>&1 | tail -1)
DYLIB="$TARGET/release/libexponential_ui_ffi.dylib"
[ -f "$DYLIB" ] || { echo "no $DYLIB"; exit 1; }

if [ "$WHICH" = swift ] || [ "$WHICH" = all ]; then
  echo "== swift"
  swiftc -O -module-name ExponentialUIFFISuite -Xcc -fmodule-map-file="$HERE/bindings/swift/module.modulemap" -I "$HERE/bindings/swift" \
    -L "$TARGET/release" -lexponential_ui_ffi \
    "$HERE/bindings/swift/ExponentialUIFFI.swift" "$HERE/tests/swift/main.swift" -o "$OUT/swift-suite"
  DYLD_LIBRARY_PATH="$TARGET/release" "$OUT/swift-suite" "$FIXTURES"
fi

if [ "$WHICH" = kotlin ] || [ "$WHICH" = all ]; then
  echo "== kotlin"
  G="$HOME/.gradle/caches/modules-2/files-2.1"
  jar() { find "$G/$1" -name "$2" 2>/dev/null | sort | tail -1; }
  JNA_JAR="${JNA_JAR:-$(jar net.java.dev.jna/jna 'jna-*.jar')}"
  STDLIB="$(jar org.jetbrains.kotlin/kotlin-stdlib 'kotlin-stdlib-2.*.jar')"
  if [ -n "${KOTLINC:-}" ]; then
    "$KOTLINC" -cp "$JNA_JAR" "$HERE/bindings/kotlin/at/exponential/ui/ffi/exponential_ui_ffi.kt" "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  else
    CC="$(jar org.jetbrains.kotlin/kotlin-compiler-embeddable 'kotlin-compiler-embeddable-2.*.jar'):$STDLIB:$(jar org.jetbrains.kotlin/kotlin-script-runtime 'kotlin-script-runtime-2.*.jar'):$(jar org.jetbrains.kotlin/kotlin-daemon-embeddable 'kotlin-daemon-embeddable-2.*.jar'):$(jar org.jetbrains.kotlin/kotlin-reflect 'kotlin-reflect-2.*.jar'):$(jar org.jetbrains/annotations 'annotations-*.jar'):$(jar org.jetbrains.kotlinx/kotlinx-coroutines-core-jvm 'kotlinx-coroutines-core-jvm-1.*.jar')"
    java -cp "$CC" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler -no-stdlib -cp "$STDLIB:$JNA_JAR" -jvm-target 17 \
      "$HERE/bindings/kotlin/at/exponential/ui/ffi/exponential_ui_ffi.kt" "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  fi
  java -Djna.library.path="$TARGET/release" -cp "$OUT/kotlin-suite.jar:$STDLIB:$JNA_JAR" tests.FacadeTestKt "$FIXTURES"
fi
