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
dir=$(lane_lock_dir "$1") || exit 2
owner=$2
shift 3

# Traps first: release_lock only frees our own lock, so an interrupt while
# waiting leaves the holder's lock alone, and none lands between the take
# and the trap.
trap 'release_lock "$dir" "$owner" 2>/dev/null' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
take_lock "$dir" "$owner" || exit $?
"$@"
