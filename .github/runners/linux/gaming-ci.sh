#!/usr/bin/env bash
# While a game runs, the Mint's two Docker runners become one (exp-gaming.service,
# .github/runners/README.md "Desktop first").
#   gaming-ci.sh hold     stop ONE runner as soon as it is between jobs (never
#                         mid-job), runner-2 first; keeps watching
#   gaming-ci.sh release  start the runner `hold` stopped
# The stopped runner goes offline, so pick-runner.yml counts one exp-linux
# runner and overflows to GitHub-hosted sooner. The mint-ci daemon is held to
# one job by its runner settings, not here.
set -uo pipefail
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/exponential"
MARK="$STATE/gaming-ci.stopped"
LOG="$STATE/gaming-ci.log"
mkdir -p "$STATE"
log() { echo "$(date -Is) $*" >>"$LOG"; }
running() { [ "$(docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null)" = true ]; }
busy() { docker top "$1" -eo pid,comm 2>/dev/null | grep -q Runner.Worker; }

case "${1:-}" in
  hold)
    log "game started"
    while :; do
      if [ ! -s "$MARK" ]; then
        up=()
        for c in exp-runners-runner-2-1 exp-runners-runner-1-1; do running "$c" && up+=("$c"); done
        if [ "${#up[@]}" -ge 2 ]; then
          for c in "${up[@]}"; do
            if ! busy "$c"; then
              echo "$c" >"$MARK"
              docker stop -t 30 "$c" >/dev/null && log "stopped $c (idle)"
              break
            fi
          done
        fi
      fi
      sleep 20
    done
    ;;
  release)
    if [ -s "$MARK" ]; then
      c=$(cat "$MARK")
      docker start "$c" >/dev/null && log "game ended, started $c"
      rm -f "$MARK"
    else
      log "game ended"
    fi
    ;;
  *) echo "usage: $0 hold|release" >&2; exit 2 ;;
esac
