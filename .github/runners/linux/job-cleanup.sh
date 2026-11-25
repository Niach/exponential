#!/bin/bash
# #EXP-2150: the Mint runners' job hooks (Dockerfile:
# ACTIONS_RUNNER_HOOK_JOB_STARTED + ACTIONS_RUNNER_HOOK_JOB_COMPLETED). The
# runner never cleans up: on 2026-11-25 each container kept its last job's
# checkout + cargo target (19-25 GB) until the next job's checkout, plus what
# every test run since 11-13 left in /tmp (63,000 and 41,000 entries, 16 and
# 11 GB), and the disk keeper found them busy every time. A runner runs one
# job at a time, so between jobs all of it is garbage:
#   - the checkout, all but .git (the next actions/checkout fetches into it)
#   - everything in /tmp but the runner's own work dir, X's socket dirs and the
#     diagnostics pipes of live .NET processes (the runner itself)
#   - Gradle build-cache entries older than two days, oldest first beyond
#     JOB_CLEANUP_GRADLE_GB (4)
# At a job's start it also FAILS the job below JOB_CLEANUP_FLOOR_GB (10) free:
# a full disk fails it anyway, later and less clearly.
# Never fails a job otherwise.
set -uo pipefail

TMP_ROOT="${JOB_CLEANUP_TMP:-/tmp}"
FLOOR_GB="${JOB_CLEANUP_FLOOR_GB:-10}"
GRADLE_GB="${JOB_CLEANUP_GRADLE_GB:-4}"
GRADLE_CACHE="${JOB_CLEANUP_GRADLE:-${GRADLE_USER_HOME:-$HOME/.gradle}/caches/build-cache-1}"
WORK="${RUNNER_WORKDIR:-}"
WORKSPACE="${GITHUB_WORKSPACE:-}"

free_gb() {
  if [ -n "${JOB_CLEANUP_FREE_GB:-}" ]; then echo "$JOB_CLEANUP_FREE_GB"; else df -Pk "$TMP_ROOT" | awk 'NR == 2 { printf "%d", $4 / 1048576 }'; fi
}
started="$(free_gb)"

# A job may leave root-owned files (sudo): best effort with sudo.
wipe() {
  rm -rf -- "$@" 2>/dev/null || sudo -n rm -rf -- "$@" 2>/dev/null || true
}

# The checkout: only below the runner's own work dir.
if [ -n "$WORK" ] && [ -d "$WORKSPACE" ]; then
  case "$(readlink -f "$WORKSPACE")" in
    "$(readlink -f "$WORK")"/?*)
      while IFS= read -r -d '' entry; do wipe "$entry"; done \
        < <(find "$WORKSPACE" -mindepth 1 -maxdepth 1 ! -name .git -print0 2>/dev/null)
      ;;
  esac
fi

# /tmp: the work dir's own top-level entry stays. Without a known work dir
# inside TMP_ROOT nothing is deleted.
keep=
case "$WORK" in
  "$TMP_ROOT"/?*) rest="${WORK#"$TMP_ROOT"/}"; keep="${rest%%/*}" ;;
esac
if [ -n "$keep" ]; then
  while IFS= read -r -d '' entry; do
    name="${entry##*/}"
    case "$name" in
      "$keep" | .X11-unix | .ICE-unix | .dotnet) continue ;;
      clr-debug-pipe-* | dotnet-diagnostic-*)
        pid="$(sed -E 's/^(clr-debug-pipe|dotnet-diagnostic)-([0-9]+)-.*/\2/' <<<"$name")"
        [ -d "/proc/$pid" ] && continue
        ;;
    esac
    printf '%s\0' "$entry"
  done < <(find "$TMP_ROOT" -mindepth 1 -maxdepth 1 -print0 2>/dev/null) > "$TMP_ROOT/$keep/.job-cleanup"
  xargs -0 -r -n 500 rm -rf -- < "$TMP_ROOT/$keep/.job-cleanup" 2>/dev/null \
    || xargs -0 -r -n 500 sudo -n rm -rf -- < "$TMP_ROOT/$keep/.job-cleanup" 2>/dev/null || true
  rm -f "$TMP_ROOT/$keep/.job-cleanup"
fi

# Gradle's build cache: its own 7-day cleanup let it reach 7 GB a runner.
if [ -d "$GRADLE_CACHE" ]; then
  find "$GRADLE_CACHE" -type f -mtime +1 -delete 2>/dev/null
  find "$GRADLE_CACHE" -type f -printf '%T@ %s %p\0' 2>/dev/null | sort -z -rn \
    | awk -v RS='\0' -v ORS='\0' -v max="$GRADLE_GB" '{ s += $2; if (s > max * 1073741824) { sub(/^[^ ]+ [^ ]+ /, ""); print } }' \
    | xargs -0 -r rm -f -- 2>/dev/null
fi

end="$(free_gb)"
echo "job-cleanup: ${end} GB free (was ${started} GB)"
# The started hook only (the completed one must never turn a green job red).
if [ "${1:-}" = started ] && [ "$end" -lt "$FLOOR_GB" ]; then
  echo "::error title=Disk full::${end} GB free on the runner's disk, below ${FLOOR_GB} GB after cleaning"
  exit 1
fi
exit 0
