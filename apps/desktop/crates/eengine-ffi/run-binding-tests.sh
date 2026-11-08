#!/usr/bin/env bash
# #VAPP-263: run the Swift and/or Kotlin binding suites on the HOST against the
# release dylib (what `build-ios.sh` / `build-android.sh` wrap for devices):
# both run an app through the facade with native providers (the lifecycle,
# writes, the consent round trip) and replay providers.json's validateCases.
# `swift` also runs the capabilities suite (#VAPP-265, capabilities.json) and
# typechecks their UIKit half for iOS when Xcode has the iPhone SDK.
#   bash apps/desktop/crates/eengine-ffi/run-binding-tests.sh [swift|kotlin|all]
# Swift needs Xcode's swiftc. Kotlin needs a JDK 17+ and curl: the compiler,
# stdlib and JNA are pinned to packages/exponential-ui-compose's version catalog
# (gradle cache first, else Maven Central), as in exponential-ui-ffi — or set
# KOTLINC to a kotlinc binary and/or JNA_JAR to a jna jar.
set -euo pipefail
WHICH="${1:-all}"
HERE="$(cd "$(dirname "$0")" && pwd)"
DESKTOP="$(cd "$HERE/../.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DESKTOP/target}"
export CARGO_TARGET_DIR="$TARGET"
case "$(uname -s)" in Darwin) EXT=dylib ;; *) EXT=so ;; esac
FIXTURE="$(cd "$HERE/../../../../packages/eengine/fixtures" && pwd)/providers.json"
OUT="$HERE/out/tests"
mkdir -p "$OUT"

(cd "$DESKTOP" && cargo build -p eengine-ffi --release --features cli 2>&1 | tail -1)
DYLIB="$TARGET/release/libeengine_ffi.$EXT"
[ -f "$DYLIB" ] || { echo "no $DYLIB"; exit 1; }

if [ "$WHICH" = swift ] || [ "$WHICH" = all ]; then
  echo "== swift"
  swiftc -O -module-name ExponentialEngineFFISuite -Xcc -fmodule-map-file="$HERE/bindings/swift/module.modulemap" -I "$HERE/bindings/swift" \
    -L "$TARGET/release" -leengine_ffi \
    "$HERE/bindings/swift/ExponentialEngineFFI.swift" "$HERE/tests/swift/main.swift" -o "$OUT/swift-suite"
  DYLD_LIBRARY_PATH="$TARGET/release" "$OUT/swift-suite" "$FIXTURE"

  # #VAPP-265: the capabilities as the SwiftPM products lay them out (Package.swift):
  # the binding = module ExponentialEngine; #VAPP-268: the shared core, then one
  # module per capability, then the umbrella; the suite against fakes + capabilities.json.
  echo "== swift capabilities"
  SWIFT="$HERE/swift"
  CAPS=(ExponentialEnginePhotos ExponentialEngineFiles ExponentialEngineShare ExponentialEnginePrint ExponentialEngineNotifications)
  MODS="$OUT/swift-modules"
  FFI=(-Xcc -fmodule-map-file="$HERE/bindings/swift/module.modulemap" -I "$HERE/bindings/swift")
  PKG=(-package-name ExponentialEngine)
  rm -rf "$MODS"; mkdir -p "$MODS"
  module() { # name sources... (links every module built before it)
    local name="$1"; shift
    local links=(); for m in "$MODS"/lib*.dylib; do [ -e "$m" ] && links+=("-l$(basename "$m" .dylib | sed 's/^lib//')"); done
    swiftc -module-name "$name" -enable-testing -emit-library -emit-module -emit-module-path "$MODS/$name.swiftmodule" "${PKG[@]}" \
      "${FFI[@]}" -I "$MODS" -L "$MODS" -L "$TARGET/release" ${links[@]+"${links[@]}"} "$@" -o "$MODS/lib$name.dylib"
  }
  module ExponentialEngine -leengine_ffi "$HERE/bindings/swift/ExponentialEngineFFI.swift"
  module ExponentialEngineCapabilityCore "$SWIFT/ExponentialEngineCapabilityCore"/*.swift
  for c in "${CAPS[@]}"; do module "$c" "$SWIFT/$c"/*.swift; done
  module ExponentialEngineCapabilities "$SWIFT/ExponentialEngineCapabilities"/*.swift
  LINKS=(); for m in ExponentialEngine ExponentialEngineCapabilityCore "${CAPS[@]}" ExponentialEngineCapabilities; do LINKS+=("-l$m"); done
  swiftc -module-name CapabilitiesSuite "${FFI[@]}" -I "$MODS" -L "$MODS" "${LINKS[@]}" \
    "$HERE/tests/swift-capabilities/main.swift" -o "$OUT/swift-capabilities-suite"
  DYLD_LIBRARY_PATH="$MODS:$TARGET/release" "$OUT/swift-capabilities-suite" "$(dirname "$FIXTURE")/capabilities.json"
  # The UIKit/PhotoKit half only compiles for iOS: typecheck each module against the iPhone SDK when Xcode has one.
  if IOS_SDK="$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null)" && [ -n "$IOS_SDK" ]; then
    IOS="$OUT/swift-ios"; rm -rf "$IOS"; mkdir -p "$IOS"
    ios() { # name sources...
      local name="$1"; shift
      xcrun swiftc -sdk "$IOS_SDK" -target arm64-apple-ios15.0 -module-name "$name" -emit-module -emit-module-path "$IOS/$name.swiftmodule" \
        "${PKG[@]}" "${FFI[@]}" -I "$IOS" "$@"
    }
    ios ExponentialEngine "$HERE/bindings/swift/ExponentialEngineFFI.swift"
    ios ExponentialEngineCapabilityCore "$SWIFT/ExponentialEngineCapabilityCore"/*.swift
    for c in "${CAPS[@]}"; do ios "$c" "$SWIFT/$c"/*.swift; done
    ios ExponentialEngineCapabilities "$SWIFT/ExponentialEngineCapabilities"/*.swift
    echo "iOS 15 (arm64, $(basename "$IOS_SDK")): the capability modules typecheck"
  fi
fi

if [ "$WHICH" = kotlin ] || [ "$WHICH" = all ]; then
  echo "== kotlin"
  # The binding is generated in Android mode (uniffi.toml): tests/kotlin/android-stubs
  # stand in for android.os.Build, android.system.SystemCleaner and RequiresApi.
  TOML="$HERE/../../../../packages/exponential-ui-compose/gradle/libs.versions.toml"
  ver() { sed -n "s/^$1 *= *\"\(.*\)\"/\1/p" "$TOML"; }
  KOTLIN_VERSION="$(ver kotlin)"; JNA_VERSION="$(ver jna)"
  [ -n "$KOTLIN_VERSION" ] && [ -n "$JNA_VERSION" ] || { echo "no kotlin/jna version in $TOML"; exit 1; }
  G="$HOME/.gradle/caches/modules-2/files-2.1"
  CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/eengine-ffi/maven"
  MAVEN="https://repo1.maven.org/maven2"
  mkdir -p "$CACHE"
  fetch() { # group artifact version ext -> path
    local f="$CACHE/$2-$3.$4" hit
    if [ "$4" = jar ]; then hit="$(find "$G/$1/$2/$3" -name "$2-$3.jar" 2>/dev/null | head -1)"; [ -n "$hit" ] && { echo "$hit"; return; }; fi
    [ -s "$f" ] || curl -fsSL "$MAVEN/${1//.//}/$2/$3/$2-$3.$4" -o "$f.part" && mv "$f.part" "$f" 2>/dev/null || true
    [ -s "$f" ] || { echo "cannot fetch $1:$2:$3 ($4)" >&2; exit 1; }
    echo "$f"
  }
  deps() { # group artifact version -> "group artifact version" per compile/runtime dependency
    awk '/<dependencyManagement>/{m=1} /<\/dependencyManagement>/{m=0}
      /<exclusions>/{x=1} /<\/exclusions>/{x=0; next} x{next}
      !m && /<dependency>/{d=1; g=a=v=s=o=""}
      d && /<groupId>/{gsub(/.*<groupId>|<\/groupId>.*/,""); g=$0}
      d && /<artifactId>/{gsub(/.*<artifactId>|<\/artifactId>.*/,""); a=$0}
      d && /<version>/{gsub(/.*<version>|<\/version>.*/,""); v=$0}
      d && /<scope>/{gsub(/.*<scope>|<\/scope>.*/,""); s=$0}
      d && /<optional>true/{o=1}
      d && /<\/dependency>/{d=0; if (!o && s != "test" && s != "provided") print g, a, v}' "$(fetch "$1" "$2" "$3" pom)"
  }
  JNA_JAR="${JNA_JAR:-$(fetch net.java.dev.jna jna "$JNA_VERSION" jar)}"
  STDLIB="$(fetch org.jetbrains.kotlin kotlin-stdlib "$KOTLIN_VERSION" jar)"
  ANNOTATIONS="$(deps org.jetbrains.kotlin kotlin-stdlib "$KOTLIN_VERSION" | while read -r g a v; do fetch "$g" "$a" "$v" jar; done | paste -sd: -)"
  if [ -n "${KOTLINC:-}" ]; then
    "$KOTLINC" -cp "$JNA_JAR" "$HERE/bindings/kotlin/at/exponential/engine/ffi/eengine_ffi.kt" "$HERE"/tests/kotlin/android-stubs/*.kt "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  else
    CC="$(fetch org.jetbrains.kotlin kotlin-compiler-embeddable "$KOTLIN_VERSION" jar)"
    while read -r g a v; do CC="$CC:$(fetch "$g" "$a" "$v" jar)"; done < <(deps org.jetbrains.kotlin kotlin-compiler-embeddable "$KOTLIN_VERSION")
    CC="$CC:$ANNOTATIONS"
    echo "kotlin $KOTLIN_VERSION, jna $JNA_VERSION"
    java -cp "$CC" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler -no-stdlib -cp "$STDLIB:$JNA_JAR" -jvm-target 17 \
      "$HERE/bindings/kotlin/at/exponential/engine/ffi/eengine_ffi.kt" "$HERE"/tests/kotlin/android-stubs/*.kt "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  fi
  java -Djna.library.path="$TARGET/release" -cp "$OUT/kotlin-suite.jar:$STDLIB:$ANNOTATIONS:$JNA_JAR" tests.FacadeTestKt "$FIXTURE"
fi
