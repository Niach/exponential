#!/bin/sh
# #EXP-1996: the lanes' sweep finds a CLI that knows `sweep-target` and says
# so when there is none (`bun run test:lanes`). Fake CLIs only: nothing here
# sweeps a real target.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(mktemp -d "${TMPDIR:-/tmp}/exp-lane-sweep-test.XXXXXX")
trap 'rm -rf "$root"' EXIT
fails=0
ok() { echo "ok - $1"; }
no() { echo "not ok - $1"; fails=$((fails + 1)); }
. "$here/sweep.sh"

# A CLI from before `sweep-target` (the MacBook's 0.1.0): exit 0, usage text.
old_cli() {
  mkdir -p "$(dirname "$1")"
  printf '#!/bin/sh\necho "ran $0 $*" >> "%s/calls"\n[ "$1" = --version ] && { echo "exponential 0.1.0"; exit 0; }\necho "Unknown command `$1`."\necho\necho "exponential — Exponential from your terminal"\n' "$root" > "$1"
  chmod +x "$1"
}
# A current one: prints `$2` (the sweep's log line; empty = disk not low).
new_cli() {
  mkdir -p "$(dirname "$1")"
  printf '#!/bin/sh\necho "ran $0 $*" >> "%s/calls"\n[ -n "%s" ] && echo "%s" >&2\nexit %s\n' "$root" "$2" "$2" "${3:-0}" > "$1"
  chmod +x "$1"
}
target="$root/t.cargo-target"
mkdir -p "$target"
run() { : > "$root/calls"; out=$(PATH="$1:/usr/bin:/bin" lane_sweep "$target" 2>&1); rc=$?; }

old_cli "$root/old/exponential"
new_cli "$root/new/exponential" "cargo-target sweep $target: freed 1.3 GB"
run "$root/old:$root/new"
[ "$rc" -eq 0 ] && [ "$out" = "lane.sh: cargo-target sweep $target: freed 1.3 GB" ] && ok "an old CLI first on PATH is passed over for a current one" || no "an old CLI first on PATH is passed over for a current one ($rc: $out)"
grep -q "ran $root/new/exponential sweep-target --if-low $target" "$root/calls" && ok "the current CLI sweeps the lane's target with --if-low" || no "the current CLI sweeps the lane's target with --if-low ($(cat "$root/calls"))"

run "$root/old"
[ "$rc" -eq 0 ] && ok "no current CLI is never a failure" || no "no current CLI is never a failure ($rc)"
case $out in
  "lane.sh: NOT sweeping $target: "*"too old: $root/old/exponential (exponential 0.1.0)"*"install.sh"*) ok "only an old CLI: the lane says the sweep is skipped, which binary is too old and what to install" ;;
  *) no "only an old CLI: the lane says the sweep is skipped, which binary is too old and what to install ($out)" ;;
esac
[ "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" = 1 ] && ok "one line, not the old CLI's usage text" || no "one line, not the old CLI's usage text ($out)"

run "$root/none"
case $out in
  "lane.sh: NOT sweeping $target: no \`exponential\` CLI found."*) ok "no CLI at all is said too" ;;
  *) no "no CLI at all is said too ($out)" ;;
esac

new_cli "$target/debug/exponential" "cargo-target sweep $target: freed 0.2 GB"
run "$root/old"
[ "$out" = "lane.sh: cargo-target sweep $target: freed 0.2 GB" ] && ok "the target's own build is the fallback" || no "the target's own build is the fallback ($out)"
new_cli "$target/release/exponential" "release"
run "$root/old"
[ "$out" = "lane.sh: release" ] && ok "its release build before its debug one" || no "its release build before its debug one ($out)"
run "$root/old:$root/new"
[ "$out" = "lane.sh: cargo-target sweep $target: freed 1.3 GB" ] && ok "a current CLI on PATH before the target's build" || no "a current CLI on PATH before the target's build ($out)"

new_cli "$root/pinned/exponential" "pinned"
: > "$root/calls"; out=$(LANE_SWEEP_CLI="$root/pinned/exponential" PATH="$root/new:/usr/bin:/bin" lane_sweep "$target" 2>&1)
[ "$out" = "lane.sh: pinned" ] && ok "LANE_SWEEP_CLI wins" || no "LANE_SWEEP_CLI wins ($out)"

new_cli "$root/quiet/exponential" ""
rm -rf "$target/debug" "$target/release"
run "$root/quiet"
[ "$rc" -eq 0 ] && [ -z "$out" ] && ok "disk not low: silent" || no "disk not low: silent ($out)"
[ "$(wc -l < "$root/calls" | tr -d ' ')" = 1 ] && ok "one call, no probe run" || no "one call, no probe run ($(cat "$root/calls"))"

new_cli "$root/broken/exponential" "boom" 3
run "$root/broken:$root/new"
[ "$rc" -eq 0 ] && [ "$out" = "lane.sh: sweep-target failed (exit 3) with $root/broken/exponential: boom" ] && ok "a failing sweep is shown and never fails the lane" || no "a failing sweep is shown and never fails the lane ($rc: $out)"

# Through lane.sh itself: the command still runs, the notice is on stderr.
# A private lock root, config and queue log: the machine's live lanes are
# never read or written.
mkdir -p "$root/locks"
export EXP_LANE_LOCK_ROOT="$root/locks" XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state"
unset LANE_LOG
err=$(PATH="$root/old:/usr/bin:/bin" CARGO_TARGET_DIR="$target" sh "$here/lane.sh" heavy sweep-test -- sh -c 'echo ran' 2>&1 >"$root/stdout")
[ "$(cat "$root/stdout")" = ran ] && ok "lane.sh runs the command after an unswept start" || no "lane.sh runs the command after an unswept start ($(cat "$root/stdout") / $err)"
case $err in *"NOT sweeping $target"*) ok "lane.sh shows the notice" ;; *) no "lane.sh shows the notice ($err)" ;; esac

[ "$fails" -eq 0 ] && echo "all lane sweep checks passed" || { echo "$fails lane sweep check(s) failed"; exit 1; }
