#!/usr/bin/env bash
# NEW-52: the macOS OS voice (`say`, AVSpeechSynthesizer) as a Speaker. No
# sink capture needed: `say -o` writes the synthesised clip straight to a
# file, so the wall time of the call is "request → whole utterance ready"
# (say does not stream to a file; time to first audio on the speaker is the
# same engine minus the file write). Usage:
#   scripts/os-voice-mac.sh <out-dir> [de-voice] [en-voice] < data/phrases.json
set -euo pipefail
OUT="$1"; DE="${2:-Anna}"; EN="${3:-Samantha}"; mkdir -p "$OUT"; : > "$OUT/timing.jsonl"
python3 -c 'import json,sys; [print(p["id"], p["lang"], p.get("spoken", p["ref"])) for p in json.load(sys.stdin)["phrases"]]' | while read -r id lang spoken; do
  voice="$EN"; [ "$lang" = de ] && voice="$DE"
  wav="$OUT/$id.wav"
  t0=$(python3 -c 'import time;print(int(time.time()*1000))')
  say -v "$voice" --data-format=LEI16@16000 -o "$wav" "$spoken"
  t1=$(python3 -c 'import time;print(int(time.time()*1000))')
  audio_ms=$(python3 -c 'import wave,sys;w=wave.open(sys.argv[1]);print(w.getnframes()*1000//w.getframerate())' "$wav")
  echo "{\"engine\":\"say\",\"id\":\"$id\",\"lang\":\"$lang\",\"voice\":\"$voice\",\"wall_ms\":$((t1 - t0)),\"audio_ms\":$audio_ms,\"rtf\":$(python3 -c "print(round($((t1 - t0))/max($audio_ms,1),3))"),\"out\":\"$wav\"}" >> "$OUT/timing.jsonl"
done
echo "wrote $OUT" >&2
