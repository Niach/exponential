#!/usr/bin/env bash
# While a game runs, the Mint's two Docker runners become one (exp-gaming.service,
# .github/runners/README.md "Desktop first").
#   gaming-ci.sh hold     stop runner-2, and only runner-2, as soon as it is
#                         between jobs (never mid-job); keeps runner-1 up;
#                         keeps watching
#   gaming-ci.sh release  start runner-2 again
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

KEEP=exp-runners-runner-1-1   # never stopped here: CI keeps one runner
GIVE=exp-runners-runner-2-1   # the only runner a game may stop

case "${1:-}" in
  hold)
    log "game started"
    while :; do
      # The kept runner must be up: a kill from outside (2026-11-24: every
      # container was killed with its restart policy set to no) otherwise
      # leaves the Mint with no runner at all.
      if ! running "$KEEP"; then
        docker update --restart=unless-stopped "$KEEP" >/dev/null 2>&1
        docker start "$KEEP" >/dev/null && log "started $KEEP (was down)"
      fi
      if running "$GIVE" && ! busy "$GIVE"; then
        echo "$GIVE" >"$MARK"
        docker stop -t 30 "$GIVE" >/dev/null && log "stopped $GIVE (idle)"
      fi
      sleep 20
    done
    ;;
  release)
    # Start it whether or not the mark survived (a reboot or a kill loses
    # it, and the runner then stayed down until someone noticed).
    if ! running "$GIVE"; then
      docker update --restart=unless-stopped "$GIVE" >/dev/null 2>&1
      docker start "$GIVE" >/dev/null && log "game ended, started $GIVE"
    else
      log "game ended"
    fi
    rm -f "$MARK"
    ;;
  *) echo "usage: $0 hold|release" >&2; exit 2 ;;
esac
