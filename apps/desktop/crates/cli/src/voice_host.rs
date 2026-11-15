//! EXP-1858 (voice per agent c, `fixtures/voice-call.json`): SPEECH ON A
//! SERVER. The daemon is the call host of the agents that run on it,
//! headless: no microphone and no speakers, audio only over the call stream.
//!
//! WORKER. The same speech worker as the IDE (`exponential __speech-worker`,
//! `etalk::SpeechClient`; CrispASR is a pinned runtime download, nothing is
//! linked), but only while one of the member's agents runs here (`agents.list`
//! of the synced teams, Runs on = this device). It is PROBED once (no CUDA
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
//! are spoken (`Speak`) and stream back (`SpeechAudio`). The flow events
//! (`rang`, `ready`, `noSound`, `partial`, `said`, `queued`, `replyStarted`,
//! `replyFinished`, `speechStarted`, `failed`) step `etalk::call`'s machine
//! here and go to the caller as the IDE's do.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coding::device_doctor::SpeechState;
use etalk::call::{call_greeting, call_ring_ms, call_ring_random, call_step, call_wire_event, CallEvent, CallPhase, CallState, VOICE_CALL};
use etalk::voice::{VoiceContext, VoiceMessage, VOICE};

/// Crashes in a row before the voice is given up (the desktop's rule).
const MAX_CRASHES: u32 = 3;
/// How often the agents that run here are re-read.
const AGENTS_EVERY: Duration = Duration::from_secs(60);
/// The loop's tick (timers, the reply poll).
const TICK: Duration = Duration::from_millis(100);
/// The mic check listens at least this long after `CallListening`.
const MIC_CHECK_MIN: Duration = Duration::from_millis(1500);
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
    /// The member's conversation with the agent (None = the first send makes it).
    pub root: Option<String>,
}

/// What the host's loop reacts to.
pub enum Input {
    Agents(Vec<HostedAgent>),
    Settings(etalk::voice::VoiceSettings),
    Ring { call_id: String, agent_id: String },
    Answered(Answered),
    /// The network side gave up on a ring (logged, nothing to step).
    Unanswered { call_id: String },
    Worker(etalk::Event),
    Link { call_id: String, event: steer::call_host::CallHostEvent },
    /// A send answered: the conversation it went to.
    Sent { call_id: String, session_id: String },
    Stop,
}

/// What the host asks of the network (a thread each; tests record).
pub trait Net: Send {
    /// Answer `call_id`: the host ticket, the agent's voice, its conversation.
    fn answer(&self, call_id: String, agent_id: String);
    /// Send `text` to the agent (`root` = its conversation, None = new).
    fn send(&self, call_id: String, agent: HostedAgent, root: Option<String>, text: String);
    /// The rows of `root` (opened), for the reply watch.
    fn messages(&self, root: &str) -> Vec<domain::rows::MessageRow>;
}

/// The voice's state as the doctor reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Download {
    #[default]
    Idle,
    Running,
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
        Download::Running => return Some(SpeechState::Downloading),
        Download::Failed if voices.is_none() => return Some(SpeechState::Failed),
        _ => {}
    }
    Some(if voices?.is_empty() { SpeechState::NoGpu } else { SpeechState::Ready })
}

/// The running call.
struct Call {
    call_id: String,
    /// The worker's call id.
    id: String,
    agent: HostedAgent,
    link: Box<dyn Link>,
    language: String,
    root: Option<String>,
    state: CallState,
    started: Instant,
    ring: Duration,
    rang: bool,
    listening_at: Option<Instant>,
    heard: bool,
    mic_checked: bool,
    greeting_id: String,
    /// The reply (or greeting) being spoken.
    speaking: Option<String>,
    queued: usize,
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

    fn probe(&mut self) {
        self.up = true;
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
            Input::Ring { call_id, agent_id } => self.on_ring(call_id, agent_id),
            Input::Answered(answered) => self.on_answered(answered, now),
            Input::Unanswered { call_id } => log::info!("[voice] call {call_id}: not answered here"),
            Input::Worker(event) => self.on_worker(event, now),
            Input::Link { call_id, event } => self.on_link(&call_id, event, now),
            Input::Sent { call_id, session_id } => {
                if let Some(call) = self.call.as_mut().filter(|call| call.call_id == call_id) {
                    call.root.get_or_insert(session_id);
                }
            }
            Input::Stop => {
                self.fail("dropped", now);
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
            self.fail("unavailable", now);
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
            self.probe();
        }
    }

    fn on_ring(&mut self, call_id: String, agent_id: String) {
        if self.call.is_some() || !self.answers() {
            log::info!("[voice] call {call_id}: not answered here (busy or no voice)");
            return;
        }
        if !self.agents.iter().any(|agent| agent.id == agent_id) {
            // The list lags a Runs on move by up to a minute: the network
            // side re-reads the agent and refuses one that runs elsewhere.
            log::info!("[voice] call {call_id}: {agent_id} is not (yet) known to run here");
        }
        self.net.answer(call_id, agent_id);
    }

    fn on_answered(&mut self, answered: Answered, now: Instant) {
        if self.call.is_some() {
            answered.link.end();
            return;
        }
        self.counter += 1;
        let id = format!("call-{}", self.counter);
        let language = if answered.voice.language.is_empty() { self.settings.language().to_string() } else { answered.voice.language.clone() };
        // The agent's own voice first, then the call (the worker warms up
        // and answers `CallListening`).
        self.up = true;
        self.speech.prepare(etalk::VoiceChoice { language: language.clone(), ..answered.voice });
        self.speech.call(&id, &language);
        // The rows there now are history: the newest one's instant arms the
        // watch (the server's clock, never this machine's).
        let mut seen = HashSet::new();
        let mut armed_at: Option<(i64, String)> = None;
        if let Some(root) = answered.root.as_deref() {
            for row in self.net.messages(root) {
                if let Some(at) = row.created_at.as_deref().and_then(|at| etalk::voice::parse_instant(at).map(|ms| (ms, at.to_string()))) {
                    if armed_at.as_ref().is_none_or(|(mark, _)| at.0 > *mark) {
                        armed_at = Some(at);
                    }
                }
                seen.insert(row.id);
            }
        }
        let state = call_step(&CallState::default(), &CallEvent::Start { device: String::new(), language: Some(language.clone()), fallback: false });
        log::info!("[voice] call {}: answered for agent {} ({language})", answered.call_id, answered.agent.id);
        self.call = Some(Call {
            call_id: answered.call_id,
            greeting_id: format!("{id}-greeting"),
            id,
            agent: answered.agent,
            link: answered.link,
            language,
            root: answered.root,
            state,
            started: now,
            ring: Duration::from_millis(call_ring_ms(call_ring_random())),
            rang: false,
            listening_at: None,
            heard: false,
            mic_checked: false,
            speaking: None,
            queued: 0,
            seen,
            armed_at: armed_at.map_or_else(|| "1970-01-01T00:00:00Z".to_string(), |(_, at)| at),
            polled: now,
        });
        self.last_used = now;
    }

    /// One flow step: the caller hears it, the machine follows; rang and
    /// ready answered = the greeting.
    fn step(&mut self, event: CallEvent) {
        let Some(call) = self.call.as_mut() else { return };
        if let Some(wire) = call_wire_event(&event) {
            call.link.send_event(wire);
        }
        let before = call.state.phase;
        call.state = call_step(&call.state, &event);
        call.state.toast = None;
        if before == CallPhase::Ringing && call.state.phase == CallPhase::Speaking {
            let greeting = call_greeting(Some(&call.language));
            if self.voices.as_ref().is_some_and(|voices| voices.is_empty()) {
                // Nothing speaks it: listen at once.
                self.step(CallEvent::ReplyFinished);
                return;
            }
            call.speaking = Some(call.greeting_id.clone());
            self.speech.speak(&call.greeting_id, &greeting);
        }
    }

    /// End the call on the worker (and the caller when `tell`), then step.
    fn close(&mut self, tell: bool, event: CallEvent, now: Instant) {
        let Some(call) = self.call.as_ref() else { return };
        self.speech.hangup(&call.id);
        if call.speaking.is_some() {
            self.speech.stop();
        }
        if tell {
            call.link.end();
        }
        self.step(event);
        log::info!("[voice] call {}: ended", call_id(&self.call));
        self.call = None;
        self.last_used = now;
    }

    /// Fail the call with `code` (the caller's toast), then hang up.
    fn fail(&mut self, code: &str, now: Instant) {
        let Some(call) = self.call.as_ref() else { return };
        log::warn!("[voice] call {}: failed {code}", call.call_id);
        call.link.send_event(serde_json::json!({ "t": "failed", "code": code }));
        self.close(true, CallEvent::Failed(code.to_string()), now);
    }

    fn on_link(&mut self, call_id: &str, event: steer::call_host::CallHostEvent, now: Instant) {
        use steer::call_host::CallHostEvent;
        let Some(id) = self.call.as_ref().filter(|call| call.call_id == call_id).map(|call| call.id.clone()) else { return };
        match event {
            CallHostEvent::Ready => log::info!("[voice] call {call_id}: the caller is on the line"),
            CallHostEvent::Audio(pcm) => self.speech.call_audio(&id, &pcm),
            CallHostEvent::Mute(muted) => {
                self.speech.call_mute(&id, muted);
                self.step(if muted { CallEvent::Mute } else { CallEvent::Unmute });
            }
            CallHostEvent::Ended { reason } => self.close(false, CallEvent::HungUp(reason), now),
            CallHostEvent::Dropped(reason) => {
                log::warn!("[voice] call {call_id}: the stream dropped: {reason}");
                self.fail("dropped", now);
            }
        }
    }

    fn on_worker(&mut self, event: etalk::Event, now: Instant) {
        let before = self.speech_state();
        let current = self.call.as_ref().map(|call| call.id.clone());
        let call_owns = |id: &str| current.as_deref() == Some(id);
        match event {
            etalk::Event::Downloading { .. } => self.download = Download::Running,
            // Still not spoken for: `Voices` settles it (no flap to no row).
            etalk::Event::ModelReady => {}
            etalk::Event::Voices { voices } => {
                // A failed download settles as no voices: it is not "no GPU".
                let failed = self.download == Download::Failed && voices.is_empty();
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
                if let Some(call) = self.call.as_mut() {
                    call.listening_at = Some(now);
                }
                self.step(CallEvent::Ready);
            }
            etalk::Event::Level { id, level, .. } if call_owns(&id) => {
                if let Some(call) = self.call.as_mut() {
                    if call.listening_at.is_some() && level >= VOICE_CALL.constants.no_sound_level {
                        call.heard = true;
                    }
                }
            }
            etalk::Event::Partial { id, text } if call_owns(&id) => self.step(CallEvent::Partial(text)),
            etalk::Event::SpeechStarted { id } if call_owns(&id) => {
                if let Some(call) = self.call.as_ref() {
                    call.link.flush();
                }
                self.step(CallEvent::SpeechStarted);
            }
            etalk::Event::Said { id, text } if call_owns(&id) => self.said(text),
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
                self.reply_done(&id);
            }
            etalk::Event::Failed { id, error } => {
                log::warn!("[voice] not spoken: {error}");
                self.reply_done(&id);
            }
            etalk::Event::ListenFailed { reason: etalk::ListenFailure::Downloading, .. } => {}
            etalk::Event::ListenFailed { id, reason, detail } if call_owns(&id) => {
                log::warn!("[voice] the call could not listen: {detail}");
                let code = match reason {
                    etalk::ListenFailure::Unavailable => "unavailable",
                    _ => "dropped",
                };
                self.fail(code, now);
            }
            etalk::Event::Exited => {
                self.up = false;
                if self.call.is_some() {
                    log::warn!("[voice] the speech worker exited during a call");
                    self.fail("dropped", now);
                }
                if self.download == Download::Running {
                    self.download = Download::Idle;
                }
                self.crashes += 1;
                if self.crashes >= MAX_CRASHES && self.download != Download::Unavailable {
                    log::warn!("[voice] the speech worker crashed {} times in a row; giving up until a restart", self.crashes);
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

    /// A reply (or the greeting) ended: the call listens again.
    fn reply_done(&mut self, id: &str) {
        let Some(call) = self.call.as_mut() else { return };
        if call.speaking.as_deref() != Some(id) {
            return;
        }
        call.speaking = None;
        if call.state.phase == CallPhase::Speaking {
            self.step(CallEvent::ReplyFinished);
        }
    }

    /// A finished utterance: a plain message to the agent.
    fn said(&mut self, text: String) {
        let Some(call) = self.call.as_mut() else { return };
        if text.trim().is_empty() {
            return;
        }
        // Words during the ring are not the call's (EXP-1857).
        if call.state.phase == CallPhase::Ringing {
            log::info!("[voice] call {}: an utterance during the ring was dropped", call.call_id);
            return;
        }
        let busy = matches!(call.state.phase, CallPhase::Thinking | CallPhase::Speaking);
        if busy {
            call.queued += 1;
        }
        let (call_id, agent, root, queued) = (call.call_id.clone(), call.agent.clone(), call.root.clone(), call.queued);
        self.step(if busy { CallEvent::Queued { count: queued, text: text.clone() } } else { CallEvent::Said(text.clone()) });
        self.net.send(call_id, agent, root, text);
    }

    /// The loop's tick: the ring, the give-up, the mic check, the reply
    /// watch and the idle exit.
    pub fn tick(&mut self, now: Instant) {
        if self.call.is_none() && !self.up && self.hosting() && self.download != Download::Unavailable && self.retry_at.is_some_and(|at| now >= at) {
            self.retry_at = None;
            self.download = Download::Idle;
            self.probe();
        }
        if self.call.is_none() {
            if self.up && self.voices.is_some() && self.download != Download::Running && now.duration_since(self.last_used) >= self.idle_after {
                log::info!("[voice] idle for {} s: the speech worker exits", self.idle_after.as_secs());
                self.speech.shutdown();
                self.up = false;
            }
            return;
        }
        let Some(call) = self.call.as_ref() else { return };
        let elapsed = now.duration_since(call.started);
        if !call.rang && elapsed >= call.ring {
            if let Some(call) = self.call.as_mut() {
                call.rang = true;
            }
            self.step(CallEvent::Rang);
        }
        let Some(call) = self.call.as_ref() else { return };
        if call.state.phase == CallPhase::Ringing && !call.state.ready && elapsed >= Duration::from_millis(VOICE_CALL.constants.max_ring_ms) {
            log::warn!("[voice] call {}: the voice was not ready after the longest ring", call.call_id);
            return self.fail("unavailable", now);
        }
        if let Some(listening) = call.listening_at.filter(|_| !call.mic_checked) {
            let until = (call.started + call.ring).max(listening + MIC_CHECK_MIN);
            if now >= until {
                let silent = !call.heard;
                if let Some(call) = self.call.as_mut() {
                    call.mic_checked = true;
                }
                if silent {
                    self.step(CallEvent::NoSound);
                }
            }
        }
        if self.call.as_ref().is_some_and(|call| now.duration_since(call.polled) >= REPLY_POLL) {
            self.watch_replies(now);
        }
        self.last_used = now;
    }

    /// The agent's newest new reply in the call's conversation is spoken.
    fn watch_replies(&mut self, now: Instant) {
        let Some(call) = self.call.as_mut() else { return };
        call.polled = now;
        let Some(root) = call.root.clone() else { return };
        let rows = self.net.messages(&root);
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
        let Some(row) = fresh
            .iter()
            .filter(|row| etalk::voice::should_speak(&message(row), &context))
            .max_by_key(|row| row.created_at.as_deref().and_then(etalk::voice::parse_instant).unwrap_or(0))
        else {
            return;
        };
        let spoken = etalk::voice::spoken_text(row.body.as_deref().unwrap_or(""), VOICE.max_chars, &call.language);
        if spoken.is_empty() {
            return;
        }
        let id = row.id.clone();
        self.speech.speak(&id, &spoken);
        call.speaking = Some(id);
        // The agent took the queued lines in with this turn.
        call.queued = 0;
        self.step(CallEvent::ReplyStarted(etalk::voice::first_sentence(&spoken).to_string()));
    }
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

fn call_id(call: &Option<Call>) -> &str {
    call.as_ref().map_or("", |call| call.call_id.as_str())
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
    /// The synced message rows of a session (opened).
    pub messages: Arc<dyn Fn(&str) -> Vec<domain::rows::MessageRow> + Send + Sync>,
    /// Raised when the `speech` row changed (the next beat carries it).
    pub beat_soon: Arc<AtomicBool>,
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
    /// The conversation's key per root (sealing a send).
    keys: Arc<Mutex<std::collections::HashMap<String, (u32, [u8; 32])>>>,
}

impl Net for DaemonNet {
    fn answer(&self, call_id: String, agent_id: String) {
        let (deps, tx, keys) = (Arc::clone(&self.deps), self.tx.clone(), Arc::clone(&self.keys));
        let _ = std::thread::Builder::new().name("voice-answer".into()).spawn(move || {
            match answer(&deps, &tx, &keys, &call_id, &agent_id) {
                Ok(answered) => {
                    let _ = tx.send(Input::Answered(answered));
                }
                Err(err) => {
                    log::warn!("[voice] call {call_id}: {err}");
                    let _ = tx.send(Input::Unanswered { call_id });
                }
            }
        });
    }

    fn send(&self, call_id: String, agent: HostedAgent, root: Option<String>, text: String) {
        let (deps, tx, keys) = (Arc::clone(&self.deps), self.tx.clone(), Arc::clone(&self.keys));
        let _ = std::thread::Builder::new().name("voice-send".into()).spawn(move || {
            let key = root.as_ref().and_then(|root| keys.lock().ok()?.get(root).copied());
            let body = match key {
                Some((epoch, public)) => enet_core::sealed::seal_body(&public, epoch, &text).unwrap_or_else(|err| {
                    log::warn!("[voice] the utterance goes out plaintext, sealing failed: {err}");
                    text.clone()
                }),
                None => text.clone(),
            };
            let input = match root.as_deref() {
                Some(root) => api::sessions::SendInput::message(root, &body, Vec::new()),
                None => api::sessions::SendInput {
                    team_id: Some(agent.team_id.clone()),
                    agent_id: Some(agent.id.clone()),
                    text: Some(body),
                    ..Default::default()
                },
            };
            match api::sessions::send(&deps.trpc, &input) {
                Ok(out) => {
                    let _ = tx.send(Input::Sent { call_id, session_id: out.session_id });
                }
                // The call goes on; the words are in the log, never lost silently.
                Err(err) => log::warn!("[voice] call {call_id}: the send failed: {err}"),
            }
        });
    }

    fn messages(&self, root: &str) -> Vec<domain::rows::MessageRow> {
        (self.deps.messages)(root)
    }
}

/// The ring's network half: the agent (it must run HERE), its voice and
/// clip, its conversation and key, the host ticket and the socket.
fn answer(
    deps: &Arc<Deps>,
    tx: &flume::Sender<Input>,
    keys: &Mutex<std::collections::HashMap<String, (u32, [u8; 32])>>,
    call_id: &str,
    agent_id: &str,
) -> Result<Answered, String> {
    let agent = api::agents::get(&deps.trpc, agent_id).map_err(|err| format!("the agent could not be read: {err}"))?;
    if agent.device_id.as_deref() != Some(deps.device_id.as_str()) {
        return Err(format!("{agent_id} does not run here"));
    }
    let voice = agent_voice(deps, &agent);
    let target = api::sessions::send_target(&deps.trpc, Some(agent_id), None).map_err(|err| format!("the conversation could not be read: {err}"))?;
    if let (Some(root), Some(epoch), Some(public)) = (target.root_id.as_ref(), target.epoch, target.public_key.as_deref()) {
        if let (Ok(public), Ok(mut keys)) = (enet_core::sealed::public_key_from_b64(public), keys.lock()) {
            keys.insert(root.clone(), (epoch, public));
        }
    }
    let ticket = api::voice::voice_call_host_ticket(&deps.trpc, call_id, Some(&deps.device_id)).map_err(|err| format!("the call host ticket was refused: {err}"))?;
    let events = tx.clone();
    let id = call_id.to_string();
    let link = steer::call_host::connect(&deps.runtime, ticket.url, move |event| {
        let _ = events.send(Input::Link { call_id: id.clone(), event });
    });
    Ok(Answered {
        call_id: call_id.to_string(),
        agent: HostedAgent { id: agent.id.clone(), team_id: agent.team_id.clone() },
        link: Box::new(link),
        voice,
        root: target.root_id,
    })
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
/// built-in voice speaks).
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
        .get_bytes(&format!("/api/attachments/{clip_id}"))
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

/// The agents of the synced teams whose Runs on is this device.
fn hosted_agents(deps: &Deps) -> Option<Vec<HostedAgent>> {
    let mut hosted = Vec::new();
    for team_id in (deps.teams)() {
        let agents = api::agents::list(&deps.trpc, &team_id).ok()?;
        hosted.extend(hosted_of(&agents, &deps.device_id));
    }
    hosted.sort_by(|a, b| a.id.cmp(&b.id));
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
    let net = DaemonNet { deps: Arc::clone(&deps), tx: tx.clone(), keys: Arc::default() };
    let speech = Arc::new(Mutex::new(None));
    let handle = VoiceHost { tx: tx.clone(), speech: Arc::clone(&speech) };
    let agents_tx = tx.clone();
    let agents_deps = Arc::clone(&deps);
    let stopped = Arc::new(AtomicBool::new(false));
    let agents_stopped = Arc::clone(&stopped);
    // The agents that run here, read off the loop (a request per team).
    let _ = std::thread::Builder::new().name("voice-agents".into()).spawn(move || {
        while !agents_stopped.load(Ordering::SeqCst) {
            if let Some(agents) = hosted_agents(&agents_deps) {
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
        rows: Arc<Mutex<Vec<domain::rows::MessageRow>>>,
    }

    impl Net for FakeNet {
        fn answer(&self, call_id: String, agent_id: String) {
            self.log.push(format!("answer {call_id} {agent_id}"));
        }
        fn send(&self, call_id: String, _agent: HostedAgent, root: Option<String>, text: String) {
            self.log.push(format!("send {call_id} {} {text}", root.unwrap_or_default()));
        }
        fn messages(&self, _root: &str) -> Vec<domain::rows::MessageRow> {
            self.rows.lock().unwrap().clone()
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
        now: Instant,
    }

    impl Rig {
        fn new() -> Self {
            let (speech, net, link) = (Log::default(), Log::default(), Log::default());
            let rows = Arc::new(Mutex::new(Vec::new()));
            let now = Instant::now();
            let host = Host::new(Box::new(FakeSpeech(speech.clone())), Box::new(FakeNet { log: net.clone(), rows: Arc::clone(&rows) }), now);
            Rig { host, speech, net, link, rows, now }
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
            }));
        }
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
        let mut rig = Rig::ready();
        for _ in 0..MAX_CRASHES {
            rig.handle(Input::Worker(etalk::Event::Exited));
        }
        assert_eq!(rig.host.speech_state(), Some(SpeechState::Failed));
    }

    /// A ring: the agent's voice is prepared, the call listens, rang +
    /// ready answer it with the greeting in the agent's language; an
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
        // The ring's longest earliest answer: rang, then the greeting.
        rig.advance(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        assert_eq!(rig.link.take(), [r#"event {"t":"rang"}"#], "the caller was heard: no noSound");
        assert_eq!(rig.speech.take(), ["speak call-1-greeting Hallo?"]);
        rig.handle(Input::Worker(etalk::Event::SpeechAudio { id: "call-1-greeting".into(), pcm: etalk::encode_pcm(&[1, 2, 3]), rate: 24_000 }));
        assert_eq!(rig.link.take(), ["speech 3 24000"]);
        rig.handle(Input::Worker(etalk::Event::Finished { id: "call-1-greeting".into(), stopped: false }));
        assert_eq!(rig.link.take(), [r#"event {"t":"replyFinished"}"#]);
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
        rig.handle(Input::Worker(etalk::Event::Exited));
        let events = rig.link.take();
        assert_eq!(events.first().map(String::as_str), Some(r#"event {"t":"failed","code":"dropped"}"#), "{events:?}");
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
}
