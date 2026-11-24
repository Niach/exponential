# etalk-wakeword

Voice's wake word with a model of its own (EXP-1424): an openWakeWord-compatible
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
- `Detector::rest` (#NEW-126): samples from a quiet room run no model (each
  frame counts as a score of 0) and the last `REST_FRAMES` are kept, so the
  next `push` scores exactly as if every frame had been pushed.

## What listening costs (#NEW-126)

The always-on cost is the whole listening path, not the classifier: the
microphone, Silero and the three networks, of which the speech embedding is
nearly all (it runs over its full 76-frame window every 80 ms). So the model
rests while the room is quiet (`etalk-engine`'s `wake.rs`: no Silero frame
at 0.1 or over for 1.28 s) and Silero alone listens.

Speech worker CPU (user + system, every thread) over 120 s of a quiet room,
`Hey spezl`, 3 runs each (the issue's own figure on the M4 Pro, the app on the
real microphone, was 5.0 s = 4.2 %):

| machine, microphone | before | after |
|---|---|---|
| Linux (i9-13900K, CUDA build), `pw-record` 16 kHz from a virtual PipeWire source | 6.4 – 7.2 s (5.3 – 6.0 % of one core) | 2.1 – 2.4 s (1.8 – 2.0 %) |
| M4 Pro (macOS 27), `wake_listen idle`: a stand-in microphone, 48 kHz in 10 ms chunks | 4.6 – 5.2 s (3.8 – 4.4 %) | 1.9 s (1.5 – 1.6 %) |

Where the Linux time goes, per second of audio on the listening thread: before
the embedding ~41 ms, the mel spectrogram ~3.5 ms, the classifier ~2.7 ms and
Silero ~9 ms (together 5.4 – 6.1 s of the 120 s); after Silero and the
bookkeeping, 1.2 – 1.3 s. The rest is the CUDA runtime's idle event thread
(0.8 – 1.1 s, Linux GPU build only) and, outside the worker, `pw-record`
(0.4 s). No `Level` frame is made while scanning (only a Listen, a turn or a
call draws them). The M4 Pro's listening thread (`sample`, before) agrees:
of its working time 77 % in the model's networks (tract), 23 % in Silero,
nothing else. With sound in the room (talk, music, a TV) the model scores
every frame as before.

Measure it without a microphone (the worker's own listening thread on a
stand-in device; `--rate 16000 --chunk-ms 64` is Linux's recorder, the
default 48 kHz in 10 ms chunks is cpal's on macOS and Windows):

```bash
cargo run --release -p etalk-engine --example wake_listen -- <data-dir> idle --seconds 120
cargo run --release -p etalk-engine --example wake_listen -- <data-dir> clips <clips-dir>   # what still wakes
```

macOS loads the speech runtime only from the running executable's bundle: put
the built example in a stand-in one (`WL.app/Contents/MacOS/wake_listen`, with
`WL.app/Contents/Frameworks/*.dylib` copied from an installed Exponential.app)
and run it from there.

`examples/wake_model.rs` times the three networks alone, fed as fast as they
decode (M4 Pro: ~11 ms per second of audio; in real time, with cold caches
between frames, they cost several times that).

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

`docs/third-party-licences.md`, "Voice wake word models (EXP-1424)". We never
ship openWakeWord's own pre-trained wake-word models (CC BY-NC-SA 4.0).
