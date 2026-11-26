#!/bin/bash
# #EXP-2150: the Mint runners' job hook (`bun run test:lanes`).
set -u
here="$(cd "$(dirname "$0")" && pwd)"
[ "$(uname -s)" = Linux ] || { echo "job-cleanup: Linux only, skipped"; exit 0; }
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
fails=0
ok() { echo "ok - $1"; }
no() { echo "not ok - $1"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else no "$1"; fi; }

t="$root/tmp"
ws="$t/runner-1/work/exponential/exponential"
setup() {
  rm -rf "$t"
  mkdir -p "$ws/.git" "$ws/apps/desktop/target/debug" "$t/runner-1/work/_temp" "$t/runner-1/work/_actions" \
    "$t/.tmpAbCdEf/desktops" "$t/exp-read-any-file-1" "$t/.X11-unix" "$root/gradle"
  touch "$ws/Cargo.toml" "$t/clr-debug-pipe-$$-1-in" "$t/clr-debug-pipe-999999999-1-in" "$t/dotnet-diagnostic-$$-1-socket"
  rm -f "$root"/gradle/*
  truncate -s 3G "$root/gradle/new"; truncate -s 3G "$root/gradle/older"; touch -d '1 hour ago' "$root/gradle/older"
  touch -d '3 days ago' "$root/gradle/stale"
}
run() {
  env JOB_CLEANUP_TMP="$t" JOB_CLEANUP_GRADLE="$root/gradle" JOB_CLEANUP_FREE_GB="${FREE:-100}" \
    RUNNER_WORKDIR="${WORKDIR-$t/runner-1/work}" GITHUB_WORKSPACE="${WS-$ws}" bash "$here/$1.sh"
}

setup
out="$(run job-completed)"; rc=$?
check "the completed hook ends green" "[ $rc -eq 0 ]"
check "the checkout is emptied" "[ ! -e '$ws/apps' ] && [ ! -e '$ws/Cargo.toml' ]"
check ".git stays for the next fetch" "[ -d '$ws/.git' ]"
check "the runner's own dirs stay" "[ -d '$t/runner-1/work/_temp' ] && [ -d '$t/runner-1/work/_actions' ]"
check "what tests left in /tmp goes" "[ ! -e '$t/.tmpAbCdEf' ] && [ ! -e '$t/exp-read-any-file-1' ]"
check "X's sockets and a live .NET process's pipes stay" "[ -d '$t/.X11-unix' ] && [ -e '$t/clr-debug-pipe-$$-1-in' ] && [ -e '$t/dotnet-diagnostic-$$-1-socket' ]"
check "a dead process's pipe goes" "[ ! -e '$t/clr-debug-pipe-999999999-1-in' ]"
check "Gradle's cache keeps the newest under its cap" "[ -e '$root/gradle/new' ] && [ ! -e '$root/gradle/older' ] && [ ! -e '$root/gradle/stale' ]"
check "it says what it did" "[ '$out' = 'job-cleanup: 100 GB free (was 100 GB)' ]"

setup
WORKDIR= run job-completed >/dev/null
check "without a work dir nothing is deleted" "[ -e '$ws/Cargo.toml' ] && [ -e '$t/.tmpAbCdEf' ]"
setup
mkdir -p "$root/elsewhere"; touch "$root/elsewhere/keep"
WS="$root/elsewhere" run job-completed >/dev/null
check "a workspace outside the work dir is never touched" "[ -e '$root/elsewhere/keep' ]"
setup
ln -s "$root/elsewhere" "$t/runner-1/work/link"
WS="$t/runner-1/work/link" run job-completed >/dev/null
check "a symlink out of the work dir is never followed" "[ -e '$root/elsewhere/keep' ]"
ln -s "$root/elsewhere" "$t/outlink"
run job-completed >/dev/null
check "a symlink in /tmp goes, its target stays" "[ ! -e '$t/outlink' ] && [ -e '$root/elsewhere/keep' ]"

setup
FREE=9 run job-started >/dev/null; check "a job does not start on a full disk" "[ $? -eq 1 ]"
FREE=9 run job-completed >/dev/null; check "a full disk never turns a finished job red" "[ $? -eq 0 ]"

[ "$fails" -eq 0 ] && echo "all job cleanup checks passed" || { echo "$fails job cleanup check(s) failed"; exit 1; }
