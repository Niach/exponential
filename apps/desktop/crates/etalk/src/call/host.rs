//! #EXP-1992: THE CALL HOST, pure. What a host does around [`call_step`]
//! while it runs a call: the ring and its give-up, the mic check, the
//! agent's own voice and opening line, the utterances and their queue, the
//! replies, the hang-up and the failure. Locked by `voice-call.json`
//! `hostCases` (replayed by the Rust hosts only: phones and web never host a
//! call). The desktop app (`etalk_gpui::voice_call`) and a server (the CLI
//! daemon's `voice_host`) are shells around ONE [`CallHost`]: each hands in
//! what happened ([`HostInput`], on its own clock in ms) and does what comes
//! back ([`HostEffect`], in order) with its worker, the caller's link and
//! the server. The rings, the worker's lifecycle, the reply watch's reads
//! and the network stay theirs.

use serde::Deserialize;

use super::{
    call_opening, call_ring_ms, call_step, call_voice_ready, call_wire_event, CallEvent, CallOpening, CallPhase, CallState, CallVoice, CallVoiceReady,
    VOICE_CALL,
};
use crate::ListenFailure;

/// S-3: the mic check listens at least this long after `CallListening`.
pub const MIC_CHECK_MIN_MS: u64 = 1500;

/// A call as its host starts it (the ring: phase ringing).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStart {
    /// The worker's call id.
    pub id: String,
    /// The ring's call id (a streamed call), for the log; None = the worker's.
    #[serde(default)]
    pub call_id: Option<String>,
    /// The language the call is held in (its agent's).
    pub language: String,
    /// A streamed call: its audio and its flow events cross the relay
    /// (false = this device's own microphone and output).
    pub remote: bool,
    /// EXP-1423: the host's one draw for this ring ([`call_ring_ms`]).
    pub random: f64,
    /// The device that speaks (`copy.failed`'s `{device}`) and whether it
    /// is not the agent's own (EXP-1857).
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub fallback: bool,
    /// #EXP-1961: the call's own session when it is known at the start
    /// (None = none, or one asked later: [`HostInput::SessionAsked`]).
    #[serde(default)]
    pub session: Option<String>,
    /// FEED-276: the agent's own voice on this host at the start.
    #[serde(default)]
    pub voice: CallVoice,
    /// EXP-1859: a TURN call (a wake turn dialled here): the wake phrase
    /// the caller heard. No ring, no greeting, one utterance, one reply
    /// ([`turn_host_step`]). None = an ordinary call.
    #[serde(default)]
    pub turn: Option<String>,
}

/// EXP-1859 (voice-call.json `turnHostCases`): where a turn call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnHostState {
    /// The worker hears the turn.
    #[default]
    Listening,
    /// The utterance was sent: the first reply that starts now is the turn's.
    Waiting,
    /// The turn's one reply is spoken.
    Speaking,
    /// Hung up.
    Done,
}

/// What happened in a turn call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum TurnHostEvent {
    /// The worker's one utterance (the phrase already taken off).
    Said { text: String },
    /// Only the phrase was heard.
    Empty,
    ReplyStarted,
    ReplyFinished,
    /// `constants.turnReplyWaitMs` since the send.
    ReplyWaitOver,
}

/// What the host does in a turn call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum TurnHostAction {
    /// The utterance goes into the member's conversation with the agent.
    Send { text: String },
    /// The reply that started is spoken on the call's stream.
    Speak,
    /// The reason-less `call_end`.
    HangUp,
}

/// EXP-1859: one step of a turn call on its host: ONE utterance is sent (a
/// blank one or `empty` hangs up at once), the FIRST reply that starts
/// after the send is spoken and its end hangs up, and no reply within
/// `turnReplyWaitMs` hangs up.
pub fn turn_host_step(state: TurnHostState, event: &TurnHostEvent) -> (TurnHostState, Vec<TurnHostAction>) {
    use TurnHostState::*;
    match (state, event) {
        (Listening, TurnHostEvent::Said { text }) if !text.trim().is_empty() => (Waiting, vec![TurnHostAction::Send { text: text.clone() }]),
        (Listening, TurnHostEvent::Said { .. } | TurnHostEvent::Empty) => (Done, vec![TurnHostAction::HangUp]),
        (Waiting, TurnHostEvent::ReplyStarted) => (Speaking, vec![TurnHostAction::Speak]),
        (Waiting, TurnHostEvent::ReplyWaitOver) => (Done, vec![TurnHostAction::HangUp]),
        (Speaking, TurnHostEvent::ReplyFinished) => (Done, vec![TurnHostAction::HangUp]),
        _ => (state, Vec::new()),
    }
}

/// The host's timers ([`HostInput::Tick`] fires every one that is due).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostTimer {
    /// EXP-1423: the ring's earliest answer passed (`rang`).
    Ring,
    /// S-3: `maxRingMs` without `ready`: failed `unavailable`.
    GiveUp,
    /// S-3: the mic check's end.
    MicCheck,
    /// FEED-276: `hostVoice.waitMs` since the ring started.
    VoiceWait,
    /// #EXP-1961: `openingWaitMs` since the voice was in.
    OpeningWait,
    /// EXP-1859: a turn call's `turnReplyWaitMs` since its utterance was sent.
    ReplyWait,
}

const TIMERS: [HostTimer; 6] =
    [HostTimer::Ring, HostTimer::GiveUp, HostTimer::MicCheck, HostTimer::VoiceWait, HostTimer::OpeningWait, HostTimer::ReplyWait];

/// What happened, as the host's shell hands it in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HostInput {
    /// The ring's clock starts and the worker listens: a streamed call once
    /// the caller is on the line (#EXP-1941: never before), a call run here
    /// at its start. Once.
    Begin,
    /// The host's clock moved: every timer due fires, in [`HostTimer`] order.
    Tick,
    /// One timer fired (a host that arms them one by one).
    Timer { timer: HostTimer },
    /// The worker listens (`CallListening`).
    Listening,
    /// The microphone's level (`Level`).
    Level { level: f32 },
    /// The worker's `Partial`, `SpeechStarted` and `Said`.
    Partial { text: String },
    SpeechStarted,
    Said { text: String },
    /// EXP-1859: a turn call's worker heard only the phrase (`CallEmpty`).
    Empty,
    /// The worker could not listen (`ListenFailed`).
    ListenFailed { reason: ListenFailure },
    /// FEED-276: the agent's clip as this host has it now; `preparing` = a
    /// Prepare with it just went out (its `Voices` not in yet).
    Voice {
        clip: CallVoice,
        #[serde(default)]
        preparing: bool,
    },
    /// FEED-276: the worker answered a Prepare (`Voices`).
    Prepared,
    /// #EXP-1961: the call's own session is being asked.
    SessionAsked,
    /// #EXP-1961: the server answered (None = no call session: the chat
    /// takes the call).
    Session {
        #[serde(default)]
        session: Option<String>,
    },
    /// The agent's reply the call would say: the newest new one of the
    /// call's conversation ([`crate::voice::reply_to_speak`]), its spoken text.
    Reply { id: String, text: String },
    /// What the worker said ended: `Finished`, or a Stop.
    Spoken { id: String },
    /// What the worker was to say could not be said (`Failed`, or nothing
    /// here speaks).
    NotSpoken { id: String },
    /// Mute: the person here, or the caller's `call_mute`.
    Mute { muted: bool },
    /// The other side's `call_end` (`hangUpRules`).
    HungUp {
        #[serde(default)]
        reason: Option<String>,
    },
    /// End call (or Cancel) here.
    End,
    /// The host fails the call with `code`: its worker died, the stream
    /// dropped, a setting took the worker away.
    Fail { code: String },
    /// #EXP-1982: an utterance this host can never store (the call ends).
    Unstorable,
}

/// What the host's shell does now, in order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "do", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HostEffect {
    /// A streamed call's `call_event` to the caller (`wire.control`).
    Event { event: serde_json::Value },
    /// Barge-in: the caller drops the reply audio it still holds.
    Flush,
    /// The worker's `Call` (`ring_ms` = how long a call run here rings on
    /// its own output; a streamed one rings on the caller).
    /// EXP-1859: `turn` = a turn call's phrase (the worker hears ONE turn).
    Listen {
        id: String,
        language: String,
        remote: bool,
        ring_ms: u32,
        #[serde(default)]
        turn: Option<String>,
    },
    /// The worker's `CallMute`.
    Mute { id: String, muted: bool },
    /// The worker says `text` (the opening line, or a reply; `id` = its row).
    Speak { id: String, text: String },
    /// The worker stops what it says.
    Stop,
    /// The worker's `Hangup`.
    Hangup { id: String },
    /// The caller is hung up on (`call_end` from here).
    End,
    /// An utterance to the agent: into the call's own session (#EXP-1961),
    /// or (None) the chat.
    Send {
        text: String,
        #[serde(default)]
        session: Option<String>,
    },
    /// #EXP-1961: the call's own session hears the hang-up (`voice.callEnded`).
    CallEnded { session: String },
    /// The call ended with a failure: its toast (`copy.failed`).
    Toast { text: String },
}

/// The code a `ListenFailed` fails the call with (None = `downloading`: a
/// voice still downloading keeps ringing).
pub fn listen_failed_code(reason: ListenFailure) -> Option<&'static str> {
    Some(match reason {
        ListenFailure::Downloading => return None,
        ListenFailure::NoMicrophone => "no_microphone",
        ListenFailure::NotGranted => "not_granted",
        // NEW-57: voice-call.json `failed.meeting`.
        ListenFailure::Meeting => "meeting",
        // S-3: the download or the load failed.
        ListenFailure::Unavailable => "unavailable",
        ListenFailure::Failed => "dropped",
    })
}

/// One call on its host.
#[derive(Debug, Clone)]
pub struct CallHost {
    id: String,
    /// The log's name for the call (the ring's call id, or the worker's).
    label: String,
    language: String,
    remote: bool,
    state: CallState,
    /// EXP-1423: this call's earliest answer, ms after the ring started.
    ring_ms: u64,
    /// When the ring started ([`HostInput::Begin`]).
    started: Option<u64>,
    rang: bool,
    /// S-3: when the worker listened, and whether a level reached `noSoundLevel` since.
    listening_at: Option<u64>,
    heard: bool,
    mic_checked: bool,
    /// FEED-276: the agent's clip, a Prepare with it unconfirmed, the bound passed.
    clip: CallVoice,
    preparing: bool,
    voice_waited: bool,
    /// The wait for the agent's voice was logged.
    waiting_logged: bool,
    /// #EXP-1961: the voice is in; `ready` waits for the opening line.
    voice_ready_at: Option<u64>,
    opening_waited: bool,
    /// #EXP-1961: the opening line that came during the ring (when, its row, its text).
    opening: Option<(u64, String, String)>,
    /// The opening line's id once it is said.
    opening_said: Option<String>,
    /// #EXP-1961: the pickup was silent: a late opening is never said.
    late_opening: bool,
    /// #EXP-1961: an utterance went out (a later reply is no opening).
    sent: bool,
    /// S-3: rang and ready answered it (the timer counts from here).
    answered_at: Option<u64>,
    session: Option<String>,
    session_pending: bool,
    /// #EXP-1981: utterances said while the call's session was asked.
    held: Vec<String>,
    /// Utterances said while the agent's turn ran (the queue bar's share).
    queued: usize,
    /// What the worker says for the call now (the opening or a reply).
    speaking: Option<String>,
    /// #FEED-268: what the end line counts (never a word of the call).
    utterances: u32,
    replies: u32,
    /// EXP-1859: a turn call: its phrase, where it stands, when it sent.
    turn_phrase: Option<String>,
    turn: TurnHostState,
    turn_sent_at: Option<u64>,
}

/// #FEED-268: who ended a call and why, as the end line names it
/// (voice-call.json `hangUpRules`): a reason-less `call_end` is the caller's
/// End call; `idle` and `max_length` are the relay's (or this host's own
/// socket); `dropped` is the relay saying the caller's socket went away.
fn ended_by(event: &CallEvent, remote: bool) -> (&'static str, String) {
    match event {
        CallEvent::End => (if remote { "host" } else { "user" }, "hung_up".into()),
        CallEvent::HungUp(None) => ("caller", "hung_up".into()),
        CallEvent::HungUp(Some(reason)) => match reason.as_str() {
            "dropped" => ("caller", "dropped".into()),
            "idle" | "max_length" => ("relay", reason.clone()),
            _ => ("link", reason.clone()),
        },
        CallEvent::Failed(code) => ("host", format!("failed:{code}")),
        _ => ("host", "other".into()),
    }
}

impl CallHost {
    /// The call rings (`start`; a streamed call's `start` never goes out).
    pub fn new(start: HostStart) -> Self {
        let state = call_step(
            &CallState::default(),
            &CallEvent::Start { device: start.device, language: Some(start.language.clone()), fallback: start.fallback },
        );
        CallHost {
            label: start.call_id.unwrap_or_else(|| start.id.clone()),
            id: start.id,
            language: start.language,
            remote: start.remote,
            state,
            ring_ms: call_ring_ms(start.random),
            started: None,
            rang: false,
            listening_at: None,
            heard: false,
            mic_checked: false,
            clip: start.voice,
            preparing: false,
            voice_waited: false,
            waiting_logged: false,
            voice_ready_at: None,
            opening_waited: false,
            opening: None,
            opening_said: None,
            late_opening: false,
            sent: false,
            answered_at: None,
            session: start.session,
            session_pending: false,
            held: Vec::new(),
            queued: 0,
            speaking: None,
            utterances: 0,
            replies: 0,
            turn_phrase: start.turn,
            turn: TurnHostState::default(),
            turn_sent_at: None,
        }
    }

    /// #EXP-1981: what a call session that answers after its call ended
    /// gets: the hang-up (it would read as running, and the next call would
    /// resume it).
    pub fn session_after_end(label: &str, session: Option<String>) -> Vec<HostEffect> {
        let Some(session) = session else { return Vec::new() };
        log::info!("[voice] call {label}: its session {session} came after the hang-up");
        vec![HostEffect::CallEnded { session }]
    }

    /// The worker's call id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The flow state (what the call surfaces draw: [`super::call_view`]).
    pub fn state(&self) -> &CallState {
        &self.state
    }

    /// The call is over (its last effects went out).
    pub fn ended(&self) -> bool {
        self.state.phase == CallPhase::Hidden
    }

    /// The ring started ([`HostInput::Begin`]).
    pub fn begun(&self) -> bool {
        self.started.is_some()
    }

    /// #EXP-1961: the call's own session (None = the chat's).
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// #EXP-1981: the call's own session is still asked: nothing of the
    /// chat is the call's meanwhile.
    pub fn session_pending(&self) -> bool {
        self.session_pending
    }

    /// When rang and ready answered it (the timer's zero).
    pub fn answered_at(&self) -> Option<u64> {
        self.answered_at
    }

    /// What the worker says for the call now.
    pub fn speaking(&self) -> Option<&str> {
        self.speaking.as_deref()
    }

    /// #EXP-1961: the opening line's id once it is said at the pickup.
    pub fn opening(&self) -> Option<&str> {
        self.opening_said.as_deref()
    }

    /// Utterances queued behind the agent's turn.
    pub fn queued(&self) -> usize {
        self.queued
    }

    /// #EXP-1961: the voice is in and the pickup waits for the opening line
    /// (the host looks for it now).
    pub fn awaits_opening(&self) -> bool {
        self.state.phase == CallPhase::Ringing && !self.state.ready && self.voice_ready_at.is_some()
    }

    /// When `timer` is due (None = not armed).
    pub fn due(&self, timer: HostTimer) -> Option<u64> {
        let started = self.started?;
        let ringing = self.state.phase == CallPhase::Ringing && !self.state.ready;
        match timer {
            HostTimer::Ring => (!self.rang).then_some(started + self.ring_ms),
            HostTimer::GiveUp => ringing.then_some(started + VOICE_CALL.constants.max_ring_ms),
            HostTimer::MicCheck => {
                // EXP-1859: a turn call has no ring to check the microphone in.
                let listening = self.listening_at.filter(|_| !self.mic_checked && !self.is_turn())?;
                Some((started + self.ring_ms).max(listening + MIC_CHECK_MIN_MS))
            }
            HostTimer::VoiceWait => {
                let waits = ringing && self.listening_at.is_some() && matches!(self.voice(), CallVoice::Fetching | CallVoice::Installing);
                (waits && !self.voice_waited).then_some(started + VOICE_CALL.host_voice.wait_ms)
            }
            HostTimer::OpeningWait => {
                let ready = self.voice_ready_at.filter(|_| ringing && !self.opening_waited)?;
                Some(ready + VOICE_CALL.call_session.opening_wait_ms)
            }
            HostTimer::ReplyWait => {
                let sent = self.turn_sent_at.filter(|_| self.turn == TurnHostState::Waiting)?;
                Some(sent + VOICE_CALL.constants.turn_reply_wait_ms)
            }
        }
    }

    /// EXP-1859: this is a turn call.
    pub fn is_turn(&self) -> bool {
        self.turn_phrase.is_some()
    }

    /// The next instant a timer is due (a host that arms one wakes then).
    pub fn next_due(&self) -> Option<u64> {
        TIMERS.iter().filter_map(|timer| self.due(*timer)).min()
    }

    /// One input at `now` (the host's clock, ms): what the host does, in order.
    pub fn handle(&mut self, input: HostInput, now: u64) -> Vec<HostEffect> {
        let mut out = Vec::new();
        if self.ended() {
            if let HostInput::Session { session } = input {
                return Self::session_after_end(&self.label, session);
            }
            return out;
        }
        match input {
            HostInput::Begin => self.begin(now, &mut out),
            HostInput::Tick => {
                for timer in TIMERS {
                    if !self.ended() && self.due(timer).is_some_and(|at| at <= now) {
                        self.fire(timer, now, &mut out);
                    }
                }
            }
            HostInput::Timer { timer } => self.fire(timer, now, &mut out),
            HostInput::Listening => {
                if self.listening_at.is_none() {
                    log::info!("[voice] call {}: the worker listens {} ms after the ring started", self.label, now.saturating_sub(self.started.unwrap_or(now)));
                }
                self.listening_at = Some(now);
                self.check_voice(now, &mut out);
            }
            HostInput::Level { level } => {
                if self.listening_at.is_some() && level >= VOICE_CALL.constants.no_sound_level {
                    self.heard = true;
                }
            }
            HostInput::Partial { text } => self.step(CallEvent::Partial(text), now, &mut out),
            HostInput::SpeechStarted => {
                if self.remote {
                    out.push(HostEffect::Flush);
                }
                self.step(CallEvent::SpeechStarted, now, &mut out);
            }
            HostInput::Said { text } if self.is_turn() => self.turn_event(TurnHostEvent::Said { text }, None, now, &mut out),
            HostInput::Said { text } => self.said(text, now, &mut out),
            HostInput::Empty => {
                if self.is_turn() {
                    self.turn_event(TurnHostEvent::Empty, None, now, &mut out);
                }
            }
            HostInput::ListenFailed { reason } => {
                if let Some(code) = listen_failed_code(reason) {
                    log::warn!("[voice] call {}: the call could not listen ({reason:?})", self.label);
                    self.fail(code, now, &mut out);
                }
            }
            HostInput::Voice { clip, preparing } => {
                self.clip = clip;
                self.preparing |= preparing;
                self.check_voice(now, &mut out);
            }
            HostInput::Prepared => {
                if std::mem::take(&mut self.preparing) {
                    self.check_voice(now, &mut out);
                }
            }
            HostInput::SessionAsked => {
                if self.session.is_none() {
                    self.session_pending = true;
                }
            }
            HostInput::Session { session } => self.on_session(session, now, &mut out),
            HostInput::Reply { id, text } => self.reply(id, text, now, &mut out),
            HostInput::Spoken { id } => self.spoken(&id, now, &mut out),
            HostInput::NotSpoken { id } => {
                // EXP-1930: an opening line that cannot be said: nothing will be.
                if self.opening_said.as_deref() == Some(id.as_str()) {
                    log::warn!("[voice] call {}: the opening line could not be said", self.label);
                    self.fail("unavailable", now, &mut out);
                } else {
                    self.spoken(&id, now, &mut out);
                }
            }
            HostInput::Mute { muted } => {
                out.push(HostEffect::Mute { id: self.id.clone(), muted });
                self.step(if muted { CallEvent::Mute } else { CallEvent::Unmute }, now, &mut out);
            }
            HostInput::HungUp { reason } => self.close(false, CallEvent::HungUp(reason), now, &mut out),
            HostInput::End => self.close(true, CallEvent::End, now, &mut out),
            HostInput::Fail { code } => self.fail(&code, now, &mut out),
            HostInput::Unstorable => {
                log::warn!("[voice] call {}: an utterance can never be stored here: the call ends", self.label);
                self.fail("dropped", now, &mut out);
            }
        }
        out
    }

    fn begin(&mut self, now: u64, out: &mut Vec<HostEffect>) {
        if self.started.is_some() {
            return;
        }
        self.started = Some(now);
        if self.is_turn() {
            log::info!("[voice] call {}: a wake turn (no ring)", self.label);
        } else {
            log::info!("[voice] call {}: ringing ({}, answers after {} ms at the earliest)", self.label, if self.remote { "streamed from a caller" } else { "run here" }, self.ring_ms);
        }
        // A call run here rings on its own output until the opening (at
        // most `maxRingMs`); a streamed one rings on the caller; EXP-1859:
        // a turn call never rings.
        let ring_ms = if self.remote || self.is_turn() { 0 } else { VOICE_CALL.constants.max_ring_ms as u32 };
        out.push(HostEffect::Listen {
            id: self.id.clone(),
            language: self.language.clone(),
            remote: self.remote,
            ring_ms,
            turn: self.turn_phrase.clone(),
        });
        if self.is_turn() {
            // EXP-1859: no ring: `rang` and `ready` at once.
            self.rang = true;
            self.step(CallEvent::Rang, now, out);
            self.step(CallEvent::Ready, now, out);
        }
    }

    /// EXP-1859: one [`turn_host_step`] of a turn call, and what it asks
    /// for (`reply` = the row and text a `replyStarted` would speak).
    fn turn_event(&mut self, event: TurnHostEvent, reply: Option<(String, String)>, now: u64, out: &mut Vec<HostEffect>) {
        let (next, actions) = turn_host_step(self.turn, &event);
        self.turn = next;
        for action in actions {
            match action {
                TurnHostAction::Send { text } => {
                    self.sent = true;
                    self.utterances += 1;
                    self.turn_sent_at = Some(now);
                    self.step(CallEvent::Said(text.clone()), now, out);
                    out.push(HostEffect::Send { text, session: self.session.clone() });
                }
                TurnHostAction::Speak => {
                    let Some((id, text)) = reply.clone() else { continue };
                    self.speaking = Some(id.clone());
                    self.replies += 1;
                    out.push(HostEffect::Speak { id, text: text.clone() });
                    self.step(CallEvent::ReplyStarted(crate::voice::first_sentence(&text).to_string()), now, out);
                }
                TurnHostAction::HangUp => self.close(true, CallEvent::End, now, out),
            }
        }
    }

    fn fire(&mut self, timer: HostTimer, now: u64, out: &mut Vec<HostEffect>) {
        match timer {
            HostTimer::Ring => {
                if !std::mem::replace(&mut self.rang, true) {
                    self.step(CallEvent::Rang, now, out);
                }
            }
            HostTimer::GiveUp => {
                if self.state.phase == CallPhase::Ringing && !self.state.ready {
                    log::warn!("[voice] call {}: the voice was not ready after the longest ring", self.label);
                    self.fail("unavailable", now, out);
                }
            }
            HostTimer::MicCheck => {
                if self.listening_at.is_some() && !std::mem::replace(&mut self.mic_checked, true) && !self.heard {
                    log::info!("[voice] call {}: the microphone stayed silent", self.label);
                    self.step(CallEvent::NoSound, now, out);
                }
            }
            HostTimer::VoiceWait => {
                self.voice_waited = true;
                self.check_voice(now, out);
            }
            HostTimer::OpeningWait => {
                self.opening_waited = true;
                self.check_opening(now, out);
            }
            HostTimer::ReplyWait => {
                if self.turn == TurnHostState::Waiting {
                    log::info!("[voice] call {}: no reply in time: the turn ends", self.label);
                    self.turn_event(TurnHostEvent::ReplyWaitOver, None, now, out);
                }
            }
        }
    }

    /// FEED-276: the clip as `hostVoice` reads it (landed, its Prepare
    /// unconfirmed = installing).
    fn voice(&self) -> CallVoice {
        if self.clip == CallVoice::Installed && self.preparing { CallVoice::Installing } else { self.clip }
    }

    /// FEED-276 (`hostVoice`): once the worker listens, `ready` goes when
    /// the agent's own voice is in, or with the built-in voice when its clip
    /// failed or `hostVoice.waitMs` passed.
    fn check_voice(&mut self, now: u64, out: &mut Vec<HostEffect>) {
        if self.listening_at.is_none() || self.state.ready || self.state.phase != CallPhase::Ringing {
            return;
        }
        let voice = self.voice();
        let elapsed = now.saturating_sub(self.started.unwrap_or(now));
        let waited = if self.voice_waited { elapsed.max(VOICE_CALL.host_voice.wait_ms) } else { elapsed };
        match call_voice_ready(voice, waited) {
            CallVoiceReady::Wait => {
                if !std::mem::replace(&mut self.waiting_logged, true) {
                    log::info!("[voice] call {}: waiting for the agent's own voice ({voice:?})", self.label);
                }
            }
            CallVoiceReady::Ready => self.voice_in(now, out),
            CallVoiceReady::BuiltIn => {
                log::warn!("[voice] call {}: the agent's own voice is not here ({voice:?} after {waited} ms); the built-in voice speaks", self.label);
                self.voice_in(now, out);
            }
        }
    }

    /// The voice is in: `ready`, or (#EXP-1961) with a call session the
    /// opening line is waited for first (`callSession.openingCases`).
    fn voice_in(&mut self, now: u64, out: &mut Vec<HostEffect>) {
        if self.session.is_none() && !self.session_pending {
            return self.step(CallEvent::Ready, now, out);
        }
        self.voice_ready_at.get_or_insert(now);
        self.check_opening(now, out);
    }

    /// #EXP-1961: a held `ready` goes once the opening line is in, or
    /// `openingWaitMs` after the voice was (a silent pickup); #EXP-1981 a
    /// call whose session never came takes the chat at once.
    fn check_opening(&mut self, now: u64, out: &mut Vec<HostEffect>) {
        if self.state.ready || self.state.phase != CallPhase::Ringing {
            return;
        }
        let Some(ready_at) = self.voice_ready_at else { return };
        if self.session.is_none() && !self.session_pending {
            return self.step(CallEvent::Ready, now, out);
        }
        let deadline = ready_at + VOICE_CALL.call_session.opening_wait_ms;
        let now = if self.opening_waited { now.max(deadline) } else { now };
        let opening = self.opening.as_ref().filter(|_| self.session.is_some()).map(|(at, _, _)| *at);
        match call_opening(Some(ready_at), opening, now) {
            CallOpening::Wait => {}
            CallOpening::Speak => self.step(CallEvent::Ready, now, out),
            CallOpening::Listen => {
                log::info!("[voice] call {}: no opening line in time: picked up silently", self.label);
                self.step(CallEvent::Ready, now, out);
            }
        }
    }

    /// One flow step: the caller hears it, the machine follows; rang and
    /// ready answered = the pickup.
    fn step(&mut self, event: CallEvent, now: u64, out: &mut Vec<HostEffect>) {
        if self.remote {
            if let Some(event) = call_wire_event(&event) {
                out.push(HostEffect::Event { event });
            }
        }
        let before = self.state.phase;
        self.state = call_step(&self.state, &event);
        if let Some(text) = self.state.toast.take() {
            out.push(HostEffect::Toast { text });
        }
        if before == CallPhase::Ringing && self.state.phase == CallPhase::Listening {
            self.pick_up(now, out);
        }
    }

    /// S-3 (#EXP-1966): answered: the agent's own opening line is said, or
    /// it listens at once (#EXP-1961 no fixed greeting, a call without its
    /// session too).
    fn pick_up(&mut self, now: u64, out: &mut Vec<HostEffect>) {
        self.answered_at = Some(now);
        // EXP-1859: a turn call has no ring, no opening and nothing to finish.
        if self.is_turn() {
            return;
        }
        let opening = self.opening.is_some() && self.session.is_some();
        log::info!(
            "[voice] call {}: answered {} ms after the ring started ({})",
            self.label,
            now.saturating_sub(self.started.unwrap_or(now)),
            if opening { "the opening line is said" } else { "picked up listening" }
        );
        match self.opening.clone().filter(|_| self.session.is_some()) {
            Some((_, id, text)) => {
                self.opening_said = Some(id.clone());
                self.speaking = Some(id.clone());
                out.push(HostEffect::Speak { id, text: text.clone() });
                self.step(CallEvent::ReplyStarted(crate::voice::first_sentence(&text).to_string()), now, out);
            }
            None => {
                // #EXP-1981: a session still asked has its opening still to come, too.
                self.late_opening = self.session.is_some() || self.session_pending;
                self.step(CallEvent::ReplyFinished, now, out);
            }
        }
    }

    /// A finished utterance: a plain message to the agent.
    fn said(&mut self, text: String, now: u64, out: &mut Vec<HostEffect>) {
        if text.trim().is_empty() {
            return;
        }
        // S-3: words during the ring are not the call's.
        if self.state.phase == CallPhase::Ringing {
            log::info!("[voice] call {}: an utterance during the ring was dropped", self.label);
            return;
        }
        let busy = matches!(self.state.phase, CallPhase::Thinking | CallPhase::Speaking);
        if busy {
            self.queued += 1;
        }
        self.sent = true;
        self.utterances += 1;
        let event = if busy { CallEvent::Queued { count: self.queued, text: text.clone() } } else { CallEvent::Said(text.clone()) };
        self.step(event, now, out);
        if self.session_pending {
            // #EXP-1981: its answer is read where the session says: held.
            self.held.push(text);
        } else {
            out.push(HostEffect::Send { text, session: self.session.clone() });
        }
    }

    /// #EXP-1981: the call's session answered: its utterances go there (or
    /// the chat's on None), and a held `ready` may go.
    fn on_session(&mut self, session: Option<String>, now: u64, out: &mut Vec<HostEffect>) {
        if !std::mem::take(&mut self.session_pending) {
            return;
        }
        match session {
            Some(session) => {
                log::info!("[voice] call {}: in its own session {session}", self.label);
                self.session = Some(session);
            }
            // No session: the chat takes the call (no late opening to drop).
            None => self.late_opening = false,
        }
        for text in std::mem::take(&mut self.held) {
            out.push(HostEffect::Send { text, session: self.session.clone() });
        }
        self.check_opening(now, out);
    }

    /// The agent's reply: during the ring a call session's opening line
    /// (said at the pickup), else said now; a late opening after a silent
    /// pickup is never said.
    fn reply(&mut self, id: String, text: String, now: u64, out: &mut Vec<HostEffect>) {
        // #EXP-1981: nothing of the chat is the call's while its session is asked.
        if self.session_pending {
            return;
        }
        // EXP-1859: a turn call speaks the FIRST reply after its send, only.
        if self.is_turn() {
            return self.turn_event(TurnHostEvent::ReplyStarted, Some((id, text)), now, out);
        }
        if self.state.phase == CallPhase::Ringing {
            // #EXP-1936: a reply while it rings is not the call's; #EXP-1961:
            // its own session's is the opening line.
            if self.session.is_some() {
                if self.opening.is_none() {
                    self.opening = Some((now, id, text));
                }
                self.check_opening(now, out);
            }
            return;
        }
        if std::mem::take(&mut self.late_opening) && !self.sent {
            log::info!("[voice] call {}: the opening line came after the pickup: not said", self.label);
            return;
        }
        self.speaking = Some(id.clone());
        self.replies += 1;
        // The agent took the queued lines in with this turn.
        self.queued = 0;
        out.push(HostEffect::Speak { id, text: text.clone() });
        self.step(CallEvent::ReplyStarted(crate::voice::first_sentence(&text).to_string()), now, out);
    }

    /// What the worker said for the call ended: it listens again (a
    /// barge-in already did).
    fn spoken(&mut self, id: &str, now: u64, out: &mut Vec<HostEffect>) {
        if self.speaking.as_deref() != Some(id) {
            return;
        }
        self.speaking = None;
        if self.state.phase == CallPhase::Speaking {
            self.step(CallEvent::ReplyFinished, now, out);
        }
        // EXP-1859: the turn's one reply ended: the host hangs up.
        if self.is_turn() {
            self.turn_event(TurnHostEvent::ReplyFinished, None, now, out);
        }
    }

    /// Hang up the worker (and the caller when `tell`), then step `event`.
    fn close(&mut self, tell: bool, event: CallEvent, now: u64, out: &mut Vec<HostEffect>) {
        out.push(HostEffect::Hangup { id: self.id.clone() });
        if self.speaking.take().is_some() {
            out.push(HostEffect::Stop);
        }
        if tell && self.remote {
            out.push(HostEffect::End);
        }
        if let Some(session) = self.session.clone() {
            out.push(HostEffect::CallEnded { session });
        }
        // #EXP-1981: ended while its session was asked: what it held goes
        // into the chat, never lost.
        for text in std::mem::take(&mut self.held) {
            out.push(HostEffect::Send { text, session: None });
        }
        // #FEED-268: ONE line that says who ended it, why and how far it got.
        let (by, reason) = ended_by(&event, self.remote);
        let phase = self.state.phase;
        self.step(event, now, out);
        log::info!(
            "[voice] call {}: ended by={by} reason={reason} phase={phase:?} answered={} lasted={} ms listening={} heard={} utterances={} replies={}",
            self.label,
            self.answered_at.is_some(),
            now.saturating_sub(self.started.unwrap_or(now)),
            self.listening_at.is_some(),
            self.heard,
            self.utterances,
            self.replies,
        );
    }

    /// Fail the call with `code`: a streamed caller hears the code BEFORE
    /// the hang-up (its toast, never the reason-less silent end).
    fn fail(&mut self, code: &str, now: u64, out: &mut Vec<HostEffect>) {
        log::warn!("[voice] call {}: failed {code}", self.label);
        if self.remote {
            out.push(HostEffect::Event { event: serde_json::json!({ "t": "failed", "code": code }) });
        }
        self.close(true, CallEvent::Failed(code.to_string()), now, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #FEED-268: the end line names who ended the call and why (`hangUpRules`).
    #[test]
    fn the_end_line_names_who_ended_the_call_and_why() {
        let by = |event: CallEvent, remote: bool| ended_by(&event, remote);
        assert_eq!(by(CallEvent::HungUp(None), true), ("caller", "hung_up".to_string()));
        assert_eq!(by(CallEvent::HungUp(Some("dropped".into())), true), ("caller", "dropped".to_string()));
        assert_eq!(by(CallEvent::HungUp(Some("idle".into())), true), ("relay", "idle".to_string()));
        assert_eq!(by(CallEvent::HungUp(Some("max_length".into())), true), ("relay", "max_length".to_string()));
        assert_eq!(by(CallEvent::HungUp(Some("no_key".into())), true), ("link", "no_key".to_string()));
        assert_eq!(by(CallEvent::End, true), ("host", "hung_up".to_string()));
        assert_eq!(by(CallEvent::End, false), ("user", "hung_up".to_string()));
        assert_eq!(by(CallEvent::Failed("unavailable".into()), true), ("host", "failed:unavailable".to_string()));
    }

    /// One event of a `hostCases` row, at its instant.
    #[derive(Deserialize)]
    struct Timed {
        at: u64,
        #[serde(flatten)]
        input: HostInput,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        start: HostStart,
        events: Vec<Timed>,
        effects: Vec<Vec<HostEffect>>,
        phase: String,
        #[serde(default)]
        next_due: Option<u64>,
    }

    fn phase_name(phase: CallPhase) -> &'static str {
        match phase {
            CallPhase::Hidden => "hidden",
            CallPhase::Ringing => "ringing",
            CallPhase::Listening => "listening",
            CallPhase::Thinking => "thinking",
            CallPhase::Speaking => "speaking",
        }
    }

    /// #EXP-1992: every row of `hostCases`, effect by effect.
    #[test]
    fn the_call_host_replays_the_host_cases() {
        let fixture: serde_json::Value = serde_json::from_str(super::super::VOICE_CALL_JSON).unwrap();
        let cases: Vec<Case> = serde_json::from_value(fixture["hostCases"].clone()).expect("hostCases parse");
        assert!(cases.len() >= 20, "{}", cases.len());
        for case in cases {
            assert_eq!(case.events.len(), case.effects.len(), "{}: one effect list per event", case.name);
            let mut host = CallHost::new(case.start);
            for (index, (event, expected)) in case.events.into_iter().zip(case.effects).enumerate() {
                let input = format!("{:?}", event.input);
                let effects = host.handle(event.input, event.at);
                assert_eq!(effects, expected, "{}: event {index} ({input} at {})", case.name, event.at);
            }
            assert_eq!(phase_name(host.state().phase), case.phase, "{}", case.name);
            if let Some(due) = case.next_due {
                assert_eq!(host.next_due(), Some(due), "{}: next due", case.name);
            }
        }
    }

    /// A tick at any instant fires exactly what one timer at a time would.
    #[test]
    fn a_tick_fires_the_timers_in_order() {
        let start = HostStart {
            id: "call-1".into(),
            call_id: None,
            language: "en".into(),
            remote: true,
            random: 0.,
            device: "mint".into(),
            fallback: false,
            session: None,
            voice: CallVoice::None,
            turn: None,
        };
        let mut ticked = CallHost::new(start.clone());
        let mut timed = CallHost::new(start);
        for host in [&mut ticked, &mut timed] {
            host.handle(HostInput::Begin, 0);
            host.handle(HostInput::Listening, 0);
        }
        let all = ticked.handle(HostInput::Tick, 10_000);
        let mut one_by_one = Vec::new();
        while let Some(due) = timed.next_due().filter(|due| *due <= 10_000) {
            let timer = TIMERS.into_iter().find(|timer| timed.due(*timer) == Some(due)).unwrap();
            one_by_one.extend(timed.handle(HostInput::Timer { timer }, due));
        }
        assert_eq!(all, one_by_one);
        assert_eq!(ticked.state(), timed.state());
    }

    #[derive(Deserialize)]
    struct TurnCase {
        name: String,
        events: Vec<TurnHostEvent>,
        actions: Vec<TurnHostAction>,
    }

    /// EXP-1859: every row of `turnHostCases` through the pure step.
    #[test]
    fn the_turn_host_step_replays_the_turn_host_cases() {
        let fixture: serde_json::Value = serde_json::from_str(super::super::VOICE_CALL_JSON).unwrap();
        let cases: Vec<TurnCase> = serde_json::from_value(fixture["turnHostCases"].clone()).expect("turnHostCases parse");
        assert!(cases.len() >= 8, "{}", cases.len());
        for case in cases {
            let mut state = TurnHostState::default();
            let mut actions = Vec::new();
            for event in &case.events {
                let (next, did) = turn_host_step(state, event);
                state = next;
                actions.extend(did);
            }
            assert_eq!(actions, case.actions, "{}", case.name);
        }
    }

    fn turn_start() -> HostStart {
        HostStart {
            id: "call-1".into(),
            call_id: Some("ring-1".into()),
            language: "en".into(),
            remote: true,
            random: 0.9,
            device: "mint".into(),
            fallback: false,
            session: None,
            voice: CallVoice::Fetching,
            turn: Some("Hey Ops".into()),
        }
    }

    fn turn_host() -> CallHost {
        let mut host = CallHost::new(turn_start());
        let begun = host.handle(HostInput::Begin, 0);
        // No ring: the worker hears ONE turn, and rang + ready go at once.
        assert_eq!(
            begun,
            vec![
                HostEffect::Listen { id: "call-1".into(), language: "en".into(), remote: true, ring_ms: 0, turn: Some("Hey Ops".into()) },
                HostEffect::Event { event: serde_json::json!({ "t": "rang" }) },
                HostEffect::Event { event: serde_json::json!({ "t": "ready" }) },
            ]
        );
        assert_eq!(host.state().phase, CallPhase::Listening);
        assert_eq!(host.handle(HostInput::Listening, 5), Vec::new(), "no greeting, no opening");
        // No ring timer, no give-up, no mic check, no wait for the agent's voice.
        assert_eq!(host.next_due(), None);
        host
    }

    /// EXP-1859: the [`CallHost`] runs every `turnHostCases` row as a turn
    /// call: send = the utterance's Send, speak = a Speak, hangUp = the
    /// worker's Hangup + the reason-less End.
    #[test]
    fn a_turn_call_follows_the_turn_host_cases() {
        let fixture: serde_json::Value = serde_json::from_str(super::super::VOICE_CALL_JSON).unwrap();
        let cases: Vec<TurnCase> = serde_json::from_value(fixture["turnHostCases"].clone()).unwrap();
        for case in cases {
            let mut host = turn_host();
            let mut actions = Vec::new();
            let mut now = 10;
            let mut reply = 0;
            for event in &case.events {
                now += 10;
                let effects = match event {
                    TurnHostEvent::Said { text } => host.handle(HostInput::Said { text: text.clone() }, now),
                    TurnHostEvent::Empty => host.handle(HostInput::Empty, now),
                    TurnHostEvent::ReplyStarted => {
                        reply += 1;
                        host.handle(HostInput::Reply { id: format!("m-{reply}"), text: "All quiet. Nothing new.".into() }, now)
                    }
                    TurnHostEvent::ReplyFinished => host.handle(HostInput::Spoken { id: format!("m-{reply}") }, now),
                    TurnHostEvent::ReplyWaitOver => {
                        let due = host.next_due();
                        match due {
                            Some(due) => host.handle(HostInput::Tick, due),
                            // Not armed (the reply started): a late timer does nothing.
                            None => host.handle(HostInput::Timer { timer: HostTimer::ReplyWait }, now),
                        }
                    }
                };
                for effect in effects {
                    match effect {
                        HostEffect::Send { text, session } => {
                            assert_eq!(session, None, "{}: the chat takes it", case.name);
                            actions.push(TurnHostAction::Send { text });
                        }
                        HostEffect::Speak { .. } => actions.push(TurnHostAction::Speak),
                        HostEffect::End => actions.push(TurnHostAction::HangUp),
                        HostEffect::Toast { text } => panic!("{}: a turn's end is silent ({text})", case.name),
                        _ => {}
                    }
                }
            }
            assert_eq!(actions, case.actions, "{}", case.name);
            assert_eq!(host.ended(), case.actions.last() == Some(&TurnHostAction::HangUp), "{}", case.name);
        }
    }

    /// EXP-1859: a turn call's wire: said, the reply's start and end, then
    /// the hang-up; the reply wait is armed only between the send and the reply.
    #[test]
    fn a_turn_call_tells_the_caller_and_arms_the_reply_wait() {
        let mut host = turn_host();
        assert_eq!(host.handle(HostInput::Partial { text: "what's".into() }, 20), vec![HostEffect::Event { event: serde_json::json!({ "t": "partial", "text": "what's" }) }]);
        assert_eq!(
            host.handle(HostInput::Said { text: "What's new?".into() }, 100),
            vec![
                HostEffect::Event { event: serde_json::json!({ "t": "said", "text": "What's new?" }) },
                HostEffect::Send { text: "What's new?".into(), session: None },
            ]
        );
        assert_eq!(host.next_due(), Some(100 + VOICE_CALL.constants.turn_reply_wait_ms));
        assert_eq!(
            host.handle(HostInput::Reply { id: "m-1".into(), text: "Two issues. Both small.".into() }, 900),
            vec![
                HostEffect::Speak { id: "m-1".into(), text: "Two issues. Both small.".into() },
                HostEffect::Event { event: serde_json::json!({ "t": "replyStarted", "text": "Two issues." }) },
            ]
        );
        assert_eq!(host.next_due(), None);
        assert_eq!(
            host.handle(HostInput::Spoken { id: "m-1".into() }, 4000),
            vec![
                HostEffect::Event { event: serde_json::json!({ "t": "replyFinished" }) },
                HostEffect::Hangup { id: "call-1".into() },
                HostEffect::End,
            ]
        );
        assert!(host.ended());
    }

    /// EXP-1859: a host failure keeps `failed {code}` + the end; the
    /// caller's own end hangs the worker up and tells nobody.
    #[test]
    fn a_turn_call_fails_and_is_cancelled_like_a_call() {
        let mut host = turn_host();
        let failed = host.handle(HostInput::Fail { code: "dropped".into() }, 50);
        assert_eq!(failed[0], HostEffect::Event { event: serde_json::json!({ "t": "failed", "code": "dropped" }) });
        assert!(failed.contains(&HostEffect::End) && host.ended());
        let mut host = turn_host();
        assert_eq!(host.handle(HostInput::HungUp { reason: None }, 50), vec![HostEffect::Hangup { id: "call-1".into() }]);
        assert!(host.ended());
    }

    #[test]
    fn a_listen_failure_reads_its_code() {
        assert_eq!(listen_failed_code(ListenFailure::Downloading), None, "it rings on");
        assert_eq!(listen_failed_code(ListenFailure::NotGranted), Some("not_granted"));
        assert_eq!(listen_failed_code(ListenFailure::Failed), Some("dropped"));
        for code in ["no_microphone", "not_granted", "meeting", "unavailable", "dropped"] {
            assert!(VOICE_CALL.copy.failed.contains_key(code), "{code} has copy");
        }
    }
}
