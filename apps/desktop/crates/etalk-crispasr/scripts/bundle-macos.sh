#!/usr/bin/env bash
# eTalk: bundle CrispASR's prebuilt Metal runtime (libcrispasr + ggml, MIT)
# into a macOS app, for EVERY host app (Exponential's build-macos-app.sh, spezl).
#
#   bundle-macos.sh <App.app> <executable> [cache-dir]
#
# The speech worker loads the runtime only by absolute path from
# Contents/Frameworks of its own executable (src/platform.rs `bundled_dir`).
# Pinned: the release asset's published sha256, checked BEFORE the archive is
# opened; only the six dylibs are taken, under the install names their @rpath
# references use. Apple Silicon only (the Metal build is arm64; an Intel build
# gets no voice). Run it BEFORE codesigning the bundle: the host signs the
# dylibs in Contents/Frameworks with its own identity (library validation stays
# on, notarization covers them). The runtime's licence and notices belong in
# the host's NOTICES.
set -euo pipefail
APP_DIR="${1:?usage: bundle-macos.sh <App.app> <executable> [cache-dir]}"
BIN="${2:?usage: bundle-macos.sh <App.app> <executable> [cache-dir]}"
CRISPASR_CACHE="${3:-${CARGO_TARGET_DIR:-$(pwd)/target}/crispasr-cache}"

CRISPASR_URL="https://github.com/CrispStrobe/CrispASR/releases/download/v0.8.45/libcrispasr-macos-arm64.tar.gz"
CRISPASR_SHA256="fe39ad4cd5cd96afcba2bf0dc4614897a70a2bc028cd83d99cd748124db5002f"
CRISPASR_TOP="libcrispasr-macos-arm64/lib"
CRISPASR_DYLIBS="libcrispasr.0.8.45.dylib:libcrispasr.1.dylib libggml.0.26.0.dylib:libggml.0.dylib libggml-base.0.26.0.dylib:libggml-base.0.dylib libggml-blas.0.26.0.dylib:libggml-blas.0.dylib libggml-cpu.0.26.0.dylib:libggml-cpu.0.dylib libggml-metal.0.26.0.dylib:libggml-metal.0.dylib"
if lipo -archs "$BIN" 2>/dev/null | grep -qw arm64; then
  CRISPASR_ARCHIVE="${CRISPASR_CACHE}/libcrispasr-macos-arm64-v0.8.45.tar.gz"
  mkdir -p "$CRISPASR_CACHE"
  if [ ! -f "$CRISPASR_ARCHIVE" ] || [ "$(shasum -a 256 "$CRISPASR_ARCHIVE" | cut -d' ' -f1)" != "$CRISPASR_SHA256" ]; then
    rm -f "$CRISPASR_ARCHIVE"
    curl -fsSL --retry 3 -o "${CRISPASR_ARCHIVE}.part" "$CRISPASR_URL"
    actual="$(shasum -a 256 "${CRISPASR_ARCHIVE}.part" | cut -d' ' -f1)"
    if [ "$actual" != "$CRISPASR_SHA256" ]; then
      rm -f "${CRISPASR_ARCHIVE}.part"
      echo "CrispASR runtime failed verification: sha256 $actual, pinned $CRISPASR_SHA256" >&2
      exit 1
    fi
    mv "${CRISPASR_ARCHIVE}.part" "$CRISPASR_ARCHIVE"
  fi
  CRISPASR_UNPACK="$(mktemp -d)"
  mkdir -p "$APP_DIR/Contents/Frameworks"
  for pair in $CRISPASR_DYLIBS; do
    src="${pair%%:*}"; dst="${pair##*:}"
    tar -xzf "$CRISPASR_ARCHIVE" -C "$CRISPASR_UNPACK" "${CRISPASR_TOP}/${src}"
    # A regular file only (never a symlink out of the archive).
    [ -f "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" ] && [ ! -L "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" ] \
      || { echo "CrispASR archive: ${src} is not a regular file" >&2; exit 1; }
    cp "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" "$APP_DIR/Contents/Frameworks/${dst}"
  done
  rm -rf "$CRISPASR_UNPACK"
else
  echo "note: not an arm64 build — bundling without the voice runtime (no voice on this build)"
fi
