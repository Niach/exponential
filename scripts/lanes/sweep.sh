#!/bin/sh
# #EXP-1865 / #EXP-1996: the lanes' low-disk sweep of the shared cargo target
# (sourced by lane.sh).
#
#   lane_sweep <cargo-target-dir>
#
# Runs `exponential sweep-target --if-low <dir>` with the first CLI that
# knows the command: $LANE_SWEEP_CLI, every `exponential` on PATH, then the
# target's own build (`<dir>/release/exponential`, `<dir>/debug/exponential`).
# On the MacBook the PATH one was 0.1.0 from August: it answered "Unknown
# command `sweep-target`" and every lane skipped the sweep without a word
# (#EXP-1996). Now an old CLI is passed over, and when none is left the lane
# says so once on stderr. Always returns 0: a sweep never fails a lane.

# The candidates, one per line.
lane_sweep_candidates() {
  [ -n "${LANE_SWEEP_CLI:-}" ] && echo "$LANE_SWEEP_CLI"
  old_ifs=$IFS
  IFS=:
  for d in $PATH; do
    [ -n "$d" ] && [ -x "$d/exponential" ] && [ ! -d "$d/exponential" ] && echo "$d/exponential"
  done
  IFS=$old_ifs
  for p in release debug; do
    [ -x "$1/$p/exponential" ] && echo "$1/$p/exponential"
  done
  return 0
}

lane_sweep() {
  sweep_target=$1
  sweep_old=
  sweep_seen=
  sweep_candidates=$(lane_sweep_candidates "$sweep_target")
  old_ifs=$IFS
  IFS='
'
  for cli in $sweep_candidates; do
    IFS=$old_ifs
    case "
$sweep_seen
" in *"
$cli
"*) continue ;; esac
    sweep_seen="$sweep_seen
$cli"
    sweep_out=$("$cli" sweep-target --if-low "$sweep_target" 2>&1)
    sweep_rc=$?
    case $sweep_out in
      *"Unknown command"*)
        sweep_old="$sweep_old${sweep_old:+, }$cli ($("$cli" --version 2>/dev/null | head -n 1))"
        continue
        ;;
    esac
    if [ "$sweep_rc" -ne 0 ]; then
      echo "lane.sh: sweep-target failed (exit $sweep_rc) with $cli: $sweep_out" >&2
    elif [ -n "$sweep_out" ]; then
      echo "lane.sh: $sweep_out" >&2
    fi
    return 0
  done
  IFS=$old_ifs
  if [ -n "$sweep_old" ]; then
    echo "lane.sh: NOT sweeping $sweep_target: no \`exponential\` here knows \`sweep-target\` (too old: $sweep_old). Install a current CLI (curl -fsSL https://exponential.at/install.sh | sh) or point LANE_SWEEP_CLI at one." >&2
  else
    echo "lane.sh: NOT sweeping $sweep_target: no \`exponential\` CLI found. Install it (curl -fsSL https://exponential.at/install.sh | sh) or point LANE_SWEEP_CLI at one." >&2
  fi
  return 0
}
