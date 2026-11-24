//! EXP-1858 (voice per agent c, `fixtures/voice-call.json`): SPEECH ON A
//! SERVER. The daemon is the call host of the agents that run on it,
//! headless: no microphone and no speakers, audio only over the call stream.
//!
//! WORKER. The same speech worker as the IDE (`exponential __speech-worker`,
//! `etalk::SpeechClient`; CrispASR is a pinned runtime download, nothing is
//! linked), but only while one of the member's agents runs here (`agents.list`
//! of the synced teams and the member's own Assistant, #EXP-2080; Runs on =
//! this device). It is PROBED once (no CUDA
//! driver = nothing downloads, `speech` missing at once; else the voice
//! downloads, loads and warms up), then exits: an idle server holds no GPU
//! memory. A ring brings it up again (the ring hides the load) and it exits
//! `serverIdleMs` after the last call.
//!
//! DOCTOR. The `speech` row (device-doctor.json) is the desktop's: ok once a
//! voice answered, off while it downloads, missing without a usable GPU,
//! error once it failed to load (or crashed [`MAX_CRASHES`] times in a row).
//! None while no agent runs here (callers then fall back, `callTarget`).
//!
//! CALL. A `call_ring` on the control socket: `voice.callHostTicket`
//! (naming this device) → `steer::call_host::connect`; the worker listens
//! (`Call`, remote) to the caller's stream; every `Said` goes into the
//! member's conversation with the agent (`sessions.send`, sealed with the
//! conversation's key); the agent's replies (the daemon's synced messages)
//! are spoken (`Speak`) and stream back (`SpeechAudio`). #EXP-1992: the call
//! itself runs on `etalk::call::host` (`hostCases`), the desktop app's call
//! host too: this file hands it the worker's, the link's and the server's
//! events and does what it answers; the rings, the probe, the idle exit, the
//! reply watch's reads and the network are this daemon's.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coding::device_doctor::SpeechState;
use etalk::call::host::{CallHost, HostEffect, HostInput, HostStart};
use etalk::call::{call_ring_random, VOICE_CALL};
use etalk::voice::{reply_to_speak, VoiceContext, VoiceMessage};

/// Crashes in a row before the voice is given up (the desktop's rule).
const MAX_CRASHES: u32 = 3;
/// How often the agents that run here are re-read.
const AGENTS_EVERY: Duration = Duration::from_secs(60);
/// The loop's tick (timers, the reply poll).
const TICK: Duration = Duration::from_millis(100);
/// The probe's call: it only fetches and loads the recognizer.
const PROBE_CALL: &str = "probe";
/// How often a running call looks for the agent's reply.
const REPLY_POLL: Duration = Duration::from_millis(300);

/// One agent that runs on this device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostedAgent {
    pub id: String,
    pub team_id: String,
}

/// The worker as the host drives it (`etalk::SpeechClient`; tests record).
pub trait Speech: Send {
    fn prepare(&self, voice: etalk::VoiceChoice);
    fn call(&self, id: &str, language: &str);
    fn call_audio(&self, id: &str, pcm: &[i16]);
    /// #EXP-1941: `ms` of the caller's audio were lost before the next audio.
    fn call_lost(&self, id: &str, ms: u32);
    fn call_mute(&self, id: &str, muted: bool);
    fn speak(&self, id: &str, text: &str);
    fn stop(&self);
    fn hangup(&self, id: &str);
    fn shutdown(&self);
}

impl Speech for etalk::SpeechClient {
    fn prepare(&self, voice: etalk::VoiceChoice) {
        etalk::SpeechClient::prepare(self, voice);
    }
    fn call(&self, id: &str, language: &str) {
        // Remote: the audio comes only as CallAudio; a server never rings.
        etalk::SpeechClient::call(self, id, language, true, 0);
    }
    fn call_audio(&self, id: &str, pcm: &[i16]) {
        etalk::SpeechClient::call_audio(self, id, pcm);
    }
    fn call_lost(&self, id: &str, ms: u32) {
        etalk::SpeechClient::call_lost(self, id, ms);
    }
    fn call_mute(&self, id: &str, muted: bool) {
        etalk::SpeechClient::call_mute(self, id, muted);
    }
    fn speak(&self, id: &str, text: &str) {
        etalk::SpeechClient::speak(self, id, text);
    }
    fn stop(&self) {
        etalk::SpeechClient::stop(self);
    }
    fn hangup(&self, id: &str) {
        etalk::SpeechClient::hangup(self, id);
    }
    fn shutdown(&self) {
        etalk::SpeechClient::shutdown(self);
    }
}

/// The caller's side of a call (`steer::call_host::CallHost`; tests record).
pub trait Link: Send {
    fn send_event(&self, event: serde_json::Value);
    fn send_speech(&self, pcm: Vec<i16>, rate: u32);
    fn flush(&self);
    fn end(&self);
}

impl Link for steer::call_host::CallHost {
    fn send_event(&self, event: serde_json::Value) {
        steer::call_host::CallHost::send_event(self, event);
    }
    fn send_speech(&self, pcm: Vec<i16>, rate: u32) {
        steer::call_host::CallHost::send_speech(self, pcm, rate);
    }
    fn flush(&self) {
        steer::call_host::CallHost::flush(self);
    }
    fn end(&self) {
        steer::call_host::CallHost::end(self);
    }
}

/// A ring the network side answered: the link is up and the call knows
/// whose voice it speaks with and where its words go.
pub struct Answered {
    pub call_id: String,
    pub agent: HostedAgent,
    pub link: Box<dyn Link>,
    /// The agent's own voice (its language, a local clip and its transcript).
    pub voice: etalk::VoiceChoice,
    /// The member's conversation with the agent (None = the first send makes
    /// it; #EXP-1941: a sealed call's key source resolves it, and may create
    /// it: [`Input::Root`] brings it).
    pub root: Option<String>,
    /// #EXP-1961: the call's own session (a child of `root`): utterances go
    /// there and its replies are spoken; None = the old path (the chat).
    /// #EXP-1981: a sealed call asks for it once [`Input::Root`] came.
    pub session: Option<String>,
}

/// #EXP-1982: a ring this host joins only to fail it (the caller's toast,
/// `code`), never listening: the `failed` goes once the caller is on the
/// line, then the hang-up.
pub struct Refused {
    pub call_id: String,
    pub link: Box<dyn Link>,
    pub code: &'static str,
}

/// What the network side made of a ring.
pub enum Answer {
    Call(Answered),
    Refuse(Refused),
}

/// What the host's loop reacts to.
pub enum Input {
    Agents(Vec<HostedAgent>),
    Settings(etalk::voice::VoiceSettings),
    Ring { call_id: String, agent_id: String },
    Answered(Answered),
    /// #EXP-1982: a ring joined only to fail it.
    Refused(Refused),
    /// The network side gave up on a ring (logged, nothing to step).
    Unanswered { call_id: String },
    Worker(etalk::Event),
    Link { call_id: String, event: steer::call_host::CallHostEvent },
    /// A send answered: the conversation it went to.
    Sent { call_id: String, session_id: String },
    /// #EXP-1941: a sealed call's key source adopted this root (a first
    /// call's chat exists from here on): the first utterance goes into it.
    Root { call_id: String, root: String },
    /// #EXP-1981: `voice.callSession` answered a sealed call (asked once its
    /// root came; None = no call session: the chat takes the call).
    Session { call_id: String, session: Option<String> },
    /// #EXP-1982: an utterance this host could only send in clear was
    /// refused: its words can never be stored, the call ends.
    Unstored { call_id: String },
    Stop,
}

/// What the host asks of the network (a thread each; tests record).
pub trait Net: Send {
    /// Answer `call_id`: the host ticket, the agent's voice, its conversation.
    fn answer(&self, call_id: String, agent_id: String);
    /// Send `text` to the agent (`root` = its conversation, None = new;
    /// #EXP-1961 `session` = the call's own session, sealed with `root`'s key).
    fn send(&self, call_id: String, agent: HostedAgent, root: Option<String>, session: Option<String>, text: String);
    /// #EXP-1981: ask `voice.callSession` for a sealed call once its key
    /// source adopted the root; the answer comes back as [`Input::Session`].
    fn ask_call_session(&self, call_id: String, agent_id: String);
    /// #EXP-1961: the hang-up of a call with its own session.
    fn call_ended(&self, session: String);
    /// The rows of `root` (opened), for the reply watch: EXP-1930 only those
    /// created at or after `since` (None = all, once at the answer).
    fn messages(&self, root: &str, since: Option<&str>) -> Vec<domain::rows::MessageRow>;
}

/// The voice's state as the doctor reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Download {
    #[default]
    Idle,
    Running,
    /// EXP-1930: not up YET (a slow warm-up): probed again, never NoGpu.
    Slow,
    Failed,
    /// Crashed [`MAX_CRASHES`] times in a row: given up until a restart.
    Unavailable,
}

/// The `speech` row (None = no row): pure, the desktop's `speech_row`.
fn speech_row(hosting: bool, download: &Download, voices: Option<&[String]>) -> Option<SpeechState> {
    if !hosting {
        return None;
    }
    match download {
        Download::Unavailable => return Some(SpeechState::Failed),
        Download::Running | Download::Slow => return Some(SpeechState::Downloading),
        // EXP-1930: nothing stays loaded on a server: a voice that failed to
        // load is failed whatever answered before, until a probe answers.
        Download::Failed => return Some(SpeechState::Failed),
        _ => {}
    }
    Some(if voices?.is_empty() { SpeechState::NoGpu } else { SpeechState::Ready })
}

/// The running call: the call host and this daemon's share of it.
struct Call {
    call_id: String,
    agent: HostedAgent,
    link: Box<dyn Link>,
    language: String,
    /// #EXP-1992: the call itself (the worker's call id is its `id`).
    host: CallHost,
    root: Option<String>,
    /// Rows already looked at, and the instant older ones are history.
    seen: HashSet<String>,
    armed_at: String,
    polled: Instant,
}

/// The host: pure state + the worker, the link and the network behind
/// traits; [`spawn`] runs it on its own thread.
pub struct Host {
    speech: Box<dyn Speech>,
    net: Box<dyn Net>,
    agents: Vec<HostedAgent>,
    settings: etalk::voice::VoiceSettings,
    download: Download,
    /// The languages the worker last reported (None = not yet).
    voices: Option<Vec<String>>,
    /// A worker runs (spawned by a request, not exited since).
    up: bool,
    /// The probe's Prepare went out (once per hosting spell).
    probed: bool,
    /// The recognizer loaded once (the probe call listened).
    ears: bool,
    crashes: u32,
    /// The probe runs again then (a failed download, a crash before it answered).
    retry_at: Option<Instant>,
    retries: u32,
    last_used: Instant,
    call: Option<Call>,
    /// #EXP-1941: the ring in work (one at a time: a forged ring costs
    /// nothing while a real one is answered).
    answering: Option<String>,
    /// #EXP-1941: a call id is answered once.
    answered: etalk::call::AnsweredCalls,
    /// #EXP-1981: a root its key source adopted before the answer was in
    /// (the call id, the root): the call takes it at the answer.
    early_root: Option<(String, String)>,
    /// #EXP-1982: the ring being refused, and when it is hung up anyway.
    refusing: Option<(Refused, Instant)>,
    born: Instant,
    counter: u64,
    idle_after: Duration,
}

impl Host {
    pub fn new(speech: Box<dyn Speech>, net: Box<dyn Net>, now: Instant) -> Self {
        Self {
            speech,
            net,
            agents: Vec::new(),
            settings: etalk::voice::VoiceSettings::default(),
            download: Download::Idle,
            voices: None,
            up: false,
            probed: false,
            ears: false,
            crashes: 0,
            retry_at: None,
            retries: 0,
            last_used: now,
            call: None,
            answering: None,
            answered: etalk::call::AnsweredCalls::default(),
            early_root: None,
            refusing: None,
            born: now,
            counter: 0,
            idle_after: Duration::from_millis(VOICE_CALL.constants.server_idle_ms),
        }
    }

    /// The doctor's `speech` row.
    pub fn speech_state(&self) -> Option<SpeechState> {
        speech_row(!self.agents.is_empty(), &self.download, self.voices.as_deref())
    }

    fn hosting(&self) -> bool {
        !self.agents.is_empty()
    }

    /// Probe again after 30 s, doubling up to 30 min.
    fn retry_later(&mut self, now: Instant) {
        let delay = Duration::from_secs(30u64 << self.retries.min(6)).min(Duration::from_secs(30 * 60));
        self.retries += 1;
        log::info!("[voice] the voice is tried again in {} s", delay.as_secs());
        self.retry_at = Some(now + delay);
    }

    fn probe(&mut self, now: Instant) {
        self.up = true;
        // EXP-1930: a probe is use: the idle exit never kills it.
        self.last_used = now;
        self.speech.prepare(etalk::VoiceChoice { language: self.settings.language().to_string(), ..Default::default() });
    }

    /// A worker that speaks: rings are answered (a downloading one too).
    fn answers(&self) -> bool {
        matches!(self.speech_state(), Some(SpeechState::Ready | SpeechState::Downloading))
    }

    pub fn handle(&mut self, input: Input, now: Instant) {
        match input {
            Input::Agents(agents) => self.on_agents(agents, now),
            Input::Settings(settings) => self.settings = settings,
            Input::Ring { call_id, agent_id } => self.on_ring(call_id, agent_id, now),
            Input::Answered(answered) => self.on_answered(answered, now),
            Input::Refused(refused) => self.on_refused(refused, now),
            Input::Unstored { call_id } => self.on_unstored(&call_id, now),
            Input::Unanswered { call_id } => {
                log::info!("[voice] call {call_id}: not answered here");
                self.answering.take_if(|answering| *answering == call_id);
            }
            Input::Root { call_id, root } => self.on_root(&call_id, root, now),
            Input::Session { call_id, session } => self.on_session(&call_id, session, now),
            Input::Worker(event) => self.on_worker(event, now),
            Input::Link { call_id, event } => self.on_link(&call_id, event, now),
            Input::Sent { call_id, session_id } => {
                if let Some(call) = self.call.as_mut().filter(|call| call.call_id == call_id) {
                    call.root.get_or_insert(session_id);
                }
            }
            Input::Stop => {
                if let Some((refused, _)) = self.refusing.take() {
                    refused.link.end();
                }
                self.call_input(HostInput::Fail { code: "dropped".into() }, now);
                self.speech.shutdown();
                self.up = false;
            }
        }
    }

    fn on_agents(&mut self, agents: Vec<HostedAgent>, now: Instant) {
        if agents == self.agents {
            return;
        }
        let was = self.hosting();
        if agents.is_empty() {
            log::info!("[voice] no agent runs here any more: the speech worker stops");
        } else if !was {
            log::info!("[voice] {} agent(s) run here: probing the voice", agents.len());
        }
        self.agents = agents;
        if !self.hosting() {
            self.call_input(HostInput::Fail { code: "unavailable".into() }, now);
            self.speech.shutdown();
            self.up = false;
            self.probed = false;
            self.retry_at = None;
            return;
        }
        if !self.probed {
            // The probe: download (only with a GPU driver), load, warm up;
            // the worker exits once it answered (`Voices`).
            self.probed = true;
            self.crashes = 0;
            if self.download == Download::Unavailable {
                self.download = Download::Idle;
            }
            self.probe(now);
        }
    }

    fn on_ring(&mut self, call_id: String, agent_id: String, now: Instant) {
        if self.call.is_some() || !self.answers() {
            log::info!("[voice] call {call_id}: not answered here (busy or no voice)");
            return;
        }
        // #EXP-1941: one ring in work at a time, and one call id once.
        if let Some(answering) = self.answering.as_deref() {
            log::info!("[voice] call {call_id}: not answered here ({answering} is being answered)");
            return;
        }
        if self.answered.answered(&call_id, self.ms(now)) {
            log::warn!("[voice] call {call_id}: this call id was answered before; the ring is dropped");
            return;
        }
        self.answering = Some(call_id.clone());
        if !self.agents.iter().any(|agent| agent.id == agent_id) {
            // The list lags a Runs on move by up to a minute: the network
            // side re-reads the agent and refuses one that runs elsewhere.
            log::info!("[voice] call {call_id}: {agent_id} is not (yet) known to run here");
        }
        self.net.answer(call_id, agent_id);
    }

    fn ms(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.born).as_millis() as u64
    }

    /// The rows of `root` there now are history: the newest one's instant
    /// arms the reply watch (the server's clock, never this machine's).
    fn history(&self, root: &str) -> (HashSet<String>, Option<String>) {
        let mut seen = HashSet::new();
        let mut armed_at: Option<(i64, String)> = None;
        for row in self.net.messages(root, None) {
            if let Some(at) = row.created_at.as_deref().and_then(|at| etalk::voice::parse_instant(at).map(|ms| (ms, at.to_string()))) {
                if armed_at.as_ref().is_none_or(|(mark, _)| at.0 > *mark) {
                    armed_at = Some(at);
                }
            }
            seen.insert(row.id);
        }
        (seen, armed_at.map(|(_, at)| at))
    }

    /// #EXP-1941: the root a sealed call adopted, before anything is said.
    /// #EXP-1981: from here on the chat exists (a first call's too): the
    /// call's own session is asked as a clear call asks it at the answer.
    fn on_root(&mut self, call_id: &str, root: String, now: Instant) {
        if self.call.is_none() && self.answering.as_deref() == Some(call_id) {
            // Adopted before the answer was in: the call takes it then.
            self.early_root = Some((call_id.to_string(), root));
            return;
        }
        if !self.call.as_ref().is_some_and(|call| call.call_id == call_id && call.root.is_none()) {
            return;
        }
        let (seen, armed_at) = self.history(&root);
        let at = self.ms(now);
        let Some(call) = self.call.as_mut() else { return };
        call.seen.extend(seen);
        if let Some(armed_at) = armed_at {
            call.armed_at = armed_at;
        }
        call.root = Some(root);
        if call.host.session().is_none() && !call.host.session_pending() {
            // Nothing to do yet: the answer comes as `Input::Session`.
            let _ = call.host.handle(HostInput::SessionAsked, at);
            let (call_id, agent_id) = (call.call_id.clone(), call.agent.id.clone());
            self.net.ask_call_session(call_id, agent_id);
        }
    }

    /// #EXP-1981: a sealed call's session answered. Its rows there now are
    /// history and the reply watch follows it, as in a clear call; None =
    /// the chat takes the call (a held `ready` goes at once). A call that
    /// ended meanwhile tells its session the hang-up.
    fn on_session(&mut self, call_id: &str, session: Option<String>, now: Instant) {
        if !self.call.as_ref().is_some_and(|call| call.call_id == call_id) {
            for effect in CallHost::session_after_end(call_id, session) {
                if let HostEffect::CallEnded { session } = effect {
                    self.net.call_ended(session);
                }
            }
            return;
        }
        if !self.call.as_ref().is_some_and(|call| call.host.session_pending()) {
            return;
        }
        if let Some((seen, armed_at)) = session.as_deref().map(|session| self.history(session)) {
            if let Some(call) = self.call.as_mut() {
                call.seen.extend(seen);
                if let Some(armed_at) = armed_at {
                    call.armed_at = armed_at;
                }
            }
        }
        // What was said meanwhile goes where its answer is read.
        self.call_input(HostInput::Session { session }, now);
    }

    /// #EXP-1982: a ring joined only to fail it: nothing warms up or
    /// listens; `failed` goes once the caller is on the line ([`Self::on_link`]),
    /// a caller that never joins is hung up after the relay's ring (`tick`).
    fn on_refused(&mut self, refused: Refused, now: Instant) {
        self.answering.take_if(|answering| *answering == refused.call_id);
        // It got its ticket: this call id is not answered again.
        self.answered.note(&refused.call_id, self.ms(now));
        log::warn!("[voice] call {}: refused ({}): the chat is end-to-end encrypted and this host has no key holder", refused.call_id, refused.code);
        if let Some((old, _)) = self.refusing.take() {
            old.link.end();
        }
        self.refusing = Some((refused, now + Duration::from_millis(VOICE_CALL.constants.ring_timeout_ms)));
    }

    /// #EXP-1982: the refused ring's caller is on the line (or gone).
    fn on_refusing_link(&mut self, event: steer::call_host::CallHostEvent) {
        use steer::call_host::CallHostEvent;
        match event {
            CallHostEvent::Ready => {
                if let Some((refused, _)) = self.refusing.take() {
                    refused.link.send_event(serde_json::json!({ "t": "failed", "code": refused.code }));
                    refused.link.end();
                }
            }
            CallHostEvent::Ended { .. } | CallHostEvent::Dropped(_) => self.refusing = None,
            _ => {}
        }
    }

    /// #EXP-1982: an utterance that can never be stored ends the call.
    fn on_unstored(&mut self, call_id: &str, now: Instant) {
        if self.call.as_ref().is_some_and(|call| call.call_id == call_id) {
            self.call_input(HostInput::Unstorable, now);
        }
    }

    fn on_answered(&mut self, answered: Answered, now: Instant) {
        self.answering.take_if(|answering| *answering == answered.call_id);
        // It got its ticket: this call id is not answered again.
        self.answered.note(&answered.call_id, self.ms(now));
        if self.call.is_some() {
            answered.link.end();
            return;
        }
        self.counter += 1;
        let id = format!("call-{}", self.counter);
        let language = if answered.voice.language.is_empty() { self.settings.language().to_string() } else { answered.voice.language.clone() };
        // The agent's own voice at once (the warm-up). #EXP-1941: the
        // worker's CALL starts at `Ready` (`on_link`), when the caller is on
        // the line: nothing is heard or said before both ends proved the key.
        self.up = true;
        self.speech.prepare(etalk::VoiceChoice { language: language.clone(), ..answered.voice });
        let (seen, armed_at) = answered.session.as_deref().or(answered.root.as_deref()).map(|root| self.history(root)).unwrap_or_default();
        // FEED-276: its clip was fetched before the answer (`clip_file`): the voice is in.
        let host = CallHost::new(HostStart {
            id,
            call_id: Some(answered.call_id.clone()),
            language: language.clone(),
            remote: true,
            random: call_ring_random(),
            device: String::new(),
            fallback: false,
            session: answered.session,
            voice: etalk::call::CallVoice::None,
        });
        log::info!("[voice] call {}: answered for agent {} ({language})", answered.call_id, answered.agent.id);
        self.call = Some(Call {
            call_id: answered.call_id,
            agent: answered.agent,
            link: answered.link,
            language,
            host,
            root: answered.root,
            seen,
            armed_at: armed_at.unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string()),
            polled: now,
        });
        self.last_used = now;
        if let Some((call_id, root)) = self.early_root.take() {
            self.on_root(&call_id, root, now);
        }
    }

    /// #EXP-1992: one input to the running call's host, and what it answers.
    fn call_input(&mut self, input: HostInput, now: Instant) {
        let at = self.ms(now);
        let Some(call) = self.call.as_mut() else { return };
        let effects = call.host.handle(input, at);
        self.apply(effects, now);
    }

    /// What the call's host answered, done here in order: the worker, the
    /// caller's link, the server. An ended call is gone.
    fn apply(&mut self, effects: Vec<HostEffect>, now: Instant) {
        let Some(call) = self.call.as_ref() else { return };
        for effect in effects {
            match effect {
                HostEffect::Event { event } => call.link.send_event(event),
                HostEffect::Flush => call.link.flush(),
                HostEffect::Listen { id, language, .. } => self.speech.call(&id, &language),
                HostEffect::Mute { id, muted } => self.speech.call_mute(&id, muted),
                HostEffect::Speak { id, text } => self.speech.speak(&id, &text),
                HostEffect::Stop => self.speech.stop(),
                HostEffect::Hangup { id } => self.speech.hangup(&id),
                HostEffect::End => call.link.end(),
                HostEffect::Send { text, session } => self.net.send(call.call_id.clone(), call.agent.clone(), call.root.clone(), session, text),
                HostEffect::CallEnded { session } => self.net.call_ended(session),
                // A server has no screen: the caller got the code.
                HostEffect::Toast { .. } => {}
            }
        }
        if call.host.ended() {
            self.call = None;
            self.last_used = now;
        }
    }

    fn on_link(&mut self, call_id: &str, event: steer::call_host::CallHostEvent, now: Instant) {
        use steer::call_host::CallHostEvent;
        if self.refusing.as_ref().is_some_and(|(refused, _)| refused.call_id == call_id) {
            return self.on_refusing_link(event);
        }
        let Some(id) = self.call.as_ref().filter(|call| call.call_id == call_id).map(|call| call.host.id().to_string()) else { return };
        match event {
            CallHostEvent::Ready => {
                log::info!("[voice] call {call_id}: the caller is on the line");
                self.call_input(HostInput::Begin, now);
            }
            CallHostEvent::Audio(pcm) => self.speech.call_audio(&id, &pcm),
            // Question 9: the worker knows where an utterance begins and ends.
            CallHostEvent::Lost { ms } => self.speech.call_lost(&id, ms),
            CallHostEvent::Mute(muted) => self.call_input(HostInput::Mute { muted }, now),
            CallHostEvent::Ended { reason } => self.call_input(HostInput::HungUp { reason }, now),
            CallHostEvent::Dropped(reason) => {
                log::warn!("[voice] call {call_id}: the stream dropped: {reason}");
                self.call_input(HostInput::Fail { code: "dropped".into() }, now);
            }
        }
    }

    fn on_worker(&mut self, event: etalk::Event, now: Instant) {
        let before = self.speech_state();
        let current = self.call.as_ref().map(|call| call.host.id().to_string());
        let call_owns = |id: &str| current.as_deref() == Some(id);
        match event {
            etalk::Event::Downloading { .. } => self.download = Download::Running,
            // Before the first `Voices` it is not spoken for yet (no flap to
            // no row); after it, it is the recognizer's download ending.
            etalk::Event::ModelReady => {
                if self.voices.is_some() && self.download == Download::Running {
                    self.download = Download::Idle;
                }
            }
            // EXP-1930: a slow warm-up (or backing off after one): the empty
            // `Voices` that follows is "not yet", never "no usable GPU".
            etalk::Event::VoiceRetry { .. } => {
                if self.download != Download::Failed {
                    self.download = Download::Slow;
                }
            }
            etalk::Event::Voices { voices } => {
                // A failed download (or a slow warm-up) settles as no voices:
                // it is not "no GPU".
                let failed = matches!(self.download, Download::Failed | Download::Slow) && voices.is_empty();
                // EXP-1930: a call whose voice did not come up is unavailable,
                // never a mute call (a server loads its voice for the call).
                if voices.is_empty() && self.call.is_some() {
                    log::warn!("[voice] the voice did not load for the call");
                    self.call_input(HostInput::Fail { code: "unavailable".into() }, now);
                }
                if !failed {
                    self.download = Download::Idle;
                    if voices.is_empty() && self.voices.as_ref().is_none_or(|known| !known.is_empty()) {
                        log::info!("[voice] no usable GPU here: calls fall back to a desktop");
                    }
                    self.voices = Some(voices);
                    self.retries = 0;
                }
                // The probe answered. A voice here: the recognizer is
                // fetched and loaded too (a probe call), so the first ring
                // never waits for a download; then (or with no GPU at once)
                // the worker exits: an idle server holds no voice.
                let speaks = self.voices.as_ref().is_some_and(|voices| !voices.is_empty());
                if self.call.is_none() && self.up {
                    if speaks && !self.ears {
                        self.last_used = now;
                        self.speech.call(PROBE_CALL, self.settings.language());
                    } else {
                        self.speech.shutdown();
                        self.up = false;
                    }
                }
                if failed {
                    self.retry_later(now);
                }
            }
            etalk::Event::DownloadFailed { error } => {
                log::warn!("[voice] the voice download failed: {error}");
                self.download = Download::Failed;
            }
            etalk::Event::CallListening { id } if id == PROBE_CALL => {
                self.ears = true;
                self.speech.hangup(PROBE_CALL);
                if self.call.is_none() {
                    self.speech.shutdown();
                    self.up = false;
                }
            }
            etalk::Event::ListenFailed { id, reason, detail } if id == PROBE_CALL && !matches!(reason, etalk::ListenFailure::Downloading) => {
                log::warn!("[voice] the recognizer did not load: {detail}");
                self.ears = true;
                self.speech.hangup(PROBE_CALL);
                if self.call.is_none() {
                    self.speech.shutdown();
                    self.up = false;
                }
            }
            etalk::Event::CallListening { id } if call_owns(&id) => {
                self.call_input(HostInput::Listening, now);
                // #EXP-1961: `ready` waits for the agent's opening line;
                // #EXP-1981: a sealed call's session still asked, too.
                if self.call.as_ref().is_some_and(|call| call.host.awaits_opening()) {
                    self.watch_replies(now);
                }
            }
            etalk::Event::Level { id, level, .. } if call_owns(&id) => self.call_input(HostInput::Level { level }, now),
            etalk::Event::Partial { id, text } if call_owns(&id) => self.call_input(HostInput::Partial { text }, now),
            etalk::Event::SpeechStarted { id } if call_owns(&id) => self.call_input(HostInput::SpeechStarted, now),
            etalk::Event::Said { id, text } if call_owns(&id) => self.call_input(HostInput::Said { text }, now),
            etalk::Event::SpeechAudio { pcm, rate, .. } => {
                let Some(call) = self.call.as_ref() else { return };
                if let Ok(samples) = etalk::decode_pcm(&pcm) {
                    call.link.send_speech(samples, rate);
                }
            }
            etalk::Event::Finished { id, stopped } => {
                if !stopped {
                    self.crashes = 0;
                }
                self.call_input(HostInput::Spoken { id }, now);
            }
            etalk::Event::Failed { id, error } => {
                log::warn!("[voice] not spoken: {error}");
                // EXP-1930: an opening line that cannot be spoken fails the call.
                self.call_input(HostInput::NotSpoken { id }, now);
            }
            etalk::Event::ListenFailed { reason: etalk::ListenFailure::Downloading, .. } => {}
            etalk::Event::ListenFailed { id, reason, detail } if call_owns(&id) => {
                log::warn!("[voice] the call could not listen: {detail}");
                self.call_input(HostInput::ListenFailed { reason }, now);
            }
            etalk::Event::Exited { code, signal } => {
                self.up = false;
                // #ERR-29: the dead ends are errors (GlitchTip events), their
                // text fixed so they group; the worker's crash is its own. A
                // worker that never started or was stopped with the daemon
                // has said so itself.
                if self.call.is_some() {
                    if etalk::crashed(code, signal) {
                        log::error!("[voice] the speech worker exited during a call");
                    } else {
                        log::warn!("[voice] the speech worker exited during a call");
                    }
                    self.call_input(HostInput::Fail { code: "dropped".into() }, now);
                }
                if self.download == Download::Running {
                    self.download = Download::Idle;
                }
                self.crashes += 1;
                if self.crashes >= MAX_CRASHES && self.download != Download::Unavailable {
                    log::warn!("[voice] the speech worker crashed {} times in a row; giving up until a restart", self.crashes);
                    log::error!("[voice] the speech worker kept crashing; voice given up until a restart");
                    self.download = Download::Unavailable;
                } else if self.voices.is_none() {
                    // The probe never answered: it runs again later.
                    self.retry_later(now);
                }
            }
            _ => {}
        }
        if self.speech_state() != before {
            log::info!("[voice] speech: {:?}", self.speech_state());
        }
    }

    /// The loop's tick: the call's timers (the ring, the give-up, the mic
    /// check, the opening wait), the reply watch and the idle exit.
    pub fn tick(&mut self, now: Instant) {
        if self.refusing.as_ref().is_some_and(|(_, until)| now >= *until) {
            if let Some((refused, _)) = self.refusing.take() {
                log::info!("[voice] call {}: the refused caller never joined: hung up", refused.call_id);
                refused.link.end();
            }
        }
        if self.call.is_none() && !self.up && self.hosting() && self.download != Download::Unavailable && self.retry_at.is_some_and(|at| now >= at) {
            self.retry_at = None;
            self.download = Download::Idle;
            self.probe(now);
        }
        let Some(call) = self.call.as_ref() else {
            if self.up && self.voices.is_some() && self.download != Download::Running && now.duration_since(self.last_used) >= self.idle_after {
                log::info!("[voice] idle for {} s: the speech worker exits", self.idle_after.as_secs());
                self.speech.shutdown();
                self.up = false;
            }
            return;
        };
        // #EXP-1941: the ring's clock starts when the caller is on the line
        // (`Ready`). Before it nothing may leave: a `rang` stepped then would
        // never reach a sealing caller, which would ring for ever.
        if !call.host.begun() {
            return;
        }
        self.call_input(HostInput::Tick, now);
        if self.call.as_ref().is_some_and(|call| now.duration_since(call.polled) >= REPLY_POLL) {
            self.watch_replies(now);
        }
        self.last_used = now;
    }

    /// The agent's newest new reply in the call's conversation goes to the
    /// call's host (said, or during the ring its opening line).
    fn watch_replies(&mut self, now: Instant) {
        let Some(call) = self.call.as_mut() else { return };
        call.polled = now;
        // #EXP-1981: while its session is asked, the chat's rows are not the
        // call's (a first call's chat answering its own brief).
        if call.host.session_pending() {
            return;
        }
        let Some(root) = call.host.session().map(str::to_string).or_else(|| call.root.clone()) else { return };
        // EXP-1930: only the call's own rows, never the whole conversation.
        let rows = self.net.messages(&root, Some(&call.armed_at));
        let fresh: Vec<domain::rows::MessageRow> = rows.into_iter().filter(|row| call.seen.insert(row.id.clone())).collect();
        if fresh.is_empty() {
            return;
        }
        let voices: Vec<&str> = self.voices.iter().flatten().map(String::as_str).collect();
        let context = VoiceContext {
            agent_session_id: Some(&root),
            is_voice_device: true,
            voice_turn: true,
            language: &call.language,
            voices: &voices,
            armed_at: &call.armed_at,
            meeting: false,
        };
        // #EXP-2076 (chat-express.json `plainProseCases`): the call says and
        // captions an agent row's PROSE only, never an Express block's raw.
        let prose: Vec<Option<String>> = fresh.iter().map(spoken_prose).collect();
        let Some((id, text)) = reply_to_speak(
            fresh.iter().zip(&prose).map(|(row, prose)| (row.id.as_str(), VoiceMessage { body: prose.as_deref().or(row.body.as_deref()), ..message(row) })),
            &context,
        ) else {
            return;
        };
        self.call_input(HostInput::Reply { id, text }, now);
    }
}

/// #EXP-2076: an agent row's body as Voice reads it (the IDE's
/// `talk_host::spoken_prose`): its pure prose when it holds an Express block;
/// `None` = the body as it is.
fn spoken_prose(row: &domain::rows::MessageRow) -> Option<String> {
    let body = row.body.as_deref()?;
    (row.author_kind.as_deref() == Some("agent") && domain::chat_express::has_express_block(body))
        .then(|| domain::chat_express::plain_prose(body))
}

/// A synced row as the speak rule reads it.
fn message(row: &domain::rows::MessageRow) -> VoiceMessage<'_> {
    VoiceMessage {
        author_kind: row.author_kind.as_deref(),
        body: row.body.as_deref(),
        has_card: row.card.is_some(),
        author_session_id: row.author_session_id.as_deref(),
        session_id: Some(row.session_id.as_str()),
        created_at: row.created_at.as_deref(),
    }
}

// ---------------------------------------------------------------------------
// The daemon's wiring: the thread, the worker, the relay and the server.

/// What the daemon hands the host.
pub struct Deps {
    pub trpc: Arc<api::TrpcClient>,
    pub runtime: Arc<steer::SteerRuntime>,
    pub data_dir: PathBuf,
    pub device_id: String,
    /// The synced teams (the agents of each are read).
    pub teams: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /// The synced message rows of a session (opened), created at or after
    /// the instant when one is given.
    pub messages: Arc<dyn Fn(&str, Option<&str>) -> Vec<domain::rows::MessageRow> + Send + Sync>,
    /// Raised when the `speech` row changed (the next beat carries it).
    pub beat_soon: Arc<AtomicBool>,
    /// #EXP-1941: this install's node: its key holder gives a sealed call
    /// its key (None = no P2P here: such a daemon advertises no `call-seal`).
    pub node: Option<steer::enet::InstallNode>,
}

/// The handle the daemon keeps: ring, settings, the doctor row, stop.
#[derive(Clone)]
pub struct VoiceHost {
    tx: flume::Sender<Input>,
    speech: Arc<Mutex<Option<SpeechState>>>,
}

impl VoiceHost {
    pub fn ring(&self, call_id: String, agent_id: String) {
        let _ = self.tx.send(Input::Ring { call_id, agent_id });
    }

    pub fn settings(&self, settings: etalk::voice::VoiceSettings) {
        let _ = self.tx.send(Input::Settings(settings));
    }

    /// The `speech` row for the next doctor report (none = no row).
    pub fn add_doctor(&self, doctor: &mut coding::device_doctor::DeviceDoctor) {
        if let Some(state) = *self.speech.lock().unwrap_or_else(|poison| poison.into_inner()) {
            coding::device_doctor::add_speech(doctor, state);
        }
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Input::Stop);
    }
}

struct DaemonNet {
    deps: Arc<Deps>,
    tx: flume::Sender<Input>,
}

/// EXP-1930: an utterance's body. Into a known conversation it is sealed at
/// SEND time with the tree's current key (`seal`, created when the tree has
/// none and this install may), so a tree keyed after the call's first send
/// gets the next one sealed; a first send (no conversation yet), a keyless
/// tree this install may not key (`NotReader`) and no key holder go out
/// plain (the server takes plaintext only where it is legal). #EXP-2123:
/// any other failure (a waiting install's no-key refusal, a key that did
/// not arrive) is `None`: nothing is sent, never plaintext.
fn utterance_body(root: Option<&str>, text: &str, seal: &dyn Fn(&str, &str) -> Result<String, coding::sealing::SealError>) -> Option<String> {
    use coding::sealing::SealError;
    let Some(root) = root else { return Some(text.to_string()) };
    match seal(root, text) {
        Ok(body) => Some(body),
        Err(SealError::NoService | SealError::NotReader | SealError::ClearTree) => Some(text.to_string()),
        Err(err) => {
            log::warn!("[voice] the utterance was not sent, sealing failed: {err}");
            None
        }
    }
}

/// #EXP-1982: a send this host could only make in clear (`plain`: no key
/// holder sealed it) that the server refused for good (a 4xx that is no
/// retry): the words can never be stored (an encrypted chat takes no
/// plaintext), so the call ends instead of listening on.
fn unstored(plain: bool, err: &api::ApiError) -> bool {
    plain && matches!(err, api::ApiError::Http { status, .. } if (400..500).contains(status) && !matches!(*status, 401 | 408 | 429))
}

/// What one utterance's send asks of the server (tests fake it).
trait UtteranceWire {
    /// `sessions.sendTarget`: the member's conversation with the agent now.
    fn target(&self, agent_id: &str) -> Result<Option<String>, String>;
    fn seal(&self, root: &str, text: &str) -> Result<String, coding::sealing::SealError>;
    fn send(&self, input: &api::sessions::SendInput) -> Result<api::sessions::SendOutput, api::ApiError>;
}

struct DaemonWire<'a>(&'a Deps);

impl UtteranceWire for DaemonWire<'_> {
    fn target(&self, agent_id: &str) -> Result<Option<String>, String> {
        api::sessions::send_target(&self.0.trpc, Some(agent_id), None).map(|target| target.root_id).map_err(|err| err.to_string())
    }

    fn seal(&self, root: &str, text: &str) -> Result<String, coding::sealing::SealError> {
        coding::sealing::seal_body_for_tree(root, text)
    }

    fn send(&self, input: &api::sessions::SendInput) -> Result<api::sessions::SendOutput, api::ApiError> {
        api::sessions::send(&self.0.trpc, input)
    }
}

/// What became of one utterance's send.
#[derive(Debug, PartialEq, Eq)]
enum Delivered {
    /// Stored in this session (the conversation, new or known).
    Sent(String),
    /// #EXP-1982: refused in clear for good: the call ends.
    Unstored,
    /// Failed otherwise: logged, the call goes on.
    Failed,
}

/// One utterance to the agent: sealed into a known conversation (`root`,
/// the call's own `session` in its tree), plain as a first send by agentId.
/// #EXP-2013: a first send refused in clear may have raced the chat's first
/// message (the conversation the answer read as missing exists by now):
/// the target is read again and, named, the words go sealed into it once
/// more before the call ends.
fn deliver_utterance(wire: &dyn UtteranceWire, call_id: &str, agent: &HostedAgent, root: Option<&str>, session: Option<&str>, text: &str) -> Delivered {
    // The call's own session is in the chat's tree: the chat's key.
    let Some(body) = utterance_body(root, text, &|root, text| wire.seal(root, text)) else {
        log::warn!("[voice] call {call_id}: the utterance could not be encrypted, it was not sent");
        return Delivered::Failed;
    };
    let plain = body == text;
    let input = match session.or(root) {
        Some(target) => api::sessions::SendInput::message(target, &body, Vec::new()),
        None => api::sessions::SendInput { team_id: Some(agent.team_id.clone()), agent_id: Some(agent.id.clone()), text: Some(body), ..Default::default() },
    };
    let err = match wire.send(&input) {
        Ok(out) => return Delivered::Sent(out.session_id),
        Err(err) => err,
    };
    if !unstored(plain, &err) {
        // The call goes on; the words are in the log, never lost silently.
        log::warn!("[voice] call {call_id}: the send failed: {err}");
        return Delivered::Failed;
    }
    log::warn!("[voice] call {call_id}: the send was refused in clear: {err}");
    if root.is_none() {
        match wire.target(&agent.id) {
            Ok(Some(found)) => {
                let body = utterance_body(Some(&found), text, &|root, text| wire.seal(root, text));
                if let Some(body) = body.filter(|body| body != text) {
                    log::info!("[voice] call {call_id}: the conversation {found} exists now: the utterance goes sealed into it");
                    match wire.send(&api::sessions::SendInput::message(&found, &body, Vec::new())) {
                        Ok(out) => return Delivered::Sent(out.session_id),
                        Err(err) if unstored(true, &err) => log::warn!("[voice] call {call_id}: the sealed send was refused too: {err}"),
                        Err(err) => {
                            log::warn!("[voice] call {call_id}: the sealed send failed: {err}");
                            return Delivered::Failed;
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(err) => log::warn!("[voice] call {call_id}: the conversation could not be read again: {err}"),
        }
    }
    // #EXP-1982: words that can never be stored end the call.
    Delivered::Unstored
}

impl Net for DaemonNet {
    fn answer(&self, call_id: String, agent_id: String) {
        let (ring, tx) = (DaemonNet { deps: Arc::clone(&self.deps), tx: self.tx.clone() }, self.tx.clone());
        let _ = std::thread::Builder::new().name("voice-answer".into()).spawn(move || {
            match answer(&ring, &call_id, &agent_id) {
                Ok(Answer::Call(answered)) => {
                    let _ = tx.send(Input::Answered(answered));
                }
                Ok(Answer::Refuse(refused)) => {
                    let _ = tx.send(Input::Refused(refused));
                }
                Err(err) => {
                    log::warn!("[voice] call {call_id}: {err}");
                    let _ = tx.send(Input::Unanswered { call_id });
                }
            }
        });
    }

    fn send(&self, call_id: String, agent: HostedAgent, root: Option<String>, session: Option<String>, text: String) {
        let (deps, tx) = (Arc::clone(&self.deps), self.tx.clone());
        let _ = std::thread::Builder::new().name("voice-send".into()).spawn(move || {
            let wire = DaemonWire(&deps);
            let input = match deliver_utterance(&wire, &call_id, &agent, root.as_deref(), session.as_deref(), &text) {
                Delivered::Sent(session_id) => Input::Sent { call_id, session_id },
                Delivered::Unstored => Input::Unstored { call_id },
                Delivered::Failed => return,
            };
            let _ = tx.send(input);
        });
    }

    fn messages(&self, root: &str, since: Option<&str>) -> Vec<domain::rows::MessageRow> {
        (self.deps.messages)(root, since)
    }

    fn ask_call_session(&self, call_id: String, agent_id: String) {
        let (net, tx) = (DaemonNet { deps: Arc::clone(&self.deps), tx: self.tx.clone() }, self.tx.clone());
        let _ = std::thread::Builder::new().name("voice-session".into()).spawn(move || {
            let session = Ring::call_session(&net, &agent_id, &call_id);
            let _ = tx.send(Input::Session { call_id, session });
        });
    }

    fn call_ended(&self, session: String) {
        let deps = Arc::clone(&self.deps);
        let _ = std::thread::Builder::new().name("voice-ended".into()).spawn(move || {
            if let Err(err) = api::voice::voice_call_ended(&deps.trpc, &session) {
                log::warn!("[voice] the call's end did not reach the server: {err}");
            }
        });
    }
}

/// The member's conversation with the agent as the server names it now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Chat {
    root: String,
    /// The tree has a key: it takes no plaintext from a writer.
    keyed: bool,
}

/// What a ring asks of the server and the relay, in the order it is asked.
trait Ring {
    fn device_id(&self) -> &str;
    /// `voice.callHostTicket`, naming this device and the ring's agent.
    fn ticket(&self, call_id: &str, agent_id: &str) -> Result<api::voice::CallTicket, String>;
    fn agent(&self, agent_id: &str) -> Result<api::agents::Agent, String>;
    fn voice(&self, agent: &api::agents::Agent) -> etalk::VoiceChoice;
    /// The member's conversation with the agent (a clear call).
    fn chat(&self, agent_id: &str) -> Result<Option<Chat>, String>;
    /// #EXP-1982: this process has a key holder (or one is about to run):
    /// it can write into an encrypted chat.
    fn seals(&self) -> bool;
    /// #EXP-1961: the call's own session (None = the chat takes the call).
    fn call_session(&self, agent_id: &str, call_id: &str) -> Option<String>;
    /// The call host socket, in the mode the ticket names.
    fn connect(&self, call_id: &str, agent_id: &str, ticket: api::voice::CallTicket) -> Box<dyn Link>;
}

/// The ring's network half. #EXP-1941: the host ticket FIRST, naming this
/// device and the ring's agent: the relay writes the ring, and only the
/// server's answer vouches for its call id and its agent. A ring the server
/// refuses reads nothing more (no agent, no voice clip, no conversation).
/// Then the agent (it must run HERE), its voice, and the socket in the mode
/// the ticket names. A sealed call's conversation is its key source's.
fn answer(ring: &dyn Ring, call_id: &str, agent_id: &str) -> Result<Answer, String> {
    let ticket = ring.ticket(call_id, agent_id)?;
    let agent = ring.agent(agent_id)?;
    if agent.device_id.as_deref() != Some(ring.device_id()) {
        return Err(format!("{agent_id} does not run here"));
    }
    let voice = ring.voice(&agent);
    // A sealed call's conversation is its key source's (it may create it).
    let chat = if ticket.seal { None } else { ring.chat(agent_id)? };
    // #EXP-1982: a clear call into an encrypted chat on a host with no key
    // holder could store none of its words: refused at the ring, never
    // listening (the caller's toast: "{device} couldn't get its voice ready").
    if chat.as_ref().is_some_and(|chat| chat.keyed) && !ring.seals() {
        return Ok(Answer::Refuse(Refused { call_id: call_id.to_string(), link: ring.connect(call_id, agent_id, ticket), code: "unavailable" }));
    }
    let root = chat.map(|chat| chat.root);
    // #EXP-1961: the call's own session, started while it rings; no chat yet
    // = the old path. #EXP-1981: a sealed call asks once its key source
    // adopted the root (`Host::on_root`).
    let session = match root.as_deref() {
        Some(_) => ring.call_session(agent_id, call_id),
        None => None,
    };
    Ok(Answer::Call(Answered {
        call_id: call_id.to_string(),
        agent: HostedAgent { id: agent.id.clone(), team_id: agent.team_id.clone() },
        link: ring.connect(call_id, agent_id, ticket),
        voice,
        root,
        session,
    }))
}

impl Ring for DaemonNet {
    fn device_id(&self) -> &str {
        &self.deps.device_id
    }

    fn ticket(&self, call_id: &str, agent_id: &str) -> Result<api::voice::CallTicket, String> {
        // #FEED-268: how long the ring waited for its ticket (the relay
        // gives up on a ring nobody joins in `ringTimeoutMs`).
        let asked = Instant::now();
        let ticket = api::voice::voice_call_host_ticket_for(&self.deps.trpc, call_id, &self.deps.device_id, agent_id).map_err(|err| format!("the call host ticket was refused: {err}"))?;
        log::info!("[voice] call {call_id}: the host ticket came after {} ms (sealed {})", asked.elapsed().as_millis(), ticket.seal);
        Ok(ticket)
    }

    fn agent(&self, agent_id: &str) -> Result<api::agents::Agent, String> {
        api::agents::get(&self.deps.trpc, agent_id).map_err(|err| format!("the agent could not be read: {err}"))
    }

    fn voice(&self, agent: &api::agents::Agent) -> etalk::VoiceChoice {
        agent_voice(&self.deps, agent)
    }

    fn chat(&self, agent_id: &str) -> Result<Option<Chat>, String> {
        let target = api::sessions::send_target(&self.deps.trpc, Some(agent_id), None).map_err(|err| format!("the conversation could not be read: {err}"))?;
        Ok(target.root_id.map(|root| Chat { root, keyed: target.epoch.is_some() }))
    }

    fn seals(&self) -> bool {
        !coding::sealing::key_holder_missing()
    }

    fn call_session(&self, agent_id: &str, call_id: &str) -> Option<String> {
        match api::voice::voice_call_session(&self.deps.trpc, agent_id, Some(call_id)) {
            Ok(session) => session.map(|session| session.session_id),
            Err(err) => {
                log::info!("[voice] call {call_id}: no call session ({err}): the chat takes the call");
                None
            }
        }
    }

    fn connect(&self, call_id: &str, agent_id: &str, ticket: api::voice::CallTicket) -> Box<dyn Link> {
        let (events, id) = (self.tx.clone(), call_id.to_string());
        // Told `seal`: the sealed wire only. Its key source is this install's
        // key holder; without a node there is none and the call ends `no_key`.
        let sealing = ticket.seal.then(|| {
            let (roots, root_call) = (self.tx.clone(), call_id.to_string());
            let keys: Arc<dyn steer::call_keys::CallKeySource> = match self.deps.node.clone() {
                Some(node) => Arc::new(steer::call_keys::HostKeys {
                    ops: Arc::new(steer::call_keys::InstallKeyOps { trpc: Arc::clone(&self.deps.trpc), node }),
                    call_id: call_id.to_string(),
                    agent_id: agent_id.to_string(),
                    device_id: self.deps.device_id.clone(),
                    adopted: Some(Arc::new(move |root| {
                        let _ = roots.send(Input::Root { call_id: root_call.clone(), root });
                    })),
                }),
                None => Arc::new(steer::call_keys::FixedKeySource { key: None, delay: Duration::ZERO }),
            };
            steer::call_keys::CallSealing { call_id: call_id.to_string(), agent_id: agent_id.to_string(), device_id: self.deps.device_id.clone(), caller_target: None, keys }
        });
        Box::new(steer::call_host::connect_sealed(&self.deps.runtime, ticket.url, call_id, sealing, move |event| {
            let _ = events.send(Input::Link { call_id: id.clone(), event });
        }))
    }
}

/// #EXP-1847: the agent's own voice here: its language, its uploaded clip
/// (fetched once into `{data_dir}/voice-clips/{clipId}.wav`) with its
/// transcript; none = the built-in voice.
fn agent_voice(deps: &Deps, agent: &api::agents::Agent) -> etalk::VoiceChoice {
    let voice = domain::agent_settings::AgentVoice::from_value(agent.voice.as_ref());
    let language = voice.language.clone().filter(|language| !language.is_empty()).unwrap_or_default();
    let clip = voice.clip_id.as_deref().filter(|id| !id.is_empty()).and_then(|id| clip_file(deps, id));
    match clip {
        Some(file) => etalk::VoiceChoice { language, file: file.to_string_lossy().into_owned(), transcript: voice.transcript.unwrap_or_default() },
        None => etalk::VoiceChoice { language, ..Default::default() },
    }
}

/// The clip's local copy, fetched on first use (None = it failed: the
/// built-in voice speaks). FEED-276 (voice-call.json `hostVoice`): the
/// fetch runs before the call is joined and the worker is prepared with the
/// clip before its `Call`, so the call's `ready` includes the agent's own
/// voice; it is bounded by `hostVoice.waitMs` (under the relay's
/// `ringTimeoutMs`), after which the built-in voice speaks, logged.
fn clip_file(deps: &Deps, clip_id: &str) -> Option<PathBuf> {
    let safe = clip_id.len() <= 64 && clip_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !safe {
        return None;
    }
    let dir = deps.data_dir.join("voice-clips");
    let file = dir.join(format!("{clip_id}.wav"));
    if file.is_file() {
        return Some(file);
    }
    let fetched = deps
        .trpc
        .get_bytes_within(&format!("/api/attachments/{clip_id}"), Duration::from_millis(VOICE_CALL.host_voice.wait_ms))
        .map_err(|err| err.to_string())
        .and_then(|(bytes, _)| {
            std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
            let part = dir.join(format!("{clip_id}.wav.part"));
            std::fs::write(&part, &bytes).map_err(|err| err.to_string())?;
            std::fs::rename(&part, &file).map_err(|err| err.to_string())
        });
    match fetched {
        Ok(()) => Some(file),
        Err(err) => {
            log::warn!("[voice] voice clip {clip_id} could not be fetched; the built-in voice speaks: {err}");
            None
        }
    }
}

/// The agents whose Runs on is this device: the synced teams' and the
/// member's own Assistant.
fn hosted_agents(trpc: &api::TrpcClient, teams: &[String], device_id: &str) -> Option<Vec<HostedAgent>> {
    hosted_from(
        teams,
        device_id,
        |team_id| api::agents::list(trpc, team_id),
        || own_assistant(|| api::voice::voice_assistant_id(trpc), |id| api::agents::get(trpc, id)),
    )
}

/// The member's own Assistant, READ only (#EXP-2080): `voice.get` names it
/// and `agents.get` reads it. Never `agents.ensureAssistant`: that creates
/// one on first use, and a daemon is never the client that creates a
/// member's Assistant (NEW-99: its device is a desktop). None yet = `None`:
/// nothing is asked further, nothing is hosted.
fn own_assistant(
    named: impl Fn() -> Result<Option<String>, api::ApiError>,
    get: impl Fn(&str) -> Result<api::agents::Agent, api::ApiError>,
) -> Result<Option<api::agents::Agent>, api::ApiError> {
    match named()? {
        Some(id) => get(&id).map(Some),
        None => Ok(None),
    }
}

/// The agents that run on `device_id` as the server names them: every synced
/// team's list, and the member's own Assistant (#EXP-2080: after the move,
/// #EXP-1984, it lives in their personal team, never a synced one, and no
/// team's list names it; before the move it is a row of a team's list too:
/// one entry per agent). `None` = a read did not reach the server (the last
/// answer stands). A server that REFUSES the Assistant ask, or answers it
/// in a shape this build cannot read (an older one), hosts the teams' agents
/// as before.
fn hosted_from(
    teams: &[String],
    device_id: &str,
    list: impl Fn(&str) -> Result<Vec<api::agents::Agent>, api::ApiError>,
    assistant: impl Fn() -> Result<Option<api::agents::Agent>, api::ApiError>,
) -> Option<Vec<HostedAgent>> {
    let mut hosted = Vec::new();
    for team_id in teams {
        hosted.extend(hosted_of(&list(team_id).ok()?, device_id));
    }
    match assistant() {
        Ok(assistant) => hosted.extend(hosted_of(assistant.as_slice(), device_id)),
        Err(api::ApiError::Transport { .. } | api::ApiError::Unauthorized | api::ApiError::UpgradeRequired) => return None,
        Err(err) => log::debug!("[voice] the Assistant was not named ({err}): the teams' agents only"),
    }
    hosted.sort_by(|a, b| a.id.cmp(&b.id));
    hosted.dedup_by(|a, b| a.id == b.id);
    Some(hosted)
}

/// Pure: the agents among `agents` that run on `device_id`.
fn hosted_of(agents: &[api::agents::Agent], device_id: &str) -> Vec<HostedAgent> {
    agents
        .iter()
        .filter(|agent| agent.device_id.as_deref() == Some(device_id))
        .map(|agent| HostedAgent { id: agent.id.clone(), team_id: agent.team_id.clone() })
        .collect()
}

/// Start the host on its own thread. The worker is this executable
/// (`__speech-worker`); nothing runs until an agent runs here.
pub fn spawn(deps: Deps) -> Option<VoiceHost> {
    let exe = std::env::current_exe().ok()?;
    let deps = Arc::new(deps);
    let (tx, rx) = flume::unbounded::<Input>();
    let events = tx.clone();
    let client = etalk::SpeechClient::new(exe, deps.data_dir.clone(), move |event| {
        let _ = events.send(Input::Worker(event));
    });
    let net = DaemonNet { deps: Arc::clone(&deps), tx: tx.clone() };
    let speech = Arc::new(Mutex::new(None));
    let handle = VoiceHost { tx: tx.clone(), speech: Arc::clone(&speech) };
    let agents_tx = tx.clone();
    let agents_deps = Arc::clone(&deps);
    let stopped = Arc::new(AtomicBool::new(false));
    let agents_stopped = Arc::clone(&stopped);
    // The agents that run here, read off the loop (a request per team, and
    // the Assistant's two reads).
    let _ = std::thread::Builder::new().name("voice-agents".into()).spawn(move || {
        while !agents_stopped.load(Ordering::SeqCst) {
            if let Some(agents) = hosted_agents(&agents_deps.trpc, &(agents_deps.teams)(), &agents_deps.device_id) {
                if agents_tx.send(Input::Agents(agents)).is_err() {
                    break;
                }
            }
            let until = Instant::now() + AGENTS_EVERY;
            while Instant::now() < until && !agents_stopped.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
    let beat_soon = Arc::clone(&deps.beat_soon);
    std::thread::Builder::new()
        .name("voice-host".into())
        .spawn(move || {
            let mut host = Host::new(Box::new(client), Box::new(net), Instant::now());
            loop {
                match rx.recv_timeout(TICK) {
                    Ok(Input::Stop) => {
                        host.handle(Input::Stop, Instant::now());
                        stopped.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(input) => host.handle(input, Instant::now()),
                    Err(flume::RecvTimeoutError::Timeout) => {}
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                }
                host.tick(Instant::now());
                let state = host.speech_state();
                let mut shown = speech.lock().unwrap_or_else(|poison| poison.into_inner());
                if *shown != state {
                    *shown = state;
                    beat_soon.store(true, Ordering::SeqCst);
                }
            }
        })
        .ok()?;
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<Vec<String>>>);

    impl Log {
        fn push(&self, line: String) {
            self.0.lock().unwrap().push(line);
        }
        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    struct FakeSpeech(Log);

    impl Speech for FakeSpeech {
        fn prepare(&self, voice: etalk::VoiceChoice) {
            self.0.push(format!("prepare {}", voice.language));
        }
        fn call(&self, id: &str, language: &str) {
            self.0.push(format!("call {id} {language}"));
        }
        fn call_audio(&self, id: &str, pcm: &[i16]) {
            self.0.push(format!("audio {id} {}", pcm.len()));
        }
        fn call_lost(&self, id: &str, ms: u32) {
            self.0.push(format!("lost {id} {ms}"));
        }
        fn call_mute(&self, id: &str, muted: bool) {
            self.0.push(format!("mute {id} {muted}"));
        }
        fn speak(&self, id: &str, text: &str) {
            self.0.push(format!("speak {id} {text}"));
        }
        fn stop(&self) {
            self.0.push("stop".into());
        }
        fn hangup(&self, id: &str) {
            self.0.push(format!("hangup {id}"));
        }
        fn shutdown(&self) {
            self.0.push("shutdown".into());
        }
    }

    struct FakeLink(Log);

    impl Link for FakeLink {
        fn send_event(&self, event: serde_json::Value) {
            self.0.push(format!("event {event}"));
        }
        fn send_speech(&self, pcm: Vec<i16>, rate: u32) {
            self.0.push(format!("speech {} {rate}", pcm.len()));
        }
        fn flush(&self) {
            self.0.push("flush".into());
        }
        fn end(&self) {
            self.0.push("end".into());
        }
    }

    struct FakeNet {
        log: Log,
        /// The store reads (`messages`).
        reads: Log,
        rows: Arc<Mutex<Vec<domain::rows::MessageRow>>>,
    }

    impl Net for FakeNet {
        fn answer(&self, call_id: String, agent_id: String) {
            self.log.push(format!("answer {call_id} {agent_id}"));
        }
        fn send(&self, call_id: String, _agent: HostedAgent, root: Option<String>, session: Option<String>, text: String) {
            self.log.push(format!("send {call_id} {} {text}", session.or(root).unwrap_or_default()));
        }
        fn ask_call_session(&self, call_id: String, agent_id: String) {
            self.log.push(format!("session {call_id} {agent_id}"));
        }
        fn call_ended(&self, session: String) {
            self.log.push(format!("ended {session}"));
        }
        fn messages(&self, root: &str, since: Option<&str>) -> Vec<domain::rows::MessageRow> {
            self.reads.push(format!("messages {root} {}", since.unwrap_or("all")));
            let rows = self.rows.lock().unwrap().clone();
            rows.into_iter().filter(|row| since.is_none_or(|since| row.created_at.as_deref().is_some_and(|at| at >= since))).collect()
        }
    }

    fn agent() -> HostedAgent {
        HostedAgent { id: "agent-1".into(), team_id: "team-1".into() }
    }

    fn row(id: &str, author: &str, body: &str, at: &str) -> domain::rows::MessageRow {
        serde_json::from_value(serde_json::json!({
            "id": id, "session_id": "root-1", "root_id": "root-1", "author_kind": author, "body": body, "created_at": at,
        }))
        .unwrap()
    }

    struct Rig {
        host: Host,
        speech: Log,
        net: Log,
        link: Log,
        rows: Arc<Mutex<Vec<domain::rows::MessageRow>>>,
        reads: Log,
        now: Instant,
    }

    impl Rig {
        fn new() -> Self {
            let (speech, net, link) = (Log::default(), Log::default(), Log::default());
            let rows = Arc::new(Mutex::new(Vec::new()));
            let now = Instant::now();
            let reads = Log::default();
            let host = Host::new(Box::new(FakeSpeech(speech.clone())), Box::new(FakeNet { log: net.clone(), reads: reads.clone(), rows: Arc::clone(&rows) }), now);
            Rig { host, speech, net, link, rows, reads, now }
        }

        fn handle(&mut self, input: Input) {
            self.host.handle(input, self.now);
        }

        fn advance(&mut self, by: Duration) {
            self.now += by;
            self.host.tick(self.now);
        }

        /// Hosting, probed and ready (the GPU speaks), the worker exited.
        fn ready() -> Self {
            let mut rig = Rig::new();
            rig.handle(Input::Agents(vec![agent()]));
            rig.handle(Input::Worker(etalk::Event::Voices { voices: vec!["en".into(), "de".into()] }));
            assert_eq!(rig.speech.take(), ["prepare en", "call probe en"], "a voice: the recognizer loads too");
            rig.handle(Input::Worker(etalk::Event::CallListening { id: "probe".into() }));
            assert_eq!(rig.speech.take(), ["hangup probe", "shutdown"]);
            rig
        }

        fn answered(&mut self) {
            self.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
            self.handle(Input::Answered(Answered {
                call_id: "c1".into(),
                agent: agent(),
                link: Box::new(FakeLink(self.link.clone())),
                voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
                root: Some("root-1".into()),
                session: None,
            }));
            // The caller is on the line: the worker's call starts.
            self.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        }

        /// #EXP-1961: a call with its own session `call-s`.
        fn answered_in_session(&mut self) {
            self.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
            self.handle(Input::Answered(Answered {
                call_id: "c1".into(),
                agent: agent(),
                link: Box::new(FakeLink(self.link.clone())),
                voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
                root: Some("root-1".into()),
                session: Some("call-s".into()),
            }));
            // The caller is on the line: the worker's call starts.
            self.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        }
    }

    /// #EXP-1941: the worker warms up at the answer, but its CALL starts at
    /// `Ready`: a warm worker's `ready` never leaves before the caller's
    /// first frame opened, and the caller still gets it.
    #[test]
    fn the_workers_call_starts_when_the_caller_is_on_the_line() {
        let mut rig = Rig::ready();
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Answered(Answered {
            call_id: "c1".into(),
            agent: agent(),
            link: Box::new(FakeLink(rig.link.clone())),
            voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
            root: None,
            session: None,
        }));
        assert_eq!(rig.speech.take(), ["prepare de"], "the warm-up only");
        // A handshake longer than the ring: no `rang` is stepped into the void.
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert!(rig.link.take().is_empty(), "nothing leaves before the caller is on the line");
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        assert_eq!(rig.speech.take(), ["call call-1 de"], "once");
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        assert_eq!(rig.link.take(), [r#"event {"t":"ready"}"#]);
        // A sealed call's key source adopted the chat: the first utterance goes
        // into it (#EXP-1981: its call session is asked; none here).
        rig.handle(Input::Root { call_id: "c1".into(), root: "root-9".into() });
        assert_eq!(rig.reads.take(), ["messages root-9 all"], "its rows there now are history");
        rig.handle(Input::Session { call_id: "c1".into(), session: None });
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert_eq!(rig.link.take()[0], r#"event {"t":"rang"}"#, "the ring ends on the caller's clock");
        rig.handle(Input::Worker(etalk::Event::Finished { id: "call-1-greeting".into(), stopped: false }));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "hello".into() }));
        assert_eq!(rig.net.take(), ["answer c1 agent-1", "session c1 agent-1", "send c1 root-9 hello"]);
    }

    /// A sealed call (#EXP-1941): answered with no root and no session; its
    /// key source adopts the chat before the host's hello.
    fn answered_sealed(rig: &mut Rig) {
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Answered(Answered {
            call_id: "c1".into(),
            agent: agent(),
            link: Box::new(FakeLink(rig.link.clone())),
            voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
            root: None,
            session: None,
        }));
    }

    /// #EXP-1981: a sealed call gets its own call session (#EXP-1961) once
    /// its key source adopted the root, as a clear call does at the answer:
    /// `ready` waits for it and then for the agent's opening line, which is
    /// said at the pickup; utterances go to the call session, sealed with
    /// the chat's key, and the hang-up is told.
    #[test]
    fn a_sealed_call_gets_its_session_once_its_root_is_adopted() {
        let mut rig = Rig::ready();
        rig.rows.lock().unwrap().push(row("old", "agent", "Earlier reply.", "2020-01-01T00:00:00Z"));
        answered_sealed(&mut rig);
        assert_eq!(rig.net.take(), ["answer c1 agent-1"], "no session before the root is known");
        rig.handle(Input::Root { call_id: "c1".into(), root: "root-1".into() });
        assert_eq!(rig.net.take(), ["session c1 agent-1"], "asked as soon as the root is adopted");
        rig.handle(Input::Root { call_id: "c1".into(), root: "root-1".into() });
        assert!(rig.net.take().is_empty(), "asked once");
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.advance(Duration::from_millis(1000));
        assert!(rig.link.take().is_empty(), "ready waits for the session still asked");
        // The root's own rows meanwhile are not the call's.
        rig.rows.lock().unwrap().push(row("brief", "agent", "Waiting for the next message.", "2099-01-01T00:00:00Z"));
        rig.advance(REPLY_POLL);
        rig.reads.take();
        rig.handle(Input::Session { call_id: "c1".into(), session: Some("call-s".into()) });
        assert_eq!(rig.reads.take(), ["messages call-s all"], "its rows there now are history");
        rig.rows.lock().unwrap().push(call_row("o1", "Hallo Danny, der Build ist grün.", "2099-01-01T00:00:01Z"));
        rig.advance(REPLY_POLL);
        assert_eq!(rig.link.take(), [r#"event {"t":"ready"}"#], "the opening line is in: ready");
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        let spoken: Vec<String> = rig.speech.take().into_iter().filter(|line| line.starts_with("speak")).collect();
        assert_eq!(spoken, ["speak o1 Hallo Danny, der Build ist grün."], "the call's own opening, never the chat's row");
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"replyStarted","text":"Hallo Danny, der Build ist grün."}"#.to_string()), "{events:?}");
        rig.handle(Input::Worker(etalk::Event::Finished { id: "o1".into(), stopped: false }));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "Danke".into() }));
        assert_eq!(rig.net.take(), ["send c1 call-s Danke"]);
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.net.take(), ["ended call-s"]);
    }

    /// #EXP-1981: the session is slow: the pickup could wait no longer
    /// (`openingWaitMs`), so what the caller says meanwhile is held and sent
    /// into the session once it answers (its replies are the ones read);
    /// its late opening line is never played as the answer.
    #[test]
    fn words_said_while_the_session_is_asked_wait_for_it() {
        let mut rig = Rig::ready();
        answered_sealed(&mut rig);
        rig.handle(Input::Root { call_id: "c1".into(), root: "root-1".into() });
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.advance(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"ready"}"#.to_string()), "picked up silently: {events:?}");
        rig.net.take();
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "Wie spät ist es?".into() }));
        assert!(rig.net.take().is_empty(), "held while the session is asked");
        rig.handle(Input::Session { call_id: "c1".into(), session: Some("call-s".into()) });
        assert_eq!(rig.net.take(), ["send c1 call-s Wie spät ist es?"]);
        rig.speech.take();
        rig.rows.lock().unwrap().push(call_row("r1", "Drei Uhr.", "2099-01-01T00:00:00Z"));
        rig.advance(REPLY_POLL);
        assert_eq!(rig.speech.take(), ["speak r1 Drei Uhr."], "the session's answer is spoken");
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.net.take(), ["ended call-s"]);
        // A call that ends while still asked sends what it held into the chat.
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Answered(Answered {
            call_id: "c2".into(),
            agent: agent(),
            link: Box::new(FakeLink(rig.link.clone())),
            voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
            root: None,
            session: None,
        }));
        rig.handle(Input::Root { call_id: "c2".into(), root: "root-1".into() });
        rig.handle(Input::Link { call_id: "c2".into(), event: steer::call_host::CallHostEvent::Ready });
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-2".into() }));
        rig.advance(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-2".into(), text: "Hallo?".into() }));
        rig.net.take();
        rig.handle(Input::Link { call_id: "c2".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.net.take(), ["send c2 root-1 Hallo?"], "never lost");
    }

    /// #EXP-1981: the session answered none (no room): a held `ready` goes
    /// at once and the chat takes the call; a root adopted before the answer
    /// was in still asks; a session that comes after the hang-up hears it.
    #[test]
    fn a_sealed_call_without_a_session_takes_the_chat() {
        let mut rig = Rig::ready();
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Root { call_id: "c1".into(), root: "root-1".into() });
        rig.handle(Input::Answered(Answered {
            call_id: "c1".into(),
            agent: agent(),
            link: Box::new(FakeLink(rig.link.clone())),
            voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
            root: None,
            session: None,
        }));
        assert_eq!(rig.net.take(), ["answer c1 agent-1", "session c1 agent-1"], "the early root is taken at the answer");
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        assert!(rig.link.take().is_empty(), "held while the session is asked");
        rig.handle(Input::Session { call_id: "c1".into(), session: None });
        assert_eq!(rig.link.take(), [r#"event {"t":"ready"}"#]);
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "hallo".into() }));
        assert_eq!(rig.net.take(), ["send c1 root-1 hallo"]);
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert!(rig.net.take().is_empty(), "no session to tell");
        // The next sealed call hangs up while its session is asked.
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Answered(Answered {
            call_id: "c2".into(),
            agent: agent(),
            link: Box::new(FakeLink(rig.link.clone())),
            voice: etalk::VoiceChoice { language: "de".into(), ..Default::default() },
            root: None,
            session: None,
        }));
        rig.handle(Input::Root { call_id: "c2".into(), root: "root-1".into() });
        rig.handle(Input::Link { call_id: "c2".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.net.take(), ["answer c2 agent-1", "session c2 agent-1"]);
        rig.handle(Input::Session { call_id: "c2".into(), session: Some("s2".into()) });
        assert_eq!(rig.net.take(), ["ended s2"], "a session that answers after the hang-up hears it");
    }

    /// #EXP-1941: one ring in work at a time, and one call id once: a ring
    /// whose id got its ticket in the last five minutes is dropped.
    #[test]
    fn a_ring_is_answered_one_at_a_time_and_a_call_id_once() {
        let mut rig = Rig::ready();
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c1 agent-1"], "the rings behind it are dropped");
        // The server refused it: the next ring is taken.
        rig.handle(Input::Unanswered { call_id: "c1".into() });
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c2 agent-1"]);
        rig.handle(Input::Unanswered { call_id: "c2".into() });
        // A call that was answered and ended: its id is not answered again.
        rig.answered();
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert!(rig.host.call.is_none());
        rig.net.take();
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert!(rig.net.take().is_empty(), "the relay rang a vouched id again");
        rig.advance(Duration::from_millis(etalk::call::CALL_ANSWERED_ONCE_MS));
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c1 agent-1"], "longer than an id lives");
    }

    struct FakeRing {
        log: Log,
        ticket: Result<bool, String>,
        runs_on: &'static str,
        /// #EXP-1982: the chat has a key; this host has a key holder.
        keyed: bool,
        seals: bool,
    }

    impl Ring for FakeRing {
        fn device_id(&self) -> &str {
            "here"
        }
        fn ticket(&self, call_id: &str, agent_id: &str) -> Result<api::voice::CallTicket, String> {
            self.log.push(format!("ticket {call_id} {agent_id}"));
            self.ticket.clone().map(|seal| api::voice::CallTicket { url: "wss://relay".into(), seal })
        }
        fn agent(&self, agent_id: &str) -> Result<api::agents::Agent, String> {
            self.log.push("agent".into());
            Ok(serde_json::from_value(serde_json::json!({ "id": agent_id, "teamId": "team-1", "deviceId": self.runs_on })).unwrap())
        }
        fn voice(&self, _agent: &api::agents::Agent) -> etalk::VoiceChoice {
            self.log.push("voice".into());
            etalk::VoiceChoice::default()
        }
        fn chat(&self, _agent_id: &str) -> Result<Option<Chat>, String> {
            self.log.push("root".into());
            Ok(Some(Chat { root: "root-1".into(), keyed: self.keyed }))
        }
        fn seals(&self) -> bool {
            self.seals
        }
        fn call_session(&self, _agent_id: &str, _call_id: &str) -> Option<String> {
            self.log.push("session".into());
            Some("call-s".into())
        }
        fn connect(&self, _call_id: &str, _agent_id: &str, ticket: api::voice::CallTicket) -> Box<dyn Link> {
            self.log.push(format!("connect seal={}", ticket.seal));
            Box::new(FakeLink(Log::default()))
        }
    }

    /// #EXP-1941: a ring gets its ticket BEFORE the agent, its voice or its
    /// conversation are read; a ring the server refuses reads nothing more.
    #[test]
    fn a_ring_asks_for_its_ticket_first() {
        let ring = |ticket: Result<bool, String>, runs_on| FakeRing { log: Log::default(), ticket, runs_on, keyed: true, seals: true };
        let clear = ring(Ok(false), "here");
        let Ok(Answer::Call(answered)) = answer(&clear, "c1", "agent-1") else { panic!("a call") };
        assert_eq!(clear.log.take(), ["ticket c1 agent-1", "agent", "voice", "root", "session", "connect seal=false"]);
        assert_eq!(answered.root.as_deref(), Some("root-1"));
        // Told `seal`: the conversation is the key source's (it may create it).
        let sealed = ring(Ok(true), "here");
        let Ok(Answer::Call(answered)) = answer(&sealed, "c1", "agent-1") else { panic!("a call") };
        assert!(answered.root.is_none());
        assert_eq!(sealed.log.take(), ["ticket c1 agent-1", "agent", "voice", "connect seal=true"]);
        let refused = ring(Err("no ticket".into()), "here");
        assert!(answer(&refused, "c1", "agent-1").is_err());
        assert_eq!(refused.log.take(), ["ticket c1 agent-1"], "a forged ring costs one request");
        let elsewhere = ring(Ok(false), "elsewhere");
        assert!(answer(&elsewhere, "c1", "agent-1").is_err());
        assert_eq!(elsewhere.log.take(), ["ticket c1 agent-1", "agent"], "no socket for an agent that runs elsewhere");
    }

    /// #EXP-1982: a clear call into an ENCRYPTED chat on a host with no key
    /// holder could store none of its words: it is refused at the ring with
    /// `unavailable` (no call session is started, nothing listens). A keyless
    /// chat still works here, a host with a holder answers, and a sealed call
    /// keeps its own path (its key source ends it `no_key`).
    #[test]
    fn a_clear_call_into_an_encrypted_chat_without_a_key_holder_is_refused() {
        let ring = |ticket: bool, keyed, seals| FakeRing { log: Log::default(), ticket: Ok(ticket), runs_on: "here", keyed, seals };
        let holderless = ring(false, true, false);
        let Ok(Answer::Refuse(refused)) = answer(&holderless, "c1", "agent-1") else { panic!("refused at the ring") };
        assert_eq!((refused.call_id.as_str(), refused.code), ("c1", "unavailable"));
        assert_eq!(holderless.log.take(), ["ticket c1 agent-1", "agent", "voice", "root", "connect seal=false"], "no call session for a refused call");
        assert!(matches!(answer(&ring(false, false, false), "c1", "agent-1"), Ok(Answer::Call(_))), "a keyless chat takes plaintext");
        assert!(matches!(answer(&ring(false, true, true), "c1", "agent-1"), Ok(Answer::Call(_))), "a key holder seals");
        assert!(matches!(answer(&ring(true, true, false), "c1", "agent-1"), Ok(Answer::Call(_))), "a sealed call keeps its own path");
    }

    /// #EXP-1982: a refused ring never listens: once the caller is on the
    /// line it gets `failed {unavailable}` (its toast "{device} couldn't get
    /// its voice ready") and the hang-up; the next ring is answered. One the
    /// caller never joins is hung up after the relay's ring.
    #[test]
    fn a_refused_ring_fails_unavailable_once_the_caller_is_on_the_line() {
        let mut rig = Rig::ready();
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c1 agent-1"]);
        rig.handle(Input::Refused(Refused { call_id: "c1".into(), link: Box::new(FakeLink(rig.link.clone())), code: "unavailable" }));
        assert!(rig.speech.take().is_empty(), "no voice warms up for it");
        assert!(rig.link.take().is_empty(), "nothing before the caller is on the line");
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ready });
        assert_eq!(rig.link.take(), [r#"event {"t":"failed","code":"unavailable"}"#, "end"]);
        assert!(rig.host.call.is_none());
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert!(rig.net.take().is_empty(), "its id got a ticket: a ring of it again is dropped");
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c2 agent-1"], "the next ring is answered");
        rig.handle(Input::Refused(Refused { call_id: "c2".into(), link: Box::new(FakeLink(rig.link.clone())), code: "unavailable" }));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.ring_timeout_ms));
        assert_eq!(rig.link.take(), ["end"], "a caller that never joins: hung up after the ring");
    }

    /// #EXP-1982 (backstop): an utterance this host could only send in clear
    /// and the server refused (the chat was keyed meanwhile) ends the call
    /// `dropped` instead of listening on and swallowing every word.
    #[test]
    fn an_utterance_refused_in_clear_ends_the_call() {
        let refusal = |status| api::ApiError::Http { status, message: "This chat is end-to-end encrypted: its messages must be encrypted too".into() };
        assert!(unstored(true, &refusal(400)));
        assert!(!unstored(false, &refusal(400)), "a sealed send refused is another reason");
        assert!(!unstored(true, &refusal(401)) && !unstored(true, &refusal(429)), "retried, not refused");
        assert!(!unstored(true, &api::ApiError::Transport { message: "offline".into(), offline: true }));
        let mut rig = Rig::ready();
        rig.answered();
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "hello".into() }));
        rig.link.take();
        rig.handle(Input::Unstored { call_id: "c1".into() });
        let events = rig.link.take();
        assert_eq!(events.first().map(String::as_str), Some(r#"event {"t":"failed","code":"dropped"}"#), "{events:?}");
        assert!(events.contains(&"end".to_string()));
        assert!(rig.host.call.is_none());
    }

    #[test]
    fn the_speech_row_is_the_desktops() {
        assert_eq!(speech_row(false, &Download::Idle, Some(&["en".into()])), None, "no agent runs here: no row");
        assert_eq!(speech_row(true, &Download::Idle, None), None, "not probed yet");
        assert_eq!(speech_row(true, &Download::Running, None), Some(SpeechState::Downloading));
        assert_eq!(speech_row(true, &Download::Idle, Some(&[])), Some(SpeechState::NoGpu));
        assert_eq!(speech_row(true, &Download::Idle, Some(&["en".into()])), Some(SpeechState::Ready));
        assert_eq!(speech_row(true, &Download::Failed, None), Some(SpeechState::Failed));
        assert_eq!(speech_row(true, &Download::Unavailable, Some(&["en".into()])), Some(SpeechState::Failed));
    }

    #[test]
    fn only_agents_that_run_here_are_hosted() {
        let agents: Vec<api::agents::Agent> = serde_json::from_value(serde_json::json!([
            { "id": "a", "teamId": "t", "deviceId": "here" },
            { "id": "b", "teamId": "t", "deviceId": "elsewhere" },
            { "id": "c", "teamId": "t" },
        ]))
        .unwrap();
        assert_eq!(hosted_of(&agents, "here"), vec![HostedAgent { id: "a".into(), team_id: "t".into() }]);
    }

    fn listed(value: serde_json::Value) -> Vec<api::agents::Agent> {
        serde_json::from_value(value).unwrap()
    }

    /// #EXP-2080 (after the Assistant move, #EXP-1984): the member's ONE
    /// Assistant lives in their personal team, which is never synced and
    /// which no team's `agents.list` names: it is asked for on its own, and
    /// one bound to this device is hosted like any other agent.
    #[test]
    fn the_members_own_assistant_is_hosted_past_the_synced_teams() {
        let teams = vec!["t".to_string()];
        let list = |_: &str| Ok(listed(serde_json::json!([{ "id": "a", "teamId": "t", "deviceId": "here" }])));
        let assistant = || Ok(listed(serde_json::json!([{ "id": "z", "teamId": "personal", "deviceId": "here", "role": "assistant" }])).pop());
        assert_eq!(
            hosted_from(&teams, "here", list, assistant),
            Some(vec![
                HostedAgent { id: "a".into(), team_id: "t".into() },
                HostedAgent { id: "z".into(), team_id: "personal".into() },
            ])
        );
        // No synced team's agent runs here: the Assistant alone brings the voice up.
        let none = |_: &str| Ok(Vec::new());
        assert_eq!(hosted_from(&teams, "here", none, assistant), Some(vec![HostedAgent { id: "z".into(), team_id: "personal".into() }]));
    }

    /// Switch off (self-hosted): the Assistant is a row of a synced team's
    /// list as well: one entry. One that runs elsewhere, or none: not hosted.
    #[test]
    fn the_assistant_is_one_entry_and_only_when_it_runs_here() {
        let teams = vec!["t".to_string(), "u".to_string()];
        let list = |team: &str| Ok(if team == "t" { listed(serde_json::json!([{ "id": "z", "teamId": "t", "deviceId": "here", "role": "assistant" }])) } else { Vec::new() });
        let same = || Ok(listed(serde_json::json!([{ "id": "z", "teamId": "t", "deviceId": "here", "role": "assistant" }])).pop());
        assert_eq!(hosted_from(&teams, "here", list, same), Some(vec![HostedAgent { id: "z".into(), team_id: "t".into() }]));
        let elsewhere = || Ok(listed(serde_json::json!([{ "id": "y", "teamId": "personal", "deviceId": "desktop" }])).pop());
        assert_eq!(hosted_from(&teams, "here", list, elsewhere), Some(vec![HostedAgent { id: "z".into(), team_id: "t".into() }]));
        assert_eq!(hosted_from(&teams, "here", list, || Ok(None)), Some(vec![HostedAgent { id: "z".into(), team_id: "t".into() }]));
    }

    /// #EXP-2080: no Assistant yet = nothing created, nothing hosted. The
    /// daemon only READS (`voice.get`, then `agents.get` when one is named):
    /// a member's first Assistant is created by a client of theirs, on a
    /// desktop (NEW-99), never by a daemon's poll.
    #[test]
    fn no_assistant_yet_creates_nothing_and_hosts_nothing() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let line = String::from_utf8_lossy(&buf[..n]).lines().next().unwrap_or_default().to_string();
                let body = if line.contains("/agents.list") {
                    r#"{"result":{"data":{"agents":[]}}}"#
                } else if line.contains("/voice.get") {
                    r#"{"result":{"data":{"voiceDeviceId":null,"agentId":null}}}"#
                } else {
                    r#"{"result":{"data":{"agent":{"id":"made","teamId":"personal","deviceId":"here","role":"assistant"}}}}"#
                };
                log.lock().unwrap().push(line);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            }
        });
        let trpc = api::TrpcClient::new(&base, Arc::new(api::StaticToken("tok".into())));
        assert_eq!(hosted_agents(&trpc, &["t".to_string()], "here"), Some(Vec::new()));
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[0].starts_with("GET /api/trpc/agents.list?"), "{seen:?}");
        assert!(seen[1].starts_with("GET /api/trpc/voice.get"), "{seen:?}");
        assert!(seen.iter().all(|line| !line.starts_with("POST") && !line.contains("ensureAssistant")), "{seen:?}");
        // Pure: none named = the agent is never read.
        assert_eq!(own_assistant(|| Ok(None), |_| panic!("no agent is read")).unwrap(), None);
    }

    /// A read that did not reach the server leaves the last answer standing
    /// (None), the Assistant's as a team's; a server that refuses the ask
    /// (an older one) hosts the teams' agents as before.
    #[test]
    fn a_failed_assistant_read_never_drops_the_hosted_agents() {
        let teams = vec!["t".to_string()];
        let list = |_: &str| Ok(listed(serde_json::json!([{ "id": "a", "teamId": "t", "deviceId": "here" }])));
        let offline = || Err(api::ApiError::Transport { message: "down".into(), offline: true });
        assert_eq!(hosted_from(&teams, "here", list, offline), None);
        let signed_out = || Err(api::ApiError::Unauthorized);
        assert_eq!(hosted_from(&teams, "here", list, signed_out), None);
        let unreadable = || Err(api::ApiError::Decode("an older answer".into()));
        assert_eq!(hosted_from(&teams, "here", list, unreadable), Some(vec![HostedAgent { id: "a".into(), team_id: "t".into() }]));
        let refused = || Err(api::ApiError::Http { status: 404, message: "No procedure".into() });
        assert_eq!(hosted_from(&teams, "here", list, refused), Some(vec![HostedAgent { id: "a".into(), team_id: "t".into() }]));
    }

    /// The N95 case: no agent here = no worker, no row; an agent here and
    /// no usable GPU = `missing` at once, the worker gone, rings unanswered.
    #[test]
    fn a_server_without_a_gpu_reports_it_and_never_answers() {
        let mut rig = Rig::new();
        assert_eq!(rig.host.speech_state(), None);
        assert!(rig.speech.take().is_empty(), "nothing runs before an agent runs here");
        rig.handle(Input::Agents(vec![agent()]));
        assert_eq!(rig.speech.take(), ["prepare en"], "the probe");
        rig.handle(Input::Worker(etalk::Event::Voices { voices: Vec::new() }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::NoGpu));
        assert_eq!(rig.speech.take(), ["shutdown"], "the probe answered: the worker exits");
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert!(rig.net.take().is_empty(), "a server that can't speak never answers (the caller fell back)");
    }

    #[test]
    fn a_downloading_voice_answers_and_the_agent_moving_away_stops_it() {
        let mut rig = Rig::new();
        rig.handle(Input::Agents(vec![agent()]));
        rig.handle(Input::Worker(etalk::Event::Downloading { received: 1, total: 10 }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Downloading));
        rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c1 agent-1"], "it rings until the voice is ready");
        rig.speech.take();
        rig.handle(Input::Agents(Vec::new()));
        assert_eq!(rig.host.speech_state(), None);
        assert_eq!(rig.speech.take(), ["shutdown"]);
    }

    #[test]
    fn a_failed_download_is_probed_again_later() {
        let mut rig = Rig::new();
        rig.handle(Input::Agents(vec![agent()]));
        rig.handle(Input::Worker(etalk::Event::DownloadFailed { error: "offline".into() }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Failed));
        rig.handle(Input::Worker(etalk::Event::Voices { voices: Vec::new() }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Failed), "not no GPU");
        assert_eq!(rig.speech.take(), ["prepare en", "shutdown"]);
        rig.advance(Duration::from_secs(29));
        assert!(rig.speech.take().is_empty());
        rig.advance(Duration::from_secs(1));
        assert_eq!(rig.speech.take(), ["prepare en"], "the probe runs again");
        rig.handle(Input::Worker(etalk::Event::Voices { voices: vec!["en".into()] }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Ready));
        assert_eq!(rig.speech.take(), ["call probe en"]);
    }

    #[test]
    fn three_crashes_in_a_row_fail_the_voice() {
        etalk::log_capture::start();
        let mut rig = Rig::ready();
        for _ in 0..MAX_CRASHES {
            rig.handle(Input::Worker(etalk::Event::Exited { code: None, signal: Some(11) }));
        }
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Failed));
        // #ERR-29: the give-up is an error, the level GlitchTip sends.
        assert!(etalk::log_capture::error_logged("[voice] the speech worker kept crashing; voice given up until a restart"), "{:?}", etalk::log_capture::records());
    }

    /// A ring: the agent's voice is prepared, the call listens, rang +
    /// ready answer it (#EXP-1961: no fixed greeting: it listens); an
    /// utterance goes to its conversation; the agent's reply is spoken and
    /// streams back; the caller hangs up.
    #[test]
    fn a_call_runs_on_the_server_like_on_a_desktop() {
        let mut rig = Rig::ready();
        rig.rows.lock().unwrap().push(row("old", "agent", "Earlier reply.", "2020-01-01T00:00:00Z"));
        rig.answered();
        assert_eq!(rig.net.take(), ["answer c1 agent-1"]);
        assert_eq!(rig.speech.take(), ["prepare de", "call call-1 de"]);
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        assert_eq!(rig.link.take(), [r#"event {"t":"ready"}"#]);
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Audio(vec![0; 320]) });
        rig.handle(Input::Worker(etalk::Event::Level { id: "call-1".into(), level: 0.5, bands: Default::default() }));
        assert_eq!(rig.speech.take(), ["audio call-1 320"]);
        // #EXP-1941: a loss on the way goes to the worker, which knows the utterance.
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Lost { ms: 60 } });
        assert_eq!(rig.speech.take(), ["lost call-1 60"]);
        // The ring's longest earliest answer: rang, then the greeting.
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert_eq!(rig.link.take(), [r#"event {"t":"rang"}"#, r#"event {"t":"replyFinished"}"#], "the caller was heard: no noSound");
        assert!(rig.speech.take().is_empty(), "no fixed greeting");
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "Wie spät ist es?".into() }));
        assert_eq!(rig.link.take(), [r#"event {"t":"said","text":"Wie spät ist es?"}"#]);
        assert_eq!(rig.net.take(), ["send c1 root-1 Wie spät ist es?"]);
        // The agent replies in the conversation: spoken, never the old row.
        rig.rows.lock().unwrap().push(row("r1", "agent", "Es ist drei Uhr. Sonst noch etwas?", "2099-01-01T00:00:00Z"));
        rig.advance(REPLY_POLL);
        assert_eq!(rig.speech.take(), ["speak r1 Es ist drei Uhr. Sonst noch etwas?"]);
        assert_eq!(rig.link.take(), [r#"event {"t":"replyStarted","text":"Es ist drei Uhr."}"#]);
        // The caller hangs up mid-reply: the worker stops and hangs up.
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.speech.take(), ["hangup call-1", "stop"]);
        assert!(rig.host.call.is_none());
        // Idle: the worker exits after `serverIdleMs`, not before.
        rig.advance(Duration::from_millis(VOICE_CALL.constants.server_idle_ms) - Duration::from_secs(1));
        assert!(rig.speech.take().is_empty());
        rig.advance(Duration::from_secs(1));
        assert_eq!(rig.speech.take(), ["shutdown"]);
    }

    fn call_row(id: &str, body: &str, at: &str) -> domain::rows::MessageRow {
        let mut row = row(id, "agent", body, at);
        row.session_id = "call-s".into();
        row
    }

    /// #EXP-1961 (#FEED-275): a call with its own session: no fixed
    /// greeting; `ready` waits for the agent's own opening line, which is
    /// said at the pickup; utterances go to the call session; the hang-up
    /// tells the server.
    #[test]
    fn a_call_session_opens_with_the_agents_own_line() {
        let mut rig = Rig::ready();
        rig.answered_in_session();
        rig.speech.take();
        rig.advance(Duration::from_millis(1000));
        assert!(rig.link.take().is_empty(), "the voice is not in yet: ringing");
        // The opening line is written during the ring; the voice comes in.
        rig.rows.lock().unwrap().push(call_row("o1", "Hallo Danny, der Build ist grün.", "2099-01-01T00:00:00Z"));
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        assert_eq!(rig.link.take(), [r#"event {"t":"ready"}"#]);
        assert!(rig.speech.take().iter().all(|line| !line.starts_with("speak")), "nothing before the pickup");
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert_eq!(rig.speech.take(), ["speak o1 Hallo Danny, der Build ist grün."]);
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"replyStarted","text":"Hallo Danny, der Build ist grün."}"#.to_string()), "{events:?}");
        assert!(!events.contains(&r#"event {"t":"replyFinished"}"#.to_string()), "{events:?}");
        rig.handle(Input::Worker(etalk::Event::SpeechAudio { id: "o1".into(), pcm: etalk::encode_pcm(&[1, 2, 3]), rate: 24_000 }));
        assert_eq!(rig.link.take(), ["speech 3 24000"]);
        rig.handle(Input::Worker(etalk::Event::Finished { id: "o1".into(), stopped: false }));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "Danke".into() }));
        assert_eq!(rig.net.take(), ["answer c1 agent-1", "send c1 call-s Danke"]);
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Ended { reason: None } });
        assert_eq!(rig.net.take(), ["ended call-s"]);
    }

    /// #EXP-1961: no opening line `openingWaitMs` after the voice: a silent
    /// pickup on Listening, and the late line is never spoken.
    #[test]
    fn a_call_session_without_an_opening_picks_up_silently() {
        let mut rig = Rig::ready();
        rig.answered_in_session();
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.advance(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"ready"}"#.to_string()), "{events:?}");
        assert!(events.contains(&r#"event {"t":"replyFinished"}"#.to_string()), "{events:?}");
        rig.speech.take();
        rig.rows.lock().unwrap().push(call_row("o1", "Hallo!", "2099-01-01T00:00:00Z"));
        rig.advance(REPLY_POLL);
        assert!(rig.speech.take().is_empty(), "a late opening is not spoken");
        rig.rows.lock().unwrap().push(call_row("r2", "Noch etwas?", "2099-01-01T00:00:01Z"));
        rig.advance(REPLY_POLL);
        assert_eq!(rig.speech.take(), ["speak r2 Noch etwas?"]);
    }

    /// #EXP-1992 (#EXP-1936): a reply that lands while a call without its
    /// own session still rings is not the call's: never said into the ring,
    /// nor after the pickup (it was said here before).
    #[test]
    fn a_reply_during_the_ring_is_not_the_calls() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.speech.take();
        rig.link.take();
        rig.rows.lock().unwrap().push(row("r1", "agent", "The answer to what you typed. More.", "2099-01-01T00:00:00Z"));
        rig.advance(REPLY_POLL);
        assert!(rig.speech.take().iter().all(|line| !line.starts_with("speak")), "never said into the ring");
        assert!(rig.link.take().is_empty(), "no replyStarted while it rings");
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert!(rig.speech.take().iter().all(|line| !line.starts_with("speak")), "nor after the pickup");
    }

    /// #EXP-1992: voice-call.json `hostCases` through the DAEMON's own
    /// wiring, event by event: what reaches the worker, the caller's link
    /// and the server is what the row says. Every row a server can run: a
    /// streamed call (its clip is fetched before the answer, and nobody
    /// presses End there).
    #[test]
    fn the_daemon_replays_the_host_cases() {
        use steer::call_host::CallHostEvent;
        let fixture: serde_json::Value = serde_json::from_str(etalk::call::VOICE_CALL_JSON).unwrap();
        let kind = |event: &serde_json::Value| event["t"].as_str().unwrap().to_string();
        let mut replayed = 0;
        for case in fixture["hostCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let start: HostStart = serde_json::from_value(case["start"].clone()).unwrap();
            let events = case["events"].as_array().unwrap();
            let foreign = events.iter().any(|event| matches!(kind(event).as_str(), "voice" | "prepared" | "end" | "timer"));
            if !start.remote || start.voice != etalk::call::CallVoice::None || foreign {
                continue;
            }
            replayed += 1;
            // A sealed call: its key source adopts the chat, then its session is asked.
            let sealed = events.iter().any(|event| kind(event) == "sessionAsked");
            let mut rig = Rig::ready();
            let base = rig.now;
            rig.handle(Input::Ring { call_id: "c1".into(), agent_id: "agent-1".into() });
            rig.handle(Input::Answered(Answered {
                call_id: "c1".into(),
                agent: agent(),
                link: Box::new(FakeLink(rig.link.clone())),
                voice: etalk::VoiceChoice { language: start.language.clone(), ..Default::default() },
                root: (!sealed).then(|| "root-1".to_string()),
                session: start.session.clone(),
            }));
            // The row's own draw for the ring (the daemon draws a random one).
            rig.host.call.as_mut().unwrap().host = CallHost::new(HostStart { call_id: Some("c1".into()), ..start });
            (rig.speech.take(), rig.link.take(), rig.net.take());
            let id = || "call-1".to_string();
            let mut rows = 0;
            for (index, (event, effects)) in events.iter().zip(case["effects"].as_array().unwrap()).enumerate() {
                rig.now = base + Duration::from_millis(event["at"].as_u64().unwrap());
                let text = || event["text"].as_str().unwrap_or_default().to_string();
                let row_id = || event["id"].as_str().unwrap_or_default().to_string();
                let link = |event| Input::Link { call_id: "c1".into(), event };
                match kind(event).as_str() {
                    "begin" => rig.handle(link(CallHostEvent::Ready)),
                    "tick" => rig.host.tick(rig.now),
                    "listening" => rig.handle(Input::Worker(etalk::Event::CallListening { id: id() })),
                    "level" => rig.handle(Input::Worker(etalk::Event::Level { id: id(), level: event["level"].as_f64().unwrap() as f32, bands: Default::default() })),
                    "partial" => rig.handle(Input::Worker(etalk::Event::Partial { id: id(), text: text() })),
                    "speechStarted" => rig.handle(Input::Worker(etalk::Event::SpeechStarted { id: id() })),
                    "said" => rig.handle(Input::Worker(etalk::Event::Said { id: id(), text: text() })),
                    "listenFailed" => {
                        let reason = serde_json::from_value(event["reason"].clone()).unwrap();
                        rig.handle(Input::Worker(etalk::Event::ListenFailed { id: id(), reason, detail: String::new() }));
                    }
                    "sessionAsked" => rig.handle(Input::Root { call_id: "c1".into(), root: "root-1".into() }),
                    "session" => rig.handle(Input::Session { call_id: "c1".into(), session: event["session"].as_str().map(str::to_string) }),
                    // The agent's reply lands in the conversation the call reads; the watch finds it.
                    "reply" => {
                        rows += 1;
                        let conversation = rig.host.call.as_ref().and_then(|call| call.host.session().map(str::to_string));
                        let mut reply = row(&row_id(), "agent", &text(), &format!("2099-01-01T00:00:{rows:02}Z"));
                        reply.session_id = conversation.unwrap_or_else(|| "root-1".into());
                        rig.rows.lock().unwrap().push(reply);
                        rig.host.watch_replies(rig.now);
                    }
                    "spoken" => rig.handle(Input::Worker(etalk::Event::Finished { id: row_id(), stopped: false })),
                    "notSpoken" => rig.handle(Input::Worker(etalk::Event::Failed { id: row_id(), error: "not spoken".into() })),
                    "mute" => rig.handle(link(CallHostEvent::Mute(event["muted"].as_bool().unwrap()))),
                    "hungUp" => rig.handle(link(CallHostEvent::Ended { reason: event["reason"].as_str().map(str::to_string) })),
                    "fail" => {
                        assert_eq!(event["code"], "dropped", "{name}: a server fails a call itself when its worker dies");
                        rig.handle(Input::Worker(etalk::Event::Exited { code: None, signal: Some(11) }));
                    }
                    "unstorable" => rig.handle(Input::Unstored { call_id: "c1".into() }),
                    other => panic!("{name}: no daemon input for {other}"),
                }
                let (mut speech, mut link, mut net) = (Vec::new(), Vec::new(), Vec::new());
                for effect in serde_json::from_value::<Vec<HostEffect>>(effects.clone()).unwrap() {
                    match effect {
                        HostEffect::Event { event } => link.push(format!("event {event}")),
                        HostEffect::Flush => link.push("flush".to_string()),
                        HostEffect::End => link.push("end".to_string()),
                        HostEffect::Listen { id, language, .. } => speech.push(format!("call {id} {language}")),
                        HostEffect::Mute { id, muted } => speech.push(format!("mute {id} {muted}")),
                        HostEffect::Speak { id, text } => speech.push(format!("speak {id} {text}")),
                        HostEffect::Stop => speech.push("stop".to_string()),
                        HostEffect::Hangup { id } => speech.push(format!("hangup {id}")),
                        HostEffect::Send { text, session } => net.push(format!("send c1 {} {text}", session.as_deref().unwrap_or("root-1"))),
                        HostEffect::CallEnded { session } => net.push(format!("ended {session}")),
                        // A server has no screen.
                        HostEffect::Toast { .. } => {}
                    }
                }
                let at = format!("{name}: event {index} ({})", kind(event));
                assert_eq!(rig.speech.take(), speech, "the worker, {at}");
                assert_eq!(rig.link.take(), link, "the link, {at}");
                // Asking for the call session is the daemon's own (no effect of the call).
                let sent: Vec<String> = rig.net.take().into_iter().filter(|line| !line.starts_with("session ")).collect();
                assert_eq!(sent, net, "the server, {at}");
            }
            let phase = rig.host.call.as_ref().map_or(etalk::call::CallPhase::Hidden, |call| call.host.state().phase);
            assert_eq!(format!("{phase:?}").to_lowercase(), case["phase"].as_str().unwrap(), "{name}");
        }
        assert!(replayed >= 20, "{replayed} rows a server can run");
    }

    #[test]
    fn a_silent_caller_gets_no_sound_and_words_during_the_ring_are_dropped() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.handle(Input::Worker(etalk::Event::Said { id: "call-1".into(), text: "hello?".into() }));
        assert!(rig.net.take().iter().all(|line| !line.starts_with("send")), "said during the ring: dropped");
        rig.link.take();
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"noSound"}"#.to_string()), "{events:?}");
    }

    #[test]
    fn a_worker_crash_fails_the_call_and_a_busy_server_ignores_a_second_ring() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.net.take();
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        assert!(rig.net.take().is_empty(), "busy: the relay tells the caller");
        etalk::log_capture::start();
        rig.handle(Input::Worker(etalk::Event::Exited { code: None, signal: Some(11) }));
        let events = rig.link.take();
        assert_eq!(events.first().map(String::as_str), Some(r#"event {"t":"failed","code":"dropped"}"#), "{events:?}");
        // #ERR-29: a call the worker's death dropped is an error.
        assert!(etalk::log_capture::error_logged("[voice] the speech worker exited during a call"), "{:?}", etalk::log_capture::records());
        assert!(events.contains(&"end".to_string()));
        assert!(rig.host.call.is_none());
    }

    #[test]
    fn a_voice_never_ready_fails_unavailable_after_the_longest_ring() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.link.take();
        rig.advance(Duration::from_millis(VOICE_CALL.constants.max_ring_ms));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"failed","code":"unavailable"}"#.to_string()), "{events:?}");
    }

    /// EXP-1930 (voice-hosts-3): a voice that fails to load inside the ring
    /// fails the call `unavailable` (never a mute call), and the row says
    /// failed, so the next ring falls back instead of ringing a mute server.
    #[test]
    fn a_voice_that_fails_to_load_in_the_ring_fails_the_call() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.link.take();
        rig.handle(Input::Worker(etalk::Event::DownloadFailed { error: "out of memory".into() }));
        rig.handle(Input::Worker(etalk::Event::Voices { voices: Vec::new() }));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"failed","code":"unavailable"}"#.to_string()), "{events:?}");
        assert!(rig.host.call.is_none());
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Failed), "nothing stays loaded on a server");
        rig.net.take();
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        assert!(rig.net.take().is_empty(), "the next caller falls back");
    }

    /// EXP-1930 (voice-hosts-3): an opening line that cannot be spoken
    /// fails the call `unavailable` (#EXP-1961: the agent's own line).
    #[test]
    fn an_opening_that_fails_fails_the_call() {
        let mut rig = Rig::ready();
        rig.answered_in_session();
        rig.rows.lock().unwrap().push(call_row("o1", "Hallo!", "2099-01-01T00:00:00Z"));
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "call-1".into() }));
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert!(rig.speech.take().contains(&"speak o1 Hallo!".to_string()));
        rig.link.take();
        rig.handle(Input::Worker(etalk::Event::Failed { id: "o1".into(), error: "the voice is unavailable for now".into() }));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"failed","code":"unavailable"}"#.to_string()), "{events:?}");
        assert!(rig.host.call.is_none());
    }

    /// EXP-1930 (voice-engine-speak-2, voice-hosts-4): a slow warm-up at the
    /// probe is not "no usable GPU": the row says preparing, rings are still
    /// answered, and the probe runs again until the voice answers.
    #[test]
    fn a_slow_warm_up_is_probed_again_not_no_gpu() {
        let mut rig = Rig::new();
        rig.handle(Input::Agents(vec![agent()]));
        rig.handle(Input::Worker(etalk::Event::VoiceRetry { in_ms: 60_000 }));
        rig.handle(Input::Worker(etalk::Event::Voices { voices: Vec::new() }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Downloading), "not up yet, never NoGpu");
        assert_eq!(rig.speech.take(), ["prepare en", "shutdown"]);
        rig.advance(Duration::from_secs(30));
        assert_eq!(rig.speech.take(), ["prepare en"], "probed again");
        rig.handle(Input::Worker(etalk::Event::Voices { voices: vec!["en".into()] }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Ready));
        assert_eq!(rig.speech.take(), ["call probe en"]);
    }

    /// EXP-1930 (voice-hosts-3 variant): a slow warm-up inside a ring fails
    /// that call `unavailable` and leaves no "no usable GPU" behind.
    #[test]
    fn a_slow_warm_up_in_the_ring_fails_the_call_and_tries_again() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.link.take();
        rig.handle(Input::Worker(etalk::Event::VoiceRetry { in_ms: 60_000 }));
        rig.handle(Input::Worker(etalk::Event::Voices { voices: Vec::new() }));
        let events = rig.link.take();
        assert!(events.contains(&r#"event {"t":"failed","code":"unavailable"}"#.to_string()), "{events:?}");
        assert_ne!(rig.host.speech_state(), Some(SpeechState::NoGpu));
        assert!(rig.host.retry_at.is_some(), "probed again later");
    }

    /// EXP-1930 (voice-hosts-5): a probe that starts long after the daemon
    /// (or the last call) is never killed by the idle exit.
    #[test]
    fn a_late_probe_survives_the_idle_exit() {
        let mut rig = Rig::new();
        rig.advance(Duration::from_secs(11 * 60));
        rig.handle(Input::Agents(vec![agent()]));
        rig.advance(TICK);
        assert_eq!(rig.speech.take(), ["prepare en"], "the probe runs");
        rig.handle(Input::Worker(etalk::Event::Voices { voices: vec!["en".into()] }));
        rig.advance(TICK);
        assert_eq!(rig.speech.take(), ["call probe en"], "the recognizer loads, the worker stays");
        rig.handle(Input::Worker(etalk::Event::CallListening { id: "probe".into() }));
        assert_eq!(rig.speech.take(), ["hangup probe", "shutdown"]);
    }

    /// EXP-1930 (voice-hosts-6): the recognizer's download after a known
    /// voice ends with `ModelReady`: the row is Ready again and the idle
    /// exit works.
    #[test]
    fn model_ready_ends_the_recognizers_download() {
        let mut rig = Rig::new();
        rig.handle(Input::Agents(vec![agent()]));
        rig.handle(Input::Worker(etalk::Event::Voices { voices: vec!["en".into()] }));
        rig.handle(Input::Worker(etalk::Event::Downloading { received: 1, total: 10 }));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Downloading));
        rig.handle(Input::Worker(etalk::Event::ModelReady));
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Ready));
        rig.speech.take();
        rig.advance(Duration::from_millis(VOICE_CALL.constants.server_idle_ms));
        assert_eq!(rig.speech.take(), ["shutdown"], "idle: the worker exits");
    }

    /// EXP-1930 (voice-hosts-1): the first utterance creates (or wakes) the
    /// conversation in clear; the daemon then keys the tree; the second one
    /// is sealed with that key, never refused as plaintext in a keyed tree.
    #[test]
    fn an_utterance_is_sealed_with_the_trees_key_at_send_time() {
        use coding::sealing::SealError;
        let keyed = std::sync::atomic::AtomicBool::new(false);
        let seal = |root: &str, text: &str| -> Result<String, SealError> {
            if keyed.load(Ordering::SeqCst) {
                Ok(format!("sealed({root}):{text}"))
            } else {
                Err(SealError::NotReader)
            }
        };
        assert_eq!(utterance_body(None, "hello", &seal).as_deref(), Some("hello"), "a new conversation: the send makes it");
        assert_eq!(utterance_body(Some("root-1"), "hello", &seal).as_deref(), Some("hello"), "a keyless tree nobody may key: plain");
        keyed.store(true, Ordering::SeqCst);
        assert_eq!(utterance_body(Some("root-1"), "and now?", &seal).as_deref(), Some("sealed(root-1):and now?"));
        assert_eq!(utterance_body(Some("root-1"), "x", &|_: &str, _: &str| Err(SealError::NoService)).as_deref(), Some("x"), "no key holder: plain");
        // #EXP-2123: a waiting install's no-key refusal is never plaintext.
        let no_key = |_: &str, _: &str| Err(SealError::NoKey(coding::sealing::NO_KEY_REFUSAL.into()));
        assert_eq!(utterance_body(Some("root-1"), "secret", &no_key), None);
        assert_eq!(utterance_body(Some("root-1"), "secret", &|_: &str, _: &str| Err(SealError::KeyMakerWaits)), None);
    }

    /// EXP-1930 (voice-hosts-8): the reply watch reads only the rows since
    /// the call was answered, never the whole conversation every poll.
    #[test]
    fn the_reply_watch_reads_only_the_calls_rows() {
        let mut rig = Rig::ready();
        rig.rows.lock().unwrap().push(row("old", "agent", "Earlier reply.", "2020-01-01T00:00:00Z"));
        rig.answered();
        assert_eq!(rig.reads.take(), ["messages root-1 all"], "the history, once");
        rig.advance(REPLY_POLL);
        assert_eq!(rig.reads.take(), ["messages root-1 2020-01-01T00:00:00Z"]);
    }

    /// EXP-1930 (voice-hosts-2, voice-hosts-10): a call whose socket went
    /// silent (`call_host`'s liveness deadline: `Dropped`) fails `dropped`
    /// and the server answers the next ring, never busy until a restart.
    #[test]
    fn a_silent_link_ends_the_call_and_frees_the_server() {
        let mut rig = Rig::ready();
        rig.answered();
        rig.net.take();
        rig.link.take();
        rig.handle(Input::Link { call_id: "c1".into(), event: steer::call_host::CallHostEvent::Dropped("the call went silent".into()) });
        let events = rig.link.take();
        assert_eq!(events.first().map(String::as_str), Some(r#"event {"t":"failed","code":"dropped"}"#), "{events:?}");
        assert!(rig.host.call.is_none());
        rig.handle(Input::Ring { call_id: "c2".into(), agent_id: "agent-1".into() });
        assert_eq!(rig.net.take(), ["answer c2 agent-1"]);
    }

    /// #EXP-2013: a call answered before the chat's first message was read
    /// (no conversation yet) sends its first utterance in clear by agentId;
    /// refused (the new chat is keyed), the target is read again, names the
    /// conversation now, and the words go sealed into it: stored, the call
    /// lives. A target still missing ends the call as before.
    #[test]
    fn a_first_utterance_refused_in_clear_goes_sealed_into_the_chat_that_exists_now() {
        use std::cell::RefCell;
        struct Wire {
            target: Option<String>,
            sends: RefCell<Vec<String>>,
            reads: RefCell<u32>,
        }
        impl UtteranceWire for Wire {
            fn target(&self, _: &str) -> Result<Option<String>, String> {
                *self.reads.borrow_mut() += 1;
                Ok(self.target.clone())
            }
            fn seal(&self, root: &str, text: &str) -> Result<String, coding::sealing::SealError> {
                Ok(format!("sealed({root}):{text}"))
            }
            fn send(&self, input: &api::sessions::SendInput) -> Result<api::sessions::SendOutput, api::ApiError> {
                let to = input.session_id.clone().or(input.agent_id.clone().map(|agent| format!("agent:{agent}"))).unwrap_or_default();
                self.sends.borrow_mut().push(format!("{to} {}", input.text.clone().unwrap_or_default()));
                match input.session_id.clone() {
                    Some(session_id) => Ok(api::sessions::SendOutput { session_id, ..Default::default() }),
                    None => Err(api::ApiError::Http { status: 400, message: "This chat is end-to-end encrypted: its messages must be encrypted too".into() }),
                }
            }
        }
        let wire = Wire { target: Some("root-1".into()), sends: RefCell::default(), reads: RefCell::default() };
        assert_eq!(deliver_utterance(&wire, "c1", &agent(), None, None, "hello"), Delivered::Sent("root-1".into()));
        assert_eq!(*wire.sends.borrow(), ["agent:agent-1 hello", "root-1 sealed(root-1):hello"]);
        assert_eq!(*wire.reads.borrow(), 1, "read again once");

        let gone = Wire { target: None, sends: RefCell::default(), reads: RefCell::default() };
        assert_eq!(deliver_utterance(&gone, "c1", &agent(), None, None, "hello"), Delivered::Unstored, "nothing to seal for: the call ends");

        let known = Wire { target: Some("root-1".into()), sends: RefCell::default(), reads: RefCell::default() };
        assert_eq!(deliver_utterance(&known, "c1", &agent(), Some("root-1"), Some("call-s"), "hi"), Delivered::Sent("call-s".into()));
        assert_eq!(*known.reads.borrow(), 0, "a known conversation reads nothing");

        // The host adopts the conversation: the next utterance is sealed into it.
        let mut rig = Rig::ready();
        answered_sealed(&mut rig);
        rig.handle(Input::Sent { call_id: "c1".into(), session_id: "root-1".into() });
        assert_eq!(rig.host.call.as_ref().and_then(|call| call.root.as_deref()), Some("root-1"));
    }

    /// #EXP-2076 (chat-express.json `plainProseCases`): the call says an agent
    /// row's prose only; a block-only reply says nothing; a person's text and
    /// a block-less agent row stay as they are.
    #[test]
    fn spoken_prose_reads_an_agent_rows_prose() {
        let sample = "Checked the three lanes.\n<a2ui>\nroot = Text(text=\"Build 1482 is green\")\n</a2ui>\nAll green.";
        let agent = row("m1", "agent", sample, "2026-11-18T09:10:00.000Z");
        assert_eq!(spoken_prose(&agent).as_deref(), Some("Checked the three lanes.\n\nAll green."));
        let only = row("m2", "agent", "<a2ui>\nroot = Text(text=\"x\")\n</a2ui>", "2026-11-18T09:10:00.000Z");
        assert_eq!(spoken_prose(&only).as_deref(), Some(""));
        assert_eq!(spoken_prose(&row("m3", "agent", "Plain words.", "2026-11-18T09:10:00.000Z")), None);
        assert_eq!(spoken_prose(&row("m4", "user", sample, "2026-11-18T09:10:00.000Z")), None);
    }
}
