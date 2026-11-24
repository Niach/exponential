#!/bin/sh
# #EXP-1868: the session manifest. Everything a session creates OUTSIDE its
# cgroup scope (an iOS simulator, an Android AVD, a DerivedData or /tmp build
# dir, a compose project) is listed here by whoever creates it, and the daemon
# deletes exactly what is listed when the session ends, or when it is reaped
# after a crash (coding::session_manifest). No guessing by name.
#
#   scripts/lanes/session-manifest.sh add <sim|avd|dir|compose|lane> <value>
#   scripts/lanes/session-manifest.sh path      # this session's manifest file
#   scripts/lanes/session-manifest.sh list      # what it lists
#   . scripts/lanes/session-manifest.sh         # then: session_manifest_add <kind> <value>
#
# Kinds and values:
#   sim      a simulator UDID (`xcrun simctl create` prints it): simctl delete
#   avd      an AVD name: avdmanager delete avd (or its .avd dir + .ini)
#   dir      an absolute path under a temp dir, Xcode's DerivedData or the
#            session's worktree: rm -rf (anything else is refused at cleanup)
#   compose  a compose project name: docker compose -p <name> down --volumes
#   lane     a running scripts/lanes/lane.sh's pid (#FEED-306, lane.sh adds
#            and removes its own): its whole process tree is ended (TERM,
#            then KILL), only while that pid still runs a lane.sh of this
#            session
#
# The session is EXP_MCP_SESSION_ID (every agent run has it). Outside a session
# (a person's shell, CI) `add` does nothing and says so on stderr: nothing
# would ever clean it up, so nothing is recorded.

session_manifest_dir() {
  echo "${EXP_SESSION_MANIFEST_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/exponential/session-manifests}"
}

session_manifest_path() {
  _sm_session=${EXP_MCP_SESSION_ID:-}
  case "$_sm_session" in
    '' | *[!A-Za-z0-9-]*) return 1 ;;
  esac
  echo "$(session_manifest_dir)/$_sm_session"
}

# Appends "<kind>\t<value>" once (a second add of the same line is a no-op).
session_manifest_add() {
  case "${1:-}" in
    sim | avd | dir | compose | lane) ;;
    *) echo "session-manifest: unknown kind '${1:-}' (sim, avd, dir, compose, lane)" >&2; return 2 ;;
  esac
  [ -n "${2:-}" ] || { echo "session-manifest: add $1 needs a value" >&2; return 2; }
  if [ "$1" = lane ]; then
    case "$2" in *[!0-9]*) echo "session-manifest: a lane is a pid" >&2; return 2 ;; esac
  fi
  case "$2" in *"	"* | *"
"*) echo "session-manifest: a value is one line without tabs" >&2; return 2 ;; esac
  if [ "$1" = dir ]; then
    case "$2" in /*) ;; *) echo "session-manifest: a dir must be an absolute path" >&2; return 2 ;; esac
  fi
  _sm_file=$(session_manifest_path) || { echo "session-manifest: no session (EXP_MCP_SESSION_ID): $1 $2 not recorded" >&2; return 0; }
  mkdir -p "$(dirname "$_sm_file")" || return 1
  _sm_line=$(printf '%s\t%s' "$1" "$2")
  grep -qxF "$_sm_line" "$_sm_file" 2>/dev/null && return 0
  printf '%s\n' "$_sm_line" >> "$_sm_file"
}

# #FEED-306: drops the "<kind>\t<value>" line again (lane.sh at its end:
# a pid listed after its run could name someone else's process later).
session_manifest_remove() {
  _sm_file=$(session_manifest_path) || return 0
  [ -f "$_sm_file" ] || return 0
  _sm_line=$(printf '%s\t%s' "$1" "$2")
  grep -qxF "$_sm_line" "$_sm_file" || return 0
  grep -vxF "$_sm_line" "$_sm_file" > "$_sm_file.$$" ; mv -f "$_sm_file.$$" "$_sm_file"
}

# Run as a command (not sourced): `$0` is this file.
case "${0##*/}" in
  session-manifest.sh)
    case "${1:-}" in
      add) shift; session_manifest_add "$@" ;;
      path) session_manifest_path || { echo "session-manifest: no session (EXP_MCP_SESSION_ID)" >&2; exit 1; } ;;
      list) f=$(session_manifest_path) && [ -f "$f" ] && cat "$f"; exit 0 ;;
      *) echo "usage: $0 add <sim|avd|dir|compose|lane> <value> | path | list" >&2; exit 2 ;;
    esac
    ;;
esac
