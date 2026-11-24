#!/usr/bin/env bash
# EXP-1424: train a wake-word classifier for etalk-wakeword, reproducibly.
#
#   train/train.sh [config.yaml] [workdir] [stage…]
#
# Stages: env setup generate augment train export eval (default: all, in
# that order). On a shared machine (NEW-129: mint's heavy lane has a 45 min
# hold) run them a few per call, with staged.py's cuts in place of the long
# ones: generate-fast for generate, augment-clips + features for augment,
# train-phase ×3 for train.
#
# Defaults: train/hey_spezl.yaml, ~/.cache/etalk-wakeword-train (any disk
# with ~40 GB free for the 25k-sample settings: the setup step downloads livekit-wakeword's
# pre-computed ACAV100M negative features (~16 GB), MUSAN noise (~1.1 GB), MIT
# room impulse responses and the Piper VITS checkpoint (~166 MB)).
# Needs uv (0.7+), espeak-ng and ffmpeg (macOS: brew install uv espeak-ng ffmpeg).
# Trains on CUDA, Apple MPS or the CPU (livekit-wakeword picks).
#
# Output: <workdir>/output/<model_name>/<model_name>.onnx = the classifier
# (input `embeddings` (1, 16, 96), output `score`), its sha256 and size for
# `crates/etalk-wakeword/src/models.rs`, and <model_name>_eval.json (the
# threshold to ship = `optimal_threshold`).
set -euo pipefail

# livekit-wakeword at a pinned revision (Apache-2.0).
LIVEKIT_WAKEWORD_REV=cdb82b8ebc179b216e1c5bf360dbdc1ccd5288f2

here="$(cd "$(dirname "$0")" && pwd)"
config="$(cd "$(dirname "${1:-$here/hey_spezl.yaml}")" && pwd)/$(basename "${1:-$here/hey_spezl.yaml}")"
work="${2:-$HOME/.cache/etalk-wakeword-train}"
shift $(($# < 2 ? $# : 2))
[ $# -gt 0 ] || set -- env setup generate augment train export eval
mkdir -p "$work"
cd "$work"

py="$work/livekit-wakeword/.venv/bin/python"
lkww() { (cd "$work" && "$py" -m livekit.wakeword "$@"); }
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi; }

for stage in "$@"; do
  echo "== $stage: $(date -u +%FT%TZ)"
  case "$stage" in
    env)
      if [ ! -d livekit-wakeword ]; then
        git clone https://github.com/livekit/livekit-wakeword livekit-wakeword
      fi
      git -C livekit-wakeword fetch --quiet origin "$LIVEKIT_WAKEWORD_REV" || true
      git -C livekit-wakeword checkout --quiet "$LIVEKIT_WAKEWORD_REV"
      # Python 3.11 + the train, export and eval extras, isolated in the workdir.
      (cd livekit-wakeword && uv sync --python 3.11 --extra train --extra export --extra eval)
      ;;
    setup)
      # The config livekit-wakeword reads: `negative_weights` (ours) becomes
      # repeated `custom_negative_phrases` lines.
      "$py" - "$config" "$work/config.yaml" <<'PY'
import sys, yaml
config = yaml.safe_load(open(sys.argv[1]))
weights = config.pop("negative_weights", None) or {}
phrases = config.get("custom_negative_phrases") or []
for phrase in weights:
    assert phrase in phrases, f"negative_weights: {phrase!r} is not in custom_negative_phrases"
config["custom_negative_phrases"] = [phrase for phrase in phrases for _ in range(int(weights.get(phrase, 1)))]
yaml.safe_dump(config, open(sys.argv[2], "w"), sort_keys=False, allow_unicode=True)
PY
      lkww setup --config "$work/config.yaml"
      ;;
    generate | augment | train | export | eval)
      lkww "$stage" "$work/config.yaml"
      ;;
    generate-fast | augment-clips | features | train-phase)
      (cd "$work" && "$py" "$here/staged.py" "$work/config.yaml" "$stage")
      ;;
    *)
      echo "unknown stage: $stage" >&2
      exit 2
      ;;
  esac
done

name="$(sed -n 's/^model_name: *//p' "$work/config.yaml")"
model="$work/output/$name/$name.onnx"
if [ -f "$model" ]; then
  echo "model: $model ($(wc -c < "$model" | tr -d ' ') bytes)"
  sha256 "$model"
fi
