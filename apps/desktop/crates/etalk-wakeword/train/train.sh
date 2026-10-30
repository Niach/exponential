#!/usr/bin/env bash
# EXP-1424: train a wake-word classifier for etalk-wakeword, reproducibly.
#
#   train/train.sh [config.yaml] [workdir]
#
# Defaults: train/hey_spezl.yaml, ~/Library/Caches/etalk-wakeword-train (any
# disk with ~20 GB free: the setup step downloads livekit-wakeword's
# pre-computed ACAV100M negative features (~16 GB), MUSAN noise (~1.1 GB), MIT
# room impulse responses and the Piper VITS checkpoint (~166 MB)).
# Needs uv, espeak-ng and ffmpeg (macOS: brew install uv espeak-ng ffmpeg).
# Trains on CUDA, Apple MPS or the CPU (livekit-wakeword picks).
#
# Output: <workdir>/output/<model_name>/<model_name>.onnx = the classifier
# (input `embeddings` (1, 16, 96), output `score`), plus its sha256 for
# `crates/etalk-wakeword/src/models.rs`.
set -euo pipefail

# livekit-wakeword at a pinned revision (Apache-2.0).
LIVEKIT_WAKEWORD_REV=cdb82b8ebc179b216e1c5bf360dbdc1ccd5288f2

here="$(cd "$(dirname "$0")" && pwd)"
config="$(cd "$(dirname "${1:-$here/hey_spezl.yaml}")" && pwd)/$(basename "${1:-$here/hey_spezl.yaml}")"
work="${2:-$HOME/Library/Caches/etalk-wakeword-train}"
mkdir -p "$work"
cd "$work"

if [ ! -d livekit-wakeword ]; then
  git clone https://github.com/livekit/livekit-wakeword livekit-wakeword
fi
git -C livekit-wakeword fetch --quiet origin "$LIVEKIT_WAKEWORD_REV" || true
git -C livekit-wakeword checkout --quiet "$LIVEKIT_WAKEWORD_REV"

# Python 3.11 + the train and export extras, isolated in the workdir.
(cd livekit-wakeword && uv sync --python 3.11 --extra train --extra export --extra eval)
lkww() { (cd "$work" && uv run --project "$work/livekit-wakeword" livekit-wakeword "$@"); }

cp "$config" "$work/config.yaml"
lkww setup --config "$work/config.yaml"
lkww run "$work/config.yaml"

name="$(sed -n 's/^model_name: *//p' "$work/config.yaml")"
model="$(find "$work/output" -name "$name.onnx" | head -1)"
echo "model: $model"
shasum -a 256 "$model"
