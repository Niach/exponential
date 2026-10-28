//! NEW-57: MEETING MODE in the IDE on the member's voice device
//! (`packages/domain-contract/fixtures/meeting.json`). The host side: it
//! follows the voice settings (`meetingStartedAt`, `desktopAudio`,
//! `desktopAudioApps`) handed over on every heartbeat, asks the speech
//! worker to listen per channel, folds the finalized segments through
//! `domain::meeting::flush_step`, sends each batch to the member's
//! conversation with the voice agent (`sessions.send` with `meeting: true`),
//! keeps the raw segments in the IDE's SQLite for 30 days and owns the pause.
//!
//! The rest of the IDE reads [`MeetingState`] (a gpui Global; observe it
//! with `cx.observe_global::<MeetingState>`) and flips the pause with
//! [`set_paused`].
//!
//! A meeting runs here while THIS device is the voice device, an agent is
//! picked and `meetingStartedAt` is set (`clearOnNone`); anything else, a
//! sign-out included (its settings reset to the defaults), stops it. The
//! meeting's id is its `meetingStartedAt`, so a restarted IDE or worker
//! picks the same meeting up. FLUSH: segments queue; `wake` (the phrase
//! opening a `you` segment), `ptt` (a push-to-talk key-down, which the
//! worker answers `MeetingFlushNow`), `tick` (every second) and `turnDone`
//! (the agent's reply row after the turn's own message, or the turn
//! timeout) step `flush_step`; a send carries every queued segment as
//! `meeting_message`. A failed send puts its segments back at the head of
//! the queue (the next tick retries).
//!
//! STOP (meeting.json FLUSH): a stop by the settings (`voice.stopMeeting`,
//! another voice device or agent) asks the worker to DRAIN (every channel's
//! open segment closed and transcribed, then `MeetingStopped`) and sends
//! everything still queued as ONE final `meeting: true` batch past the
//! in-flight slot; a failed final send leaves it in the local log only. A
//! sign-out or account change sends nothing more ([`stop_drains`]).

mod log;

use std::path::PathBuf;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};
use sync::{Collection, Store};

use domain::meeting::{flush_step, meeting_message, DesktopAudio, DesktopAudioCapability, FlushEvent, FlushState, MeetingSegment, MEETING};
use domain::rows::MessageRow;
use domain::voice::{UtteranceTarget, VoiceSettings};

use self::log::{LoggedSegment, SegmentLog};

/// What the tray, the rail, Settings → Voice and the call host read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeetingState {
    /// A meeting runs on THIS device (the voice device, `meetingStartedAt` set).
    pub running: bool,
    /// The pause hotkey holds every channel (and screen memory) paused.
    pub paused: bool,
    /// `domain::meeting::who(...)` for the running meeting (the tray label).
    pub who: String,
}

impl Global for MeetingState {}

/// How often the queue is looked at (`tick`).
const TICK: Duration = Duration::from_secs(1);
/// How long a draining stop waits for the worker's `MeetingStopped`
/// before the final batch goes anyway (the worker may be gone).
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the local log is pruned while the IDE runs.
const PRUNE_EVERY_MS: i64 = 3_600_000;

/// The meeting THIS device should hold now (None = none).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Wanted {
    /// `meetingStartedAt`: the meeting's id (the local log's `meeting_id`).
    id: String,
    agent_id: String,
    language: String,
    desktop_audio: DesktopAudio,
    app_ids: Vec<String>,
}

fn wanted_for(settings: &VoiceSettings, device_id: &str) -> Option<Wanted> {
    if settings.voice_device_id.as_deref() != Some(device_id) {
        return None;
    }
    let settings = settings.clone().clear_on_none();
    if !settings.meeting() {
        return None;
    }
    Some(Wanted {
        id: settings.meeting_started_at.clone()?,
        agent_id: settings.agent_id.clone()?,
        language: settings.language().to_string(),
        desktop_audio: settings.desktop_audio,
        app_ids: settings.desktop_audio_apps.iter().map(|app| app.id.clone()).collect(),
    })
}

/// The worker's selection for `desktop_audio` with the picked apps.
fn selection_of(desktop_audio: DesktopAudio, app_ids: &[String]) -> talk::MeetingApps {
    match desktop_audio {
        DesktopAudio::Off => talk::MeetingApps::Off,
        DesktopAudio::All => talk::MeetingApps::All,
        DesktopAudio::Include => talk::MeetingApps::Include(app_ids.to_vec()),
        DesktopAudio::Exclude => talk::MeetingApps::Exclude(app_ids.to_vec()),
    }
}

fn capability_of(capability: talk::apps::Capability) -> DesktopAudioCapability {
    match capability {
        talk::apps::Capability::None => DesktopAudioCapability::None,
        talk::apps::Capability::System => DesktopAudioCapability::System,
        talk::apps::Capability::PerApp => DesktopAudioCapability::PerApp,
    }
}

/// Agent turns per meeting hour (`[meeting] turns/h`, RISK MEASURED).
fn turns_per_hour(turns: u32, elapsed_ms: i64) -> f64 {
    if elapsed_ms <= 0 {
        return 0.0;
    }
    f64::from(turns) * 3_600_000.0 / elapsed_ms as f64
}

/// The queue and `flush_step`'s state: `segments[i]` arrived at
/// `state.queued[i]`.
#[derive(Debug, Default)]
struct Batcher {
    state: FlushState,
    segments: Vec<MeetingSegment>,
}

impl Batcher {
    /// One flush event at `at`; the segments to send now (with when each
    /// was queued), if any.
    fn step(&mut self, event: FlushEvent, at: i64, segment: Option<MeetingSegment>) -> Option<(Vec<MeetingSegment>, Vec<i64>)> {
        if let Some(segment) = segment {
            self.segments.push(segment);
        }
        let mut ats = self.state.queued.clone();
        if event == FlushEvent::Segment {
            ats.push(at);
        }
        let send = flush_step(&mut self.state, event, at)?.min(self.segments.len());
        ats.truncate(send);
        Some((self.segments.drain(..send).collect(), ats))
    }

    /// A stop's final batch: everything queued, the in-flight slot ignored.
    fn take_all(&mut self) -> Vec<MeetingSegment> {
        self.state = FlushState::default();
        std::mem::take(&mut self.segments)
    }

    /// The send of `segments` (queued at `ats`) failed: they go back to the
    /// head of the queue and no turn is in flight.
    fn requeue(&mut self, segments: Vec<MeetingSegment>, ats: Vec<i64>) {
        self.state.in_flight_since = None;
        self.state.queued.splice(0..0, ats);
        self.segments.splice(0..0, segments);
    }
}

/// The turn in flight: its own message (once the send answered).
#[derive(Clone, Debug)]
struct Turn {
    session_id: String,
    message_id: Option<String>,
}

/// A stop sends the final batch only for the account that ran the meeting
/// (a sign-out or another account sends nothing more).
fn stop_drains(started_by: Option<&str>, signed_in: Option<&str>) -> bool {
    started_by.is_some() && started_by == signed_in
}

/// A draining stop: the worker's last segments still join `batcher`.
struct Stopping {
    id: String,
    agent_id: String,
    batcher: Batcher,
}

/// A meeting turn's `sessions.send` input: the member's conversation with
/// the voice agent (voice.json `utteranceInput`), `meeting: true`.
fn send_input(agent_id: &str, text: &str, cx: &App) -> Option<api::sessions::SendInput> {
    let conversation = crate::session_data::my_conversation(agent_id, cx).map(|row| row.id);
    let team_id = Store::try_global(cx)
        .and_then(|store| store.collections().agents.read(cx).iter().find(|row| row.id == agent_id).and_then(|row| row.team_id.clone()))
        .unwrap_or_default();
    let input = match domain::voice::utterance_input(text, &team_id, Some(agent_id), conversation.as_deref())? {
        UtteranceTarget::Session { session_id, text } => api::sessions::SendInput::message(&session_id, &text, Vec::new()),
        UtteranceTarget::Agent { team_id, agent_id, text } => crate::chat_launch::agent_send_input(&team_id, &agent_id, &text, Vec::new()),
    };
    Some(input.meeting())
}

/// The running meeting.
struct Running {
    /// The account signed in when it started.
    account: Option<String>,
    wanted: Wanted,
    capability: talk::apps::Capability,
    started_ms: i64,
    turns: u32,
    batcher: Batcher,
    turn: Option<Turn>,
    /// Bumped by every send: an older send's answer is ignored.
    generation: u64,
}

pub(crate) struct MeetingHost {
    device_id: String,
    client: Option<talk::SpeechClient>,
    log: Option<SegmentLog>,
    running: Option<Running>,
    stopping: Option<Stopping>,
    paused: bool,
    last_prune_ms: i64,
    tick_generation: u64,
}

struct MeetingHostGlobal(Entity<MeetingHost>);

impl Global for MeetingHostGlobal {}

fn host(cx: &App) -> Option<Entity<MeetingHost>> {
    cx.try_global::<MeetingHostGlobal>().map(|global| global.0.clone())
}

/// Install the Global and start following the voice settings. ONCE, after
/// `speak_replies::install` (it shares that worker and its settings).
pub fn init(cx: &mut App) {
    cx.set_global(MeetingState::default());
    let Some(store) = Store::try_global(cx) else { return };
    let Some(speaker) = crate::speak_replies::entity(cx) else { return };
    let Some(auth) = cx.try_global::<crate::session::AuthContext>() else { return };
    let data_dir: PathBuf = auth.data_dir.clone();
    let device_id = steer::persistent_device_id(&data_dir);
    let messages = store.collections().messages.clone();
    let log = match SegmentLog::open(&data_dir) {
        Ok(log) => Some(log),
        Err(err) => {
            ::log::warn!("[meeting] the local segment log is unavailable: {err}");
            None
        }
    };
    let entity = cx.new(|cx: &mut Context<MeetingHost>| {
        cx.observe(&speaker, |this: &mut MeetingHost, _, cx| this.follow(cx)).detach();
        cx.observe(&messages, |this: &mut MeetingHost, collection, cx| this.on_messages(&collection, cx)).detach();
        MeetingHost {
            device_id,
            client: crate::speak_replies::client(cx),
            log,
            running: None,
            stopping: None,
            paused: false,
            last_prune_ms: 0,
            tick_generation: 0,
        }
    });
    entity.update(cx, |this, cx| {
        this.prune();
        this.follow(cx);
    });
    cx.set_global(MeetingHostGlobal(entity));
}

/// The current state (default before [`init`]).
pub fn state(cx: &App) -> MeetingState {
    cx.try_global::<MeetingState>().cloned().unwrap_or_default()
}

/// Pause or resume listening (the hotkey and the tray's Pause listening):
/// every meeting channel and screen memory (`screen::set_hotkey_paused`).
pub fn set_paused(paused: bool, cx: &mut App) {
    screen::set_hotkey_paused(paused);
    if let Some(entity) = host(cx) {
        entity.update(cx, |this, _| {
            this.paused = paused;
            if let (Some(running), Some(client)) = (this.running.as_ref(), this.client.as_ref()) {
                client.meeting_pause(running.wanted.id.clone(), paused);
            }
        });
    }
    let mut next = state(cx);
    next.paused = paused;
    cx.set_global(next);
}

/// Speech-worker events that belong to the meeting.
pub(crate) fn is_meeting_event(event: &talk::Event) -> bool {
    matches!(
        event,
        talk::Event::MeetingSegment { .. }
            | talk::Event::MeetingWake { .. }
            | talk::Event::MeetingFlushNow { .. }
            | talk::Event::MeetingListening { .. }
            | talk::Event::MeetingStopped { .. }
    )
}

/// A meeting event from the worker.
pub(crate) fn on_worker_event(event: talk::Event, cx: &mut App) {
    if let Some(entity) = host(cx) {
        entity.update(cx, |this, cx| this.on_worker(event, cx));
    }
}

/// The worker came up (`Ready`): a running meeting is handed to it again
/// (a new worker knows none). Called inside the speaker's update.
pub(crate) fn worker_ready(cx: &mut App) {
    if let Some(entity) = host(cx) {
        entity.update(cx, |this, cx| this.hand_over(cx));
    }
}

/// A push-to-talk key-down during a meeting (`ptt`): flush now.
pub(crate) fn flush_now(cx: &mut App) {
    if let Some(entity) = host(cx) {
        entity.update(cx, |this, cx| this.step(FlushEvent::Ptt, None, cx));
    }
}

impl MeetingHost {
    /// The settings changed (or the speaker notified): start, re-hand or stop.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            self.client = crate::speak_replies::client(cx);
        }
        let settings = crate::speak_replies::settings(cx);
        let wanted = wanted_for(&settings, &self.device_id);
        let current = self.running.as_ref().map(|running| running.wanted.clone());
        if wanted == current {
            return;
        }
        match (current, wanted) {
            (Some(old), Some(new)) if old.id == new.id => {
                // The same meeting with other apps, agent or language.
                if let Some(running) = self.running.as_mut() {
                    running.wanted = new;
                }
                self.hand_over(cx);
            }
            (current, wanted) => {
                if current.is_some() {
                    self.stop(cx);
                }
                if let Some(wanted) = wanted {
                    self.start(wanted, cx);
                }
            }
        }
        self.publish(&settings, cx);
    }

    fn start(&mut self, wanted: Wanted, cx: &mut Context<Self>) {
        let capability = talk::apps::capability();
        ::log::info!("[meeting] started on this device ({:?} desktop audio, {:?})", wanted.desktop_audio, capability);
        self.running = Some(Running {
            account: crate::speak_replies::session_account(cx),
            wanted,
            capability,
            started_ms: crate::session_data::now_ms(),
            turns: 0,
            batcher: Batcher::default(),
            turn: None,
            generation: 0,
        });
        self.prune();
        self.hand_over(cx);
        self.tick_generation += 1;
        let generation = self.tick_generation;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(TICK).await;
            let alive = this.update(cx, |this, cx| {
                if this.tick_generation != generation || this.running.is_none() {
                    return false;
                }
                this.step(FlushEvent::Tick, None, cx);
                if crate::session_data::now_ms() - this.last_prune_ms >= PRUNE_EVERY_MS {
                    this.prune();
                }
                true
            });
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    /// Tell the worker which meeting to hold (again).
    fn hand_over(&mut self, cx: &mut Context<Self>) {
        let Some(running) = self.running.as_ref() else { return };
        let Some(client) = self.client.as_ref() else { return };
        let settings = crate::speak_replies::settings(cx);
        let agent = crate::voice_tray::agent_label(&settings, cx);
        let phrase = domain::voice::wake_phrase(&settings, agent.as_deref()).unwrap_or_default();
        let wanted = &running.wanted;
        client.meeting(
            wanted.id.clone(),
            wanted.language.clone(),
            phrase,
            MEETING.copy.you.clone(),
            selection_of(wanted.desktop_audio, &wanted.app_ids),
            running.capability,
        );
        if self.paused {
            client.meeting_pause(wanted.id.clone(), true);
        }
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(running) = self.running.take() else { return };
        let drain = stop_drains(running.account.as_deref(), crate::speak_replies::session_account(cx).as_deref());
        if let Some(client) = self.client.as_ref() {
            client.meeting_stop(running.wanted.id.clone(), drain);
        }
        self.tick_generation += 1;
        let elapsed = crate::session_data::now_ms() - running.started_ms;
        ::log::info!(
            "[meeting] turns/h {:.1} ({} turn(s) in {} min; {} segment(s) never sent)",
            turns_per_hour(running.turns, elapsed),
            running.turns,
            elapsed / 60_000,
            running.batcher.segments.len()
        );
        // An older stop still draining sends its batch now.
        self.finish_stop(None, cx);
        if drain {
            let id = running.wanted.id.clone();
            self.stopping = Some(Stopping { id: id.clone(), agent_id: running.wanted.agent_id.clone(), batcher: running.batcher });
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DRAIN_TIMEOUT).await;
                let _ = this.update(cx, |this, cx| this.finish_stop(Some(&id), cx));
            })
            .detach();
        }
        // The pause belongs to the meeting (screen memory resumes too).
        if self.paused {
            self.paused = false;
            screen::set_hotkey_paused(false);
        }
        cx.notify();
    }

    /// The draining stop `id` (None = whichever) is over: everything it
    /// queued goes out as ONE final batch, past the in-flight slot; a
    /// failure leaves it in the local log only.
    fn finish_stop(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        if !self.stopping.as_ref().is_some_and(|stopping| id.is_none_or(|id| stopping.id == id)) {
            return;
        }
        let Some(mut stopping) = self.stopping.take() else { return };
        let batch = stopping.batcher.take_all();
        if batch.is_empty() {
            return;
        }
        let count = batch.len();
        let Some(input) = send_input(&stopping.agent_id, &meeting_message(&batch), cx) else { return };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            ::log::warn!("[meeting] the final batch was not sent (signed out); {count} segment(s) stay in the local log");
            return;
        };
        cx.background_executor()
            .spawn(async move {
                match api::sessions::send(&trpc, &input) {
                    Ok(_) => ::log::info!("[meeting] sent the final batch ({count} segment(s))"),
                    Err(err) => ::log::warn!("[meeting] the final batch was not sent ({err}); {count} segment(s) stay in the local log"),
                }
            })
            .detach();
    }

    /// The Global the tray and Settings read.
    fn publish(&self, settings: &VoiceSettings, cx: &mut Context<Self>) {
        let who = self
            .running
            .as_ref()
            .map(|running| domain::meeting::who(settings.desktop_audio, &settings.desktop_audio_apps, capability_of(running.capability)))
            .unwrap_or_default();
        let next = MeetingState { running: self.running.is_some(), paused: self.paused && self.running.is_some(), who };
        if cx.try_global::<MeetingState>() != Some(&next) {
            cx.set_global(next);
            cx.refresh_windows();
        }
    }

    fn prune(&mut self) {
        let now = crate::session_data::now_ms();
        self.last_prune_ms = now;
        let Some(log) = self.log.as_ref() else { return };
        match log.prune(now, domain::meeting::LOCAL_RETENTION_DAYS) {
            Ok(0) => {}
            Ok(pruned) => ::log::info!("[meeting] pruned {pruned} local segment(s) past {} days", domain::meeting::LOCAL_RETENTION_DAYS),
            Err(err) => ::log::warn!("[meeting] pruning the local log failed: {err}"),
        }
    }

    fn on_worker(&mut self, event: talk::Event, cx: &mut Context<Self>) {
        let ours = |id: &str, running: &Option<Running>| running.as_ref().is_some_and(|running| running.wanted.id == id);
        let stopping = |id: &str, stopping: &Option<Stopping>| stopping.as_ref().is_some_and(|stopping| stopping.id == id);
        match event {
            // A draining stop's last segments: logged and into the final batch.
            talk::Event::MeetingSegment { id, channel, label, text, start_ms, end_ms, words } if stopping(&id, &self.stopping) => {
                self.log_segment(&id, &channel, &label, &text, start_ms, end_ms, &words);
                if let Some(stopping) = self.stopping.as_mut() {
                    stopping.batcher.segments.push(MeetingSegment { channel, label, text, start_ms });
                }
            }
            talk::Event::MeetingStopped { id } => self.finish_stop(Some(&id), cx),
            talk::Event::MeetingSegment { id, channel, label, text, start_ms, end_ms, words } if ours(&id, &self.running) => {
                self.log_segment(&id, &channel, &label, &text, start_ms, end_ms, &words);
                let segment = MeetingSegment { channel, label, text, start_ms };
                self.step(FlushEvent::Segment, Some(segment), cx);
            }
            talk::Event::MeetingWake { id } if ours(&id, &self.running) => self.step(FlushEvent::Wake, None, cx),
            talk::Event::MeetingFlushNow { id } if ours(&id, &self.running) => self.step(FlushEvent::Ptt, None, cx),
            talk::Event::MeetingListening { id, listening, reason } if ours(&id, &self.running) => {
                if listening {
                    ::log::info!("[meeting] listening");
                } else if let Some(reason) = reason {
                    ::log::info!("[meeting] not listening ({reason:?}); the worker retries");
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn log_segment(&self, id: &str, channel: &str, label: &str, text: &str, start_ms: i64, end_ms: i64, words: &[talk::MeetingWord]) {
        let Some(log) = self.log.as_ref() else { return };
        let row = LoggedSegment {
            meeting_id: id.to_string(),
            channel: channel.to_string(),
            label: label.to_string(),
            start_ms,
            end_ms,
            text: text.to_string(),
            words: serde_json::to_string(words).unwrap_or_else(|_| "[]".into()),
        };
        if let Err(err) = log.insert(&row) {
            ::log::warn!("[meeting] the segment was not logged: {err}");
        }
    }

    /// One flush event now; a due batch goes out.
    fn step(&mut self, event: FlushEvent, segment: Option<MeetingSegment>, cx: &mut Context<Self>) {
        let now = crate::session_data::now_ms();
        let Some(running) = self.running.as_mut() else { return };
        let before = running.batcher.state.in_flight_since;
        let Some((batch, ats)) = running.batcher.step(event, now, segment) else {
            // A turn that timed out (or finished) is no longer waited for.
            if before.is_some() && running.batcher.state.in_flight_since.is_none() {
                running.turn = None;
            }
            return;
        };
        if batch.is_empty() {
            return;
        }
        self.send(batch, ats, cx);
    }

    fn send(&mut self, batch: Vec<MeetingSegment>, ats: Vec<i64>, cx: &mut Context<Self>) {
        let Some(running) = self.running.as_mut() else { return };
        running.turn = None;
        running.generation += 1;
        let generation = running.generation;
        let agent_id = running.wanted.agent_id.clone();
        let Some(input) = send_input(&agent_id, &meeting_message(&batch), cx) else { return };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            ::log::warn!("[meeting] not signed in; {} segment(s) wait", batch.len());
            running.batcher.requeue(batch, ats);
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::sessions::send(&trpc, &input) }).await;
            let _ = this.update(cx, |this, _| {
                let Some(running) = this.running.as_mut().filter(|running| running.generation == generation) else { return };
                match result {
                    Ok(output) => {
                        running.turns += 1;
                        running.turn = Some(Turn { session_id: output.session_id, message_id: output.message_id });
                    }
                    Err(err) => {
                        ::log::warn!("[meeting] the turn was not sent ({err}); {} segment(s) wait", batch.len());
                        running.batcher.requeue(batch, ats);
                    }
                }
            });
        })
        .detach();
    }

    /// `turnDone`: the agent's reply row after the turn's own message
    /// landed in the conversation (push to talk's rule).
    fn on_messages(&mut self, collection: &Entity<Collection<MessageRow>>, cx: &mut Context<Self>) {
        let Some(turn) = self.running.as_ref().and_then(|running| running.turn.clone()) else { return };
        let Some(store) = Store::try_global(cx) else { return };
        let sessions = domain::message_conversation::conversation_sessions(store.collections().conversations.read(cx).iter());
        let replied = {
            let rows = collection.read(cx);
            let in_session = |row: &&MessageRow| {
                domain::message_conversation::session_of(row, &sessions).unwrap_or(row.session_id.as_str()) == turn.session_id
            };
            let sent_at = turn
                .message_id
                .as_deref()
                .and_then(|id| rows.iter().find(|row| row.id == id))
                .and_then(|row| row.created_at.as_deref().and_then(domain::voice::parse_instant));
            let Some(sent_at) = sent_at else { return };
            rows.iter().filter(in_session).any(|row| {
                row.author_kind.as_deref() == Some("agent")
                    && row.author_session_id.is_none()
                    && row.body.as_deref().is_some_and(|body| !body.trim().is_empty())
                    && row.created_at.as_deref().and_then(domain::voice::parse_instant).is_some_and(|at| at > sent_at)
            })
        };
        if replied {
            if let Some(running) = self.running.as_mut() {
                running.turn = None;
            }
            self.step(FlushEvent::TurnDone, None, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::meeting::AudioApp;

    fn settings() -> VoiceSettings {
        VoiceSettings {
            voice_device_id: Some("mint".into()),
            agent_id: Some("a1".into()),
            meeting_started_at: Some("2026-10-28T09:00:00Z".into()),
            desktop_audio: DesktopAudio::Include,
            desktop_audio_apps: vec![AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() }],
            language: "de".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_meeting_runs_only_on_the_voice_device_while_started() {
        let wanted = wanted_for(&settings(), "mint").expect("this device holds it");
        assert_eq!(wanted.id, "2026-10-28T09:00:00Z");
        assert_eq!((wanted.agent_id.as_str(), wanted.language.as_str()), ("a1", "de"));
        assert_eq!(selection_of(wanted.desktop_audio, &wanted.app_ids), talk::MeetingApps::Include(vec!["us.zoom.xos".into()]));
        assert_eq!(wanted_for(&settings(), "laptop"), None, "another device");
        assert_eq!(wanted_for(&VoiceSettings { meeting_started_at: None, ..settings() }, "mint"), None, "stopped");
        assert_eq!(wanted_for(&VoiceSettings { meeting_started_at: Some(String::new()), ..settings() }, "mint"), None);
        assert_eq!(wanted_for(&VoiceSettings { agent_id: None, ..settings() }, "mint"), None, "clearOnNone");
        assert_eq!(wanted_for(&VoiceSettings::default(), "mint"), None, "signed out: the defaults");
        assert_eq!(selection_of(DesktopAudio::Off, &[]), talk::MeetingApps::Off);
        assert_eq!(selection_of(DesktopAudio::Exclude, &["x".into()]), talk::MeetingApps::Exclude(vec!["x".into()]));
    }

    fn segment(channel: &str, start_ms: i64) -> MeetingSegment {
        MeetingSegment { channel: channel.into(), label: channel.into(), text: format!("at {start_ms}"), start_ms }
    }

    #[test]
    fn segments_batch_through_flush_step_and_a_failed_send_requeues() {
        let flush = MEETING.flush;
        let mut batcher = Batcher::default();
        assert_eq!(batcher.step(FlushEvent::Segment, 0, Some(segment("you", 0))), None);
        assert_eq!(batcher.step(FlushEvent::Segment, 10, Some(segment("zoom", 5))), None);
        // A wake sends everything queued, coalesced.
        let (sent, ats) = batcher.step(FlushEvent::Wake, 20, None).expect("wake flushes");
        assert_eq!(sent, [segment("you", 0), segment("zoom", 5)]);
        assert_eq!(ats, [0, 10]);
        // In flight: the queue grows, a trigger waits for turnDone.
        assert_eq!(batcher.step(FlushEvent::Segment, 30, Some(segment("you", 30))), None);
        assert_eq!(batcher.step(FlushEvent::Ptt, 40, None), None);
        let (sent, ats) = batcher.step(FlushEvent::TurnDone, 50, None).expect("the waiting trigger fires");
        assert_eq!(sent, [segment("you", 30)]);
        // That send failed: back at the head, and the next tick past maxAge retries.
        batcher.requeue(sent, ats);
        assert_eq!(batcher.state.in_flight_since, None);
        assert_eq!(batcher.step(FlushEvent::Segment, 60, Some(segment("you", 60))), None);
        let (sent, ats) = batcher.step(FlushEvent::Tick, 30 + flush.max_age_ms, None).expect("the oldest is due");
        assert_eq!(sent, [segment("you", 30), segment("you", 60)]);
        assert_eq!(ats, [30, 60]);
        assert!(batcher.segments.is_empty() && batcher.state.queued.is_empty());
    }

    /// meeting.json FLUSH: a stop sends everything still queued as ONE
    /// final batch, the in-flight slot ignored; a sign-out sends nothing.
    #[test]
    fn a_stop_sends_everything_queued_past_the_turn_in_flight() {
        let mut batcher = Batcher::default();
        batcher.step(FlushEvent::Segment, 0, Some(segment("you", 0)));
        let (sent, _) = batcher.step(FlushEvent::Wake, 10, None).expect("a turn goes out");
        assert_eq!(sent.len(), 1);
        // In flight: the next segments only queue.
        assert_eq!(batcher.step(FlushEvent::Segment, 20, Some(segment("zoom", 15))), None);
        assert_eq!(batcher.step(FlushEvent::Wake, 30, None), None);
        let mut stopping = Stopping { id: "m1".into(), agent_id: "a1".into(), batcher };
        // The worker's drained segment joins the final batch.
        stopping.batcher.segments.push(segment("you", 25));
        let last = stopping.batcher.take_all();
        assert_eq!(last, [segment("zoom", 15), segment("you", 25)], "one batch, everything");
        assert!(stopping.batcher.take_all().is_empty() && stopping.batcher.state == FlushState::default());
        let message = meeting_message(&last);
        assert!(message.contains("zoom: at 15") && message.contains("you: at 25"), "{message}");

        assert!(stop_drains(Some("acct-1"), Some("acct-1")), "a stop by the settings drains");
        assert!(!stop_drains(Some("acct-1"), None), "signed out: nothing more is sent");
        assert!(!stop_drains(Some("acct-1"), Some("acct-2")), "another account: nothing more is sent");
        assert!(!stop_drains(None, None));
    }

    #[test]
    fn turns_per_hour_reads_the_meeting_hour() {
        assert_eq!(turns_per_hour(6, 3_600_000), 6.0);
        assert_eq!(turns_per_hour(3, 1_800_000), 6.0);
        assert_eq!(turns_per_hour(3, 0), 0.0);
    }

    #[test]
    fn capability_maps_to_the_domain() {
        assert_eq!(capability_of(talk::apps::Capability::PerApp), DesktopAudioCapability::PerApp);
        assert_eq!(capability_of(talk::apps::Capability::System), DesktopAudioCapability::System);
        assert_eq!(capability_of(talk::apps::Capability::None), DesktopAudioCapability::None);
    }
}
