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

take_lock "$dir" "$owner" || exit $?
trap 'release_lock "$dir" "$owner"' EXIT
trap 'exit 130' INT TERM
"$@"
