//! NEW-53: exponentialTalk "Speak replies" in the IDE (`fixtures/voice.json`).
//!
//! Watches the synced `messages`: a row [`domain::voice::should_speak`]
//! accepts (the assistant's final reply in the member's conversation with
//! the agent picked as Assistant on THIS device, `launch_defaults.voice`) is
//! reduced to [`domain::voice::spoken_text`] and handed to the speech worker
//! (`talk::SpeechClient`, the IDE's own executable as `__speech-worker`).
//! While it plays, the conversation's Dock shows the Speaking banner with
//! Stop (`steer_viewer::dock_banners`). The worker is started and the voice
//! fetched as soon as the setting turns on, so the first reply does not wait
//! for a 350 MB download; it is shut down when the setting turns off.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};
use sync::{Collection, Store};

use domain::rows::{DeviceRow, MessageRow};
use domain::voice::{VoiceContext, VoiceMessage, VOICE};

/// This device's voice settings, off `launch_defaults.voice` of its row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeviceVoice {
    pub assistant_agent_id: Option<String>,
    pub speak_replies: bool,
}

impl DeviceVoice {
    /// Speaking is on: an assistant is picked and Speak replies is on.
    pub(crate) fn active(&self) -> bool {
        self.speak_replies && self.assistant_agent_id.is_some()
    }
}

/// Read `voice` off a device row's `launch_defaults` (an object, or the
/// JSON string some shapes deliver).
pub(crate) fn device_voice(row: &DeviceRow) -> DeviceVoice {
    let parsed = match row.launch_defaults.as_ref() {
        Some(serde_json::Value::String(raw)) => serde_json::from_str::<serde_json::Value>(raw).ok(),
        Some(value) => Some(value.clone()),
        None => None,
    };
    let voice = parsed.as_ref().and_then(|value| value.get("voice"));
    DeviceVoice {
        assistant_agent_id: voice
            .and_then(|voice| voice.get("assistantAgentId"))
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        speak_replies: voice.and_then(|voice| voice.get("speakReplies")).and_then(|on| on.as_bool()).unwrap_or(false),
    }
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
    Running,
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
    /// The signed-in account the worker serves (`None` = signed out).
    account: Option<String>,
    /// Worker crashes since the last utterance that played to its end.
    crashes: u32,
    pub speaking: Option<Speaking>,
    pub download: Download,
}

struct SpeakRepliesGlobal(Entity<SpeakReplies>);

impl Global for SpeakRepliesGlobal {}

/// Install the watcher ONCE, after `Store::open` and the `AuthContext`.
pub fn install(cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let collections = store.collections();
    let messages = collections.messages.clone();
    let devices = collections.devices.clone();
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
        cx.observe(&devices, |this: &mut SpeakReplies, _, cx| this.on_settings(cx)).detach();
        // A sign-out or another account ends the voice (and the worker).
        cx.observe(&state, |this: &mut SpeakReplies, _, cx| this.on_session(cx)).detach();
        SpeakReplies {
            gate: Gate::default(),
            client,
            prepared: DeviceVoice::default(),
            account: None,
            crashes: 0,
            speaking: None,
            download: Download::Idle,
        }
    });
    cx.set_global(SpeakRepliesGlobal(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            let _ = cx.update(|cx| {
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

/// The reply being spoken, if any.
pub(crate) fn speaking(cx: &App) -> Option<Speaking> {
    global(cx).and_then(|entity| entity.read(cx).speaking.clone())
}

/// The voice download state (the Speak replies row's detail).
pub(crate) fn download(cx: &App) -> Download {
    global(cx).map(|entity| entity.read(cx).download.clone()).unwrap_or_default()
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

/// THIS device's voice settings (the GUI's own row).
pub(crate) fn own_voice(cx: &App) -> DeviceVoice {
    let Some(store) = Store::try_global(cx) else { return DeviceVoice::default() };
    let device_id = steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
    store
        .collections()
        .devices
        .read(cx)
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id.as_str()))
        .map(device_voice)
        .unwrap_or_default()
}

fn session_account(cx: &App) -> Option<String> {
    match Store::try_global(cx)?.session(cx) {
        sync::SessionPhase::Synced { account_id } => Some(account_id),
        _ => None,
    }
}

impl SpeakReplies {
    fn on_session(&mut self, cx: &mut Context<Self>) {
        let account = session_account(cx);
        if account == self.account {
            return;
        }
        self.account = account;
        // Another account (or none): silence, end the worker, forget the
        // history; the new account's settings prepare it again.
        if let Some(client) = self.client.as_ref() {
            client.shutdown();
        }
        self.gate = Gate::default();
        self.prepared = DeviceVoice::default();
        self.crashes = 0;
        self.speaking = None;
        self.download = Download::Idle;
        cx.notify();
        self.on_settings(cx);
    }

    fn on_settings(&mut self, cx: &mut Context<Self>) {
        let voice = own_voice(cx);
        let wanted = if voice.active() { voice } else { DeviceVoice::default() };
        if wanted == self.prepared {
            return;
        }
        let was_active = self.prepared.active();
        self.prepared = wanted.clone();
        // A new setting is a fresh start, a given-up voice included.
        self.crashes = 0;
        if self.download == Download::Unavailable {
            self.download = Download::Idle;
        }
        let Some(client) = self.client.as_ref() else { return };
        if wanted.active() {
            // The assistant changed: what the old one said stops.
            if self.speaking.take().is_some() {
                client.stop();
            }
            if !was_active {
                client.prepare();
            }
        } else {
            client.shutdown();
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
        let voice = own_voice(cx);
        let Some(agent_id) = voice.assistant_agent_id.as_deref().filter(|_| voice.speak_replies) else {
            return;
        };
        let Some(conversation) = crate::session_data::my_conversation(agent_id, cx) else { return };
        let Some(store) = Store::try_global(cx) else { return };
        let sessions = domain::message_conversation::conversation_sessions(store.collections().conversations.read(cx).iter());
        let context = VoiceContext {
            assistant_session_id: Some(conversation.id.as_str()),
            speak_replies: true,
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
        let spoken = domain::voice::spoken_text(row.body.as_deref().unwrap_or(""), VOICE.max_chars);
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
        match event {
            talk::Event::Downloading { .. } => self.download = Download::Running,
            talk::Event::ModelReady => self.download = Download::Idle,
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
            talk::Event::Failed { id, .. } => {
                if self.speaking.as_ref().is_some_and(|speaking| speaking.message_id == id) {
                    self.speaking = None;
                }
            }
            talk::Event::Exited => self.on_exited(cx),
            talk::Event::Ready { .. } | talk::Event::Started { .. } => {}
        }
        cx.notify();
    }

    /// The worker ended. A shutdown we asked for (setting off, sign-out)
    /// needs nothing; a crash with the setting on restarts it with backoff
    /// (2, 4, 8 s) so the voice stays resident, and the third crash in a
    /// row gives up until the setting changes.
    fn on_exited(&mut self, cx: &mut Context<Self>) {
        self.speaking = None;
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
                        client.prepare();
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
    fn device_voice_reads_objects_and_json_strings() {
        let mut row: DeviceRow = serde_json::from_value(serde_json::json!({"id": "d1"})).unwrap();
        assert_eq!(device_voice(&row), DeviceVoice::default());
        row.launch_defaults = Some(serde_json::json!({"voice": {"assistantAgentId": "a1", "speakReplies": true}}));
        assert!(device_voice(&row).active());
        row.launch_defaults = Some(serde_json::Value::String(r#"{"voice":{"speakReplies":true}}"#.into()));
        let voice = device_voice(&row);
        assert!(voice.speak_replies && !voice.active(), "no assistant = not active");
    }
}
