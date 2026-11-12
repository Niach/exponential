#!/usr/bin/env bash
# #LOGS-1/#LOGS-2: upload debug files (ELF debug info, dSYM, PDB) to GlitchTip:
#   .github/scripts/glitchtip-debug-files.sh <project> <path>...
# or an Android R8 mapping under the ProGuard UUID the build baked in
# (BuildConfig.SENTRY_PROGUARD_UUID, sent with every event):
#   .github/scripts/glitchtip-debug-files.sh <project> --proguard <uuid> <mapping.txt>
# env: SENTRY_AUTH_TOKEN (upload-only), CF_ACCESS_CLIENT_ID +
# CF_ACCESS_CLIENT_SECRET (the Access service token in front of
# glitchtip.exponential.at). Any of them missing = a notice, exit 0.
#
# sentry-cli is the pinned release binary, sha256-checked (no npx: the
# self-hosted Linux runners have node without npm). An upload is tried
# three times (30 s, then 90 s apart), so a GlitchTip redeploy (~30 s of
# Cloudflare 530s) does not cost a release its symbols. A failure warns
# and exits 0; GLITCHTIP_STRICT=1 makes it exit 1 (the backfill workflow).
# Nothing secret is echoed.
set -uo pipefail

SENTRY_CLI_VERSION=2.58.6
project="${1:?usage: $0 <project> [--proguard <uuid>] <path>...}"
shift
proguard_uuid=""
if [ "${1:-}" = "--proguard" ]; then
  proguard_uuid="${2:?--proguard needs the ProGuard UUID of the build}"
  shift 2
fi

fail() {
  echo "::warning title=GlitchTip::$1"
  [ "${GLITCHTIP_STRICT:-}" = 1 ] && exit 1
  exit 0
}

if [ -z "${SENTRY_AUTH_TOKEN:-}" ] || [ -z "${CF_ACCESS_CLIENT_ID:-}" ] || [ -z "${CF_ACCESS_CLIENT_SECRET:-}" ]; then
  echo "::notice title=GlitchTip::GLITCHTIP_AUTH_TOKEN / CF_ACCESS_CLIENT_ID / CF_ACCESS_CLIENT_SECRET not all set: debug files not uploaded"
  [ "${GLITCHTIP_STRICT:-}" = 1 ] && exit 1
  exit 0
fi

files=()
for path in "$@"; do
  [ -z "$path" ] && continue
  if [ -e "$path" ]; then files+=("$path"); else echo "::warning title=GlitchTip::debug file missing: $path"; fi
done
[ ${#files[@]} -eq 0 ] && fail "no debug files to upload"
tmp="${RUNNER_TEMP:-/tmp}"
command -v cygpath >/dev/null && tmp="$(cygpath -u "$tmp")" # Windows (Git Bash)

# #ERR-15: upload only what symbolication reads. An ELF carrying DWARF
# (an unstripped binary, or a .debug file) goes up as its debug sections
# alone, zlib-compressed (objcopy --only-keep-debug
# --compress-debug-sections=zlib): the same build-id, the same lookups in
# GlitchTip's symbolic, a fifth of the bytes (Linux exp-desktop 1.2 GB →
# 0.24 GB). A directory holding such an ELF is uploaded file by file.
# dSYMs, PDBs, stripped .so files and R8 mappings go up as they are.
is_elf() { [ -f "$1" ] && [ "$(head -c 4 "$1" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ]; }
has_dwarf() { is_elf "$1" && [[ "$(readelf -SW "$1" 2>/dev/null)" == *" .debug_info "* ]]; }
bytes() { wc -c <"$1" | tr -d ' '; }
dir_has_dwarf() {
  local f
  while IFS= read -r -d '' f; do has_dwarf "$f" && return 0; done < <(find "$1" -type f -print0)
  return 1
}
if [ -z "$proguard_uuid" ] && command -v readelf >/dev/null; then
  if command -v objcopy >/dev/null; then
    slim="$tmp/glitchtip-dif"
    rm -rf "$slim"
    expanded=()
    for path in "${files[@]}"; do
      if [ -d "$path" ] && dir_has_dwarf "$path"; then
        while IFS= read -r -d '' f; do expanded+=("$f"); done < <(find "$path" -type f -print0)
      else
        expanded+=("$path")
      fi
    done
    files=()
    before=0 after=0 n=0
    for f in "${expanded[@]}"; do
      if has_dwarf "$f"; then
        n=$((n + 1))
        mkdir -p "$slim/$n"
        out="$slim/$n/$(basename "$f")"
        if objcopy --only-keep-debug --compress-debug-sections=zlib "$f" "$out"; then
          before=$((before + $(bytes "$f"))) after=$((after + $(bytes "$out")))
          echo "$(basename "$f"): $(bytes "$f") → $(bytes "$out") bytes"
          files+=("$out")
          continue
        fi
        echo "::warning title=GlitchTip::objcopy failed on $f: uploading it whole"
      fi
      files+=("$f")
    done
    [ "$n" -gt 0 ] && echo "GlitchTip debug files: $before → $after bytes"
  elif [ -n "$(for f in "${files[@]}"; do has_dwarf "$f" && echo y; done)" ]; then
    echo "::warning title=GlitchTip::no objcopy: the ELF files go up whole, debug info uncompressed"
  fi
fi

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset=sentry-cli-Linux-x86_64 sum=36b689311b399d9332950d84f4299aee682d2d290a770a77372b30a74f7e7add ;;
  Linux-aarch64 | Linux-arm64) asset=sentry-cli-Linux-aarch64 sum=79e60095ab461eac70c23ce750499bff5c6bffb95d364ccf38d3259557403987 ;;
  Darwin-*) asset=sentry-cli-Darwin-universal sum=728d5a8c48d3e94d0a3e644431dcaad19c28b126a00eb37930e67bd146905d96 ;;
  MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) asset=sentry-cli-Windows-x86_64.exe sum=99c1bf7a3d18df2b62ee5052f7e8f5d1e065e218442cdf0cfa2ede0b9cf7b938 ;;
  *) fail "sentry-cli: unsupported platform $(uname -s)-$(uname -m)" ;;
esac
cli="$tmp/$asset"
if [ ! -x "$cli" ]; then
  curl -fsSL --retry 3 -o "$cli" "https://github.com/getsentry/sentry-cli/releases/download/$SENTRY_CLI_VERSION/$asset" \
    || fail "sentry-cli download failed: debug files not uploaded"
  if command -v sha256sum >/dev/null; then got=$(sha256sum "$cli"); else got=$(shasum -a 256 "$cli"); fi
  [ "${got%% *}" = "$sum" ] || { rm -f "$cli"; fail "sentry-cli checksum mismatch: debug files not uploaded"; }
  chmod +x "$cli"
fi

export SENTRY_URL=https://glitchtip.exponential.at SENTRY_ORG=exponential SENTRY_PROJECT="$project"
if [ -n "$proguard_uuid" ]; then
  upload=(upload-proguard --uuid "$proguard_uuid")
else
  upload=(debug-files upload)
fi
for attempt in 1 2 3; do
  if "$cli" --header "CF-Access-Client-Id:$CF_ACCESS_CLIENT_ID" --header "CF-Access-Client-Secret:$CF_ACCESS_CLIENT_SECRET" \
      "${upload[@]}" "${files[@]}"; then
    exit 0
  fi
  [ "$attempt" = 3 ] && break
  wait=$((attempt == 1 ? 30 : 90))
  echo "GlitchTip upload attempt $attempt failed, retrying in ${wait}s"
  sleep "$wait"
done
fail "debug-file upload to $project failed after 3 attempts; the release goes on without symbols"
