# etalk-wakeword

eTalk's wake word with a model of its own (EXP-1424): an openWakeWord-compatible
detector in pure Rust (tract), gpui-free, shared by Exponential's speech worker
and spezl.

```
16 kHz PCM ─▶ melspectrogram.onnx ─▶ embedding_model.onnx ─▶ <phrase>.onnx ─▶ Trigger
   80 ms        8 mel frames / step     1 × 96 per step         last 16 → 0…1    threshold,
   frames       (+480 samples context)  (last 76 mel frames)                     patience, debounce
```

- `FrontEnd` (mel + embedding) and `Classifier` load once and are shared;
  each stream gets a `Detector` (`push` scores every whole frame, `detect`
  returns where the first detection ended).
- `models.rs`: which phrases have a model, and every file as a pinned
  download (url, sha256, size). A model without a digest is unpublished and
  never picked. Nothing is in the binary.
- CPU: `cargo run --release -p etalk-wakeword --example wake_model [-- 120 models/hey_spezl.onnx]`
  (M4 Pro: ~11 ms per second of audio, ~1.1 % of one core, either classifier).

## Shipped models (`models/`)

| phrase | file | threshold | validation (19.2 h, synthetic positives) |
|---|---|---|---|
| Hey spezl (spezl, #YO-1) | `hey_spezl.onnx` (172 KB) | 0.18 | 0.10 false accepts/h, 83 % recall (`hey_spezl_eval.json`) |

v1 was trained on a Mac (MPS, ~3 h) with `train/hey_spezl.yaml`. Spot-checked with macOS `say`
voices: 7/7 "Hey spezl" fire (peaks 0.34–0.57), ordinary sentences peak at ≤ 0.13, but
"Hey special" fires too (0.48–0.59; an English voice says both alike). For a v2, train on a
GPU with livekit-wakeword's prod settings (25k samples, 100k steps, medium model) and
"hey special" weighted as a negative.

## In Exponential

`etalk-engine`'s wake path (`wake.rs`) hears each phrase through a
`WakeDetector`: `ModelDetector` when the phrase has a published model whose
files are ready, else `TranscriptMatcher` (Parakeet on each Silero segment's
start, unchanged). Fixture: `packages/etalk/fixtures/talk-listen.json`
`wake.detector`. Exponential's own phrases (`Hey {agent}`, typed ones) have no
model, so they keep the matcher.

## Training a phrase (`train/`)

```bash
brew install uv espeak-ng ffmpeg        # or the distro packages
crates/etalk-wakeword/train/train.sh [train/<phrase>.yaml] [workdir]
```

livekit-wakeword (Apache-2.0, pinned revision) generates Piper TTS samples of
the phrase and of near misses, augments them with noise and room impulse
responses, trains a conv-attention classifier and exports ONNX (input
`embeddings` (1, 16, 96), output `score`). The setup step downloads ~17 GB
(ACAV100M negative features, MUSAN, Piper). Copy the exported model to
`models/<phrase>.onnx`, then pin its sha256 and size in `models.rs`.

User-typed phrases are NOT trained on the device: a typed phrase without a
shipped model keeps the transcript matcher (training needs Python, ~17 GB of
data and hours of GPU/MPS time; a phone or a laptop on battery can't do that
per member).

## Licences

`docs/third-party-licences.md`, "eTalk wake word models (EXP-1424)". We never
ship openWakeWord's own pre-trained wake-word models (CC BY-NC-SA 4.0).
