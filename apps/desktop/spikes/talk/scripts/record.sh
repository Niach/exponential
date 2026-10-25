#!/usr/bin/env bash
# NEW-52: record the 20 phrases with a real voice. Prompts each phrase, records
# from the default microphone until Enter, writes out/human-<name>/<id>.wav at
# 16 kHz mono so the same harness commands score it. Usage:
#   scripts/record.sh <name> < data/phrases.json
set -euo pipefail
NAME="$1"; OUT="out/human-$NAME"; mkdir -p "$OUT"
python3 -c 'import json,sys; [print(p["id"], "|", p.get("spoken", p["ref"])) for p in json.load(sys.stdin)["phrases"]]' | while IFS='|' read -r id text; do
  id=$(echo "$id" | tr -d ' ')
  echo; echo ">>> $id: $text"; read -r -p "Enter to start, Enter again to stop" </dev/tty
  pw-record --rate 16000 --channels 1 --format s16 "$OUT/$id.wav" & rec=$!
  read -r </dev/tty; kill -INT $rec; wait $rec 2>/dev/null || true
done
echo "wrote $OUT"
