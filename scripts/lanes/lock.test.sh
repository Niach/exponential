#!/bin/sh
# EXP-1393: the lane lock rules, end to end (`bun run test:lanes`).
set -u
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lock.sh"
root=$(mktemp -d "${TMPDIR:-/tmp}/exp-lane-test.XXXXXX")
trap 'rm -rf "$root"' EXIT
# The machine config must not leak into the checks (mint says heavy=2).
export XDG_CONFIG_HOME="$root/config"
# Nor may the checks write the machine's queue log.
export XDG_STATE_HOME="$root/state"
unset LANE_LOG
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

# A dead holder (an ended session) is taken over; a live one, a pid-less one
# and one on another host never are (the 2026-11-08 30bda336 incident).
host=$(lane_lock_host)
sh -c 'exit 0' & deadpid=$!; wait "$deadpid"
mkdir "$root/dead" && echo "gone 1 $deadpid $host" > "$root/dead/owner"
LANE_LOCK_POLL_SECONDS=0
LANE_LOCK_PID=$$ take_lock "$root/dead" me 2>/dev/null && [ "$(lock_owner "$root/dead")" = me ] && ok "take_lock takes over a dead holder's lock" || no "take_lock takes over a dead holder's lock"
[ "$(cut -d' ' -f3 "$root/dead/owner")" = "$$" ] && ok "the owner file records the holder's pid" || no "the owner file records the holder's pid"
[ "$(cut -d' ' -f4 "$root/dead/owner")" = "$host" ] && ok "the owner file records the host" || no "the owner file records the host"
[ ! -e "$root/dead.takeover" ] && ok "the takeover mutex is gone" || no "the takeover mutex is gone"
mkdir "$root/live" && echo "alive 1 $$ $host" > "$root/live/owner"
lock_holder_dead "$root/live" && no "a live holder is not dead" || ok "a live holder is not dead"
mkdir "$root/nopid" && echo "old 1" > "$root/nopid/owner"
lock_holder_dead "$root/nopid" && no "a pid-less lock is never taken over" || ok "a pid-less lock is never taken over"
mkdir "$root/dash" && echo "hand 1 - $host" > "$root/dash/owner"
lock_holder_dead "$root/dash" && no "a lock taken without LANE_LOCK_PID is never taken over" || ok "a lock taken without LANE_LOCK_PID is never taken over"
mkdir "$root/remote" && echo "far 1 $deadpid other-host" > "$root/remote/owner"
lock_holder_dead "$root/remote" && no "another host's lock is never taken over" || ok "another host's lock is never taken over"
mkdir "$root/busy" "$root/busy.takeover" && echo "gone 1 $deadpid $host" > "$root/busy/owner"
lock_take_over "$root/busy" 2>/dev/null; [ -d "$root/busy" ] && ok "a held takeover mutex blocks a second taker" || no "a held takeover mutex blocks a second taker"
LANE_LOCK_POLL_SECONDS=30

# lane.sh: runs inside the lock, releases after, keeps the command's exit.
export EXP_LANE_LOCK_ROOT="$root"
held=$("$here/lane.sh" heavy me -- sh -c 'cat "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-1/owner"' | cut -d' ' -f1)
[ "$held" = me ] && ok "lane.sh runs with the lane lock held" || no "lane.sh runs with the lane lock held"
pid=$("$here/lane.sh" heavy me -- sh -c 'cut -d" " -f3 "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-1/owner"')
case "$pid" in '' | *[!0-9]*) no "lane.sh records its pid" ;; *) ok "lane.sh records its pid" ;; esac
[ ! -e "$root/exp-perf-heavy-1" ] && ok "lane.sh releases the lane lock" || no "lane.sh releases the lane lock"
"$here/lane.sh" ios me -- sh -c 'exit 7'
[ $? -eq 7 ] && ok "lane.sh keeps the command's exit status" || no "lane.sh keeps the command's exit status"

# lane.sh never frees a lock someone else took mid-run.
"$here/lane.sh" android me -- sh -c 'echo "other 1" > "$EXP_LANE_LOCK_ROOT/exp-android-emu-1/owner"' 2>/dev/null
[ "$(lock_owner "$root/exp-android-emu-1")" = other ] && ok "lane.sh leaves a lock it no longer owns" || no "lane.sh leaves a lock it no longer owns"


# Slots (exp-lane-slots `heavy=2`): two lane.sh runs hold the heavy lane at
# once, each its own slot dir, both released after; the default stays 1.
[ "$(lane_slots heavy)" = 1 ] && ok "a lane has one slot by default" || no "a lane has one slot by default"
printf 'heavy=2\n' > "$root/exp-lane-slots"
[ "$(lane_slots heavy)" = 2 ] && ok "exp-lane-slots raises a lane's slots" || no "exp-lane-slots raises a lane's slots"
[ "$(lane_slots ios)" = 1 ] && ok "other lanes keep one slot" || no "other lanes keep one slot"
"$here/lane.sh" heavy first -- sh -c 'touch "$EXP_LANE_LOCK_ROOT/first-in"; sleep 3' &
firstpid=$!
i=0; while [ ! -e "$root/first-in" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
second=$(LANE_LOCK_POLL_SECONDS=30 "$here/lane.sh" heavy second -- sh -c 'cut -d" " -f1 "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-1/owner"; cut -d" " -f1 "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-2/owner"' | tr '\n' ' ')
[ "$second" = "first second " ] && ok "a second run takes the free slot while the first holds slot 1" || no "a second run takes the free slot while the first holds slot 1 (got '$second')"
[ ! -e "$root/exp-perf-heavy-2" ] && ok "the second run releases its slot" || no "the second run releases its slot"
wait "$firstpid"
[ ! -e "$root/exp-perf-heavy-1" ] && ok "the first run releases slot 1" || no "the first run releases slot 1"
rm -f "$root/exp-lane-slots" "$root/first-in"


# --own-target: the command sees THIS checkout's apps/desktop/target as
# CARGO_TARGET_DIR, whatever the machine exports; without it, the shared one.
top=$(git -C "$here" rev-parse --show-toplevel)
seen=$(CARGO_TARGET_DIR=/shared/target "$here/lane.sh" heavy me --own-target -- sh -c 'echo "$CARGO_TARGET_DIR"' 2>/dev/null)
[ "$seen" = "$top/apps/desktop/target" ] && ok "--own-target builds into this checkout's target" || no "--own-target builds into this checkout's target (got '$seen')"
seen=$(CARGO_TARGET_DIR=/shared/target "$here/lane.sh" heavy me -- sh -c 'echo "$CARGO_TARGET_DIR"')
[ "$seen" = /shared/target ] && ok "without --own-target the shared target stays" || no "without --own-target the shared target stays (got '$seen')"
[ ! -e "$root/exp-perf-heavy-1" ] && ok "--own-target still releases the lane" || no "--own-target still releases the lane"


# The machine config (survives a reboot) sets slots too; the lock-root file
# still overrides it.
mkdir -p "$XDG_CONFIG_HOME/exponential" && printf 'heavy=3\n' > "$XDG_CONFIG_HOME/exponential/lane-slots"
[ "$(lane_slots heavy)" = 3 ] && ok "the machine config sets a lane's slots" || no "the machine config sets a lane's slots"
printf 'heavy=1\n' > "$root/exp-lane-slots"
[ "$(lane_slots heavy)" = 1 ] && ok "the lock-root file overrides the machine config" || no "the lock-root file overrides the machine config"
rm -f "$root/exp-lane-slots" "$XDG_CONFIG_HOME/exponential/lane-slots"


# #EXP-1844: who holds a lane, for how long, and a max hold for heavy.
log="$root/state/exponential/lanes.log"
now=$(date +%s)
EXP_MCP_SESSION_ID=abcdef12-3456-7890-abcd-ef1234567890 LANE_LOCK_PID=$$ take_lock "$root/exp-perf-heavy-1" me
[ "$(lock_field "$root/exp-perf-heavy-1" 5)" = abcdef12 ] && ok "the owner file records the session" || no "the owner file records the session"
grep -q "exp-perf-heavy-1 take owner=me session=abcdef12 pid=$$ waited=0s" "$log" && ok "a take is logged" || no "a take is logged"
LANE_LOCK_PID=$$ release_lock "$root/exp-perf-heavy-1" me
grep -q "exp-perf-heavy-1 release owner=me .* held=[0-9]*s" "$log" && ok "a release is logged with its hold time" || no "a release is logged with its hold time"

mkdir "$root/described" && echo "other $((now - 125)) $$ $host sess1234" > "$root/described/owner"
d=$(lock_describe "$root/described")
[ "$d" = "held by other (session sess1234, pid $$) for 2m" ] && ok "a busy lock names owner, session, pid and age" || no "a busy lock names owner, session, pid and age (got '$d')"

# A live heavy holder past LANE_MAX_HOLD_SECONDS hands its slot to the next
# waiter, and the overrun holder can no longer release the new holder's lock.
mkdir "$root/exp-perf-heavy-1" && echo "slow $((now - 100)) $$ $host sess0001" > "$root/exp-perf-heavy-1/owner"
out=$(LANE_MAX_HOLD_SECONDS=60 LANE_LOCK_POLL_SECONDS=0 "$here/lane.sh" heavy fast -- sh -c 'cut -d" " -f1 "$EXP_LANE_LOCK_ROOT/exp-perf-heavy-1/owner"' 2>/dev/null)
[ "$out" = fast ] && ok "an overdue heavy holder hands its slot to the waiter" || no "an overdue heavy holder hands its slot to the waiter (got '$out')"
grep -q "exp-perf-heavy-1 handover owner=slow session=sess0001 pid=$$ held=[0-9]*s reason=overdue to=fast" "$log" && ok "a handover is logged with the old holder's hold" || no "a handover is logged with the old holder's hold"
mkdir "$root/exp-perf-heavy-1" && echo "slow $((now - 100)) $$ $host sess0001" > "$root/exp-perf-heavy-1/owner"
if LANE_MAX_HOLD_SECONDS=0 try_take_lock "$root/exp-perf-heavy-1" fast 2>/dev/null; then no "LANE_MAX_HOLD_SECONDS=0 never hands over"; else ok "LANE_MAX_HOLD_SECONDS=0 never hands over"; fi
if LANE_MAX_HOLD_SECONDS=600 try_take_lock "$root/exp-perf-heavy-1" fast 2>/dev/null; then no "a heavy holder within its max hold keeps the slot"; else ok "a heavy holder within its max hold keeps the slot"; fi
rm -rf "$root/exp-perf-heavy-1"
mkdir "$root/exp-ios-sim-1" && echo "sim $((now - 100000)) $$ $host sess0002" > "$root/exp-ios-sim-1/owner"
if LANE_MAX_HOLD_SECONDS=60 try_take_lock "$root/exp-ios-sim-1" fast 2>/dev/null; then no "a device lane is never handed over"; else ok "a device lane is never handed over"; fi
rm -rf "$root/exp-ios-sim-1"
mkdir "$root/handed" && echo "me $now 1 $host -" > "$root/handed/owner"
if LANE_LOCK_PID=$$ release_lock "$root/handed" me 2>/dev/null; then no "a handed-over holder never frees the new holder's lock"; else ok "a handed-over holder never frees the new holder's lock"; fi

# The holder is warned at its deadline, its command keeps running to the end.
err=$(LANE_MAX_HOLD_SECONDS=1 "$here/lane.sh" heavy long -- sh -c 'sleep 2; echo done' 2>&1)
case "$err" in *"keeps running"*done*) ok "the holder is warned at the max hold and keeps running" ;; *) no "the holder is warned at the max hold and keeps running (got '$err')" ;; esac
grep -q "exp-perf-heavy-1 overrun owner=long .* held=1s" "$log" && ok "an overrun is logged" || no "an overrun is logged"
[ ! -e "$root/exp-perf-heavy-1" ] && ok "an overrun holder still releases its slot" || no "an overrun holder still releases its slot"

# A waiter names the holder's session, is logged and listed by `status`.
mkdir "$root/exp-perf-heavy-1" && echo "busy $((now - 300)) $$ $host sess0003" > "$root/exp-perf-heavy-1/owner"
EXP_MCP_SESSION_ID=waiter99 LANE_LOCK_POLL_SECONDS=1 "$here/lane.sh" heavy queued -- true 2>"$root/waiter.err" &
waiter=$!
i=0; while ! grep -q waiting "$root/waiter.err" 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
grep -q "heavy lane busy: held by busy (session sess0003, pid $$) for 5m" "$root/waiter.err" && ok "a waiter is told who holds the lane and for how long" || no "a waiter is told who holds the lane and for how long ($(cat "$root/waiter.err"))"
status=$("$here/lane.sh" status)
echo "$status" | grep -q "^heavy slot 1: held by busy (session sess0003, pid $$) for 5m" && ok "status prints the holder" || no "status prints the holder ($status)"
echo "$status" | grep -q "exp-perf-heavy-1: queued (session waiter99, pid [0-9]*) for" && ok "status prints the waiters" || no "status prints the waiters ($status)"
echo "$status" | grep -q "^ios slot 1: free" && ok "status prints a free lane" || no "status prints a free lane"
grep -q "exp-perf-heavy-1 wait owner=queued session=waiter99 .* holder=busy holder_session=sess0003" "$log" && ok "a wait is logged with the holder" || no "a wait is logged with the holder"
rm -rf "$root/exp-perf-heavy-1"
wait "$waiter"
grep -q "take owner=queued session=waiter99 pid=[0-9]* waited=[0-9]*s" "$log" && ok "the waiter's take is logged with its wait" || no "the waiter's take is logged with its wait"
[ -z "$(ls "$root/exp-lane-waiters" 2>/dev/null)" ] && ok "a waiter that got its slot leaves the queue" || no "a waiter that got its slot leaves the queue"

# Gradle daemons don't outlive their session (#EXP-1844): an idle timeout
# well under the 3 h default.
grep -q '^org.gradle.daemon.idletimeout=1800000$' "$here/../../apps/android/gradle.properties" && ok "gradle daemons stop after 30 idle minutes" || no "gradle daemons stop after 30 idle minutes"

[ "$fails" -eq 0 ] && echo "all lane lock checks passed" || { echo "$fails lane lock check(s) failed"; exit 1; }
