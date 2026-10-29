#!/usr/bin/env bash
#
# Assemble a macOS .app bundle for the Exponential desktop IDE.
#
#   scripts/build-macos-app.sh <channel>          # channel: production | staging
#   scripts/build-macos-app.sh --stage <dir> <channel>
#       EXP-1430: only stage the Exponential-only inputs (Frameworks/ = the
#       CrispASR dylibs, Resources/ = the licences) under <dir> and print
#       app_name= / bundle_id= / frameworks_dir= / resources_dir= lines
#       (CI appends them to $GITHUB_OUTPUT and hands them to the shared
#       .github/actions/macos-app action).
#
# EXP-1430: assembly + signing live in the shared .github/actions/macos-app/
# bundle.sh (also used by Niach/spezl); this script stages the Exponential
# parts and delegates.
#
# Why a bundle at all: macOS only routes a custom URL scheme (our `exponential://`
# OAuth-callback / invite deep link, §5.7) to a Launch-Services-registered
# .app that declares `CFBundleURLTypes`. A bare Mach-O binary (what `cargo
# build` emits, and what CI used to ship) can never be that handler, so the
# callback fell through to whatever else claimed `exponential:`. This wraps the release
# binary in the bundle whose Info.plist (assets/packaging/Info.plist) declares
# the scheme, so `on_open_urls` (wired in app/src/main.rs) actually fires.
#
# Assumes the release binary is already built:
#   production: cargo build --release -p app
#   staging:    cargo build --release -p app --features staging
#
# Output: ${TARGET_DIR}/<AppName>.app (ad-hoc codesigned so it launches).
# Registration + launch is the caller's job (the `macapp:desktop` bun script
# does it locally; CI just zips + uploads the bundle).
set -euo pipefail

STAGE_ONLY=""
if [ "${1:-}" = "--stage" ]; then
  STAGE_ONLY="${2:?--stage needs a directory}"
  shift 2
fi

CHANNEL="${1:-production}"
DESKTOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "${DESKTOP_DIR}/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${DESKTOP_DIR}/target}"
# Defaults to the release binary; override with EXP_DESKTOP_BIN for a debug,
# custom or universal build (e.g. EXP_DESKTOP_BIN=target/debug/exp-desktop).
BIN="${EXP_DESKTOP_BIN:-${TARGET_DIR}/release/exp-desktop}"

# Channel identity. The bundle id is DISTINCT from the iOS app (`at.exponential`)
# so Launch Services never confuses the two; staging gets its own id + name so a
# prod and a staging build coexist (the iOS `AppConstants.isStaging` analog).
case "$CHANNEL" in
  production) BUNDLE_ID="at.exponential.desktop";         APP_NAME="Exponential" ;;
  staging)    BUNDLE_ID="at.exponential.desktop.staging"; APP_NAME="Exponential (staging)" ;;
  *) echo "unknown channel: $CHANNEL (expected production|staging)" >&2; exit 1 ;;
esac

[ -x "$BIN" ] || {
  echo "release binary not found at $BIN — build it first:" >&2
  echo "  cargo build --release -p app${CHANNEL:+ }${CHANNEL:+# add --features staging for staging}" >&2
  exit 1
}

if [ -n "$STAGE_ONLY" ]; then
  mkdir -p "$STAGE_ONLY"
  STAGE_DIR="$(cd "$STAGE_ONLY" && pwd)"
else
  STAGE_DIR="$(mktemp -d)"
  trap 'rm -rf "$STAGE_DIR"' EXIT
fi
rm -rf "${STAGE_DIR}/Frameworks" "${STAGE_DIR}/Resources"
mkdir -p "${STAGE_DIR}/Frameworks" "${STAGE_DIR}/Resources"

# --- Licences (EXP-376) ---------------------------------------------------
# Apache-2.0 section 4(a) requires giving recipients a copy of the License, and
# section 4(d) requires propagating our NOTICE. NOTICES.txt is the generated
# per-dependency inventory (packages/licenses) and carries the full OFL and ISC
# bodies for the fonts and icons embedded in the binary. bundle.sh copies
# Resources/ in BEFORE it signs (files added after signing break the seal).
cp "${REPO_ROOT}/LICENSE" "${STAGE_DIR}/Resources/LICENSE"
cp "${REPO_ROOT}/NOTICE" "${STAGE_DIR}/Resources/NOTICE"
cp "${DESKTOP_DIR}/assets/licenses/NOTICES.txt" "${STAGE_DIR}/Resources/NOTICES.txt"

# --- exponentialTalk's voice runtime (NEW-74) -----------------------------
# CrispASR's prebuilt Metal runtime (libcrispasr + ggml, MIT; its licence and
# notices are in NOTICES.txt) ships INSIDE the bundle, signed below with the
# app's identity, so notarization covers it and library validation stays on.
# The speech worker loads it only by absolute path from Contents/Frameworks of
# its own executable (crates/etalk-crispasr/src/platform.rs). Pinned: the release
# asset's published sha256, checked BEFORE the archive is opened; only the six
# dylibs are taken, under the install names their @rpath references use.
# Apple Silicon only (the Metal build is arm64). EXP-1430: a universal binary's
# x86_64 slice compiles crispasr's no-runtime `imp` (platform.rs cfg is
# macos+aarch64 only), so on Intel it never looks in Frameworks: no voice.
CRISPASR_URL="https://github.com/CrispStrobe/CrispASR/releases/download/v0.8.45/libcrispasr-macos-arm64.tar.gz"
CRISPASR_SHA256="fe39ad4cd5cd96afcba2bf0dc4614897a70a2bc028cd83d99cd748124db5002f"
CRISPASR_TOP="libcrispasr-macos-arm64/lib"
CRISPASR_DYLIBS="libcrispasr.0.8.45.dylib:libcrispasr.1.dylib libggml.0.26.0.dylib:libggml.0.dylib libggml-base.0.26.0.dylib:libggml-base.0.dylib libggml-blas.0.26.0.dylib:libggml-blas.0.dylib libggml-cpu.0.26.0.dylib:libggml-cpu.0.dylib libggml-metal.0.26.0.dylib:libggml-metal.0.dylib"
if lipo -archs "$BIN" 2>/dev/null | grep -qw arm64; then
  CRISPASR_CACHE="${TARGET_DIR}/crispasr-cache"
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
    for pair in $CRISPASR_DYLIBS; do
    src="${pair%%:*}"; dst="${pair##*:}"
    tar -xzf "$CRISPASR_ARCHIVE" -C "$CRISPASR_UNPACK" "${CRISPASR_TOP}/${src}"
    # A regular file only (never a symlink out of the archive).
    [ -f "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" ] && [ ! -L "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" ] \
      || { echo "CrispASR archive: ${src} is not a regular file" >&2; exit 1; }
    cp "${CRISPASR_UNPACK}/${CRISPASR_TOP}/${src}" "${STAGE_DIR}/Frameworks/${dst}"
  done
  rm -rf "$CRISPASR_UNPACK"
else
  echo "note: not an arm64 build — bundling without the voice runtime (no voice on this build)" >&2
fi

if [ -n "$STAGE_ONLY" ]; then
  echo "app_name=${APP_NAME}"
  echo "bundle_id=${BUNDLE_ID}"
  echo "frameworks_dir=${STAGE_DIR}/Frameworks"
  echo "resources_dir=${STAGE_DIR}/Resources"
  exit 0
fi

# --- Assemble + sign (the shared script) ------------------------------------
# Version strings: real values come from the release tag via env; local builds
# default to a dev placeholder. MACOS_SIGN_IDENTITY set → Developer ID +
# hardened runtime + timestamp (build-macos-dmg.sh then notarizes); unset →
# ad-hoc `-s -` (launches locally, not Gatekeeper-clean). NEW-78:
# entitlements = device.audio-input (push to talk's microphone). The icon is
# the macOS variant (EXP-68/EXP-143: a dark rounded-square plate), falling back
# to logo-white.svg.
LOGO_SVG="${DESKTOP_DIR}/assets/icons/logo-macos.svg"
[ -f "$LOGO_SVG" ] || LOGO_SVG="${DESKTOP_DIR}/assets/icons/logo-white.svg"
"${REPO_ROOT}/.github/actions/macos-app/bundle.sh" \
  --binary "$BIN" \
  --app-name "$APP_NAME" \
  --bundle-id "$BUNDLE_ID" \
  --executable exp-desktop \
  --plist "${DESKTOP_DIR}/assets/packaging/Info.plist" \
  --out-dir "$TARGET_DIR" \
  --version "${VERSION:-0.0.0}" \
  --build "${BUILD:-0}" \
  --entitlements "${DESKTOP_DIR}/assets/packaging/exp-desktop.entitlements" \
  --icon "$LOGO_SVG" \
  --frameworks-dir "${STAGE_DIR}/Frameworks" \
  --resource "${STAGE_DIR}/Resources" \
  --lsuielement false \
  --identity "${MACOS_SIGN_IDENTITY:-}" >/dev/null

echo "built: ${TARGET_DIR}/${APP_NAME}.app  (id=${BUNDLE_ID}, exponential:// handler)"
