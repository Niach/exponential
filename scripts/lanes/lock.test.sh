#!/bin/sh
# EXP-1393: the lane lock rules, end to end (`bun run test:lanes`).
set -u
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lock.sh"
root=$(mktemp -d "${TMPDIR:-/tmp}/exp-lane-test.XXXXXX")
trap 'rm -rf "$root"' EXIT
fails=0
ok() { echo "ok - $1"; }
no() { echo "not ok - $1"; fails=$((fails + 1)); }

# Our own lock: taken, then released.
take_lock "$root/own" me
[ "$(lock_owner "$root/own")" = me ] && ok "take_lock writes the owner" || no "take_lock writes the owner"
release_lock "$root/own" me && [ ! -e "$root/own" ] && ok "release_lock frees our own lock" || no "release_lock frees our own lock"

# A foreign-owned lock survives a release attempt (the 2026-10-24 incident).
mkdir "$root/foreign" && echo "ios-agent 1761349980" > "$root/foreign/owner"
if release_lock "$root/foreign" android-agent 2>/dev/null; then no "release_lock refuses a foreign lock"; else ok "release_lock refuses a foreign lock"; fi
[ "$(lock_owner "$root/foreign")" = ios-agent ] && ok "the foreign lock survives" || no "the foreign lock survives"

# A prefix of the owner is not the owner.
if release_lock "$root/foreign" ios 2>/dev/null; then no "release_lock matches the whole owner word"; else ok "release_lock matches the whole owner word"; fi

# A lock still being taken (no owner file yet) is nobody's to release.
mkdir "$root/taking"
if release_lock "$root/taking" me 2>/dev/null; then no "release_lock refuses an ownerless lock"; else ok "release_lock refuses an ownerless lock"; fi
[ -d "$root/taking" ] && ok "the ownerless lock survives" || no "the ownerless lock survives"

# Nothing to release = success.
release_lock "$root/absent" me && ok "release_lock on no lock is a no-op" || no "release_lock on no lock is a no-op"

# A stale ownerless dir is reclaimed by take_lock; an owned one never is.
LANE_LOCK_STALE_SECONDS=0 LANE_LOCK_POLL_SECONDS=0
touch -t 202001010000 "$root/taking"
take_lock "$root/taking" me && [ "$(lock_owner "$root/taking")" = me ] && ok "take_lock reclaims a stale ownerless dir" || no "take_lock reclaims a stale ownerless dir"
LANE_LOCK_STALE_SECONDS=300 LANE_LOCK_POLL_SECONDS=30

# The owner write never clobbers: a second writer into a taken dir loses.
mkdir "$root/raced" && echo "first 1" > "$root/raced/owner"
if (set -C; echo "second 2" > "$root/raced/owner") 2>/dev/null; then no "the owner write is noclobber"; else ok "the owner write is noclobber"; fi
grep -q 'set -C; echo' "$here/lock.sh" && ok "take_lock writes the owner with noclobber" || no "take_lock writes the owner with noclobber"

# An owner is one word (a spaced one could never release its own lock).
if take_lock "$root/spaced" "my lane" 2>/dev/null; then no "take_lock refuses a spaced owner"; else ok "take_lock refuses a spaced owner"; fi
[ ! -e "$root/spaced" ] && ok "a refused owner takes nothing" || no "a refused owner takes nothing"

# A prefix-sharing owner is not the owner (review-12 vs review-1225).
mkdir "$root/pr" && echo "review-1225 1" > "$root/pr/owner"
if release_lock "$root/pr" review-12 2>/dev/null; then no "release_lock refuses a prefix-sharing owner"; else ok "release_lock refuses a prefix-sharing owner"; fi

# lane.sh: runs inside the lock, releases after, keeps the command's exit.
export EXP_LANE_LOCK_ROOT="$root"
held=$("$here/lane.sh" heavy me -- sh -c 'cat "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-1/owner"' | cut -d' ' -f1)
[ "$held" = me ] && ok "lane.sh runs with the lane lock held" || no "lane.sh runs with the lane lock held"
[ ! -e "$root/exp-perf-heavy-1" ] && ok "lane.sh releases the lane lock" || no "lane.sh releases the lane lock"
"$here/lane.sh" ios me -- sh -c 'exit 7'
[ $? -eq 7 ] && ok "lane.sh keeps the command's exit status" || no "lane.sh keeps the command's exit status"

# lane.sh never frees a lock someone else took mid-run.
"$here/lane.sh" android me -- sh -c 'echo "other 1" > "$EXP_LANE_LOCK_ROOT/exp-android-emu-1/owner"' 2>/dev/null
[ "$(lock_owner "$root/exp-android-emu-1")" = other ] && ok "lane.sh leaves a lock it no longer owns" || no "lane.sh leaves a lock it no longer owns"

[ "$fails" -eq 0 ] && echo "all lane lock checks passed" || { echo "$fails lane lock check(s) failed"; exit 1; }
