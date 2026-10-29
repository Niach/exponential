# etalk-gpui

eTalk's gpui side, the same code in Exponential and spezl: the voice tray
(`voice_tray`), push to talk and the wake word with the overlay capsule
(`push_to_talk`, `voice_overlay`), caller mode with its call panel and
floating pill (`voice_call`), spoken replies (`speak_replies`) and meeting
mode (`meeting`), all over ONE speech worker (`etalk::SpeechClient`). The
behaviour is the fixtures' (`packages/etalk/fixtures`), replayed by the
state machines in `etalk`.

It never links an app: no server, store or relay crate. Everything it needs
from the app goes through two traits in `src/host.rs`, with plain data types.

## Install

```rust
etalk_gpui::init(cx, Box::new(MyHost::new()));
```

once, before the first window. It installs the host as a gpui Global and
starts the speaker, push to talk, the call, the meeting host and the tray, in
that order. Then the host tells eTalk what changed:

| Call | When |
| --- | --- |
| `etalk_gpui::set_settings(settings, cx)` | the member's `VoiceSettings` arrived or were stored |
| `etalk_gpui::messages_changed(cx)` | the conversation's messages changed (a reply to speak, a turn done) |
| `etalk_gpui::session_changed(cx)` | the signed-in account changed or signed out |
| `etalk_gpui::agents_changed(cx)` | an agent's name changed |
| `etalk_gpui::voice_call::on_ring(call_id, agent_id, cx)` | a remote caller rings this device |

and reads it back where it draws its own screens (`speak_replies::speaking`,
`push_to_talk::status`, `voice_call::CallPanel`, `voice_tray::rail`, ...).

## The contract

`TalkHost` (main thread, `&App`/`&mut App`):

- `brand()`: the tray's title and SVG icon, the floating windows' app id.
- `data_dir()`, `device_id()`: where the device-local files live (speech
  data, the tray's switches, the meeting log, the pill's position) and this
  machine's id. The member's voice device is one `device_id`.
- `account(cx)`, `accounts(cx)`: the signed-in account (None = signed out)
  and every account signed in here (the shared speech data is freed only
  when none of them names this device).
- `agent(id, cx)`, `conversation(agent_id, cx)`: an agent's names, team and
  icon, and the member's conversation with it.
- `messages(cx, visit)`: visit every message row (`TalkMessage`, borrowed,
  never copied) and say whether they have synced. The first synced rows are
  history; later rows are spoken when `etalk::voice::should_speak` says so.
- `send(outgoing, cx)`: prepare a send (`Outgoing`: an
  `etalk::voice::UtteranceTarget`, `meeting`, a `client_message_id`); the
  returned `SendJob` runs on a background thread. None = signed out.
- `write(VoiceWrite, done, cx)`: store a settings change (the tray's wake
  word, a meeting start or stop), hand the stored settings to `set_settings`
  (or say it failed), then run `done`.
- `connect_call(call_id, cx)`: a streamed call's `CallLink` and its events.
- `device(id, cx)`, `desktop_audio(id, cx)`: the voice device's name and
  presence, and its desktop audio (meeting mode's "who").
- `icon(TalkIcon)`: the brand mark, an icon concept, an agent's icon.
- `open(Surface, cx)`: a window, the voice settings, THE meeting consent
  dialog.
- `notify(Notice, cx)`: an error toast, a failed action, a meeting that
  stopped itself.
- `readiness_changed(cx)`: the speech state or the listening device moved
  (Exponential re-reports its device doctor).

`CallLink`: `send_event` (a flow event, JSON), `send_speech` (16-bit PCM),
`flush` (barge-in), `end`.

## A host without a server

spezl has one local agent session and no server: `account` is a fixed local
id, `accounts` lists just it, `agent` and `conversation` answer for the one
session, `messages` visits its local transcript, `send` returns a job that
hands the text to the local session, `write` stores the settings in a local
file and calls `set_settings` with them, `connect_call` returns None (no
remote callers), `device` answers for this machine only. Its device id is
the voice device in the settings it stores, so the tray, push to talk and
spoken replies run here.

`examples/host.rs` is that host, compiled (no window): `cargo run -p
etalk-gpui --example host`.
