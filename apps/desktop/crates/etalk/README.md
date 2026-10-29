# eTalk

Speech in and out for agents: push to talk, a wake word, spoken replies,
phone calls and meeting mode, on the user's own machine. One core for
Exponential and spezl.

## Crates

| Crate | What it is | Links |
| --- | --- | --- |
| `etalk` | The host side: `SpeechClient` (spawns and talks to the speech worker over JSON lines), the wire (`Request`, `Event`), the microphone doctor, per-app desktop audio, and the pure state machines `etalk::voice` (settings, push to talk, wake word, tray, spoken-reply rules), `etalk::call` (the phone call) and `etalk::meeting`. gpui-free, engine-free. | serde, regex, chrono |
| `etalk-engine` | The engines, run ONLY inside the speech worker: Silero VAD, Parakeet, Qwen3-TTS (through CrispASR), echo cancelling, playback, the wake word. | `etalk`, `etalk-crispasr` |
| `etalk-crispasr` | The pinned CrispASR runtime + models: sha256-verified downloads, loaded at run time with `libloading`, nothing linked. | |
| `etalk-codec` | The call's Opus (pure-Rust `opus-rs` by default, `libopus` behind a feature). | |
| `etalk-gpui` | The UI: tray voice, push-to-talk overlay, call panel, reply speaker, meeting mode, behind the `TalkHost` sink (see its README). | gpui |

## The contract

`packages/etalk` (`@exp/etalk`) owns the fixtures every platform replays:
`voice.json`, `voice-call.json`, `talk-listen.json`, `meeting.json`, each with
its `$comment` as the spec, plus the TypeScript references (`voice.ts`,
`voice-call.ts`, `meeting.ts`). The Rust state machines here, Swift
(`apps/ios`) and Kotlin (`apps/android`) are locked to the same cases. A
behaviour change edits the fixture first.

## Hosting the worker

The worker is the host's own executable re-run as
`<exe> __speech-worker --data-dir <dir>`, so an ONNX crash or a stuck audio
device never takes the app down. Route it first thing in `main`:

```rust
fn main() {
    if etalk::requested() {
        etalk_engine::run_and_exit();
    }
    // ... the app
}
```

Then `etalk::SpeechClient::new(std::env::current_exe()?, data_dir, |event| …)`
and call `prepare` / `speak` / `listen` / `wake` / `call` on it. Models land in
`<data_dir>/talk`.

On Apple Silicon the CrispASR Metal runtime ships inside the app bundle: run
`crates/etalk-crispasr/scripts/bundle-macos.sh <App.app> <executable>` before
codesigning (Exponential's `scripts/build-macos-app.sh` does). Linux and
Windows download the pinned runtime on first use.

## Consuming it

The crates are consumed as a git dependency on a tag, `etalk-v<version>`
(the desktop workspace version), because the fixtures are compiled in from
`packages/etalk/fixtures` and only a checkout of this repo carries them:

```toml
etalk = { git = "https://github.com/Niach/exponential", tag = "etalk-v0.14.83" }
```

`etalk-gpui` also needs the same gpui and gpui-component revisions as this
workspace (see the eUI gpui revision policy).
