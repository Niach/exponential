//! NEW-53: exponentialTalk "Speak replies" in the IDE (`fixtures/voice.json`).
//!
//! Watches the synced `messages`: a row [`domain::voice::should_speak`]
//! accepts (the agent's final reply in the member's conversation with the
//! agent picked in Settings → Voice, spoken only on the member's voice
//! device; `user_voice_settings`, delivered by every heartbeat) is
//! reduced to [`domain::voice::spoken_text`] and handed to the speech worker
//! (`talk::SpeechClient`, the IDE's own executable as `__speech-worker`).
//! While it plays, the conversation's Dock shows the Speaking banner with
//! Stop (`steer_viewer::dock_banners`). The worker is started and the voice
//! fetched as soon as the setting turns on, so the first reply does not wait
//! for the download (~2-3 GB: the CrispASR runtime and Qwen3-TTS, NEW-74);
//! it is shut down when the setting turns off. NEW-74: which languages are
//! spoken is what the worker reports (`talk::Event::Voices`): none without a
//! usable GPU, so a reply there is shown, never spoken.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};
use sync::{Collection, Store};

use domain::rows::MessageRow;
use domain::voice::{VoiceContext, VoiceMessage, VOICE};

/// What THIS device speaks for: the member's voice settings
/// (`user_voice_settings`, delivered by every heartbeat) seen from here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeviceVoice {
    pub agent_id: Option<String>,
    pub speak_replies: bool,
    /// #NEW-54: push to talk listens here (the worker stays resident for it).
    pub listen: bool,
    /// NEW-74: the language and voice file the worker speaks with.
    pub voice: talk::VoiceChoice,
}

impl DeviceVoice {
    /// The worker runs here: an agent is picked and Speak replies or (#NEW-54)
    /// push to talk is on.
    pub(crate) fn active(&self) -> bool {
        (self.speak_replies || self.listen) && self.agent_id.is_some()
    }

    /// The voice (the TTS model) is wanted: Speak replies is on.
    fn speaks(&self) -> bool {
        self.speak_replies && self.agent_id.is_some()
    }
}

/// voice.json: only the member's voice device speaks.
pub(crate) fn voice_here(settings: &domain::voice::VoiceSettings, own_device_id: &str) -> DeviceVoice {
    if settings.voice_device_id.as_deref() != Some(own_device_id) {
        return DeviceVoice::default();
    }
    let voice = talk::VoiceChoice {
        language: settings.language().to_string(),
        file: settings.voice_file.clone(),
        transcript: settings.voice_transcript.clone(),
    };
    let settings = settings.clone().clear_on_none();
    // NEW-74: the worker is prepared for every language; whether it speaks
    // is what it reports back (`voices`, empty without a usable GPU).
    DeviceVoice { agent_id: settings.agent_id, speak_replies: settings.speak_replies, listen: settings.push_to_talk, voice }
}

/// NEW-78: THIS device listens: it is the member's voice device and Push to
/// talk is on (after voice.json `clearOnNone`). Only then does its doctor
/// carry the `microphone` row.
pub(crate) fn listens_here(settings: &domain::voice::VoiceSettings, own_device_id: &str) -> bool {
    let settings = settings.clone().clear_on_none();
    settings.push_to_talk && settings.voice_device_id.as_deref() == Some(own_device_id)
}

/// [`listens_here`] for the member's current settings (false before the
/// first heartbeat delivered them).
pub(crate) fn push_to_talk_here(cx: &App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let Some(auth) = cx.try_global::<crate::session::AuthContext>() else {
        return false;
    };
    listens_here(&entity.read(cx).settings, &steer::persistent_device_id(&auth.data_dir))
}

/// The reply being spoken (for the Dock banner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Speaking {
    pub message_id: String,
    pub session_id: String,
    /// The spoken text's first sentence (the banner's title).
    pub title: String,
}

/// The voice download, for the Speak replies row's trailing detail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Download {
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
    fn observe<'a>(&mut self, ready: bool, rows: impl Iterator<Item = &'a MessageRow>) -> Vec<MessageRow> {
        if !ready {
            *self = Gate::default();
            return Vec::new();
        }
        if !self.armed {
            self.armed = true;
            let mut newest: Option<(i64, String)> = None;
            for row in rows {
                self.seen.insert(row.id.clone());
                if let Some(created) = row.created_at.as_deref() {
                    if let Some(ms) = domain::voice::parse_instant(created) {
                        if newest.as_ref().is_none_or(|(mark, _)| ms > *mark) {
                            newest = Some((ms, created.to_string()));
                        }
                    }
                }
            }
            self.watermark = newest.map(|(_, text)| text);
            return Vec::new();
        }
        rows.filter(|row| self.seen.insert(row.id.clone())).cloned().collect()
    }

    /// The arm time `should_speak` compares against: the watermark, or the
    /// epoch when the collection was empty (every later row is new).
    fn armed_at(&self) -> &str {
        self.watermark.as_deref().unwrap_or("1970-01-01T00:00:00Z")
    }
}

pub(crate) struct SpeakReplies {
    gate: Gate,
    client: Option<talk::SpeechClient>,
    /// The setting the worker was last prepared for (assistant + switch).
    prepared: DeviceVoice,
    /// The member's voice settings as the last heartbeat delivered them.
    settings: domain::voice::VoiceSettings,
    /// The signed-in account the worker serves (`None` = signed out).
    account: Option<String>,
    /// Worker crashes since the last utterance that played to its end.
    crashes: u32,
    /// voice.json `voices`: the languages the worker reported it speaks
    /// (empty until it answers, and without a usable GPU).
    voices: Vec<String>,
    pub speaking: Option<Speaking>,
    pub download: Download,
    /// #NEW-54: the worker answered `Ready` and has not exited since (the
    /// tray's Mute shows only then).
    pub worker_ready: bool,
}

struct SpeakRepliesGlobal(Entity<SpeakReplies>);

impl Global for SpeakRepliesGlobal {}

/// Install the watcher ONCE, after `Store::open` and the `AuthContext`.
pub fn install(cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let collections = store.collections();
    let messages = collections.messages.clone();
    let (tx, rx) = flume::unbounded::<talk::Event>();
    let client = std::env::current_exe().ok().map(|exe| {
        let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
        talk::SpeechClient::new(exe, data_dir, move |event| {
            let _ = tx.send(event);
        })
    });
    let state = store.state();
    let entity = cx.new(|cx: &mut Context<SpeakReplies>| {
        cx.observe(&messages, |this: &mut SpeakReplies, collection, cx| this.on_messages(&collection, cx))
            .detach();
        // A sign-out or another account ends the voice (and the worker).
        cx.observe(&state, |this: &mut SpeakReplies, _, cx| this.on_session(cx)).detach();
        SpeakReplies {
            gate: Gate::default(),
            client,
            prepared: DeviceVoice::default(),
            settings: domain::voice::VoiceSettings::default(),
            account: None,
            crashes: 0,
            voices: Vec::new(),
            speaking: None,
            download: Download::Idle,
            worker_ready: false,
        }
    });
    cx.set_global(SpeakRepliesGlobal(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            let _ = cx.update(|cx| {
                // #NEW-54: what the microphone heard goes to push to talk.
                if crate::push_to_talk::is_listen_event(&event) {
                    crate::push_to_talk::on_worker_event(event, cx);
                    return;
                }
                entity.update(cx, |this, cx| this.on_event(event, cx));
                cx.refresh_windows();
            });
        }
    })
    .detach();
}

fn global(cx: &App) -> Option<Entity<SpeakReplies>> {
    cx.try_global::<SpeakRepliesGlobal>().map(|global| global.0.clone())
}

/// The watcher itself (the voice tray observes it).
pub(crate) fn entity(cx: &App) -> Option<Entity<SpeakReplies>> {
    global(cx)
}

/// #NEW-68: the member's voice settings as the last heartbeat (or this
/// device's own save) delivered them.
pub(crate) fn settings(cx: &App) -> domain::voice::VoiceSettings {
    global(cx).map(|entity| entity.read(cx).settings.clone()).unwrap_or_default()
}

/// The reply being spoken, if any.
pub(crate) fn speaking(cx: &App) -> Option<Speaking> {
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
    let spoken = domain::voice::spoken_text(body, VOICE.max_chars, language);
    Some(Speaking {
        message_id: "dev-speaking".into(),
        session_id: session_id.to_string(),
        title: domain::voice::first_sentence(&spoken).to_string(),
    })
});

/// #NEW-54: the speech worker, for push to talk's Listen / EndListen /
/// CancelListen (one worker serves both).
pub(crate) fn client(cx: &App) -> Option<talk::SpeechClient> {
    global(cx).and_then(|entity| entity.read(cx).client.clone())
}

/// #NEW-54: the worker is up (the tray's `micAvailable`).
pub(crate) fn worker_ready(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).worker_ready)
}

/// The voice download state (the Speak replies row's detail).
pub(crate) fn download(cx: &App) -> Download {
    global(cx).map(|entity| entity.read(cx).download.clone()).unwrap_or_default()
}

/// The Speak replies row's trailing detail while the voice downloads:
/// voice.json `downloadProgress`, `None` when nothing downloads.
pub(crate) fn download_detail(cx: &App) -> Option<String> {
    match download(cx) {
        Download::Running { received, total } => Some(domain::voice::download_progress(received, total)),
        _ => None,
    }
}

/// The banner's Stop.
pub(crate) fn stop(cx: &mut App) {
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

/// NEW-67: new voice settings arrived (a heartbeat, or this device's own
/// Settings → Voice save): adopt them and start or stop the voice here.
pub(crate) fn set_settings(settings: domain::voice::VoiceSettings, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    let listened = push_to_talk_here(cx);
    // #NEW-54: push to talk first, so a Listen it drops is cancelled before
    // this may end the worker.
    crate::push_to_talk::settings_changed(&settings, cx);
    entity.update(cx, |this, cx| {
        if this.settings == settings {
            return;
        }
        this.settings = settings;
        this.on_settings(cx);
        // The voice tray follows the settings.
        cx.notify();
    });
    // NEW-78: Push to talk moved here or away — the doctor's microphone row
    // follows now (the refresh nudges the beat), not a beat later.
    if push_to_talk_here(cx) != listened {
        if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
            crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
        }
    }
}

fn session_account(cx: &App) -> Option<String> {
    match Store::try_global(cx)?.session(cx) {
        sync::SessionPhase::Synced { account_id } => Some(account_id),
        _ => None,
    }
}

impl SpeakReplies {
    /// THIS device's share of the member's voice settings. A method, not a
    /// read of the global: every caller already runs inside this entity's
    /// update, where a read panics.
    fn own_voice(&self, cx: &App) -> DeviceVoice {
        let device_id = steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
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
        // Another account (or none): silence, end the worker, forget the
        // history; the new account's settings prepare it again.
        if let Some(client) = self.client.as_ref() {
            client.shutdown();
        }
        self.worker_ready = false;
        self.gate = Gate::default();
        self.prepared = DeviceVoice::default();
        // The settings are the member's: the new account's beat brings its own.
        self.settings = domain::voice::VoiceSettings::default();
        self.crashes = 0;
        self.voices.clear();
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
            self.worker_ready = false;
            self.voices.clear();
            self.speaking = None;
            self.download = Download::Idle;
        }
        cx.notify();
    }

    fn on_messages(&mut self, collection: &Entity<Collection<MessageRow>>, cx: &mut Context<Self>) {
        let fresh = {
            let collection = collection.read(cx);
            self.gate.observe(collection.is_ready(), collection.iter())
        };
        if fresh.is_empty() {
            return;
        }
        if self.download == Download::Unavailable {
            return;
        }
        let voice = self.own_voice(cx);
        let Some(agent_id) = voice.agent_id.as_deref().filter(|_| voice.speak_replies) else {
            return;
        };
        let Some(conversation) = crate::session_data::my_conversation(agent_id, cx) else { return };
        let Some(store) = Store::try_global(cx) else { return };
        let sessions = domain::message_conversation::conversation_sessions(store.collections().conversations.read(cx).iter());
        let voices: Vec<&str> = self.voices.iter().map(String::as_str).collect();
        let context = VoiceContext {
            agent_session_id: Some(conversation.id.as_str()),
            is_voice_device: true,
            speak_replies: true,
            language: self.settings.language(),
            voices: &voices,
            armed_at: self.gate.armed_at(),
        };
        // A burst (a reconnect) speaks only its newest reply.
        let Some(row) = fresh
            .iter()
            .filter(|row| {
                let session = domain::message_conversation::session_of(row, &sessions);
                domain::voice::should_speak(
                    &VoiceMessage {
                        author_kind: row.author_kind.as_deref(),
                        body: row.body.as_deref(),
                        has_card: row.card.as_ref().is_some_and(|card| !card.is_null()),
                        author_session_id: row.author_session_id.as_deref(),
                        session_id: session,
                        created_at: row.created_at.as_deref(),
                    },
                    &context,
                )
            })
            .max_by_key(|row| row.created_at.as_deref().and_then(domain::voice::parse_instant).unwrap_or(0))
        else {
            return;
        };
        let spoken = domain::voice::spoken_text(row.body.as_deref().unwrap_or(""), VOICE.max_chars, self.settings.language());
        if spoken.is_empty() {
            return;
        }
        let Some(client) = self.client.as_ref() else { return };
        client.speak(row.id.clone(), spoken.clone());
        self.speaking = Some(Speaking {
            message_id: row.id.clone(),
            session_id: conversation.id.clone(),
            title: domain::voice::first_sentence(&spoken).to_string(),
        });
        cx.notify();
    }

    fn on_event(&mut self, event: talk::Event, cx: &mut Context<Self>) {
        let download_before = self.download.clone();
        match event {
            talk::Event::Downloading { received, total } => self.download = Download::Running { received, total },
            talk::Event::ModelReady => self.download = Download::Idle,
            talk::Event::Voices { voices } => {
                if self.download != Download::Failed {
                    self.download = Download::Idle;
                }
                if voices.is_empty() && self.voices != voices {
                    log::info!("[ui] speak replies: no usable GPU here; replies are shown, never spoken");
                }
                self.voices = voices;
            }
            talk::Event::DownloadFailed { error } => {
                log::warn!("[ui] speak replies: voice download failed: {error}");
                if self.download != Download::Failed {
                    crate::toast::show_in_active_window(crate::toast::Toast::error(VOICE.copy.download_failed.clone()), cx);
                }
                self.download = Download::Failed;
                self.speaking = None;
            }
            talk::Event::Finished { id, stopped } => {
                if !stopped {
                    // A whole utterance played: the worker is healthy.
                    self.crashes = 0;
                }
                if self.speaking.as_ref().is_some_and(|speaking| speaking.message_id == id) {
                    self.speaking = None;
                }
            }
            talk::Event::Failed { id, error } => {
                log::warn!("[ui] speak replies: the reply was not spoken: {error}");
                if self.speaking.as_ref().is_some_and(|speaking| speaking.message_id == id) {
                    self.speaking = None;
                }
            }
            talk::Event::Exited => self.on_exited(cx),
            talk::Event::Ready { .. } => self.worker_ready = true,
            talk::Event::Started { .. } => {}
            // #NEW-54: routed to push to talk before they get here.
            talk::Event::Level { .. }
            | talk::Event::Partial { .. }
            | talk::Event::Heard { .. }
            | talk::Event::Empty { .. }
            | talk::Event::ListenFailed { .. } => {}
        }
        // Settings → Voice draws the download as the Speak replies detail.
        if self.download != download_before {
            cx.refresh_windows();
        }
        cx.notify();
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
            crate::toast::show_in_active_window(crate::toast::Toast::error(VOICE.copy.unavailable.clone()), cx);
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
        let settings = domain::voice::VoiceSettings {
            voice_device_id: Some("mac".into()),
            agent_id: Some("a1".into()),
            push_to_talk: true,
            ..Default::default()
        };
        assert!(listens_here(&settings, "mac"));
        assert!(!listens_here(&settings, "pc"));
        let off = domain::voice::VoiceSettings { push_to_talk: false, ..settings.clone() };
        assert!(!listens_here(&off, "mac"));
        // clearOnNone: no agent picked = nothing listens.
        let no_agent = domain::voice::VoiceSettings { agent_id: None, ..settings };
        assert!(!listens_here(&no_agent, "mac"));
    }

    fn message(id: &str, created: &str) -> MessageRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "conversation_id": "c1",
            "created_at": created,
        }))
        .unwrap()
    }

    #[test]
    fn the_gate_treats_the_first_ready_batch_as_history() {
        let mut gate = Gate::default();
        let old = [message("m1", "2026-10-25 10:00:00+00"), message("m2", "2026-10-25 10:00:05+00")];
        assert!(gate.observe(false, old.iter()).is_empty());
        assert!(gate.observe(true, old.iter()).is_empty());
        assert_eq!(gate.armed_at(), "2026-10-25 10:00:05+00");
        let next = [old[0].clone(), old[1].clone(), message("m3", "2026-10-25 10:01:00+00")];
        let fresh = gate.observe(true, next.iter());
        assert_eq!(fresh.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["m3"]);
        assert!(gate.observe(true, next.iter()).is_empty(), "each row once");
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
        assert!(gate.observe(true, std::iter::empty()).is_empty());
        assert_eq!(gate.armed_at(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn only_the_voice_device_speaks() {
        let settings = domain::voice::VoiceSettings {
            voice_device_id: Some("d-mint".into()),
            agent_id: Some("a1".into()),
            speak_replies: true,
            ..Default::default()
        };
        assert!(voice_here(&settings, "d-mint").active());
        assert!(!voice_here(&settings, "d-mac").active(), "another device stays silent");
        let german = domain::voice::VoiceSettings {
            language: "de".into(),
            voice_file: "/home/me/clip.wav".into(),
            voice_transcript: "Hallo.".into(),
            ..settings.clone()
        };
        let here = voice_here(&german, "d-mint");
        assert!(here.active(), "every language prepares the worker; it reports what it speaks");
        assert_eq!(
            here.voice,
            talk::VoiceChoice { language: "de".into(), file: "/home/me/clip.wav".into(), transcript: "Hallo.".into() }
        );
        assert_eq!(voice_here(&settings, "d-mint").voice.language, "en", "unset reads as English");
        let no_agent = domain::voice::VoiceSettings { agent_id: None, ..settings.clone() };
        assert!(!voice_here(&no_agent, "d-mint").active(), "no agent = nothing speaks");
        // #NEW-54: push to talk alone keeps the worker, never the voice.
        let listen_only = domain::voice::VoiceSettings { speak_replies: false, push_to_talk: true, ..settings };
        let here = voice_here(&listen_only, "d-mint");
        assert!(here.active() && !here.speaks());
    }

}
