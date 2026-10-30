#!/usr/bin/env bash
# #EXP-1441: run the Swift and/or Kotlin binding suites on the HOST against the
# release dylib (what `build-ios.sh` / `build-android.sh` wrap for devices):
# both replay packages/etalk/fixtures/voice-call.json through the facade and
# round-trip a call's audio through the codec.
#   bash apps/desktop/crates/etalk-ffi/run-binding-tests.sh [swift|kotlin|all]
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
FIXTURE="$(cd "$HERE/../../../../packages/etalk/fixtures" && pwd)/voice-call.json"
OUT="$HERE/out/tests"
mkdir -p "$OUT"

(cd "$DESKTOP" && cargo build -p etalk-ffi --release --features cli 2>&1 | tail -1)
DYLIB="$TARGET/release/libetalk_ffi.$EXT"
[ -f "$DYLIB" ] || { echo "no $DYLIB"; exit 1; }

if [ "$WHICH" = swift ] || [ "$WHICH" = all ]; then
  echo "== swift"
  swiftc -O -module-name ExponentialTalkFFISuite -Xcc -fmodule-map-file="$HERE/bindings/swift/module.modulemap" -I "$HERE/bindings/swift" \
    -L "$TARGET/release" -letalk_ffi \
    "$HERE/bindings/swift/ExponentialTalkFFI.swift" "$HERE/tests/swift/main.swift" -o "$OUT/swift-suite"
  DYLD_LIBRARY_PATH="$TARGET/release" "$OUT/swift-suite" "$FIXTURE"
fi

if [ "$WHICH" = kotlin ] || [ "$WHICH" = all ]; then
  echo "== kotlin"
  TOML="$HERE/../../../../packages/exponential-ui-compose/gradle/libs.versions.toml"
  ver() { sed -n "s/^$1 *= *\"\(.*\)\"/\1/p" "$TOML"; }
  KOTLIN_VERSION="$(ver kotlin)"; JNA_VERSION="$(ver jna)"
  [ -n "$KOTLIN_VERSION" ] && [ -n "$JNA_VERSION" ] || { echo "no kotlin/jna version in $TOML"; exit 1; }
  G="$HOME/.gradle/caches/modules-2/files-2.1"
  CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/etalk-ffi/maven"
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
    "$KOTLINC" -cp "$JNA_JAR" "$HERE/bindings/kotlin/at/exponential/talk/ffi/etalk_ffi.kt" "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  else
    CC="$(fetch org.jetbrains.kotlin kotlin-compiler-embeddable "$KOTLIN_VERSION" jar)"
    while read -r g a v; do CC="$CC:$(fetch "$g" "$a" "$v" jar)"; done < <(deps org.jetbrains.kotlin kotlin-compiler-embeddable "$KOTLIN_VERSION")
    CC="$CC:$ANNOTATIONS"
    echo "kotlin $KOTLIN_VERSION, jna $JNA_VERSION"
    java -cp "$CC" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler -no-stdlib -cp "$STDLIB:$JNA_JAR" -jvm-target 17 \
      "$HERE/bindings/kotlin/at/exponential/talk/ffi/etalk_ffi.kt" "$HERE/tests/kotlin/FacadeTest.kt" -d "$OUT/kotlin-suite.jar"
  fi
  java -Djna.library.path="$TARGET/release" -cp "$OUT/kotlin-suite.jar:$STDLIB:$ANNOTATIONS:$JNA_JAR" tests.FacadeTestKt "$FIXTURE"
fi
