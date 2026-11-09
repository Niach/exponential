#!/bin/sh
# EXP-1393: run one command inside a lane: take the lane's lock, run, release
# it through release_lock (only ever our own), whatever the exit.
#
#   scripts/lanes/lane.sh <ios|android|heavy> <owner> -- <command...>
#   scripts/lanes/lane.sh heavy my-session -- bun run ios:test

set -u
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lock.sh"

[ $# -ge 4 ] && [ "$3" = "--" ] || { echo "usage: $0 <ios|android|heavy> <owner> -- <command...>" >&2; exit 2; }
lane=$1
dir=$(lane_lock_dir "$lane") || exit 2
owner=$2
shift 3

# Traps first: release_lock only frees our own lock, so an interrupt while
# waiting leaves the holder's lock alone, and none lands between the take
# and the trap.
trap 'release_lock "$dir" "$owner" 2>/dev/null' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# This shell lives for the whole run: a waiter takes over only once it is gone.
LANE_LOCK_PID=$$
lock_owner_ok "$owner" || exit 2
slots=$(lane_slots "$lane")
if [ "$slots" -le 1 ]; then
  take_lock "$dir" "$owner" || exit $?
else
  # Any free slot of the lane (lane_slots); `dir` names the one we hold, so
  # the EXIT trap releases exactly that one.
  while :; do
    k=1
    while [ "$k" -le "$slots" ]; do
      dir=$(lane_slot_dir "$lane" "$k")
      try_take_lock "$dir" "$owner" && break 2
      k=$((k + 1))
    done
    echo "lane.sh: all $slots $lane slots busy, waiting" >&2
    sleep "$LANE_LOCK_POLL_SECONDS"
  done
fi
"$@"
