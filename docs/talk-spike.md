# exponentialTalk spike (NEW-52), 2026-10-24

Slice (a) of #NEW-9 plan v2: the engines behind the planned `crates/talk`
traits, measured on mint (Linux Mint 22, i9 with 32 threads, RTX 4090 24 GB,
PipeWire, SteelSeries Arctis Nova 7). The harness is
`apps/desktop/spikes/talk/` (its README has every command); the folded
numbers are `apps/desktop/spikes/talk/numbers.json`, the input of the
`/talk` page's Numbers section (NEW-60). Not measured yet: the macbook (a
child session runs the same harness), the spec author's notebook, a human voice
(`scripts/record.sh`), a running game beside the engines.

## Test material

Twenty phrases, ten German and ten English, with the identifiers and command
words the dispatcher must hear (`data/phrases.json`). No human recordings yet:
the clips are synthetic, spoken by Kokoro v1.0 (English) and Qwen3-TTS 1.7B
(German and English) from the phrases' **spoken** forms ("E X P twelve forty
two", not "EXP-1242"). WER is scored against that spoken form, word by word,
so an engine that normalises the identifier to `exp1242` is charged four
errors for one correct token. Read the per-clip transcripts, not only the
percentages.

## Verdicts

| trait | verdict | why |
|---|---|---|
| `Transcriber` | **Nemotron 3.5 ASR streaming 0.6B** where a GPU exists (through a ggml runtime); **Whistle** as the CPU path for commands; the English streaming Zipformer only as a fallback | Nemotron is the only one of the three that hears German (Zipformer has no German model at all) and it keeps content words nearly perfect on both languages; Whistle is ten times cheaper and normalises identifiers the way the dispatcher wants, but mishears content words |
| `Vad` | **Silero through sherpa-onnx** | 15 ms of compute per clip, end-of-utterance lag = exactly the `min_silence_duration` knob (500 ms at 0.5 s, 300 ms at 0.3 s) |
| `WakeWord` | **transcript match** for "hey <agent>"; the sherpa keyword spotter for short idle phrases only | the 3 M spotter fires on `HEY` and `STOP` from a text keyword with no training, but never on the five-syllable `EXPONENTIAL`, boosted or not; a long brand word needs a trained model (openWakeWord) or a shorter idle phrase |
| `Speaker` | **Qwen3-TTS** on GPU (German and English in one model, 2x faster than real time at 1.7B), **Kokoro** on CPU for English (time to first audio ~0.5 s, RTF 0.19); the Linux OS voice is not a fallback | the owner heard espeak-ng through the headset and judged it unusable on first listen; macOS and Windows OS voices remain to be judged on the macbook and the Windows device |
| channels | **one PipeWire capture per source** works | two sinks recorded at once into two aligned 16 kHz clips; the same call on an app's node is the per-app channel |
| proposer cards | the interception point exists; a per-session permission posture is the one engine change | see "The card path" |

## ASR

Engines: Nemotron 3.5 ASR streaming 0.6B Q8_0 GGUF (handy-computer, licence
OpenMDW-1.1) in transcribe.cpp (MIT, ggml, CUDA backend); Whistle 16.9 MB
(Cactus, Apache-2.0) through cactus-rs 0.2.1 on the CPU; the streaming
Zipformer `en-2023-06-26` (Apache-2.0) through sherpa-onnx 2.0 on 4 CPU
threads. Each clip was fed in 100 ms chunks; "tail" is the compute after the
last sample (what the person waits for), "first partial" how much audio had
been heard before the first word appeared.

| engine | clips | WER | first partial | tail after end of speech | compute / audio |
|---|---|---|---|---|---|
| Nemotron, CUDA | Kokoro en | **11.5 %** | n/a (offline run) | 74–126 ms per whole clip cold, 33 ms warm | 0.03–0.07 cold, 0.02 warm |
| Nemotron, CUDA | Qwen en | 13.8 % | | | |
| Nemotron, CUDA | **Qwen de** | **20.7 %** | | | |
| Nemotron, CPU backend | Kokoro en-04 | | | 114 ms for 1.6 s | 0.07 |
| Whistle, CPU | Kokoro en | 24.1 % | clip only | 55–65 ms median, 441 ms max | **0.02** |
| Whistle, CPU | Qwen en | 35.6 % | | | |
| Whistle, CPU | Qwen de | 32.6 % | | | |
| Zipformer streaming, CPU | Kokoro en | 35.6 % | 800 ms of audio | 27 ms median, 63 ms max | 0.05 |
| Zipformer streaming, CPU | Qwen en | 20.7 % | 1200 ms of audio | 26 ms median | 0.05 |

What the errors are: Nemotron's German misses are almost all identifiers
("E X P dreizehn" → "ex preizeh", "done" → "don"); every command word and
every German content word came through. Whistle writes `exp1242`, `new54`,
`reject 1`: exactly the form the dispatcher wants, charged as errors here,
but it also turns "take two but set" into "take tuba s said" and German
"zeig mir den Diff" into English. The Zipformer drops the identifiers
entirely ("ON ACTS", "UNNEW").

Nemotron's custom-vocabulary biasing is not available for this backend in
transcribe.cpp ("model parakeet does not support vocabulary"), so the
identifier recovery stays with the Assistant agent, which is where plan v2
put it. The soniqo ONNX FP16 export run through onnxruntime 1.31 managed only
284 ms per 320 ms chunk on CUDA with dummy caches (an unoptimised probe): the
ggml runtime is the measured path, and `crates/voice` should link it (or
CrispASR, the same family) rather than the ONNX export.

Memory: Nemotron Q8 held **1,142 MiB of VRAM** (nvidia-smi); the ONNX FP16
encoder is 1.24 GB on disk; Whistle is 16.9 MB and loaded in 28–51 ms warm,
1.1 s cold.

## VAD, wake word

Silero (MIT) through sherpa-onnx: 10 of 10 segments found on both synthetic
sets, 8–23 ms of compute per clip on one thread, and the end-of-utterance lag
is the configured silence exactly: 500 ms at `min_silence_duration = 0.5`,
300 ms at 0.3. The knob is the product decision; the detector adds nothing.

Keyword spotting (sherpa-onnx `kws-zipformer-zh-en-3M`, Apache-2.0, keywords
as phone strings from the bundled `en.phone` lexicon, no training): `HEY` and
`STOP` fire on the clips that contain them (Kokoro en-01, Qwen en-01 and
de-01) at 0.005–0.012 of real time; `EXPONENTIAL` never fired on any clip at
thresholds 0.25, 0.1 and 0.05, boosted 2.5x or not, nor its prefix `EXPO`. The
3 M model is tuned for short words. For "Exponential" as the idle phrase the
options are a trained openWakeWord model (the 4090 can train one) or a
different short idle phrase; "hey <agent name>" as a transcript match while
STT runs needs nothing (both Nemotron and Whistle wrote "hey exponential").
False-accept rate per hour is not measured yet. The planned 1 h recording
(game audio, desk noise, speech) uses only the recording person's own voice
and voices with explicit consent (§201 StGB, as the spec notes), and stays
in the gitignored `out/`; nothing of it is committed.

## TTS

| engine | where | time to first audio | RTF | memory | German |
|---|---|---|---|---|---|
| Kokoro v1.0 (Apache-2.0) via sherpa-onnx 2.0, lexicon, no espeak-ng | CPU, 4 threads | **235–740 ms** (median 528; whole utterance, the callback delivers one chunk) | 0.19–0.24 | 310 MB model | **no voice** |
| Qwen3-TTS 1.7B Base Q4_K_M + mmproj Q8_0 (ggml-org) via llama.cpp `llama-tts` | RTX 4090 (Vulkan) | not streaming: whole utterance in 0.29–1.51 s | **0.49 median** (2x faster than real time) | 5.6–6.6 GB VRAM on Vulkan, 2.0 GB RSS | **yes**, all 10 German clips |
| same, first call without `-dev` | CPU | 10.4 s for 6 s | 1.7 (slower than real time) | | |
| **Qwen3-TTS 0.6B CustomVoice Q8_0** (cstr GGUF, Apache-2.0) + 12 Hz codec Q8_0 via CrispASR (MIT, ggml) | RTX 4090 (CUDA) | per sentence: the first PCM bytes of an 8 s German utterance arrived 2.2 s after launch, ~1.2 s of that is model load | **0.13–0.17** (6–7x faster than real time: 8.3 s of English in 1.37 s, 8.0 s of German in 1.06 s) | **2,250–2,300 MiB VRAM** | **yes** |
| speech-dispatcher + espeak-ng (the Linux OS voice) | CPU | not captured | | | yes |

The 0.6B CustomVoice model is the one that fits the spec's 4 GB budget: 2.3 GB of
VRAM on CUDA beside Nemotron's 1.1 GB keeps the pair under 4 GB, German and
English in one model, seven times faster than real time. The 1.7B on the
Vulkan build takes 5.6–6.6 GB. Neither runtime streams below sentence
granularity, so the time to first audio of a spoken reply is the first
sentence's generation time (0.3–0.5 s for a short sentence at 0.6B) once the
model is resident; a resident speech worker pays the ~1.2–1.5 s load once.
Kokoro's number words are not spoken at all ("1369" is skipped as
out-of-vocabulary), so the spoken forms matter for it too.

The Linux OS voice was the only engine that played through the headset
(the capture went to the wrong sink); the owner's verdict on first listen was
that it is not acceptable. Decision input for NEW-53: on Linux the
"OS voices as the CPU fallback" answer means espeak-ng, which is not
acceptable; Kokoro (English) is the CPU fallback there, and German on a
CPU-only Linux box has no good voice yet. macOS (`AVSpeechSynthesizer`) and
Windows (WinRT) voices are untested.

## Channels

Two PipeWire sinks recorded at the same time with
`pw-record -P '{ stream.capture.sink=true }' --target <sink>` (the flag that
captures a sink's monitor; without it the recorder returns silence, which
cost one round of this spike) gave two 16 kHz mono clips of 3.86 s with
onsets at 609 ms and 1,460 ms for a play offset of 800 ms: aligned within
50 ms by wall clock, ready for the per-channel ASR and the merge by word
timestamps. Per-app capture on Linux is the same call on the app's output
node.

## The card path

The spec's "Haiku proposes, the local executor confirms" maps onto code that
exists:

- The Claude adapter (`apps/desktop/crates/engine/src/adapters/claude.rs`)
  always receives `can_use_tool` control requests (`--permission-prompt-tool
  stdio` is mandatory) and answers them in `answer_can_use_tool`: a
  confinement denial, a question card for `AskUserQuestion`, and otherwise
  `permission_allow`, because every run is launched with
  `--permission-mode bypassPermissions` (EXP-772) and `--dangerously-skip-permissions`
  (`coding/src/argv.rs`).
- The engine host (`engine/src/host.rs`) already turns a
  `session/request_permission` into a parked ask through
  `mapper.on_permission` (`mapper.rs`), which is THE Dock question card ×4
  (`ask-dock.json`).

So the proposal card needs no new UI and no new wire: a session started by
the voice loop runs with a permission posture of `default` plus
`allowed_tools` for the `auto` policy and `disallowed_tools` for `deny`, and
`answer_can_use_tool` routes `confirm` / `confirm-strong` tools into the
existing ask instead of `permission_allow`. The policy table lives on the
launcher side, never in the model's prompt. Not done in the spike: the one
live run (an Assistant session with that posture, one intercepted call shown
in the Dock). That is the first task of NEW-54.

## Licences touched

sherpa-onnx 2.0 Rust crate (Apache-2.0; its 2.0.0 prebuilt dropped espeak-ng),
ONNX Runtime (MIT), Silero VAD (MIT), the Zipformer and KWS models
(Apache-2.0), Kokoro weights (Apache-2.0; the bundle still ships an
`espeak-ng-data` directory that is NOT used with the lexicon and must not be
redistributed), Whistle and cactus-rs (Apache-2.0 / MIT), Nemotron 3.5
(**OpenMDW-1.1**, NVIDIA's open model licence, commercial use allowed; to be
read before shipping), transcribe.cpp (MIT), Qwen3-TTS weights (Apache-2.0),
llama.cpp (MIT), CrispASR (MIT). Excluded as planned: espeak-ng as a linked
library, Piper, Kitten TTS, Supertonic (OpenRAIL-M).

## What this changes in the slices

- NEW-53 speak replies: the CPU fallback on Linux is Kokoro for English, not
  the OS voice; German without a GPU stays open; the `Speaker` runs in the
  resident `speech-worker` so a model loads once.
- NEW-54 push-to-talk: `Transcriber` = Nemotron (ggml) on GPU machines,
  Whistle on CPU; both wrote "hey exponential" and `reject one` reliably;
  identifiers go to the Assistant as heard.
- NEW-56 wake word: the idle spotter phrase cannot be "Exponential" with the
  stock 3 M model; train one on the 4090 or pick a short idle phrase; the
  transcript match for "hey <agent>" is free.
- NEW-57 meeting mode: PipeWire per-source capture is confirmed; the merge by
  timestamps is harness work, not research.
- Open: macbook numbers (child session), Qwen3-TTS 0.6B and CUDA VRAM, the
  Linux German CPU voice, a human recording set, false accepts per hour (own and consenting voices
  only, kept out of git), a
  running game beside the engines.
