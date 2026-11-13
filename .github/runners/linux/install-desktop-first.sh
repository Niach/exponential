#!/usr/bin/env bash
# The Mint's "Desktop first" user units (.github/runners/README.md), no root:
# expbg.slice (agent runs, their builds, the mint-ci daemon: idle CPU, 20 GB
# cap), run-.scope.d (agent runs into it), the mint-ci daemon drop-in, and the
# Steam game hook (CPUWeight + exp-gaming.service = one runner while a game
# runs). Idempotent; rerun to upgrade. --uninstall removes them.
# The root half (expci.slice, systemd-oomd) is the README's sudo line.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/exponential-desktop-first"
UNITS="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

if [ "${1:-}" = "--uninstall" ]; then
  systemctl --user stop exp-gaming.service >/dev/null 2>&1 || true
  rm -f "$UNITS/expbg.slice" "$UNITS/exp-gaming.service" \
    "$UNITS/run-.scope.d/50-exp-background.conf" \
    "$UNITS/exponential-ci-daemon.service.d/50-exp-background.conf" \
    "$UNITS/app-steam-.scope.d/50-exp-game.conf"
  rm -rf "$DEST"
  systemctl --user daemon-reload
  echo "desktop-first: uninstalled (restart exponential-ci-daemon to leave expbg.slice)"
  exit 0
fi

mkdir -p "$DEST" "$UNITS/run-.scope.d" "$UNITS/exponential-ci-daemon.service.d" "$UNITS/app-steam-.scope.d"
install -m 755 "$HERE/gaming-ci.sh" "$DEST/gaming-ci.sh"
install -m 644 "$HERE/expbg.slice" "$UNITS/expbg.slice"
install -m 644 "$HERE/run-scope-background.conf" "$UNITS/run-.scope.d/50-exp-background.conf"
install -m 644 "$HERE/ci-daemon-background.conf" "$UNITS/exponential-ci-daemon.service.d/50-exp-background.conf"
install -m 644 "$HERE/app-steam-game.conf" "$UNITS/app-steam-.scope.d/50-exp-game.conf"
sed "s|@DEST@|$DEST|g" "$HERE/exp-gaming.service" >"$UNITS/exp-gaming.service"
systemctl --user daemon-reload
echo "desktop-first: installed. A running mint-ci daemon moves into expbg.slice on its next restart:"
echo "  systemctl --user restart exponential-ci-daemon   # between jobs"
