# exponentialTalk spike (NEW-52), 2026-10-24

Slice (a) of #NEW-9 plan v2: the engines behind the planned `crates/talk`
traits, measured on mint (Linux Mint 22, i9 with 32 threads, RTX 4090 24 GB,
PipeWire, SteelSeries Arctis Nova 7). The harness is
`apps/desktop/spikes/talk/` (its README has every command); the folded
numbers are `apps/desktop/spikes/talk/numbers.json`, the input of the
`/talk` page's Numbers section (NEW-60). The macbook has its own section
below. Not measured yet: the spec author's notebook, a human voice
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
| `Speaker` | **Qwen3-TTS** on GPU (German and English in one model, 2x faster than real time at 1.7B), **Kokoro** on CPU for English (time to first audio ~0.5 s, RTF 0.19); the Linux OS voice is not a fallback | the owner heard espeak-ng through the headset and judged it unusable on first listen; the macOS compact voices are measured in the macbook section (not yet heard), Windows voices are untested |
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
False accepts are measured on corpus audio in "Wake word false accepts"
below (NEW-56). The planned 1 h recording
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
CPU-only Linux box has no good voice yet. macOS (`say`) is measured in the macbook
section below, not yet heard; Windows (WinRT) voices are untested.

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

**Built in #NEW-79.** `sessions.brief` answers `voicePolicy` for exactly the
member's root conversation with their voice agent; that launch runs
`--permission-mode default` with flag settings that `ask` for the policed
tools (so a user `allow` rule never skips `can_use_tool`). `allowedTools`
could not express the policy (the Exponential tools are 16 nouns with an
`op`), so `answer_can_use_tool` classifies by tool + `input.op` from
`packages/domain-contract/fixtures/voice-policy.json` (replayed by
`domain::voice_policy`). A proposal card is an elicitation, not a
`request_permission`: a permission card only takes its own option labels,
and "two, but priority high" needs a typed change, which the elicitation's
folded free-text row carries (no new UI). Expired or replaced cards are
withdrawn through a `_meta` key the host turns into a dismissal. Push to talk
matches confirmations on the finalized transcript before sending, and Speak
replies reads a waiting card's question aloud.

**Retired 2026-10-29.** The policy stopped every tool call of the voice
conversation at an Allow / Deny card, which made the agent unusable from a
phone. Every session, the voice conversation included, now runs with
permissions bypassed again (`--dangerously-skip-permissions`, `can_use_tool`
auto-allowed); `fixtures/voice-policy.json`, `domain::voice_policy`, the
launcher's ask layer, the push-to-talk card answers and the two examples
below are gone. `sessions.brief` still answers `voicePolicy` as screen
memory's precondition. The notes below are history.

Live run on mint (2026-10-27, `cargo run -p engine --example new79_card_path
-- haiku 5`): the real claude 2.1.310 under the voice posture, a warm
conversation, then "Run the shell command `touch …` now." The Bash call was
intercepted every time and the engine's mapper published the Dock card
(confirm-strong, read-back tail and transcript snippet) after 1351, 2819,
1536, 1380 and 1548 ms: median 1.54 s, 4 of 5 under the 2.5 s target, the
slow one a model round trip. The card was denied and the file never
appeared.

**#1264 review (2026-10-27).** A change pre-approves only `{tool}#{op}|{target}`
of a `confirm` card, once, at that tier or lower: a confirm-strong call or a
call without a target always asks again. A typed "no" / "don't …" is a deny,
only "yes, but …" is a change. `mcp__codemode__exec` is denied (a script
reaches every op past the tiers). The flag layer asks for Skill, Agent, Task
and every team server, and sets `disableBypassPermissionsMode`. Subagents:
claude 2.1.310 already refuses a subagent's `permissionMode:
bypassPermissions` when the parent is not in bypass ("not authorized by this
session … keeping parent mode"), but it does let one widen to `acceptEdits`,
so Agent / Task are now `confirm`. A codex voice agent is refused at launch,
only the member answers the cards (never a session over the bus), and a
spoken yes never answers a card past its 15-minute expiry.

**#NEW-90 live checks (2026-10-27, mint, `claude --version` = 2.1.310).**
`cargo run -p engine --example new90_policy_checks -- haiku` drives the
real CLI through the launcher's voice launch path: the confined voice flag
layer from `coding::launcher::claude_flag_settings`, the claude adapter under
`voice_policy`, and a scratch cwd that the launcher's trust seeding marks
trusted. The scratch dir is hostile on purpose. Its project settings set
`defaultMode: bypassPermissions`, `allow` Bash, Agent, Task, KillShell,
CronCreate and the plugin's tools, run a PreToolUse hook that answers
`allow` for every tool, and enable a plugin (local marketplace) that bundles
a stdio MCP server. It also defines a subagent with `permissionMode:
bypassPermissions`. Every card is denied, except that the Agent card is
allowed, so the subagent really runs. The run used the member's own login.
Nothing reached a real instance: the Exponential server URL is a closed
port.

1. **Subagent bypass: holds.** The Agent card is allowed and the subagent
   runs. Its Bash `touch` still reaches `can_use_tool` as a confirm-strong
   card, which is denied, and the marker never appears.
   `disableBypassPermissionsMode` plus the `default` launch keep the
   subagent's `permissionMode` from widening.
2. **Hook precedence: holds.** The PreToolUse hook ran (its log shows Bash)
   and answered `allow`, and the project rule allows Bash too. The forced
   `ask` still won: the call got a card, was denied, and wrote no marker.
3. **Plugin MCP servers: hold.** The control run is the bare CLI in the same
   dir without `--strict-mcp-config`. There the plugin's server connects
   (`mcp__plugin_probe_probe__mark`), next to the account's claude.ai
   connectors. With `--strict-mcp-config` the init frame lists no MCP server
   at all, even though the plugin itself still loads: the model searched for
   the tool and found nothing.
4. **Deny-by-default: fixed.** An `ask: ["*"]` rule makes 2.1.310 ask for
   EVERY tool, built-ins and MCP tools alike (`mcp__*` matches nothing).
   Before this change, any tool missing from the named ask list (KillShell,
   CronCreate, a future built-in) skipped `can_use_tool` whenever a user
   `allow` rule named it. The voice layer now asks for `*` first and keeps
   the named rules as a fallback. The policy allows the read-only tools at
   once and cards everything else (`unknown` = confirm, `otherMcp` =
   confirm-strong). The example's fourth check proves it.
   `NEW90_NO_STAR=1` drops `*` to show the gap it closes.

Decided: starting a subagent from voice stays `confirm` (one card names the
subagent and its prompt), and the subagent's own calls are carded as usual.
Offline, `launcher::tests::the_voice_conversation_asks_for_every_policed_tool`
pins the layer (`*` first, bypass disabled), and the voice-policy fixture
replays the logic. Only the live example exercises the CLI's own precedence.

**Logic (#NEW-90).** A change now needs a yes: "but …" or "and …" with no
yes or allow word is `other`, so nothing is pre-approved. A pre-approved
redo is bound to the answer. Every field that differs from the changed call
(added, changed or dropped) must have been said: a new value word for word,
or a dropped field by its key. "Yes, but priority urgent" lets `priority:
urgent` run once. The same redo with a new title, another status or
`priority: low` asks again.

## Licences touched

sherpa-onnx 2.0 Rust crate (Apache-2.0; its 2.0.0 prebuilt dropped espeak-ng),
ONNX Runtime (MIT), Silero VAD (MIT), the Zipformer and KWS models
(Apache-2.0), Kokoro weights (Apache-2.0; the bundle still ships an
`espeak-ng-data` directory that is NOT used with the lexicon and must not be
redistributed), Whistle and cactus-rs (Apache-2.0 / MIT), Nemotron 3.5
(**OpenMDW-1.1**, NVIDIA's open model licence, commercial use allowed; to be
read before shipping), transcribe.cpp (MIT), Qwen3-TTS weights (Apache-2.0),
llama.cpp (MIT), CrispASR (MIT). Excluded as planned: espeak-ng as a linked
library, Piper, Kitten TTS, Supertonic (OpenRAIL-M). Measuring data only
(NEW-56, never committed or shipped): LibriSpeech and Multilingual LibriSpeech
(CC BY 4.0, attribution: Panayotov et al. 2015, Pratap et al. 2020), Kenney
and OpenGameArt audio (CC0).

## Decision after listening (2026-10-25)

The owner heard the candidates on the macbook (macOS Anna and Samantha, Kokoro
English, Qwen3-TTS 1.7B German on Metal) after hearing espeak-ng on mint:
Kokoro sounded best, and both OS voices (espeak-ng on Linux, the macOS compact
voices) are not acceptable. So the `Speaker` plan changes:

- **OS voices are dropped as the fallback** on every platform, Windows
  included until someone hears WinRT and says otherwise.
- **Kokoro is the CPU voice** (English, ~0.5 s to first audio, RTF ~0.2,
  310 MB model downloaded on first use, Apache-2.0, through sherpa-onnx 2.0
  with the lexicon, no espeak-ng).
- **Qwen3-TTS stays the GPU voice** and the only German voice: 0.6B on CUDA
  (2.3 GB VRAM), 1.7B on Metal (~7 GB unified memory, slower than measured
  in the spike on a busy Mac).
- **German without a usable GPU has no voice**: replies are shown, not spoken,
  until a German neural CPU voice that passes the ear test turns up. Kokoro
  never reads German text with an English voice.

**Scope after listening: English only first.** The first releases speak and
listen in English. Kokoro is then the one voice on every platform, CPU only,
and Qwen3-TTS is not needed for v1 (it returns with German). For listening, the
English-only choice reopens the streaming Zipformer (first word after 0.8 s of
audio, 27 ms tail, CPU, Apache-2.0) beside Nemotron (GPU) and Whistle (CPU);
the human recording set decides between them in #NEW-54.

## What this changes in the slices

- NEW-53 speak replies: the CPU voice is Kokoro (English) on all three
  OSes, no OS voice anywhere; German without a GPU stays open; the `Speaker` runs in the
  resident `speech-worker` so a model loads once.
- NEW-54 push-to-talk: `Transcriber` = Nemotron (ggml) on GPU machines,
  Whistle on CPU; both wrote "hey exponential" and `reject one` reliably;
  identifiers go to the Assistant as heard.
- NEW-56 wake word: the idle spotter phrase cannot be "Exponential" with the
  stock 3 M model; train one on the 4090 or pick a short idle phrase; the
  transcript match for "hey <agent>" is free.
- NEW-57 meeting mode: PipeWire per-source capture is confirmed; the merge by
  timestamps is harness work, not research.
- Open: Qwen3-TTS 0.6B on Metal, the
  Linux German CPU voice, a human recording set (own and consenting voices
  only, kept out of git; also the wake word's final check), a
  running game beside the engines.

## Speech in through CrispASR (NEW-78, 2026-10-26)

Push to talk's transcriber as it ships in the speech worker: Silero VAD v6.2
(MIT, 0.88 MB) and Parakeet TDT 0.6B v3 Q8_0 GGUF (CC-BY-4.0, 674 MB), both
through CrispASR 0.8.45, sha256-pinned and verified before ggml reads them.
Measured on mint (RTX 4090, i9) with `cargo run --release -p voice --example
listen -- <data-dir> <en|de> [--cpu] <clip.wav>...`, which runs every clip
through the worker's own pipeline (`crates/voice/src/listen.rs`: 32 ms
frames, Silero segments closed after 300 ms of silence, each finished segment
transcribed at once, partials every 300 ms on the GPU only) in 20 ms chunks.
"Tail" = compute after release, what the person waits for; WER is scored
against the spoken form as above.

| device | clips | WER | tail after release | partials | load |
|---|---|---|---|---|---|
| CUDA | Kokoro en (10) | 14.9 % | **10–41 ms** | every 300 ms (1–13 per clip) | 0.72 s |
| CPU (8 threads) | Kokoro en (10) | **12.6 %** | 102–450 ms | none | 0.48 s |
| CUDA, CPU | our Qwen built-in clips, en + de (9 s each) | 0 errors | 0 ms (the last segment closed in the trailing silence) | | |

The spike's offline measurement of the same model (the `parakeet`
subcommand, sherpa-onnx int8, and CrispASR's own CLI) found CUDA 10–30 ms per
clip of up to 4 s, 11.5 % WER with no content errors, 1,246 MiB of VRAM, and
the CPU at 110–540 ms per segment and 14.9 %. Through the pipeline the errors
are the same kind: identifiers as words ("New 54" for "NEW fifty four",
"Except all" for "Accept all"), and one content word on both devices
("done" → "Dunn", a two-word clip with no context). German has only the
built-in clip so far: word for word on both devices.

One CrispASR finding: `whisper_vad_detect_speech_continue` on a FRESH VAD
context runs Silero's recurrent state from uncleared memory and scored the
first 2.7 s of speech as silence; the worker's first call per Listen is the
resetting `whisper_vad_detect_speech` (`crispasr::VadStream`).

Downloads: the transcriber needs the runtime (the CUDA build, 781 MB with
NVIDIA's wheels; on a machine without a GPU driver, NEW-78 pins the CPU-only
build of the same release: 22.7 MB on Linux, 58.4 MB on Windows) plus the two
models, fetched on the first Listen.

## Wake word false accepts (NEW-56, 2026-10-27)

The wake word as it ships (#1263: Silero segments, the first 1.2-2 s of each
segment through the resident Parakeet once, the phrase matched within
floor(letters x 0.25) edits) over **8.5 h of mixed audio per language**,
measured on mint (RTX 4090) with the speech model on the GPU and **every GPU
step run with the heavy lane held** (`scripts/lanes/lane.sh heavy`), one
measurement at a time. Harness: `cargo run --release -p voice --example wake
-- <data-dir> --phrase P --language L --checks-out F <corpus> <positives>`;
corpus and positives: `apps/desktop/spikes/talk/scripts/wake_corpus.py`;
numbers: `apps/desktop/spikes/talk/wake-numbers.json`.

**Corpus (public and CC0 only, nothing of it committed).** English:
LibriSpeech test-clean + test-other (CC BY 4.0), 3,022 utterances, 5.6 h of
speech. German: Multilingual LibriSpeech German dev (CC BY 4.0), 1,694
utterances, 6.5 h of speech. Common Voice needs a Mozilla Data Collective
login, which an unattended session does not have, so MLS took its place.
Mixed into 17 x 30 min per language, seeded, scene by scene: 80 % speech scenes
(a quiet room, CC0 game music 5-20 dB under the speech, or music plus game
effects), 20 % game-only scenes, a -65 dBFS noise floor under everything.
Background: Kenney's seven audio packs and 22 OpenGameArt music pages, all CC0.
No private clips, no scraped video.

**Positives.** Qwen3-TTS 0.6B CustomVoice (Apache-2.0), all 9 built-in
speakers, 3 texts per phrase and language (the phrase alone, + a question,
+ a command), 27 clips each.

| language | phrase | edits | checks (8.5 h) | false accepts | per 8 h | near misses | true accepts |
|---|---|---|---|---|---|---|---|
| en | Hey Triage | 2 | 5,592 | **0** | 0 | 0 | 6 / 27 |
| en | Hey Ops | 1 | 5,592 | **0** | 0 | 0 | 18 / 27 |
| en | Hey Studio | 2 | 5,592 | **0** | 0 | 0 | 25 / 27 |
| en | Hey Assistant | 3 | 5,592 | **0** | 0 | 2 | 26 / 27 |
| de | Hey Triage | 2 | 5,563 | **0** | 0 | 0 | 15 / 27 |
| de | Hey Ops | 1 | 5,563 | **0** | 0 | 0 | 21 / 27 |
| de | Hey Studio | 2 | 5,563 | **0** | 0 | 0 | 26 / 27 |
| de | Hey Assistant | 3 | 5,563 | **0** | 0 | 0 | 27 / 27 |

"Near misses" = checks one edit past the tolerance: only two in 44,620
(`He assisted at it, but`, `He was sitting there with his head down` for Hey
Assistant). 10.9 checks a minute, 0.5 % of a core on the GPU path.

**Verdict: the < 1 per 8 h target is met on corpus audio**: zero false
accepts in 17 h per phrase. Statistically, zero in 17 h bounds the rate at
1.4 per 8 h (95 %, rule of three), so this is "no false accept seen", not a
proof. Read-aloud books also almost never open a segment with "hey" (three segments
in 17 h: `Hey D.`, `Hey fever.`, `Hey, ihr da auf dem Kastell.`), which is
exactly where a wake phrase is at risk. **A human
recording (own voice, desk and game chat) stays the final check.**

**True accepts are the weak side, and mostly the voices'.** The misses are
the TTS saying the name wrong (`Hey Tree Edge`, `Patrios`, `Hey Chiyashi`,
dialect and Korean-accented speakers above all) or Parakeet fusing the
greeting (`Hatrige`, `Hatred`). Hey Studio and Hey Assistant are near
perfect; "Triage" is a hard word for these voices.

**Tuning: none.** `wake_sweep.py` replays the match over the logged
transcripts at other tolerances. At 1/3 of the letters, true accepts rise
(en Triage 6 → 17, Ops 18 → 22, Studio 25 → 27; de Triage 15 → 20, Ops 21 → 23)
and the corpus still shows zero false accepts for those three, but Hey
Assistant takes the two audiobook near misses (1.9 per 8 h). A cap at 3 edits
would avoid that, but 2 edits for `Hey Ops` take `Hey boss`, `Hey Oma`, `Hey
los`, `Hey, oh`, and 3 for `Hey Triage` take `Hey Tracy`, `Hey, tricky`:
the everyday openers the corpus cannot see. So maxEditRatio stays 0.25, and
talk-listen.json gains the corpus transcripts as `match` cases (the voices'
`Hey Triad`, `Hey Triash, was steht heute an?`, `Hey, oops`, `Hey Ups`,
`Hey Assistent`) plus the near misses a looser match would take, so the
margin is fixture-locked.

## macbook

The same harness on the owner's macbook (Apple M4 Pro, 10 performance + 4
efficiency cores, 24 GB unified memory, macOS 27.0), 2026-10-24, by a child
session; the folded numbers are `apps/desktop/spikes/talk/numbers-macbook.json`
(hand-measured values under `manual`). Same 20 phrases, same scoring. Extra
synthetic sets here: the macOS OS voice (`say`, Anna de_DE and Samantha
en_US, the compact voices installed by default, written straight to 16 kHz
files by `scripts/os-voice-mac.sh`) and Qwen3-TTS through the stock
llama.cpp b11846 macOS arm64 build on Metal. Nemotron was not run: there is
no transcribe.cpp Metal build to download and the time box went to the TTS
side.

| engine | clips | WER | first partial | tail after end of speech | compute / audio |
|---|---|---|---|---|---|
| Whistle, CPU | Kokoro en | 24.1 % | clip only | 16 ms median, 54 ms max | **0.006** |
| Whistle, CPU | Qwen en | 27.6 % | | 12 ms median | 0.004 |
| Whistle, CPU | **Qwen de** | **27.2 %** | | 19 ms median | 0.005 |
| Whistle, CPU | say en (Samantha) | 31.0 % | | 17 ms median | |
| Whistle, CPU | say de (Anna) | 45.7 % | | | |
| Zipformer streaming, CPU 4 threads | Kokoro en | 13.8 % | 800 ms of audio | 19 ms median, 24 ms max | 0.032 |
| Zipformer streaming, CPU 4 threads | Qwen en | 21.8 % | 1200 ms of audio | 7 ms median | 0.024 |
| Zipformer streaming, CPU 4 threads | say en (Samantha) | 24.1 % | | | |

Whistle on the M4 Pro is three to four times faster than on the i9 and
hears the same things: `exp1242`, `new54`, `reject 1` in the dispatcher's
form (charged as errors), German content words and commands right on the
Qwen clips ("lege ein issue an titel push to talk hot key funktioniert nicht
unter wayland", "wie viele einträge sind in meiner inbox"), the identifiers
mangled ("ixp1369", "exp12"). Whistle loads in 1.5 s cold (the weights
download once into its cache) and 13–15 ms warm, 60 MB RSS. The Zipformer
scores better here than on mint (13.8 % against 35.6 % on the Kokoro set):
a hypothesis, not a finding: Kokoro's clips were generated separately on
each machine, and the mint set was not scored on the Mac; it still drops
identifiers ("X P THIRTEEN SIXTY NINE TO DUN"). Silero: 10 of 10 segments on
Kokoro, 20 of 20 on the say set, 4–12 ms per clip, end-of-utterance lag
exactly the knob (500 ms at 0.5, 300 ms at 0.3). Keyword spotter: `HEY` on
Kokoro en-01 at 700 ms, `STOP` on Samantha's en-01 at 1,600 ms, 0.008 of
real time, and `EXPONENTIAL` / `HEY_EXPONENTIAL` / `EXPO` never, boosted or
not, as on mint.

| engine | where | time to first audio | RTF | memory | German |
|---|---|---|---|---|---|
| Kokoro v1.0 via sherpa-onnx 2.0, lexicon | CPU, 4 threads | **252–833 ms** (median 599; whole utterance in one chunk) | 0.19–0.25 (median 0.215) | 767 MB RSS, 563 ms load | **no voice** |
| Qwen3-TTS 1.7B Base Q4_K_M + mmproj Q8_0 via llama.cpp `llama-tts` b11846 | **Metal**, M4 Pro | not streaming: whole utterance in 0.98–3.85 s (median 1.84 s; vocoder a flat 0.67 s) | **0.57 median** (de 0.61, en 0.57; 1.8x faster than real time) | 6.9 GB RSS (unified memory; Metal maps the weights into the process), ~1 s of model load per call | **yes**, all 10 German clips |
| same, `-dev none` | CPU only | 3.82 s for 2.80 s | 1.36 (slower than real time; the vocoder alone 3.0 s) | 8.5 GB RSS | yes |
| macOS `say`: Anna de_DE, Samantha en_US (compact voices) | the system speech daemon | whole clip on disk in 0.75–1.1 s per call, flat regardless of length (first Anna call 4.0 s to load the voice) | 0.33 (en) / 0.39 (de) median | 32 MB in the `say` client; the daemon is not measured | yes |

Quality of the OS voice was NOT judged by ear in this session: the agent
cannot listen. To hear it, regenerate the clips on the Mac in about 20 s
(`scripts/os-voice-mac.sh out/os-voice-mac < data/phrases.json`, then
`afplay out/os-voice-mac/de-01.wav`); they are not in git. The recognisers' proxy is clear
enough to write down: Whistle hears Anna's German at 45.7 % WER against
27.2 % for Qwen3-TTS's German, and the Zipformer hears Samantha at 24.1 %
against 13.8 % for Kokoro, so both compact voices are harder to understand
than the neural engines; the Enhanced/Premium voices (a download in System
Settings → Accessibility → Spoken Content) are the fair comparison and were
not installed.

Verdict for a CPU-only Mac (no discrete GPU, but Metal on the integrated
GPU): `Transcriber` = **Whistle** (27 % WER on both languages at 0.005 of
real time; the Zipformer is English-only and drops the identifiers); `Speaker`
= **Kokoro** for English on the CPU (0.6 s to first audio) and **Qwen3-TTS
1.7B on Metal** when German is needed and 7 GB of unified memory are
acceptable (1.8x faster than real time, not streaming, so 1–4 s per reply);
German otherwise falls to the macOS compact voice, which the recognisers
understand half as well as Qwen's German and which the owner still has to hear.

Superseded on the voice side by "Decision after listening" above: the owner
heard these clips, the macOS compact voices are dropped, and v1 is English
only with Kokoro as the one voice.
