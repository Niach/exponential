---
title: API
description: "The speech worker's wire (protocol 7: requests, events, level frames), listen failures, the CrispASR runtime's sessions and the Settings → Assistant → Voice fields."
---

Sources: [`etalk/src/lib.rs`](https://github.com/Niach/exponential/blob/master/apps/desktop/crates/etalk/src/lib.rs) (the wire), [`etalk-crispasr`](https://github.com/Niach/exponential/tree/master/apps/desktop/crates/etalk-crispasr) (the runtime), [`packages/etalk/fixtures/voice.json`](https://github.com/Niach/exponential/blob/master/packages/etalk/fixtures/voice.json) (the settings). The crates and how to depend on them: [eTalk reference](/docs/libraries/etalk/reference/).

## The worker

The desktop app runs speech in a second process: its own executable, started again with a marker as the first argument (`ARGV_MARKER`, checked first in `main`).

```sh
<exe> __speech-worker --data-dir <dir>
```

Requests go to its stdin, events come back on its stdout: one JSON object per line, tagged by `"t"`, camelCase. The worker logs to stderr (a line starting `debug: ` is logged at debug). When the app dies, stdin closes and the worker exits. Models live under `{data-dir}/talk/`.

```jsonl
{"t":"listen","id":"u1","language":"de"}
{"t":"level","id":"u1","level":0.5,"bands":[0.0,0.25,0.5,0.75,1.0,0.5,0.25,0.0]}
{"t":"endListen","id":"u1"}
{"t":"heard","id":"u1","text":"Move NEW-78 to done."}
```

`SpeechClient` starts the worker on the first request and again after it dies. The wire is `PROTOCOL` 7; the worker says so in `ready`. Versions: 3 = listening, 4 = the wake word, 5 = calls, 6 = meetings, 7 = the ring and `callListening`.

## Requests

| t | Fields | Does |
|---|---|---|
| `prepare` | `voice` | Download and load the voice. Cheap to repeat. |
| `speak` | `id, text` | Say the text (already the spoken form). Replaces anything playing. |
| `stop` | | End what is being said. |
| `listen` | `id, language` | Open the microphone for one utterance. Stops any speech (barge-in) and replaces a running listen. |
| `endListen` | `id` | The key was released: close the microphone, transcribe. |
| `cancelListen` | `id` | Drop the utterance. No answer. |
| `call` | `id, language, remote, ringMs` | Listen continuously until `hangup`, answering `said` per utterance. `remote` = audio comes only as `callAudio` and replies stream back as `speechAudio`. `ringMs` = how long a local call rings while the worker warms up (0 = no ring). |
| `callAudio` | `id, pcm` | Caller audio of a remote call: base64 of i16 little-endian mono samples at 16 kHz. |
| `callMute` | `id, muted` | Drop the call's incoming audio; replies still play. |
| `hangup` | `id` | End the call. No answer events. |
| `wake` | `phrase, language` | Listen for the wake phrase (already resolved); an empty phrase = off. |
| `meeting` | `id, language, phrase, youLabel, selection, capability` | Transcribe the microphone (channel `you`) and the selected desktop audio until `meetingStop`. `selection` = `{mode: off \| all \| include \| exclude, apps}`. |
| `meetingStop` | `id, drain` | End the meeting. `drain` transcribes every open segment first, then `meetingStopped`. |
| `meetingPause` | `id, paused` | Hold or resume every channel of the meeting. |

`voice` is a `VoiceChoice`: `language` (`en` \| `de`), `file` (empty = the built-in clip) and `transcript`. A `speak` that arrives while a listen runs is answered `finished` with `stopped: true`, unplayed.

## Events

| t | Fields | Means |
|---|---|---|
| `ready` | `protocol` | The worker is up. |
| `downloading` | `received, total` | A download is running (`total` 0 = unknown). |
| `downloadFailed` | `error` | The download or its check failed; the next request retries. |
| `modelReady` | | The voice is on disk, checked and loaded. |
| `voices` | `voices` | The languages this machine can speak. Empty = no usable GPU. Sent after every `prepare`. |
| `started` | `id` | The first audio of `id` is playing. |
| `finished` | `id, stopped` | `id` ended. |
| `failed` | `id, error` | `id` could not be spoken. |
| `level` | `id, level, bands` | The microphone level while listening. |
| `partial` | `id, text` | What was heard so far. GPU only. |
| `heard` | `id, text` | The final transcript. |
| `empty` | `id` | Nobody spoke (a tap, or only noise). |
| `listenFailed` | `id, reason, detail` | Could not listen or transcribe. |
| `said` | `id, text` | A call heard one finished utterance. |
| `speechStarted` | `id` | A call's open utterance reached 250 ms of speech (barge-in); a playing reply has already ended. |
| `callListening` | `id` | A call hears: recogniser and voice loaded, the input open. Once per call. |
| `speechAudio` | `id, pcm, rate` | A reply's audio during a remote call, at most 100 ms per event, 24 kHz. |
| `woke` | `id` | The wake phrase was heard: turn `id` runs like a listen and the worker ends it. |
| `wakeListening` | `listening, reason` | The microphone is open (or closed) for the wake word. |
| `meetingSegment` | `id, channel, label, text, startMs, endMs, words` | A finished segment of one channel, with per-word timestamps where the runtime gives them. |
| `meetingWake` | `id` | The wake phrase opened the `you` segment that just came. |
| `meetingFlushNow` | `id` | Push to talk was pressed during the meeting. |
| `meetingListening` | `id, listening, reason` | The meeting listens, or stopped listening. |
| `meetingStopped` | `id` | A draining `meetingStop` is done. |

`level` comes every 32 ms (one Silero frame, 512 samples at 16 kHz): `level` and 8 `bands` from low to high pitch, each 0 to 1, already smoothed for drawing. `partial` comes every 300 ms, only when the transcriber runs on the GPU. The host also sees `exited` when the worker ends on its own; the worker never sends it.

## Listen failures

| reason | Means |
|---|---|
| `no_microphone` | No input device, or it would not open. |
| `not_granted` | The OS refused the microphone (macOS). |
| `downloading` | The speech model is still downloading. |
| `unavailable` | The model or runtime failed to download, check or load. |
| `failed` | Transcribing failed. |
| `meeting` | A meeting owns the microphone. |

## CrispASR

The worker loads CrispASR 0.8.45 at runtime, after re-hashing every file. One process loads one build: the GPU build where the driver exists, else the CPU build.

| Type | What |
|---|---|
| `Runtime` | The loaded library, its GPUs, and the sessions below. |
| `TtsSession` | `set_voice(wav, ref_text, language)`, `synthesize(text)`: Qwen3-TTS. |
| `AsrSession` | `transcribe(pcm, language)`: Parakeet. |
| `VadStream` | `push(samples)`: Silero speech probability per 32 ms frame. |

## Settings fields

One row per member, read and written through Exponential's API (`voice.get`, `voice.update`) and handed to your devices.

| Field | Type | Default |
|---|---|---|
| `voiceDeviceId` | one of your desktops, or null | `null` |
| `agentId` | an agent of one of your teams, or null | `null` |
| `pttShortcut` | a [key binding](/docs/libraries/etalk/guides/push-to-talk/) (empty = push to talk off) | `""` |
| `wakeWord` | boolean | `false` |
| `wakePhrase` | one line, ≤ 64 chars, at least 6 letters (empty = Hey `{agent}`) | `""` |
| `language` | `en` \| `de` | `en` |
| `voiceFile` | one line, ≤ 512 chars | `""` |
| `voiceTranscript` | one line, ≤ 500 chars | `""` |

Replies to your voice are always spoken. Picking None as the device or agent turns Wake word off; a bound key stays bound.

## Try it

From `apps/desktop` in the repo. Both download what they need on first run.

```sh
cargo run -p etalk-engine --example speak -- <data-dir> <en|de> "Text to say." [voice.wav "its transcript"]
cargo run -p etalk-engine --example listen -- <data-dir> <en|de> [--cpu] <clip.wav>...
```

`speak` runs the worker the way the app does and prints every event. `listen` runs WAV files through the same pipeline as the microphone and prints one JSON line per clip. They run on Linux and Windows: on macOS the runtime comes only from the signed app bundle, which `cargo run` doesn't have.
