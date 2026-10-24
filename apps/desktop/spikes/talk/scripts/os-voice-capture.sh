#!/usr/bin/env bash
# NEW-52: the OS voice as a Speaker, measured the only honest way: speak
# through speech-dispatcher (`spd-say`, the Linux OS voice; macOS uses `say`)
# and RECORD THE OUTPUT SINK with PipeWire. The wav is the synthetic test
# clip for that phrase, and the gap between the request and the first
# non-silent sample is the time to first audio. Usage:
#   scripts/os-voice-capture.sh <monitor-source> <out-dir> < phrases.json
# To keep it off the headset: a null sink (`pactl load-module module-null-sink
# sink_name=talkspike`) + a private speech-dispatcher on it
# (`AudioPulseDevice "talkspike"`, `SPEECHD_ADDRESS=unix_socket:<sock>`), and
# <monitor-source> = talkspike (the sink; the recorder captures its monitor).
set -euo pipefail
MONITOR="$1"; OUT="$2"; mkdir -p "$OUT"
case "$(uname -s)" in
  Darwin) SPEAK() { case "$2" in de) v=Anna ;; *) v=Samantha ;; esac; say -v "$v" "$1"; } ;;
  *)      SPEAK() { spd-say -w -l "$2" "$1"; } ;;
esac
python3 -c 'import json,sys; [print(p["id"], p["lang"], p.get("spoken", p["ref"])) for p in json.load(sys.stdin)["phrases"]]' | while read -r id lang ref; do
  wav="$OUT/$id.wav"
  pw-record -P "{ stream.capture.sink=true }" --target "$MONITOR" --rate 16000 --channels 1 --format s16 "$wav" &
  rec=$!
  sleep 0.5                                  # let the recorder open the stream
  t0=$(date +%s%N)
  SPEAK "$ref" "$lang"
  t1=$(date +%s%N)
  sleep 0.3
  kill -INT $rec; wait $rec 2>/dev/null || true
  echo "{\"id\":\"$id\",\"lang\":\"$lang\",\"speak_ms\":$(( (t1 - t0) / 1000000 )),\"lead_ms\":500}" >> "$OUT/timing.jsonl"
done
echo "wrote $OUT"
