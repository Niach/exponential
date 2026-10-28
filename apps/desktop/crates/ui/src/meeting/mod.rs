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
//! meeting's id is its `meetingStartedAt`, so a restarted worker picks the
//! same meeting up. #FEED-163 RESTART: a restarted IDE PROCESS never does:
//! the local log remembers the meeting it listened to, and when the voice
//! row still holds that one at the next start, the IDE stops it
//! (`voice.stopMeeting {reason: 'restart'}`) and opens THE consent dialog
//! instead ([`domain::meeting::meeting_resume_asks`]); a meeting it never
//! listened to starts as usual. FLUSH: segments queue; `wake` (the phrase
//! opening a `you` segment), `ptt` (a push-to-talk key-down, which the
//! worker answers `MeetingFlushNow`), `tick` (every second) and `turnDone`
//! (the agent's reply row after the turn's own message, or the turn
//! timeout) step `flush_step`; a send carries every queued segment, split by
//! `meeting_messages` under the server's text limit. At most
//! `flush.maxQueued` segments wait in memory (the oldest go, logged; the
//! local log keeps them). A failed send puts its segments back at the head
//! of the queue and the next try waits (`flush.backoffMs` doubling to
//! `flush.backoffMaxMs`); a refusal (4xx) drops the batch, logged. A batch
//! goes out only while the account that started the meeting is signed in.
//! A sign-out or an account switch deletes the local log.
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

use domain::meeting::{flush_step, meeting_messages, DesktopAudio, DesktopAudioCapability, FlushEvent, FlushState, MeetingSegment, MEETING};
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
    /// The worker reported the meeting's channels listening and the pause
    /// does not hold them (the tray's and the rail's dot, voice.json
    /// `trayIcon` `meetingListening`).
    pub listening: bool,
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

fn wanted_for(settings: &VoiceSettings, device_id: &str, now_ms: i64) -> Option<Wanted> {
    if settings.voice_device_id.as_deref() != Some(device_id) {
        return None;
    }
    let settings = settings.clone().clear_on_none();
    // AUTO-STOP: on this device's own clock (an offline device stops too).
    if !settings.meeting()
        || domain::meeting::meeting_expired(settings.meeting_started_at.as_deref(), now_ms, settings.auto_stop_hours())
    {
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
    /// Failed sends in a row (the backoff's exponent).
    failures: u32,
    /// No send before this (epoch ms): the backoff after a failure.
    retry_at: i64,
}

/// The wait after the `failures`-th failed send in a row (meeting.json
/// FLUSH: `backoffMs` doubling up to `backoffMaxMs`).
fn backoff_ms(failures: u32) -> i64 {
    let flush = MEETING.flush;
    let doubled = flush.backoff_ms.saturating_mul(1_i64 << failures.saturating_sub(1).min(30));
    doubled.min(flush.backoff_max_ms)
}

/// A send's answer the batch can never pass (4xx: the server refused it):
/// dropped, never requeued.
/// A 401 (an expired token, `Unauthorized` or a bare `Http 401`) is a failed
/// send, never a refusal: the batch waits for the refreshed token.
fn refused(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status, .. } if (400..500).contains(status) && !matches!(*status, 401 | 408 | 429))
}

impl Batcher {
    /// One flush event at `at`; the segments to send now (with when each
    /// was queued), if any. While backing off it only queues (a trigger is
    /// remembered).
    fn step(&mut self, event: FlushEvent, at: i64, segment: Option<MeetingSegment>) -> Option<(Vec<MeetingSegment>, Vec<i64>)> {
        if let Some(segment) = segment {
            self.segments.push(segment);
        }
        if at < self.retry_at {
            match event {
                FlushEvent::Segment => self.state.queued.push(at),
                FlushEvent::TurnDone => self.state.in_flight_since = None,
                FlushEvent::Wake | FlushEvent::Ptt if !self.state.queued.is_empty() => self.state.urgent = true,
                _ => {}
            }
            return None;
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

    /// The send of `segments` (queued at `ats`) failed at `now`: they go
    /// back to the head of the queue, no turn is in flight, and the next
    /// try waits [`backoff_ms`].
    fn requeue(&mut self, segments: Vec<MeetingSegment>, ats: Vec<i64>, now: i64) {
        self.state.in_flight_since = None;
        self.state.queued.splice(0..0, ats);
        self.segments.splice(0..0, segments);
        self.failures = self.failures.saturating_add(1);
        self.retry_at = now + backoff_ms(self.failures);
    }

    /// The send was refused (4xx): the batch is dropped, the slot freed.
    fn refused(&mut self) {
        self.state.in_flight_since = None;
        self.failures = 0;
        self.retry_at = 0;
    }

    /// A send went through: the backoff resets.
    fn sent(&mut self) {
        self.failures = 0;
        self.retry_at = 0;
    }

    /// Keep at most `flush.maxQueued` segments in memory: the oldest go
    /// (the local log keeps them). Answers how many went.
    fn cap(&mut self) -> usize {
        let over = self.segments.len().saturating_sub(MEETING.flush.max_queued);
        if over > 0 {
            self.segments.drain(..over);
            let queued_over = over.min(self.state.queued.len());
            self.state.queued.drain(..queued_over);
        }
        over
    }
}

/// The turn in flight: its own message (once the send answered).
#[derive(Clone, Debug)]
struct Turn {
    session_id: String,
    message_id: Option<String>,
}

/// A stop sends the final batch only for the account that ran the meeting
/// (a sign-out or another account sends nothing more). The same rule gates
/// every send of a running meeting.
fn stop_drains(started_by: Option<&str>, signed_in: Option<&str>) -> bool {
    started_by.is_some() && started_by == signed_in
}

/// meeting.json: a sign-out or an account switch deletes the local log (the
/// account last seen here, if any, is no longer the one signed in).
fn purges_log(last: Option<&str>, now: Option<&str>) -> bool {
    last.is_some() && last != now
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
    /// The worker's last `MeetingListening` for the running meeting.
    listening: bool,
    last_prune_ms: i64,
    tick_generation: u64,
    /// The account signed in when last looked at ([`purges_log`]).
    account: Option<String>,
    /// #FEED-163 RESTART: the meeting the previous IDE process listened to
    /// (read once at start): never listened to again without a new consent.
    listened_before: Option<String>,
    /// The restart stop + consent went out (once per process).
    resume_asked: bool,
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
    let session_state = store.state();
    let log = match SegmentLog::open(&data_dir) {
        Ok(log) => Some(log),
        Err(err) => {
            ::log::warn!("[meeting] the local segment log is unavailable: {err}");
            None
        }
    };
    let listened_before = log.as_ref().and_then(|log| {
        log.listened().unwrap_or_else(|err| {
            ::log::warn!("[meeting] reading the listened meeting failed: {err}");
            None
        })
    });
    let entity = cx.new(|cx: &mut Context<MeetingHost>| {
        cx.observe(&speaker, |this: &mut MeetingHost, _, cx| this.follow(cx)).detach();
        cx.observe(&messages, |this: &mut MeetingHost, collection, cx| this.on_messages(&collection, cx)).detach();
        cx.observe(&session_state, |this: &mut MeetingHost, _, cx| this.on_session(cx)).detach();
        MeetingHost {
            device_id,
            client: crate::speak_replies::client(cx),
            log,
            running: None,
            stopping: None,
            paused: false,
            listening: false,
            last_prune_ms: 0,
            tick_generation: 0,
            account: crate::speak_replies::session_account(cx),
            listened_before,
            resume_asked: false,
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
    if let Some(entity) = host(cx) {
        next.listening = next.running && entity.read(cx).listening && !paused;
    }
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
    /// The signed-in account changed: a sign-out or a switch deletes the
    /// local log (after the settings stop the meeting).
    fn on_session(&mut self, cx: &mut Context<Self>) {
        self.follow(cx);
        let now = crate::speak_replies::session_account(cx);
        if purges_log(self.account.as_deref(), now.as_deref()) {
            if let Some(log) = self.log.as_ref() {
                match log.clear() {
                    Ok(gone) => ::log::info!("[meeting] signed out or switched accounts: deleted {gone} local segment(s)"),
                    Err(err) => ::log::warn!("[meeting] deleting the local log failed: {err}"),
                }
            }
        }
        self.account = now;
    }

    /// The settings changed (or the speaker notified): start, re-hand or stop.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            self.client = crate::speak_replies::client(cx);
        }
        let settings = crate::speak_replies::settings(cx);
        let mut wanted = wanted_for(&settings, &self.device_id, crate::session_data::now_ms());
        // RESTART: the sitting this IDE listened to before it was gone needs
        // a new consent: stop it and ask, never listen on.
        if wanted.as_ref().is_some_and(|wanted| {
            domain::meeting::meeting_resume_asks(self.listened_before.as_deref(), Some(wanted.id.as_str()))
        }) {
            wanted = None;
            if !self.resume_asked {
                self.resume_asked = true;
                ::log::info!("[meeting] the IDE restarted during a meeting: stopped, asking for consent again");
                cx.defer(|cx| {
                    crate::meeting_mode::stop_for(domain::meeting::MeetingStopReason::Restart, cx);
                    crate::meeting_mode::request_consent(cx);
                });
            }
        }
        let current = self.running.as_ref().map(|running| running.wanted.clone());
        if wanted == current {
            return;
        }
        // AUTO-STOP: the running meeting is still the stored one, only too old.
        let auto_stopped = wanted.is_none()
            && current.as_ref().is_some_and(|current| settings.meeting_started_at.as_deref() == Some(current.id.as_str()))
            && settings.voice_device_id.as_deref() == Some(self.device_id.as_str());
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
        if auto_stopped {
            let hours = settings.auto_stop_hours();
            ::log::info!("[meeting] stopped after {hours} h (autoStop)");
            cx.defer(move |cx| {
                crate::meeting_mode::stop_for(domain::meeting::MeetingStopReason::AutoStop, cx);
                crate::meeting_mode::notify_auto_stopped(hours, cx);
            });
        }
    }

    fn start(&mut self, wanted: Wanted, cx: &mut Context<Self>) {
        let capability = talk::apps::capability();
        self.listening = false;
        // RESTART: remembered before anything listens.
        if let Some(log) = self.log.as_ref() {
            if let Err(err) = log.set_listened(&wanted.id) {
                ::log::warn!("[meeting] remembering the listened meeting failed: {err}");
            }
        }
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
                // AUTO-STOP: the tick is the clock that ends a too-old meeting.
                let hours = crate::speak_replies::settings(cx).auto_stop_hours();
                if this.running.as_ref().is_some_and(|running| {
                    domain::meeting::meeting_expired(Some(&running.wanted.id), crate::session_data::now_ms(), hours)
                }) {
                    this.follow(cx);
                    return false;
                }
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
        self.listening = false;
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
        let inputs: Vec<_> = meeting_messages(&batch, MEETING.flush.max_message_chars)
            .iter()
            .filter_map(|text| send_input(&stopping.agent_id, text, cx))
            .collect();
        if inputs.is_empty() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            ::log::warn!("[meeting] the final batch was not sent (signed out); {count} segment(s) stay in the local log");
            return;
        };
        cx.background_executor()
            .spawn(async move {
                for input in inputs {
                    if let Err(err) = api::sessions::send(&trpc, &input) {
                        ::log::warn!("[meeting] the final batch was not sent ({err}); {count} segment(s) stay in the local log");
                        return;
                    }
                }
                ::log::info!("[meeting] sent the final batch ({count} segment(s))");
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
        let running = self.running.is_some();
        let next = MeetingState { running, paused: self.paused && running, listening: running && self.listening && !self.paused, who };
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
                self.listening = listening;
                self.publish(&crate::speak_replies::settings(cx), cx);
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
        let stepped = running.batcher.step(event, now, segment);
        let dropped = running.batcher.cap();
        if dropped > 0 {
            ::log::warn!("[meeting] {dropped} queued segment(s) dropped from memory (over {}); the local log keeps them", MEETING.flush.max_queued);
        }
        let Some((batch, ats)) = stepped else {
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
        let now = crate::session_data::now_ms();
        let signed_in = crate::speak_replies::session_account(cx);
        let Some(running) = self.running.as_mut() else { return };
        running.turn = None;
        running.generation += 1;
        let generation = running.generation;
        // Only the account that started the meeting sends (a switch mid-meeting
        // never posts its transcript into another account).
        if !stop_drains(running.account.as_deref(), signed_in.as_deref()) {
            ::log::warn!("[meeting] another account is signed in; {} segment(s) stay in the local log only", batch.len());
            running.batcher.refused();
            return;
        }
        let agent_id = running.wanted.agent_id.clone();
        let inputs: Vec<_> = meeting_messages(&batch, MEETING.flush.max_message_chars)
            .iter()
            .filter_map(|text| send_input(&agent_id, text, cx))
            .collect();
        let Some(running) = self.running.as_mut() else { return };
        if inputs.is_empty() {
            running.batcher.refused();
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            ::log::warn!("[meeting] not signed in; {} segment(s) wait", batch.len());
            running.batcher.requeue(batch, ats, now);
            return;
        };
        cx.spawn(async move |this, cx| {
            // One message per split part, in order; the turn waits for the
            // reply after the LAST one.
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut sent = 0;
                    let mut last = None;
                    for input in &inputs {
                        match api::sessions::send(&trpc, input) {
                            Ok(output) => {
                                sent += 1;
                                last = Some(output);
                            }
                            Err(err) => return (sent, Err(err)),
                        }
                    }
                    (sent, last.ok_or_else(|| api::ApiError::Http { status: 0, message: "nothing to send".into() }))
                })
                .await;
            let _ = this.update(cx, |this, _| {
                let Some(running) = this.running.as_mut().filter(|running| running.generation == generation) else { return };
                let now = crate::session_data::now_ms();
                match result {
                    (_, Ok(output)) => {
                        running.turns += 1;
                        running.batcher.sent();
                        running.turn = Some(Turn { session_id: output.session_id, message_id: output.message_id });
                    }
                    (_, Err(err)) if refused(&err) => {
                        ::log::warn!("[meeting] the server refused the batch ({err}); {} segment(s) dropped (the local log keeps them)", batch.len());
                        running.batcher.refused();
                    }
                    (0, Err(err)) => {
                        ::log::warn!("[meeting] the turn was not sent ({err}); {} segment(s) wait", batch.len());
                        running.batcher.requeue(batch, ats, now);
                    }
                    (sent, Err(err)) => {
                        // Part of a split batch landed: never resend it. The
                        // rest stays in the local log only.
                        ::log::warn!("[meeting] only {sent} part(s) of the batch were sent ({err}); the rest stays in the local log");
                        running.batcher.requeue(Vec::new(), Vec::new(), now);
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

    /// An hour after `settings()`'s meeting started.
    const NOW: i64 = 1_793_181_600_000;

    #[test]
    fn a_meeting_runs_only_on_the_voice_device_while_started() {
        let wanted = wanted_for(&settings(), "mint", NOW).expect("this device holds it");
        assert_eq!(wanted.id, "2026-10-28T09:00:00Z");
        assert_eq!((wanted.agent_id.as_str(), wanted.language.as_str()), ("a1", "de"));
        assert_eq!(selection_of(wanted.desktop_audio, &wanted.app_ids), talk::MeetingApps::Include(vec!["us.zoom.xos".into()]));
        assert_eq!(wanted_for(&settings(), "laptop", NOW), None, "another device");
        assert_eq!(wanted_for(&VoiceSettings { meeting_started_at: None, ..settings() }, "mint", NOW), None, "stopped");
        assert_eq!(wanted_for(&VoiceSettings { meeting_started_at: Some(String::new()), ..settings() }, "mint", NOW), None);
        assert_eq!(wanted_for(&VoiceSettings { agent_id: None, ..settings() }, "mint", NOW), None, "clearOnNone");
        assert_eq!(wanted_for(&VoiceSettings::default(), "mint", NOW), None, "signed out: the defaults");
        // AUTO-STOP: four hours after its start the meeting is wanted nowhere.
        let after = MEETING.auto_stop.after_ms;
        assert!(wanted_for(&settings(), "mint", NOW - 3_600_000 + after - 1).is_some());
        assert_eq!(wanted_for(&settings(), "mint", NOW - 3_600_000 + after), None, "auto-stopped");
        // #FEED-163: the member's own hours (1 h: NOW is the edge).
        let one_hour = VoiceSettings { meeting_auto_stop_hours: 1, ..settings() };
        assert!(wanted_for(&one_hour, "mint", NOW - 1).is_some());
        assert_eq!(wanted_for(&one_hour, "mint", NOW), None, "auto-stopped after 1 h");
        let eight = VoiceSettings { meeting_auto_stop_hours: 8, ..settings() };
        assert!(wanted_for(&eight, "mint", NOW - 3_600_000 + after).is_some(), "8 h outlives the default");
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
        batcher.requeue(sent, ats, 50);
        assert_eq!(batcher.state.in_flight_since, None);
        assert_eq!(batcher.step(FlushEvent::Segment, 60, Some(segment("you", 60))), None);
        let (sent, ats) = batcher.step(FlushEvent::Tick, 30 + flush.max_age_ms, None).expect("the oldest is due");
        assert_eq!(sent, [segment("you", 30), segment("you", 60)]);
        assert_eq!(ats, [30, 60]);
        assert!(batcher.segments.is_empty() && batcher.state.queued.is_empty());
    }

    /// meeting.json FLUSH: a failed send waits `backoffMs` doubling up to
    /// `backoffMaxMs` before the next try; a success resets it.
    #[test]
    fn a_failed_send_backs_off_exponentially() {
        let flush = MEETING.flush;
        assert_eq!((backoff_ms(1), backoff_ms(2), backoff_ms(3)), (flush.backoff_ms, 2 * flush.backoff_ms, 4 * flush.backoff_ms));
        assert_eq!(backoff_ms(7), flush.backoff_max_ms, "1 s doubling caps at 60 s");
        assert_eq!(backoff_ms(200), flush.backoff_max_ms);

        let mut batcher = Batcher::default();
        batcher.step(FlushEvent::Segment, 0, Some(segment("you", 0)));
        let (sent, ats) = batcher.step(FlushEvent::Wake, 0, None).expect("sent");
        batcher.requeue(sent, ats, 1_000);
        // Inside the wait: a trigger and new segments only queue.
        assert_eq!(batcher.step(FlushEvent::Wake, 1_500, None), None);
        assert_eq!(batcher.step(FlushEvent::Segment, 1_600, Some(segment("you", 1_600))), None);
        let (sent, ats) = batcher.step(FlushEvent::Tick, 1_000 + flush.backoff_ms, None).expect("the remembered trigger fires once the wait is over");
        assert_eq!(sent.len(), 2);
        // A second failure in a row waits twice as long.
        batcher.requeue(sent, ats, 10_000);
        assert_eq!(batcher.retry_at, 10_000 + 2 * flush.backoff_ms);
        assert_eq!(batcher.step(FlushEvent::Ptt, 10_000 + 2 * flush.backoff_ms - 1, None), None);
        assert!(batcher.step(FlushEvent::Tick, 10_000 + 2 * flush.backoff_ms, None).is_some());
        batcher.sent();
        assert_eq!((batcher.failures, batcher.retry_at), (0, 0));
    }

    /// meeting.json FLUSH: a refusal (4xx) drops the batch, never requeued;
    /// a transport error or 5xx retries.
    #[test]
    fn a_refused_batch_is_dropped() {
        let http = |status| api::ApiError::Http { status, message: "x".into() };
        assert!(refused(&http(400)) && refused(&http(403)) && refused(&http(413)));
        assert!(!refused(&http(408)) && !refused(&http(429)) && !refused(&http(500)) && !refused(&http(503)));
        assert!(!refused(&http(401)) && !refused(&api::ApiError::Unauthorized), "an expired token retries");
        assert!(!refused(&api::ApiError::Transport { message: "down".into(), offline: true }));

        let mut batcher = Batcher::default();
        batcher.step(FlushEvent::Segment, 0, Some(segment("you", 0)));
        assert!(batcher.step(FlushEvent::Wake, 0, None).is_some());
        batcher.refused();
        assert!(batcher.segments.is_empty() && batcher.state == FlushState::default());
        // The next segment flows at once (no backoff after a refusal).
        batcher.step(FlushEvent::Segment, 10, Some(segment("you", 10)));
        assert!(batcher.step(FlushEvent::Wake, 10, None).is_some());
    }

    /// meeting.json FLUSH: at most `maxQueued` segments wait in memory, the
    /// oldest go first.
    #[test]
    fn the_in_memory_queue_is_capped() {
        let max = MEETING.flush.max_queued;
        let mut batcher = Batcher::default();
        batcher.step(FlushEvent::Segment, 0, Some(segment("you", 0)));
        assert!(batcher.step(FlushEvent::Wake, 0, None).is_some(), "a turn in flight: the rest only queues");
        for at in 1..=(max as i64 + 5) {
            batcher.step(FlushEvent::Segment, at, Some(segment("you", at)));
        }
        assert_eq!(batcher.cap(), 5);
        assert_eq!(batcher.segments.len(), max);
        assert_eq!(batcher.state.queued.len(), max);
        assert_eq!(batcher.segments[0], segment("you", 6), "the oldest went");
        assert_eq!(batcher.state.queued[0], 6);
        assert_eq!(batcher.cap(), 0);
    }

    #[test]
    fn a_sign_out_or_switch_purges_the_local_log() {
        assert!(purges_log(Some("acct-1"), None), "signed out");
        assert!(purges_log(Some("acct-1"), Some("acct-2")), "another account");
        assert!(!purges_log(Some("acct-1"), Some("acct-1")));
        assert!(!purges_log(None, Some("acct-1")), "a sign-in deletes nothing");
        assert!(!purges_log(None, None));
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
        let message = domain::meeting::meeting_message(&last);
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
