#!/usr/bin/env bash
# The web runner kit (scripts/runner-kits.sh): $1 = the kit directory to fill.
# The kit = apps/runner/web's static export (`bun run build`: the shell,
# glyphs.json with every Lucide glyph, sw.js) + NOTICES.txt beside index.html.
# `exponential bundle --target web` replaces app/, the manifest and the icons,
# prunes glyphs.json and regenerates sw.js.
set -euo pipefail
kit="$1"
web="$(cd "$(dirname "$0")/../../apps/runner/web" && pwd)"
(cd "$web" && bun run build)
cp -R "$web/dist/." "$kit/"
notices="$web/public/NOTICES.txt"
[ -f "$notices" ] || { echo "web: $notices is missing (the notices lane writes it)" >&2; exit 1; }
cp "$notices" "$kit/NOTICES.txt"
