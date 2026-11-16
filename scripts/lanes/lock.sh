# EXP-1393: the device and heavy-build lane locks, ONE implementation.
#
# A lock is a directory (`mkdir` is atomic) holding an `owner` file whose
# first word names the holder: "<owner> <epoch> <pid> <host>". Work only after your own
# take_lock returned; release only what you own. release_lock REFUSES (exit 1,
# the lock stays) when the owner file names someone else, so one lane's
# cleanup can never free another lane's device.
#
#   . scripts/lanes/lock.sh    # sh, bash or zsh; then take_lock / release_lock <dir> <owner>
#   scripts/lanes/lane.sh <lane> <owner> -- <command...>   # the whole cycle
#
# Lane locks (lane_lock_dir): ios = the simulator, android = the emulator,
# heavy = one heavy build or perf run at a time, or N at once on a machine
# whose exp-lane-slots file says `heavy=N` (lane.sh takes any free slot).
#
# The pid is LANE_LOCK_PID (lane.sh sets its own, alive for the whole run).
# A waiter on the SAME host takes over a lock whose pid is gone (a session
# ended mid-run, EXP-1393 follow-up). A lock without a pid (taken by hand
# without LANE_LOCK_PID, or by an older lock.sh) is never taken over: set
# LANE_LOCK_PID to a process that lives as long as you hold the lock.
#
# #EXP-1844: the owner file also names the session (5th word, the first 8 of
# EXP_MCP_SESSION_ID), so a waiter sees WHO holds a lane and for how long. A
# heavy-lane holder past LANE_MAX_HOLD_SECONDS (default 45 min, 0 = never) is
# handed over: the next waiter takes the slot (its build is NOT killed; it
# just stops counting against the lane), and the overrun is logged. Every
# take, wait, release, handover and overrun goes to the queue log
# (lane_log_file: ~/.local/state/exponential/lanes.log); `lane.sh status`
# prints holders and waiters.

LANE_LOCK_STALE_SECONDS=${LANE_LOCK_STALE_SECONDS:-300}
LANE_LOCK_POLL_SECONDS=${LANE_LOCK_POLL_SECONDS:-30}
LANE_MAX_HOLD_SECONDS=${LANE_MAX_HOLD_SECONDS:-2700}

lane_lock_dir() {
  case "$1" in
    ios) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-ios-sim-1" ;;
    android) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-android-emu-1" ;;
    heavy) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-perf-heavy-1" ;;
    *) echo "lane_lock_dir: unknown lane '$1' (ios, android, heavy)" >&2; return 2 ;;
  esac
}

# The holder named by the lock's owner file (empty = none written).
lock_owner() {
  cut -d' ' -f1 "$1/owner" 2>/dev/null
}

# Word <n> of <dir>'s owner file: 1 owner, 2 epoch, 3 pid, 4 host, 5 session.
lock_field() {
  cut -d' ' -f"$2" "$1/owner" 2>/dev/null
}

# This session's short id (the first 8 of EXP_MCP_SESSION_ID), `-` = none.
lane_session() {
  _lane_s=${EXP_MCP_SESSION_ID:-${EXP_SESSION_ID:-}}
  _lane_s=$(printf '%s' "$_lane_s" | cut -c1-8)
  echo "${_lane_s:--}"
}

# Seconds as `1h05m`, `12m`, `40s`.
lane_age() {
  if [ "$1" -ge 3600 ]; then
    printf '%dh%02dm\n' $(($1 / 3600)) $(($1 % 3600 / 60))
  elif [ "$1" -ge 60 ]; then
    printf '%dm\n' $(($1 / 60))
  else
    printf '%ds\n' "$1"
  fi
}

# Seconds <dir>'s holder has held it (empty = no readable start).
lock_held_seconds() {
  _lane_t=$(lock_field "$1" 2)
  case "$_lane_t" in '' | *[!0-9]*) return 1 ;; esac
  echo $(( $(date +%s) - _lane_t ))
}

# "held by <owner> (session <s>, pid <p>) for <age>": what a waiter sees.
lock_describe() {
  _lane_o=$(lock_owner "$1")
  _lane_held=$(lock_held_seconds "$1") && _lane_for=" for $(lane_age "$_lane_held")" || _lane_for=
  echo "held by ${_lane_o:-nobody yet} (session $(lock_field "$1" 5 | grep . || echo -), pid $(lock_field "$1" 3 | grep . || echo -))$_lane_for"
}

# The queue log (#EXP-1844). LANE_LOG overrides it.
lane_log_file() {
  echo "${LANE_LOG:-${XDG_STATE_HOME:-$HOME/.local/state}/exponential/lanes.log}"
}

# Appends "<utc time> <lock dir name> <event> <details…>". Never a failure.
lane_log() {
  _lane_log=$(lane_log_file)
  { mkdir -p "$(dirname "$_lane_log")" && echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $(basename "$1") $2" >> "$_lane_log"; } 2>/dev/null
  return 0
}

# How long <dir>'s holder may keep it before a waiter takes over, in seconds:
# LANE_MAX_HOLD_SECONDS for a heavy slot, 0 (no limit) for the devices: a
# simulator or emulator in use is never shared.
lane_max_hold() {
  case "$(basename "$1")" in
    exp-perf-heavy-*) echo "$LANE_MAX_HOLD_SECONDS" ;;
    *) echo 0 ;;
  esac
}

# True when <dir>'s holder is past its lane's max hold.
lock_overdue() {
  _lane_max=$(lane_max_hold "$1")
  [ "${_lane_max:-0}" -gt 0 ] 2>/dev/null || return 1
  _lane_held=$(lock_held_seconds "$1") || return 1
  [ "$_lane_held" -gt "$_lane_max" ]
}

# The waiter registry `lane.sh status` reads: one file per waiting pid.
lane_waiters_dir() {
  echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-waiters"
}

# Starts waiting for <dir> as <owner> (once per wait): logs `wait` with the
# holder, registers the waiter and remembers since when (for `take`).
lane_wait_begin() {
  [ -n "${_lane_wait_since:-}" ] && return 0
  _lane_wait_since=$(date +%s)
  lane_log "$1" "wait owner=$2 session=$(lane_session) pid=${LANE_LOCK_PID:-$$} holder=$(lock_owner "$1") holder_session=$(lock_field "$1" 5) holder_pid=$(lock_field "$1" 3)"
  _lane_wd=$(lane_waiters_dir)
  mkdir -p "$_lane_wd" 2>/dev/null && echo "$2 $_lane_wait_since ${LANE_LOCK_PID:-$$} $(lane_session) $(basename "$1")" > "$_lane_wd/$(basename "$1").${LANE_LOCK_PID:-$$}" 2>/dev/null
  return 0
}

lane_wait_end() {
  rm -f "$(lane_waiters_dir)"/*."${LANE_LOCK_PID:-$$}" 2>/dev/null
  return 0
}

# Blocks until <dir> is ours. A dir left without an owner file for longer
# than LANE_LOCK_STALE_SECONDS (a holder killed between mkdir and the write)
# is reclaimed; an owned lock is never taken over. The owner file is written
# with noclobber: if two waiters reclaim the same stale dir at once, only the
# first write wins and the other keeps waiting.
take_lock() {
  [ -n "${1:-}" ] && [ -n "${2:-}" ] || { echo "take_lock: usage: take_lock <dir> <owner>" >&2; return 2; }
  lock_owner_ok "$2" || return 2
  _lane_seen_holder=
  while ! try_take_lock "$1" "$2"; do
    lane_wait_begin "$1" "$2"
    # Named once per holder, not every poll.
    if [ "$(cat "$1/owner" 2>/dev/null)" != "$_lane_seen_holder" ]; then
      _lane_seen_holder=$(cat "$1/owner" 2>/dev/null)
      echo "take_lock: $1 busy: $(lock_describe "$1"), waiting" >&2
    fi
    sleep "$LANE_LOCK_POLL_SECONDS"
  done
}

# ONE attempt at <dir> (no wait): 0 = it is ours now. Reclaims a dead
# holder's lock and a stale ownerless dir exactly as take_lock always did,
# and hands over a heavy slot whose holder is past its max hold (#EXP-1844).
try_take_lock() {
  while ! { mkdir "$1" 2>/dev/null && (set -C; echo "$2 $(date +%s) ${LANE_LOCK_PID:--} $(lane_lock_host) $(lane_session)" > "$1/owner") 2>/dev/null; }; do
    if lock_holder_dead "$1" || lock_overdue "$1"; then
      lock_take_over "$1" "$2" && continue
    fi
    if [ -d "$1" ] && [ ! -e "$1/owner" ]; then
      # GNU stat first: its `-f` is filesystem mode, not BSD's format flag.
      # No mtime = not stale: a failed stat never reclaims.
      mtime=$(stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null) || mtime=
      if [ -n "$mtime" ] && [ $(( $(date +%s) - mtime )) -gt "$LANE_LOCK_STALE_SECONDS" ]; then
        rmdir "$1" 2>/dev/null && continue
      fi
    fi
    return 1
  done
  _lane_now=$(date +%s)
  lane_log "$1" "take owner=$2 session=$(lane_session) pid=${LANE_LOCK_PID:--} waited=$(( _lane_now - ${_lane_wait_since:-$_lane_now} ))s"
  lane_wait_end
  _lane_wait_since=
}

# How many holders <lane> admits on THIS machine (lines `heavy=2`), else 1:
# the machine's config `${XDG_CONFIG_HOME:-$HOME/.config}/exponential/lane-slots`
# (survives a reboot), then `${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-slots`
# (a one-off override; the later line wins).
# Slot 1 is the lane's classic dir (lane_lock_dir); slot k appends `-k`'s
# number in place of the trailing 1 (exp-perf-heavy-2, …).
lane_slots() {
  n=$(cat "${XDG_CONFIG_HOME:-$HOME/.config}/exponential/lane-slots" "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-slots" 2>/dev/null \
    | sed -n "s/^$1=\([0-9][0-9]*\)[[:space:]]*$/\1/p" | tail -n 1)
  case "$n" in '' | 0) echo 1 ;; *) echo "$n" ;; esac
}

lane_slot_dir() {
  base=$(lane_lock_dir "$1") || return 2
  echo "${base%-1}-$2"
}

lane_lock_host() {
  hostname 2>/dev/null | cut -d. -f1
}

# True when <dir>'s owner file names a pid on this host and no such process
# exists. No pid, another host or an unreadable file = alive (never taken).
lock_holder_dead() {
  # cut, not `set -- $line`: zsh does not word-split an unquoted variable.
  [ -r "$1/owner" ] || return 1
  _lane_pid=$(cut -d' ' -f3 "$1/owner" 2>/dev/null)
  _lane_host=$(cut -d' ' -f4 "$1/owner" 2>/dev/null)
  case "$_lane_pid" in '' | *[!0-9]*) return 1 ;; esac
  [ -n "$_lane_host" ] && [ "$_lane_host" = "$(lane_lock_host)" ] || return 1
  ! ps -p "$_lane_pid" >/dev/null 2>&1
}

# Removes a dead or overdue holder's <dir> under a short takeover mutex,
# re-checking inside it so two waiters never both remove (one could remove
# the other's fresh lock). A mutex older than 60 s (its taker died inside) is
# reclaimed. <taker> (optional) names who takes it, for the log.
lock_take_over() {
  _lane_seen=$(cat "$1/owner" 2>/dev/null) || return 1
  if ! mkdir "$1.takeover" 2>/dev/null; then
    mtime=$(stat -c %Y "$1.takeover" 2>/dev/null || stat -f %m "$1.takeover" 2>/dev/null) || mtime=
    [ -n "$mtime" ] && [ $(( $(date +%s) - mtime )) -gt 60 ] && rmdir "$1.takeover" 2>/dev/null
    return 1
  fi
  if [ "$(cat "$1/owner" 2>/dev/null)" = "$_lane_seen" ]; then
    _lane_desc=$(lock_describe "$1")
    _lane_held=$(lock_held_seconds "$1") || _lane_held=-
    if lock_holder_dead "$1"; then
      echo "take_lock: taking over $1 from '$(echo "$_lane_seen" | cut -d' ' -f1)' (pid $(echo "$_lane_seen" | cut -d' ' -f3) is gone)" >&2
      lane_log "$1" "takeover owner=$(lock_owner "$1") session=$(lock_field "$1" 5) pid=$(lock_field "$1" 3) held=${_lane_held}s reason=dead to=${2:-?}"
      rm -rf "$1"
    elif lock_overdue "$1"; then
      # #EXP-1844: the overrun holder's build keeps running; only the slot
      # moves. Its own release can no longer free ours (release_lock checks
      # the pid), and its lane.sh already warned it at the deadline.
      echo "take_lock: $1 $_lane_desc, past the lane's max hold of $(lane_age "$(lane_max_hold "$1")"): taking the slot over" >&2
      lane_log "$1" "handover owner=$(lock_owner "$1") session=$(lock_field "$1" 5) pid=$(lock_field "$1" 3) held=${_lane_held}s reason=overdue to=${2:-?}"
      rm -rf "$1"
    fi
  fi
  rmdir "$1.takeover"
}

# An owner is one word: lock_owner reads the first word back.
lock_owner_ok() {
  case "$1" in
    *" "* | *"	"*) echo "lane lock: owner '$1' must be one word" >&2; return 2 ;;
  esac
}

# Removes <dir> only when its owner file names <owner> (and, when both sides
# know one, LANE_LOCK_PID: a holder whose slot was handed over to a waiter
# of the same owner name never frees the new holder's lock). Nothing there =
# done.
release_lock() {
  [ -n "${1:-}" ] && [ -n "${2:-}" ] || { echo "release_lock: usage: release_lock <dir> <owner>" >&2; return 2; }
  lock_owner_ok "$2" || return 2
  [ -e "$1" ] || return 0
  holder=$(lock_owner "$1")
  if [ "$holder" != "$2" ]; then
    echo "release_lock: refusing to remove $1: owned by '${holder:-nobody}', not '$2'" >&2
    return 1
  fi
  _lane_pid=$(lock_field "$1" 3)
  case "$_lane_pid" in
    '' | *[!0-9]*) ;;
    *)
      if [ -n "${LANE_LOCK_PID:-}" ] && [ "$_lane_pid" != "$LANE_LOCK_PID" ]; then
        echo "release_lock: refusing to remove $1: held by pid $_lane_pid now, not $LANE_LOCK_PID (handed over)" >&2
        return 1
      fi
      ;;
  esac
  _lane_held=$(lock_held_seconds "$1") || _lane_held=-
  rm -rf "$1"
  lane_log "$1" "release owner=$2 session=$(lane_session) pid=${LANE_LOCK_PID:--} held=${_lane_held}s"
}
