//! NEW-53: exponentialTalk "Speak replies" in the IDE (`fixtures/voice.json`).
//!
//! Watches the host's messages ([`crate::messages_changed`]): a row
//! [`etalk::voice::should_speak`]
//! accepts (the agent's final reply in the member's conversation with the
//! agent picked in Settings → Voice, spoken only on the member's voice
//! device; `user_voice_settings`, delivered by every heartbeat) is
//! reduced to [`etalk::voice::spoken_text`] and handed to the speech worker
//! (`etalk::SpeechClient`, the IDE's own executable as `__speech-worker`).
//! While it plays, the conversation's Dock shows the Speaking banner with
//! Stop (Exponential's `steer_viewer::dock_banners`). S-3: SPEAK-BACK IS ALWAYS ON (the
//! setting is gone): the voice device with an agent picked keeps the worker
//! and the voice prepared, so the first reply does not wait for the
//! download (~2-3 GB: the CrispASR runtime and Qwen3-TTS, NEW-74), and what
//! is spoken is a VOICE TURN's reply only (voice.json `voiceTurn`): the
//! running call's conversation, or the reply to a push-to-talk / wake turn
//! ([`voice_turn`]); a typed message is never read aloud. NEW-74: which languages are
//! spoken is what the worker reports (`etalk::Event::Voices`): none without a
//! usable GPU, so a reply there is shown, never spoken.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};

use etalk::voice::{VoiceContext, VoiceMessage, VOICE};

use crate::host::{self, Notice, TalkMessage};

/// NEW-55: the doctor's `speech` row for THIS device (None = no row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechState {
    /// The worker speaks here.
    Ready,
    /// The voice downloads (a call rings until it is ready).
    Downloading,
    /// No usable GPU: replies are shown, never spoken.
    NoGpu,
    /// NEW-99: the voice failed to load (the download failed before any
    /// voice was known, or the worker crashed [`MAX_CRASHES`] times in a
    /// row): calls are off until a restart or a changed setting.
    Failed,
}

impl SpeechState {
    /// The doctor row's state (device-doctor.json `speech`), the one input
    /// of `etalk::call::call_speech_of`.
    pub fn doctor_state(self) -> &'static str {
        match self {
            SpeechState::Ready => "ok",
            SpeechState::Downloading => "off",
            SpeechState::NoGpu => "missing",
            SpeechState::Failed => "error",
        }
    }
}

/// NEW-55 / NEW-99: the `speech` row from the watcher's state (None = no
/// row: not the voice device, Speak replies off, or the worker has not
/// answered yet).
fn speech_row(speaks: bool, download: &Download, voices_known: bool, voices_empty: bool) -> Option<SpeechState> {
    if !speaks {
        return None;
    }
    match download {
        Download::Unavailable => return Some(SpeechState::Failed),
        Download::Running { .. } => return Some(SpeechState::Downloading),
        Download::Failed if !voices_known => return Some(SpeechState::Failed),
        _ => {}
    }
    if !voices_known {
        return None;
    }
    Some(if voices_empty { SpeechState::NoGpu } else { SpeechState::Ready })
}

/// What THIS device speaks for: the member's voice settings
/// (`user_voice_settings`, delivered by every heartbeat) seen from here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceVoice {
    pub agent_id: Option<String>,
    /// #NEW-54: push to talk or (#NEW-56) the wake word listens here.
    pub listen: bool,
    /// NEW-74: the language and voice file the worker speaks with.
    pub voice: etalk::VoiceChoice,
}

impl DeviceVoice {
    /// The worker runs here: this is the voice device and an agent is
    /// picked (S-3: speak-back is always on).
    pub fn active(&self) -> bool {
        self.agent_id.is_some()
    }

    /// The voice (the TTS model) is wanted: always where the worker runs.
    fn speaks(&self) -> bool {
        self.active()
    }
}

/// voice.json: only the member's voice device speaks.
pub fn voice_here(settings: &etalk::voice::VoiceSettings, own_device_id: &str) -> DeviceVoice {
    if settings.voice_device_id.as_deref() != Some(own_device_id) {
        return DeviceVoice::default();
    }
    let voice = etalk::VoiceChoice {
        language: settings.language().to_string(),
        file: settings.voice_file.clone(),
        transcript: settings.voice_transcript.clone(),
    };
    let settings = settings.clone().clear_on_none();
    // NEW-74: the worker is prepared for every language; whether it speaks
    // is what it reports back (`voices`, empty without a usable GPU).
    // NEW-57: a meeting keeps the worker resident too.
    let listen = settings.push_to_talk() || settings.wake_word || settings.meeting();
    DeviceVoice { agent_id: settings.agent_id, listen, voice }
}

/// NEW-78: THIS device listens: it is the member's voice device and Push to
/// talk or (#NEW-56) the wake word is on (after voice.json `clearOnNone`).
/// Only then does its doctor carry the `microphone` row.
pub fn listens_here(settings: &etalk::voice::VoiceSettings, own_device_id: &str) -> bool {
    let settings = settings.clone().clear_on_none();
    (settings.push_to_talk() || settings.wake_word) && settings.voice_device_id.as_deref() == Some(own_device_id)
}

/// [`listens_here`] for the member's current settings (false before the
/// first heartbeat delivered them).
pub fn push_to_talk_here(cx: &App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let Some(device_id) = host::device_id(cx) else {
        return false;
    };
    listens_here(&entity.read(cx).settings, &device_id)
}

/// The reply being spoken (for the Dock banner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Speaking {
    pub message_id: String,
    pub session_id: String,
    /// The spoken text's first sentence (the banner's title).
    pub title: String,
}

/// The voice download, for the Speak replies row's trailing detail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Download {
    #[default]
    Idle,
    /// Bytes so far of the whole first-use download (`total` 0 = unknown).
    Running { received: u64, total: u64 },
    Failed,
    /// The worker crashed [`MAX_CRASHES`] times in a row: no more speaking
    /// until the setting changes.
    Unavailable,
}

/// Crashes in a row before the watcher gives the voice up.
const MAX_CRASHES: u32 = 3;

/// The wait before restarting a crashed worker: 2, 4, 8 s.
fn restart_delay(crashes: u32) -> Duration {
    Duration::from_secs(1u64 << crashes.clamp(1, MAX_CRASHES))
}

/// A message row the gate let through, owned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Row {
    id: String,
    author_kind: Option<String>,
    body: Option<String>,
    has_card: bool,
    author_session_id: Option<String>,
    session_id: Option<String>,
    created_at: Option<String>,
}

impl Row {
    fn of(row: &TalkMessage<'_>) -> Self {
        let message = &row.message;
        Row {
            id: row.id.to_string(),
            author_kind: message.author_kind.map(str::to_string),
            body: message.body.map(str::to_string),
            has_card: message.has_card,
            author_session_id: message.author_session_id.map(str::to_string),
            session_id: message.session_id.map(str::to_string),
            created_at: message.created_at.map(str::to_string),
        }
    }

    fn message(&self) -> VoiceMessage<'_> {
        VoiceMessage {
            author_kind: self.author_kind.as_deref(),
            body: self.body.as_deref(),
            has_card: self.has_card,
            author_session_id: self.author_session_id.as_deref(),
            session_id: self.session_id.as_deref(),
            created_at: self.created_at.as_deref(),
        }
    }
}

/// Pure backfill gate: arms on the first ready observation (every row then
/// present is history, the newest `created_at` is the arm watermark), then
/// returns each row once.
#[derive(Default)]
struct Gate {
    armed: bool,
    seen: HashSet<String>,
    watermark: Option<String>,
}

impl Gate {
    /// One look at the feed: `rows` visits every row and answers whether
    /// the feed is ready (not ready = the gate starts over).
    fn observe(&mut self, rows: impl FnOnce(&mut dyn FnMut(&TalkMessage<'_>)) -> bool) -> Vec<Row> {
        let arming = !self.armed;
        let mut newest: Option<(i64, String)> = None;
        let mut fresh = Vec::new();
        let ready = rows(&mut |row| {
            let first = self.seen.insert(row.id.to_string());
            if arming {
                if let Some(created) = row.message.created_at {
                    if let Some(ms) = etalk::voice::parse_instant(created) {
                        if newest.as_ref().is_none_or(|(mark, _)| ms > *mark) {
                            newest = Some((ms, created.to_string()));
                        }
                    }
                }
            } else if first {
                fresh.push(Row::of(row));
            }
        });
        if !ready {
            *self = Gate::default();
            return Vec::new();
        }
        if arming {
            self.armed = true;
            self.watermark = newest.map(|(_, text)| text);
            return Vec::new();
        }
        fresh
    }

    /// The arm time `should_speak` compares against: the watermark, or the
    /// epoch when the collection was empty (every later row is new).
    fn armed_at(&self) -> &str {
        self.watermark.as_deref().unwrap_or("1970-01-01T00:00:00Z")
    }
}

pub struct SpeakReplies {
    gate: Gate,
    client: Option<etalk::SpeechClient>,
    /// The setting the worker was last prepared for (assistant + switch).
    prepared: DeviceVoice,
    /// The member's voice settings as the last heartbeat delivered them.
    settings: etalk::voice::VoiceSettings,
    /// The signed-in account the worker serves (`None` = signed out).
    account: Option<String>,
    /// Worker crashes since the last utterance that played to its end.
    crashes: u32,
    /// voice.json `voices`: the languages the worker reported it speaks
    /// (empty until it answers, and without a usable GPU).
    voices: Vec<String>,
    /// NEW-55: the worker answered `Voices` since it (re)started.
    voices_known: bool,
    pub speaking: Option<Speaking>,
    pub download: Download,
    /// #NEW-54: the worker answered `Ready` and has not exited since (the
    /// tray's Mute shows only then).
    pub worker_ready: bool,
    /// #NEW-63: when the machine's speech data goes.
    cleanup: Cleanup,
    /// NEW-58: the signed-in account's settings arrived since it signed in
    /// (the tray's Screen memory tells "no voice device" from "not yet").
    settings_known: bool,
    /// S-3: a push-to-talk / wake turn went out and waits for its reply
    /// (voice.json `voiceTurn`): that conversation's next reply is spoken.
    voice_turn: Option<VoiceTurn>,
}

/// S-3: a voice turn waiting for its reply.
#[derive(Clone, Debug)]
struct VoiceTurn {
    session_id: String,
    at: std::time::Instant,
}

/// How long a voice turn waits for its reply before a later one is
/// treated as typed.
const VOICE_TURN_TTL: Duration = Duration::from_secs(15 * 60);

/// #NEW-63 / #1278 review: `{data_dir}/talk` belongs to the MACHINE, shared
/// by every account signed in here. It goes only once the same account's
/// confirmed settings move the voice device away from here (never on the
/// first settings after an account change) and no signed-in account's last
/// confirmed settings still name this device.
#[derive(Debug, Default)]
struct Cleanup {
    /// Per account: its last confirmed settings named this device.
    /// Persisted ([`CLAIMS_FILE`]): an account not active now still counts.
    claims: std::collections::BTreeMap<String, bool>,
    /// The account whose settings were taken last in this process.
    account: Option<String>,
    /// A transition away was seen; the data goes once no signed-in account
    /// claims it and no worker runs.
    pending: bool,
}

/// `{data_dir}/voice-device-claims.json`, beside (never inside) `talk/`.
const CLAIMS_FILE: &str = "voice-device-claims.json";

impl Cleanup {
    fn load(data_dir: &std::path::Path) -> Self {
        let claims = std::fs::read(data_dir.join(CLAIMS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { claims, ..Self::default() }
    }

    fn save(&self, data_dir: &std::path::Path) {
        let Ok(bytes) = serde_json::to_vec(&self.claims) else { return };
        if let Err(err) = std::fs::write(data_dir.join(CLAIMS_FILE), bytes) {
            log::warn!("[ui] speak replies: saving the voice device claims failed: {err}");
        }
    }

    /// `account`'s confirmed settings arrived (`names_here` = they pick this
    /// device); `signed_in` = the accounts signed in on this machine.
    /// Answers whether the speech data should go now, and whether the
    /// claims changed (to persist).
    fn take(&mut self, account: &str, names_here: bool, signed_in: &[String]) -> (bool, bool) {
        let before = self.claims.clone();
        let previous = self.claims.insert(account.to_string(), names_here);
        self.claims.retain(|id, _| id == account || signed_in.iter().any(|signed| signed == id));
        let switched = self.account.as_deref().is_some_and(|last| last != account);
        self.account = Some(account.to_string());
        if previous == Some(true) && !names_here && !switched {
            self.pending = true;
        } else if names_here {
            self.pending = false;
        }
        let claimed = self.claims.values().any(|here| *here);
        (self.pending && !claimed, self.claims != before)
    }
}

struct SpeakRepliesGlobal(Entity<SpeakReplies>);

impl Global for SpeakRepliesGlobal {}

/// Install the watcher ONCE, after the host ([`crate::init`]).
pub(crate) fn install(cx: &mut App) {
    let Some(data_dir) = host::data_dir(cx) else { return };
    let (tx, rx) = flume::unbounded::<etalk::Event>();
    let client = std::env::current_exe().ok().map(|exe| {
        etalk::SpeechClient::new(exe, data_dir.clone(), move |event| {
            let _ = tx.send(event);
        })
    });
    // #NEW-66: the account signed in now, so the first state notification
    // of an already-synced store is no account change (it would end a
    // worker that is still preparing, a download included).
    let account = session_account(cx);
    let cleanup = Cleanup::load(&data_dir);
    // The host's messages and its sign-ins arrive through
    // `crate::messages_changed` and `crate::session_changed`.
    let entity = cx.new(|_: &mut Context<SpeakReplies>| {
        SpeakReplies {
            gate: Gate::default(),
            client,
            prepared: DeviceVoice::default(),
            settings: etalk::voice::VoiceSettings::default(),
            account,
            crashes: 0,
            voices: Vec::new(),
            voices_known: false,
            speaking: None,
            download: Download::Idle,
            worker_ready: false,
            cleanup,
            settings_known: false,
            voice_turn: None,
        }
    });
    cx.set_global(SpeakRepliesGlobal(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            let _ = cx.update(|cx| {
                // NEW-55 review: a worker crash ends a running call too
                // (Speak replies then restarts the worker as before).
                if matches!(event, etalk::Event::Exited) {
                    crate::voice_call::on_worker_event(etalk::Event::Exited, cx);
                }
                // NEW-55: a running call's events (its microphone, its
                // streamed reply audio) go to the call first.
                // S-3: the call repaints only its own surfaces.
                if crate::voice_call::owns(&event, cx) {
                    crate::voice_call::on_worker_event(event, cx);
                    return;
                }
                // NEW-57: what a meeting heard goes to the meeting host.
                if crate::meeting::is_meeting_event(&event) {
                    crate::meeting::on_worker_event(event, cx);
                    return;
                }
                // #NEW-54: what the microphone heard goes to push to talk.
                if crate::push_to_talk::is_listen_event(&event) {
                    crate::push_to_talk::on_worker_event(event, cx);
                    return;
                }
                // S-3: what the windows draw of it (the Speaking banner,
                // the download detail) repaints in `on_event`, only when it
                // changed: never every window for every worker event.
                entity.update(cx, |this, cx| this.on_event(event, cx));
            });
        }
    })
    .detach();
}

fn global(cx: &App) -> Option<Entity<SpeakReplies>> {
    cx.try_global::<SpeakRepliesGlobal>().map(|global| global.0.clone())
}

/// NEW-55: the doctor's `speech` row for THIS device.
pub fn speech_state(cx: &App) -> Option<SpeechState> {
    global(cx)?.read(cx).speech_state(cx)
}

/// The watcher itself (the voice tray observes it).
pub fn entity(cx: &App) -> Option<Entity<SpeakReplies>> {
    global(cx)
}

/// #NEW-68: the member's voice settings as the last heartbeat (or this
/// device's own save) delivered them.
pub fn settings(cx: &App) -> etalk::voice::VoiceSettings {
    global(cx).map(|entity| entity.read(cx).settings.clone()).unwrap_or_default()
}

/// NEW-58: [`settings`] came from the signed-in account (not the defaults
/// before its first beat).
pub fn settings_known(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).settings_known)
}

/// The reply being spoken, if any.
pub fn speaking(cx: &App) -> Option<Speaking> {
    global(cx).and_then(|entity| entity.read(cx).speaking.clone()).or_else(|| DEV_SPEAKING.clone())
}

/// DEV-ONLY (packages/shots/src/desktop-env.md): `EXP_DEV_SPEAKING=<session
/// uuid>|<language>|<reply body>` draws that session's Speaking banner as if
/// the reply were being read aloud, title = the spoken text's first sentence
/// (`spokenText` in that language), so a capture needs no GPU voice.
static DEV_SPEAKING: std::sync::LazyLock<Option<Speaking>> = std::sync::LazyLock::new(|| {
    let spec = std::env::var("EXP_DEV_SPEAKING").ok()?;
    let mut parts = spec.splitn(3, '|');
    let (session_id, language, body) = (parts.next()?, parts.next()?, parts.next()?);
    let spoken = etalk::voice::spoken_text(body, VOICE.max_chars, language);
    Some(Speaking {
        message_id: "dev-speaking".into(),
        session_id: session_id.to_string(),
        title: etalk::voice::first_sentence(&spoken).to_string(),
    })
});

/// #NEW-54: the speech worker, for push to talk's Listen / EndListen /
/// CancelListen (one worker serves both).
pub fn client(cx: &App) -> Option<etalk::SpeechClient> {
    global(cx).and_then(|entity| entity.read(cx).client.clone())
}

/// #NEW-54: the worker is up (the tray's `micAvailable`).
pub fn worker_ready(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).worker_ready)
}

/// The voice download state (the Speak replies row's detail).
pub fn download(cx: &App) -> Download {
    global(cx).map(|entity| entity.read(cx).download.clone()).unwrap_or_default()
}

/// The Speak replies row's trailing detail while the voice downloads:
/// voice.json `downloadProgress`, `None` when nothing downloads.
pub fn download_detail(cx: &App) -> Option<String> {
    match download(cx) {
        Download::Running { received, total } => Some(etalk::voice::download_progress(received, total)),
        _ => None,
    }
}

/// The banner's Stop.
pub fn stop(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, _| {
            if let Some(client) = this.client.as_ref() {
                client.stop();
            }
            this.speaking = None;
        });
        cx.refresh_windows();
    }
}

/// S-3: a push-to-talk / wake turn went out to `session_id` (the member's
/// conversation with the voice agent): its reply is read aloud (voice.json
/// `voiceTurn`; a typed message's never is).
pub fn voice_turn(session_id: &str, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, _| {
            this.voice_turn = Some(VoiceTurn { session_id: session_id.to_string(), at: std::time::Instant::now() });
        });
    }
}

/// S-3: a call was answered: the device says `text` (`callGreeting`) on the
/// call's output stream, tracked like a reply (`speaking`, its `Finished`)
/// so the call's Speaking follows it, but no agent message and no banner
/// (no session). False = nothing here can speak it.
pub fn speak_greeting(id: String, text: String, cx: &mut App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let spoken = entity.update(cx, |this, cx| {
        let Some(client) = this.client.as_ref() else { return false };
        if this.voices_known && this.voices.is_empty() {
            return false;
        }
        client.speak(id.clone(), text.clone());
        this.speaking = Some(Speaking { message_id: id, session_id: String::new(), title: text });
        cx.notify();
        true
    });
    spoken
}

/// NEW-67: new voice settings arrived (a heartbeat, or this device's own
/// voice settings save): adopt them and start or stop the voice here. The
/// host reports its readiness again when the voice moved here or away.
pub fn set_settings(settings: etalk::voice::VoiceSettings, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    let Some(host) = host::host(cx) else { return };
    // #NEW-54: push to talk first, so a Listen it drops is cancelled before
    // this may end the worker.
    crate::push_to_talk::settings_changed(&settings, cx);
    // #NEW-63: whose settings these are (the active account's: the
    // heartbeat delivers only its own), taken before the update.
    let data_dir = host.data_dir();
    let names_here = settings_name_this_device(&settings, &host.device_id());
    let accounts = host.accounts(cx);
    let signed_in = accounts.all;
    let account = accounts.active;
    entity.update(cx, |this, cx| {
        if !this.settings_known {
            this.settings_known = true;
            cx.notify();
        }
        if this.settings == settings {
            return;
        }
        this.settings = settings;
        this.on_settings(cx);
        // The voice tray follows the settings.
        cx.notify();
    });
    // NEW-55: a call that can no longer run here ends.
    crate::voice_call::settings_changed(cx);
    // #NEW-63: no account here has this device as its voice device any
    // more: its speech data goes (on the worker's lock; a running worker
    // defers it to a later beat).
    if let Some(account) = account {
        entity.update(cx, |this, _| {
            let (due, changed) = this.cleanup.take(&account, names_here, &signed_in);
            if changed {
                this.cleanup.save(&data_dir);
            }
            if due && this.client.as_ref().is_some_and(etalk::SpeechClient::remove_voice_data) {
                this.cleanup.pending = false;
            }
        });
    }
}

/// #NEW-63: delivered settings that pick this device as the voice device.
fn settings_name_this_device(settings: &etalk::voice::VoiceSettings, own_device_id: &str) -> bool {
    settings.voice_device_id.as_deref() == Some(own_device_id)
}

/// The account signed in and synced now (None = signed out).
pub fn session_account(cx: &App) -> Option<String> {
    host::account(cx)
}

/// The host's messages changed.
pub(crate) fn on_messages(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_messages(cx));
    }
}

/// The signed-in account changed.
pub(crate) fn on_session(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_session(cx));
    }
}

impl SpeakReplies {
    /// NEW-55: the doctor's `speech` row (None = no row: not the voice
    /// device, Speak replies off, or the worker has not answered yet).
    fn speech_state(&self, cx: &App) -> Option<SpeechState> {
        speech_row(self.own_voice(cx).speaks(), &self.download, self.voices_known, self.voices.is_empty())
    }

    /// THIS device's share of the member's voice settings. A method, not a
    /// read of the global: every caller already runs inside this entity's
    /// update, where a read panics.
    fn own_voice(&self, cx: &App) -> DeviceVoice {
        let device_id = host::device_id(cx).unwrap_or_default();
        voice_here(&self.settings, &device_id)
    }

    fn on_session(&mut self, cx: &mut Context<Self>) {
        let account = session_account(cx);
        if account == self.account {
            return;
        }
        self.account = account;
        // #NEW-54 (carry a): a Listen in flight is cancelled first.
        crate::push_to_talk::signed_out(cx);
        // NEW-55: and a call ends (deferred: ending it stops this speaker).
        cx.defer(crate::voice_call::end);
        // Another account (or none): silence, end the worker, forget the
        // history; the new account's settings prepare it again.
        if let Some(client) = self.client.as_ref() {
            client.shutdown();
        }
        self.worker_ready = false;
        self.gate = Gate::default();
        self.prepared = DeviceVoice::default();
        // The settings are the member's: the new account's beat brings its own.
        self.settings = etalk::voice::VoiceSettings::default();
        self.settings_known = false;
        self.crashes = 0;
        self.voices.clear();
        self.voices_known = false;
        self.speaking = None;
        self.download = Download::Idle;
        cx.notify();
        self.on_settings(cx);
    }

    fn on_settings(&mut self, cx: &mut Context<Self>) {
        let voice = self.own_voice(cx);
        let wanted = if voice.active() { voice } else { DeviceVoice::default() };
        if wanted == self.prepared {
            return;
        }
        let was_active = self.prepared.active();
        let was_speaking = self.prepared.speaks();
        let voice_changed = self.prepared.voice != wanted.voice;
        let agent_changed = self.prepared.agent_id != wanted.agent_id;
        self.prepared = wanted.clone();
        // A new setting is a fresh start, a given-up voice included.
        self.crashes = 0;
        if self.download == Download::Unavailable {
            self.download = Download::Idle;
        }
        let Some(client) = self.client.as_ref() else { return };
        if wanted.active() {
            // The assistant or the voice changed: what was said stops.
            if (agent_changed || voice_changed) && self.speaking.take().is_some() {
                client.stop();
            }
            if wanted.speaks() {
                if !was_speaking || voice_changed {
                    client.prepare(wanted.voice.clone());
                }
            } else {
                // #NEW-54: push to talk alone: the worker stays up (its
                // Listen loads the transcriber), the voice is never fetched.
                if was_speaking && self.speaking.take().is_some() {
                    client.stop();
                }
                if !was_active {
                    client.start();
                }
            }
        } else {
            client.shutdown();
            // #NEW-63: the worker is gone; a download it was killed in
            // leaves a `.part` (up to 1.8 GB) that nothing else frees. On
            // the worker's lock: a worker started meanwhile keeps its own.
            let sweeper = client.clone();
            std::thread::spawn(move || {
                let removed = sweeper.remove_partial_downloads();
                if removed > 0 {
                    log::info!("[ui] speak replies: removed {removed} partial voice download(s)");
                }
            });
            self.worker_ready = false;
            self.voices.clear();
            self.voices_known = false;
            self.speaking = None;
            self.download = Download::Idle;
        }
        cx.notify();
    }

    fn on_messages(&mut self, cx: &mut Context<Self>) {
        let Some(host) = host::host(cx) else { return };
        let fresh = self.gate.observe(|visit| host.messages(cx, visit));
        if fresh.is_empty() {
            return;
        }
        if self.download == Download::Unavailable {
            return;
        }
        let voice = self.own_voice(cx);
        // NEW-55: while a call runs, ITS agent's conversation is spoken.
        let call_agent = crate::voice_call::agent_id(cx);
        let Some(agent_id) = call_agent.as_deref().or(voice.agent_id.as_deref()).filter(|_| voice.speaks()) else {
            return;
        };
        let Some(conversation) = host::conversation(agent_id, cx) else { return };
        // S-3: only a voice turn's reply: the call's conversation, or the
        // conversation a push-to-talk / wake turn went to.
        if self.voice_turn.as_ref().is_some_and(|turn| turn.at.elapsed() > VOICE_TURN_TTL) {
            self.voice_turn = None;
        }
        let turn = self.voice_turn.as_ref().is_some_and(|turn| turn.session_id == conversation);
        let voice_turn = call_agent.is_some() || turn;
        let voices: Vec<&str> = self.voices.iter().map(String::as_str).collect();
        let context = VoiceContext {
            agent_session_id: Some(conversation.as_str()),
            is_voice_device: true,
            voice_turn,
            language: self.settings.language(),
            voices: &voices,
            armed_at: self.gate.armed_at(),
            // #NEW-57: nothing is read aloud mid-meeting.
            meeting: self.settings.meeting(),
        };
        // A burst (a reconnect) speaks only its newest reply.
        let Some(row) = fresh
            .iter()
            .filter(|row| etalk::voice::should_speak(&row.message(), &context))
            .max_by_key(|row| row.created_at.as_deref().and_then(etalk::voice::parse_instant).unwrap_or(0))
        else {
            return;
        };
        let spoken = etalk::voice::spoken_text(row.body.as_deref().unwrap_or(""), VOICE.max_chars, self.settings.language());
        if spoken.is_empty() {
            return;
        }
        let Some(client) = self.client.as_ref() else { return };
        client.speak(row.id.clone(), spoken.clone());
        // The turn got its reply.
        if turn {
            self.voice_turn = None;
        }
        self.speaking = Some(Speaking {
            message_id: row.id.clone(),
            session_id: conversation.clone(),
            title: etalk::voice::first_sentence(&spoken).to_string(),
        });
        cx.notify();
    }

    fn on_event(&mut self, event: etalk::Event, cx: &mut Context<Self>) {
        let download_before = self.download.clone();
        let speaking_before = self.speaking.clone();
        let ready_before = (self.worker_ready, self.voices_known, self.voices.clone());
        let speech_before = self.speech_state(cx);
        match event {
            etalk::Event::Downloading { received, total } => self.download = Download::Running { received, total },
            etalk::Event::ModelReady => self.download = Download::Idle,
            etalk::Event::Voices { voices } => {
                if self.download != Download::Failed {
                    self.download = Download::Idle;
                }
                if voices.is_empty() && self.voices != voices {
                    log::info!("[ui] speak replies: no usable GPU here; replies are shown, never spoken");
                }
                self.voices = voices;
                self.voices_known = true;
            }
            etalk::Event::DownloadFailed { error } => {
                log::warn!("[ui] speak replies: voice download failed: {error}");
                if self.download != Download::Failed {
                    host::notify(Notice::Error(VOICE.copy.download_failed.clone()), cx);
                }
                self.download = Download::Failed;
                self.speaking = None;
            }
            etalk::Event::Finished { id, stopped } => {
                if !stopped {
                    // A whole utterance played: the worker is healthy.
                    self.crashes = 0;
                }
                if self.speaking.as_ref().is_some_and(|speaking| speaking.message_id == id) {
                    self.speaking = None;
                }
            }
            etalk::Event::Failed { id, error } => {
                log::warn!("[ui] speak replies: the reply was not spoken: {error}");
                if self.speaking.as_ref().is_some_and(|speaking| speaking.message_id == id) {
                    self.speaking = None;
                }
            }
            etalk::Event::Exited => {
                self.on_exited(cx);
                // #NEW-56: a new worker is told the wake word again.
                crate::push_to_talk::worker_changed(false, cx);
            }
            etalk::Event::Ready { .. } => {
                self.worker_ready = true;
                crate::push_to_talk::worker_changed(true, cx);
                // NEW-57: a new worker is handed the running meeting (it
                // reads these settings: after this update).
                cx.defer(crate::meeting::worker_ready);
            }
            etalk::Event::Started { .. } => {}
            // #NEW-54: routed to push to talk before they get here.
            etalk::Event::Level { .. }
            | etalk::Event::Partial { .. }
            | etalk::Event::Heard { .. }
            | etalk::Event::Empty { .. }
            | etalk::Event::ListenFailed { .. }
            | etalk::Event::Woke { .. }
            | etalk::Event::WakeListening { .. } => {}
            // NEW-55: call events (talk protocol 5) belong to the call.
            _ => {}
        }
        // NEW-55: the doctor's speech row follows (callers enable the
        // telephone from it), now rather than a beat later.
        if self.speech_state(cx) != speech_before {
            cx.defer(host::readiness_changed);
        }
        // Settings → Voice draws the download, the Dock the Speaking banner:
        // the windows repaint only when one of them changed.
        if self.download != download_before || self.speaking != speaking_before {
            cx.refresh_windows();
        }
        if self.download != download_before || self.speaking != speaking_before || (self.worker_ready, self.voices_known, self.voices.clone()) != ready_before {
            cx.notify();
        }
    }

    /// The worker ended. A shutdown we asked for (setting off, sign-out)
    /// needs nothing; a crash with the setting on restarts it with backoff
    /// (2, 4, 8 s) so the voice stays resident, and the third crash in a
    /// row gives up until the setting changes.
    fn on_exited(&mut self, cx: &mut Context<Self>) {
        self.speaking = None;
        self.worker_ready = false;
        // A new worker reports its voices again.
        self.voices.clear();
        self.voices_known = false;
        if matches!(self.download, Download::Running { .. }) {
            self.download = Download::Idle;
        }
        if !self.prepared.active() || self.download == Download::Unavailable {
            return;
        }
        self.crashes += 1;
        if self.crashes >= MAX_CRASHES {
            log::warn!("[ui] speak replies: the speech worker crashed {} times in a row; giving up", self.crashes);
            self.download = Download::Unavailable;
            if let Some(client) = self.client.as_ref() {
                client.shutdown();
            }
            host::notify(Notice::Error(VOICE.copy.unavailable.clone()), cx);
            return;
        }
        let delay = restart_delay(self.crashes);
        let prepared = self.prepared.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, _| {
                // Still the same setting and not given up: bring it back.
                if this.prepared == prepared && this.download != Download::Unavailable {
                    if let Some(client) = this.client.as_ref() {
                        if prepared.speaks() {
                            client.prepare(prepared.voice.clone());
                        } else {
                            client.start();
                        }
                    }
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_voice_device_with_push_to_talk_listens() {
        let settings = etalk::voice::VoiceSettings {
            voice_device_id: Some("mac".into()),
            agent_id: Some("a1".into()),
            ptt_shortcut: "ctrl+space".into(),
            ..Default::default()
        };
        assert!(listens_here(&settings, "mac"));
        assert!(!listens_here(&settings, "pc"));
        let off = etalk::voice::VoiceSettings { ptt_shortcut: String::new(), ..settings.clone() };
        assert!(!listens_here(&off, "mac"));
        // #NEW-56: the wake word alone listens too.
        let wake = etalk::voice::VoiceSettings { wake_word: true, ..off.clone() };
        assert!(listens_here(&wake, "mac"));
        assert!(voice_here(&wake, "mac").listen);
        // S-3: speak-back is always on: the voice device keeps the worker
        // without a key or the wake word too.
        assert!(voice_here(&off, "mac").active() && !voice_here(&off, "mac").listen);
        // clearOnNone (S-3: only the wake word): no agent picked = the wake
        // word does not listen.
        let no_agent = etalk::voice::VoiceSettings { agent_id: None, ..wake };
        assert!(!listens_here(&no_agent, "mac"));
    }

    fn message(id: &str, created: &str) -> Row {
        Row { id: id.into(), created_at: Some(created.into()), ..Row::default() }
    }

    /// The feed of `rows`, `ready` or not.
    fn feed(rows: &[Row], ready: bool) -> impl FnOnce(&mut dyn FnMut(&TalkMessage<'_>)) -> bool + '_ {
        move |visit| {
            for row in rows {
                visit(&TalkMessage { id: &row.id, message: row.message() });
            }
            ready
        }
    }

    #[test]
    fn the_gate_treats_the_first_ready_batch_as_history() {
        let mut gate = Gate::default();
        let old = [message("m1", "2026-10-25 10:00:00+00"), message("m2", "2026-10-25 10:00:05+00")];
        assert!(gate.observe(feed(&old, false)).is_empty());
        assert!(gate.observe(feed(&old, true)).is_empty());
        assert_eq!(gate.armed_at(), "2026-10-25 10:00:05+00");
        let next = [old[0].clone(), old[1].clone(), message("m3", "2026-10-25 10:01:00+00")];
        let fresh = gate.observe(feed(&next, true));
        assert_eq!(fresh.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["m3"]);
        assert!(gate.observe(feed(&next, true)).is_empty(), "each row once");
        // Not ready: the gate starts over (the next ready batch is history).
        assert!(gate.observe(feed(&next, false)).is_empty());
        assert!(gate.observe(feed(&next, true)).is_empty());
    }

    #[test]
    fn the_speech_row_names_a_dead_voice() {
        let running = Download::Running { received: 1, total: 2 };
        assert_eq!(speech_row(false, &Download::Unavailable, false, true), None, "not speaking here: no row");
        assert_eq!(speech_row(true, &Download::Idle, false, true), None, "not answered yet");
        assert_eq!(speech_row(true, &running, false, true), Some(SpeechState::Downloading));
        assert_eq!(speech_row(true, &Download::Unavailable, false, true), Some(SpeechState::Failed), "crashed MAX_CRASHES times");
        assert_eq!(speech_row(true, &Download::Failed, false, true), Some(SpeechState::Failed), "failed before any voice");
        assert_eq!(speech_row(true, &Download::Failed, true, false), Some(SpeechState::Ready), "a known voice still speaks");
        assert_eq!(speech_row(true, &Download::Idle, true, true), Some(SpeechState::NoGpu));
        assert_eq!(speech_row(true, &Download::Idle, true, false), Some(SpeechState::Ready));
        let states = [SpeechState::Ready, SpeechState::Downloading, SpeechState::NoGpu, SpeechState::Failed];
        assert_eq!(states.map(SpeechState::doctor_state), ["ok", "off", "missing", "error"]);
    }

    #[test]
    fn restarts_back_off_and_cap() {
        assert_eq!(restart_delay(1), Duration::from_secs(2));
        assert_eq!(restart_delay(2), Duration::from_secs(4));
        assert_eq!(restart_delay(3), Duration::from_secs(8));
        assert_eq!(restart_delay(9), Duration::from_secs(8));
        assert_eq!(MAX_CRASHES, 3);
    }

    #[test]
    fn an_empty_collection_arms_at_the_epoch() {
        let mut gate = Gate::default();
        assert!(gate.observe(feed(&[], true)).is_empty());
        assert_eq!(gate.armed_at(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn only_settings_naming_this_device_claim_its_speech_data() {
        let mine = etalk::voice::VoiceSettings { voice_device_id: Some("d-mint".into()), ..Default::default() };
        assert!(settings_name_this_device(&mine, "d-mint"), "the voice device keeps its data");
        assert!(!settings_name_this_device(&mine, "d-mac"));
        assert!(!settings_name_this_device(&etalk::voice::VoiceSettings::default(), "d-mint"));
    }

    fn signed_in(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn an_account_switch_never_frees_another_accounts_speech_data() {
        let both = signed_in(&["a", "b"]);
        let mut cleanup = Cleanup::default();
        assert_eq!(cleanup.take("a", true, &both), (false, true), "A's voice device is here");
        assert_eq!(cleanup.take("a", true, &both), (false, false), "every beat again: nothing to save");
        // Switch to B, whose voice device is elsewhere (or none).
        assert_eq!(cleanup.take("b", false, &both), (false, true), "B's first settings never free A's data");
        assert_eq!(cleanup.take("b", false, &both).0, false);
        // B had picked this device and moves away: A still claims it.
        let mut cleanup = Cleanup::default();
        cleanup.take("a", true, &both);
        cleanup.take("b", true, &both);
        assert!(!cleanup.take("b", false, &both).0, "A's claim keeps the data");
        // A signs out: its claim no longer counts, B's transition is pending.
        assert!(cleanup.take("b", false, &signed_in(&["b"])).0);
    }

    #[test]
    fn the_same_account_moving_away_frees_the_speech_data() {
        let only = signed_in(&["a"]);
        let mut cleanup = Cleanup::default();
        cleanup.take("a", true, &only);
        assert!(cleanup.take("a", false, &only).0, "a confirmed transition for the same account");
        // A worker still ran: it stays pending for the next beat.
        assert!(cleanup.take("a", false, &only).0);
        cleanup.pending = false;
        assert!(!cleanup.take("a", false, &only).0, "once gone, later beats leave it");
        // Settings that never named this device: no transition.
        assert!(!Cleanup::default().take("a", false, &only).0);
        // Picked here again before it went: nothing goes.
        let mut cleanup = Cleanup::default();
        cleanup.take("a", true, &only);
        cleanup.take("a", false, &only);
        assert!(!cleanup.take("a", true, &only).0);
    }

    #[test]
    fn the_first_settings_after_an_account_change_never_free_it() {
        let both = signed_in(&["a", "b"]);
        // B named this device in an earlier stint (persisted), A was active.
        let mut cleanup = Cleanup::default();
        cleanup.take("b", true, &both);
        cleanup.take("a", false, &both);
        assert!(!cleanup.take("b", false, &both).0, "B's first settings after the switch");
        // Across a restart (no account taken yet) the persisted claim is A's own.
        let mut restarted = Cleanup { claims: [("a".to_string(), true)].into(), ..Cleanup::default() };
        assert!(restarted.take("a", false, &signed_in(&["a"])).0);
    }

    #[test]
    fn only_the_voice_device_speaks() {
        let settings = etalk::voice::VoiceSettings {
            voice_device_id: Some("d-mint".into()),
            agent_id: Some("a1".into()),
            ..Default::default()
        };
        assert!(voice_here(&settings, "d-mint").active());
        assert!(!voice_here(&settings, "d-mac").active(), "another device stays silent");
        let german = etalk::voice::VoiceSettings {
            language: "de".into(),
            voice_file: "/home/me/clip.wav".into(),
            voice_transcript: "Hallo.".into(),
            ..settings.clone()
        };
        let here = voice_here(&german, "d-mint");
        assert!(here.active(), "every language prepares the worker; it reports what it speaks");
        assert_eq!(
            here.voice,
            etalk::VoiceChoice { language: "de".into(), file: "/home/me/clip.wav".into(), transcript: "Hallo.".into() }
        );
        assert_eq!(voice_here(&settings, "d-mint").voice.language, "en", "unset reads as English");
        let no_agent = etalk::voice::VoiceSettings { agent_id: None, ..settings.clone() };
        assert!(!voice_here(&no_agent, "d-mint").active(), "no agent = nothing speaks");
        // S-3: speak-back is always on: the worker prepares the voice.
        assert!(voice_here(&settings, "d-mint").speaks());
    }

}
