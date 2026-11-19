//! NEW-55 (voice V4): CALLER MODE on the voice device's IDE
//! (`fixtures/voice-call.json`; the machine is `etalk::call`).
//!
//! A call = a conversation with ONE agent by voice. It starts from the
//! telephone in the agent chat's header (`agent_screen`), from the tray's
//! `Call {agent}`, or, streamed from a phone or web, from the relay's
//! `call_ring` on this device's control socket (the CALL HOST: the host's
//! [`CallLink`], Exponential's `steer::call_host`). The speech worker listens without a key (`Call`):
//! every closed segment comes back as `Said` and goes out as a plain
//! message through `sessions.send`, exactly like push to talk; an utterance
//! said while the agent's turn runs rides the queue bar like a typed one.
//! Every reply in that conversation is spoken (`speak_replies` speaks the
//! call's conversation while a call runs), speech during a reply stops it
//! in the worker (barge-in). A streamed call plays nothing here: the
//! worker's reply audio goes to the caller, and every flow event follows it
//! as `call_event`.
//!
//! S-3: a call RINGS first: `Start` = ringing; the worker warms up (`Call`
//! with `ring_ms`, a local call's ring tone on its one output stream until
//! the opening) and confirms with `CallListening` (= `Ready`); EXP-1423:
//! the ring's earliest answer, drawn per call (`etalk::call::call_ring_ms`), =
//! `Rang`; both = answered: it listens (#EXP-1966: no fixed greeting); the
//! agent's own opening line goes through the worker's `Speak`
//! ([`speak_replies::speak_greeting`]). No `CallListening` within `maxRingMs`
//! (or a failed download) fails it `unavailable`; a voice still downloading
//! keeps ringing. MIC CHECK: no `Level` ≥ `noSoundLevel` from
//! `CallListening` until the ring's end (at least 1.5 s of listening) steps
//! `NoSound`. FEED-276: `Ready` also waits for the agent's own voice
//! (`hostVoice`: its clip on disk and prepared; a clip still fetching after
//! `hostVoice.waitMs`, or one that failed, speaks the built-in voice,
//! logged), so the opening is never said in the stock voice. The surfaces: the in-window CALL PANEL over the agent chat's
//! body ([`CallPanel`]) and, while no IDE window is focused, the FLOATING
//! PILL (`push_to_talk::overlay`, [`Face::Call`]); both read [`face`]. The
//! live level and the timer repaint only them ([`CallLive`]).
//!
//! EXP-1857 (voice per agent b): A CALL RINGS THE AGENT'S DEVICE. The
//! telephone reads `etalk::call::call_target` ([`target`]): the agent's Runs
//! on desktop when it is one of the member's own and it speaks, else this
//! desktop, else the member's newest desktop that speaks. A LOCAL target
//! runs the call here as above; ANOTHER desktop is DIALLED (caller mode,
//! the web's and the phones' wire): `voice.startCall` → the relay's caller
//! socket (the host's [`DialLink`], Exponential's `steer::call_caller`); the
//! worker's `Dial` opens the microphone through its echo canceller (the
//! played downlink is the reference) and rings on the call's one output
//! stream until the host's `rang` + `ready` answer; the host's `call_event`s
//! step this machine, its reply plays through the worker (`DialPlay`). The
//! caller sees `Voice on {device}` (`view.via`) while the call falls back.
//! Every member desktop that speaks answers a ring (`on_ring`), not only
//! the voice device; a meeting refuses it only where it runs.
//!
//! [`speak_replies::speak_greeting`]: crate::speak_replies::speak_greeting

use std::time::{Duration, Instant};

use gpui::{
    div, px, App, AppContext as _, Context, Entity, FocusHandle, Focusable, Global, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window,
};

use etalk::call::{
    call_voice_ready, CallVoice, CallVoiceReady, call_event_of_wire, call_ring_ms, call_ring_random, call_speech_of, call_step, call_target, call_timer, call_view, call_wire_event as wire_event, CallEvent,
    CallPhase, CallPlatform, CallSpeech, CallState, CallTarget, CallTargetDevice, CallTargetInput, VOICE_CALL,
};

use crate::host::{self, CallLink, CallLinkEvent, DialError, DialLink, DialLinkEvent, Notice, TalkDesktop, DIAL_DOWN_RATE};
use crate::push_to_talk::overlay::{Face, Overlay};
use crate::speak_replies::SpeechState;
use crate::voice_overlay::{CallActions, CallFace, CallMark};

/// The mic check listens at least this long after `CallListening`.
const MIC_CHECK_MIN: Duration = Duration::from_millis(1500);
/// How often a running call looks at the windows (the pill shows while
/// none is focused) and the timer's second.
const TICK: Duration = Duration::from_millis(250);

/// The call running now.
struct Active {
    /// The worker's call id.
    id: String,
    agent_id: String,
    /// A streamed call's link (None = this device's own microphone).
    host: Option<Box<dyn CallLink>>,
    /// EXP-1857: CALLER MODE: the call's speech runs on another desktop
    /// (the worker's `Dial` holds the microphone and the output here).
    dialled: bool,
    /// EXP-1857: the dialled desktop's link, once connected.
    dial: Option<Box<dyn DialLink>>,
    /// Utterances said while the agent's turn ran (the queue bar's share).
    queued: usize,
    /// The reply being spoken (its message id, or the greeting's).
    speaking: Option<String>,
    /// S-3: the greeting's `Speak` id.
    greeting_id: String,
    started: Instant,
    /// EXP-1423: this call's earliest answer (`Rang`), drawn at its start.
    ring: Duration,
    /// S-3: when the worker confirmed it listens.
    listening_at: Option<Instant>,
    /// S-3: a `Level` reached `noSoundLevel` since `CallListening`.
    heard: bool,
    /// S-3: when rang + ready answered it (the timer counts from here).
    answered_at: Option<Instant>,
    /// FEED-276: the agent's clip landed during the ring and its Prepare
    /// went out; the worker's `Voices` confirms it.
    voice_preparing: bool,
    /// FEED-276: `hostVoice.waitMs` passed since the ring started.
    voice_waited: bool,
    /// #EXP-1961: the call's own session (a child of the chat), once the
    /// server answered; `session_pending` = still asking.
    session: Option<String>,
    session_pending: bool,
    /// #EXP-1961: the voice is in; `ready` waits for the opening line.
    voice_ready_at: Option<Instant>,
    /// #EXP-1961: the agent's opening line (its row id, its text) and when.
    opening: Option<(Instant, String, String)>,
    /// #EXP-1961: the pickup was silent: a late opening is never spoken.
    late_opening: bool,
    /// #EXP-1961: an utterance went out (a later reply is no opening).
    sent: bool,
    /// #EXP-1961: `openingWaitMs` passed since the voice was in.
    opening_waited: bool,
    /// #EXP-1981: utterances said while the call's session was still asked
    /// (a pickup that could wait no longer): sent once it answered.
    held: Vec<String>,
}

/// S-3: what changes many times a second (the microphone's level, the
/// timer's second): only the call surfaces observe it.
#[derive(Default)]
pub struct CallLive {
    level: f32,
    bands: Vec<f32>,
    timer: String,
}

pub struct VoiceCall {
    state: CallState,
    active: Option<Active>,
    overlay: Overlay,
    live: Entity<CallLive>,
    /// The pill is open (no IDE window focused).
    pill: bool,
    counter: u64,
    /// #EXP-1941: a call id is answered once (the ids this process got a
    /// ticket for, for five minutes).
    answered: etalk::call::AnsweredCalls,
    /// The toasts raised (the lifecycle tests read them).
    #[cfg(test)]
    toasts: Vec<String>,
}

struct VoiceCallGlobal(Entity<VoiceCall>);

impl Global for VoiceCallGlobal {}

fn global(cx: &App) -> Option<Entity<VoiceCall>> {
    cx.try_global::<VoiceCallGlobal>().map(|global| global.0.clone())
}

/// Install ONCE, after `speak_replies::install`.
pub(crate) fn install(cx: &mut App) {
    let live = cx.new(|_| CallLive::default());
    let entity = cx.new(|cx: &mut Context<VoiceCall>| {
        if let Some(speaker) = crate::speak_replies::entity(cx) {
            cx.observe(&speaker, |this: &mut VoiceCall, _, cx| this.on_speaker(cx)).detach();
        }
        VoiceCall {
            state: CallState::default(),
            active: None,
            overlay: Overlay::default(),
            live,
            pill: false,
            counter: 0,
            answered: etalk::call::AnsweredCalls::default(),
            #[cfg(test)]
            toasts: Vec::new(),
        }
    });
    cx.set_global(VoiceCallGlobal(entity));
}

/// The controller (the tray observes it).
pub fn entity(cx: &App) -> Option<Entity<VoiceCall>> {
    global(cx)
}

/// A call runs on this device.
pub fn active(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).active.is_some())
}

/// The agent of the running call whose speech runs HERE (Speak replies
/// speaks its conversation; EXP-1857: a dialled call's runs elsewhere).
/// #EXP-1936: none while it still rings: replies are the call's from the
/// greeting on (a Speak during the ring would end the ring tone).
pub fn agent_id(cx: &App) -> Option<String> {
    let entity = global(cx)?;
    let this = entity.read(cx);
    let active = this.active.as_ref().filter(|active| !active.dialled && this.state.phase != CallPhase::Ringing)?;
    Some(active.agent_id.clone())
}

/// #EXP-1961: the running call's own session, once it no longer rings
/// (Speak replies speaks ITS replies; None = the chat's).
pub fn session(cx: &App) -> Option<String> {
    let entity = global(cx)?;
    let this = entity.read(cx);
    let active = this.active.as_ref().filter(|active| !active.dialled && this.state.phase != CallPhase::Ringing)?;
    active.session.clone()
}

/// #EXP-1981: the running call's own session is still being asked: Speak
/// replies speaks nothing of the chat meanwhile (none of its rows are the call's).
pub(crate) fn session_pending(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).active.as_ref().is_some_and(|active| !active.dialled && active.session_pending))
}

/// #EXP-1961: a ringing call's own session still waiting for its opening
/// line, and its agent: Speak replies hands that line over ([`opening`]).
pub(crate) fn ringing_session(cx: &App) -> Option<(String, String)> {
    let entity = global(cx)?;
    let this = entity.read(cx);
    let active = this
        .active
        .as_ref()
        .filter(|active| !active.dialled && this.state.phase == CallPhase::Ringing && active.opening.is_none())?;
    Some((active.session.clone()?, active.agent_id.clone()))
}

/// #EXP-1961: the opening line arrived during the ring (said at the pickup).
pub(crate) fn opening(id: String, text: String, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        if let Some(active) = this.active.as_mut().filter(|active| active.opening.is_none()) {
            active.opening = Some((Instant::now(), id, text));
        }
        this.check_opening(cx);
    });
}

/// #EXP-1961: a reply after a silent pickup, before anything was said, is
/// the late opening: not spoken (true = drop it).
pub(crate) fn drops_late_opening(cx: &mut App) -> bool {
    let Some(entity) = global(cx) else { return false };
    entity.update(cx, |this, _| {
        let Some(active) = this.active.as_mut() else { return false };
        std::mem::take(&mut active.late_opening) && !active.sent
    })
}

/// S-3: THIS client runs a call with `agent_id` (its telephone shows the
/// panel instead of dialing).
pub fn runs_for(agent_id: &str, cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).active.as_ref().is_some_and(|active| active.agent_id == agent_id))
}

/// S-3: what the call surfaces show now (None = no call).
pub fn face(cx: &App) -> Option<CallFace> {
    global(cx)?.read(cx).face_now(cx)
}

impl VoiceCall {
    /// [`face`] from inside the controller (the pill is handed it).
    fn face_now(&self, cx: &App) -> Option<CallFace> {
        let active = self.active.as_ref()?;
        let view = call_view(&self.state, CallPlatform::Desktop)?;
        let agent = host::agent(&active.agent_id, cx);
        let live = self.live.read(cx);
        let (level, bands) = match view.activity {
            "listening" => (live.level, live.bands.clone()),
            // The reply's own loudness is not reported: a steady wave.
            "speaking" => (0.5, Vec::new()),
            _ => (0., Vec::new()),
        };
        Some(CallFace {
            agent: SharedString::from(agent.as_ref().map(|agent| agent.title.clone()).unwrap_or_default()),
            mark: CallMark {
                icon: agent.as_ref().and_then(|agent| agent.icon.clone()),
                color: agent.as_ref().and_then(|agent| agent.color.clone()),
            },
            view,
            timer: SharedString::from(answered_timer(active.answered_at)),
            level,
            bands,
        })
    }

    /// S-3: the open pill gets the face now (a step, a level, a second).
    fn push_pill(&mut self, cx: &mut App) {
        if !self.pill {
            return;
        }
        if let Some(face) = self.face_now(cx) {
            self.overlay.show(Some(Face::Call(Box::new(face))), cx);
        }
    }
}

fn answered_timer(answered_at: Option<Instant>) -> String {
    call_timer(answered_at.map_or(0, |at| at.elapsed().as_millis() as u64))
}

/// THIS machine's name as the member's devices know it (its id until synced).
fn own_name(cx: &App) -> String {
    let Some(host) = host::host(cx) else { return String::new() };
    let own = host.device_id();
    host.device(&own, cx).and_then(|row| row.label).unwrap_or(own)
}

/// EXP-1857: the member's own desktops as `callTarget` reads them, from
/// THIS client: `here` is online by definition with its own `speech`
/// (None = no row), and a list that does not hold it yet (not synced) gets
/// it first; `meeting_on` = the voice device while a meeting runs.
pub(crate) fn target_desktops(
    here: Option<(&str, &str)>,
    here_speech: Option<CallSpeech>,
    desktops: &[TalkDesktop],
    meeting_on: Option<&str>,
) -> Vec<CallTargetDevice> {
    let mut out: Vec<CallTargetDevice> = desktops
        .iter()
        .map(|desktop| {
            let own = here.is_some_and(|(id, _)| id == desktop.id);
            CallTargetDevice {
                id: desktop.id.clone(),
                name: desktop.name.clone(),
                online: own || desktop.online,
                speech: if own { here_speech } else { call_speech_of(desktop.speech.as_deref()) },
                meeting: meeting_on == Some(desktop.id.as_str()),
            }
        })
        .collect();
    if let Some((id, name)) = here.filter(|(id, _)| !out.iter().any(|desktop| desktop.id == *id)) {
        out.insert(
            0,
            CallTargetDevice { id: id.to_string(), name: name.to_string(), online: true, speech: here_speech, meeting: meeting_on == Some(id) },
        );
    }
    out
}

/// EXP-1857 (voice-call.json `callTarget`): where a call with `agent_id`
/// runs, from this desktop: the agent's Runs on, this machine, the
/// member's other desktops; `busy` = a call already runs on this client.
pub fn target(agent_id: &str, cx: &App) -> CallTarget {
    let settings = crate::speak_replies::settings(cx);
    let here = host::device_id(cx);
    let name = own_name(cx);
    let desktops = host::host(cx).map(|host| host.desktops(cx)).unwrap_or_default();
    // This machine's own `speech` row through the one mapper.
    let speech = call_speech_of(crate::speak_replies::speech_state(cx).map(SpeechState::doctor_state));
    // #NEW-57: a meeting runs on the voice device only.
    let meeting_on = settings.voice_device_id.as_deref().filter(|_| settings.meeting());
    let desktops = target_desktops(here.as_deref().map(|id| (id, name.as_str())), speech, &desktops, meeting_on);
    // EXP-1858: the member's own servers (only the agent's counts).
    let servers = host::host(cx).map(|host| host.servers(cx)).unwrap_or_default();
    let servers = target_desktops(None, None, &servers, None);
    let agent_device = host::agent(agent_id, cx).and_then(|agent| agent.device_id);
    call_target(&CallTargetInput {
        here: here.as_deref(),
        agent_device: agent_device.as_deref(),
        desktops: &desktops,
        servers: &servers,
        busy: active(cx),
    })
}

/// None = the telephone for `agent_id` is enabled; Some = disabled, with
/// the reason (EXP-1857: [`target`]'s).
pub fn unavailable(agent_id: &str, cx: &App) -> Option<String> {
    target(agent_id, cx).reason
}

/// The telephone (header) or the tray's Call: call `agent_id`, here or, on
/// another desktop, dialled.
pub fn start(agent_id: String, cx: &mut App) {
    let target = target(&agent_id, cx);
    if let Some(reason) = target.reason {
        host::notify(Notice::Error(reason), cx);
        return;
    }
    let Some(entity) = global(cx) else { return };
    let device = target.name.unwrap_or_default();
    if target.local {
        entity.update(cx, |this, cx| this.begin(agent_id, None, None, device, target.fallback, cx));
    } else {
        entity.update(cx, |this, cx| this.dial(agent_id, device, target.fallback, cx));
    }
}

/// The surfaces' mute button.
pub fn toggle_mute(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| {
            let muted = !this.state.muted;
            this.set_muted(muted, cx);
        });
    }
}

/// End call / Cancel (the surfaces' red button, Esc, the tray, sign-out,
/// a settings change).
pub fn end(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.hang_up(true, cx));
    }
}

/// What a ring (`call_ring`) gets on this desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RingAnswer {
    Answer,
    /// #NEW-57: a meeting runs HERE: joined only to fail it `meeting`.
    RefuseMeeting,
    /// #EXP-1936: a call the relay does not know runs here: joined only to
    /// fail it `busy`.
    RefuseBusy,
    /// Unable to speak: no answer (the relay tells the caller `no_answer`).
    Ignore,
}

/// EXP-1857: ANY member desktop that speaks answers (`ready` or still
/// downloading: the call rings until it is ready), whether or not it is the
/// voice device; a meeting refuses only where it runs.
pub(crate) fn ring_answer(meeting_here: bool, busy: bool, speech: Option<SpeechState>) -> RingAnswer {
    if busy {
        return RingAnswer::RefuseBusy;
    }
    if meeting_here {
        return RingAnswer::RefuseMeeting;
    }
    if !matches!(speech, Some(SpeechState::Ready | SpeechState::Downloading)) {
        return RingAnswer::Ignore;
    }
    RingAnswer::Answer
}

/// The relay rang this device (`call_ring` on the control socket): a phone,
/// web or another desktop streams a call to `agent_id`.
pub fn on_ring(call_id: String, agent_id: String, cx: &mut App) {
    // #EXP-1941: a vouched call id is answered once. An honest relay never
    // rings an id twice and a redial mints a new one.
    if global(cx).is_some_and(|entity| entity.read(cx).answered.answered(&call_id, process_ms())) {
        log::warn!("[voice] call {call_id}: this call id was answered before; the ring is dropped");
        return;
    }
    let settings = crate::speak_replies::settings(cx);
    let here = host::device_id(cx);
    let meeting_here = settings.meeting() && here.is_some() && settings.voice_device_id == here;
    match ring_answer(meeting_here, active(cx), crate::speak_replies::speech_state(cx)) {
        RingAnswer::RefuseMeeting => return refuse(call_id, agent_id, "meeting", cx),
        RingAnswer::RefuseBusy => return refuse(call_id, agent_id, "busy", cx),
        RingAnswer::Ignore => {
            log::info!("[voice] call {call_id}: not answered here");
            return;
        }
        RingAnswer::Answer => {}
    }
    let Some(entity) = global(cx) else { return };
    let Some(connecting) = host::host(cx).and_then(|host| host.connect_call(&call_id, &agent_id, false, cx)) else { return };
    let weak = entity.downgrade();
    let device = own_name(cx);
    cx.spawn(async move |cx| {
        let (host, rx) = match connecting.await {
            Ok(connection) => {
                // It got its ticket: this call id is not answered again.
                let _ = weak.update(cx, |this, _| this.answered.note(&call_id, process_ms()));
                // #EXP-1982: it can store none of the call's words here.
                if let Some(code) = connection.refuse {
                    log::warn!("[voice] call {call_id}: refused ({code}): the chat is end-to-end encrypted and this desktop has no key holder");
                    fail_when_on_the_line(connection.link, connection.events, code, cx).await;
                    return;
                }
                (connection.link, connection.events)
            }
            Err(err) => {
                log::warn!("[voice] the call host ticket was refused: {err}");
                return;
            }
        };
        if !admitted(&rx).await {
            log::info!("[voice] call {call_id}: the ring ended before the relay admitted this device");
            host.end();
            return;
        }
        let started = weak.update(cx, |this, cx| {
            if this.active.is_some() {
                host.end();
                return false;
            }
            // #EXP-1981: the call session names the ring's call id. A sealed
            // call is here only once its first frame opened: its key source
            // adopted (or created) the chat before, so the server finds it.
            this.begin(agent_id, Some(call_id), Some(host), device, false, cx);
            true
        });
        if !matches!(started, Ok(true)) {
            return;
        }
        while let Ok(event) = rx.recv_async().await {
            if weak.update(cx, |this, cx| this.on_host(event, cx)).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// This process's clock for [`etalk::call::AnsweredCalls`].
fn process_ms() -> u64 {
    static BORN: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);
    BORN.elapsed().as_millis() as u64
}

/// EXP-1929: the call starts here only once the relay admitted this
/// device (`call_ready`); a ring cancelled before (the caller hung up, the
/// ring timed out: the relay answers `no_such_call`) ends without a call,
/// silently, as the fixture ends a cancelled ring.
async fn admitted(events: &flume::Receiver<CallLinkEvent>) -> bool {
    loop {
        match events.recv_async().await {
            Ok(CallLinkEvent::Ready) => return true,
            Ok(CallLinkEvent::Ended { .. } | CallLinkEvent::Dropped(_)) | Err(_) => return false,
            Ok(_) => {}
        }
    }
}

/// The `call_event` a host sends before hanging up on a call it failed
/// (voice-call.json `wire.control`): the caller steps it as `failed`.
fn host_failed_event(code: &str) -> serde_json::Value {
    serde_json::json!({ "t": "failed", "code": code })
}

/// #NEW-57: join the call only to fail it as `code` (a `failed` flow event
/// the caller steps: `meeting`, #EXP-1936 `busy`), then hang up.
fn refuse(call_id: String, agent_id: String, code: &'static str, cx: &mut App) {
    log::info!("[voice] call {call_id}: refused ({code})");
    // #EXP-1941: refuse only: no key source. The clear `failed` ends a
    // sealing caller's call with its toast in every state.
    let Some(connecting) = host::host(cx).and_then(|host| host.connect_call(&call_id, &agent_id, true, cx)) else { return };
    cx.spawn(async move |cx| {
        let Ok(connection) = connecting.await else { return };
        fail_when_on_the_line(connection.link, connection.events, code, cx).await;
    })
    .detach();
}

/// A refused call: `failed {code}` once the caller is on the line (within
/// the relay's ring), then the hang-up.
async fn fail_when_on_the_line(host: Box<dyn CallLink>, rx: flume::Receiver<CallLinkEvent>, code: &str, cx: &mut gpui::AsyncApp) {
    let started = std::time::Instant::now();
    while started.elapsed() < ring_timeout() {
        match rx.try_recv() {
            Ok(CallLinkEvent::Ready) => {
                host.send_event(host_failed_event(code));
                break;
            }
            Ok(CallLinkEvent::Ended { .. } | CallLinkEvent::Dropped(_)) | Err(flume::TryRecvError::Disconnected) => break,
            Ok(_) | Err(flume::TryRecvError::Empty) => {
                cx.background_executor().timer(Duration::from_millis(50)).await;
            }
        }
    }
    host.end();
}

/// A speech-worker event that belongs to the call (routed here before
/// push to talk and Speak replies see it).
pub fn owns(event: &etalk::Event, cx: &App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let this = entity.read(cx);
    let Some(active) = this.active.as_ref() else { return false };
    match event {
        etalk::Event::Said { id, .. } | etalk::Event::SpeechStarted { id } | etalk::Event::CallListening { id } => *id == active.id,
        etalk::Event::Level { id, .. } | etalk::Event::Partial { id, .. } | etalk::Event::ListenFailed { id, .. } => *id == active.id,
        etalk::Event::SpeechAudio { .. } => active.host.is_some(),
        etalk::Event::DialAudio { id, .. } => *id == active.id,
        _ => false,
    }
}

/// FEED-276: the host's agents changed (before Speak replies follows them).
pub(crate) fn on_agents(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_agents(cx));
    }
}

/// FEED-276: the worker answered a Prepare (`Voices`).
pub(crate) fn voice_prepared(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_voice_prepared(cx));
    }
}

pub fn on_worker_event(event: etalk::Event, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_worker(event, cx));
    }
}

/// Settings changed: a call that can no longer run here ends (#NEW-57: a
/// meeting started on this desktop; a dialled call's microphone the worker
/// refuses itself).
pub fn settings_changed(cx: &mut App) {
    let settings = crate::speak_replies::settings(cx);
    let here = host::device_id(cx);
    let meeting_here = settings.meeting() && here.is_some() && settings.voice_device_id == here;
    if meeting_here && active(cx) {
        end(cx);
    }
    // #EXP-1936: the worker is no longer wanted here (the Assistant is
    // gone) and was shut down, which sends no `Exited`: the call ends as it
    // does when the worker dies, a streamed caller told why.
    let unwanted = here.is_some_and(|here| !crate::speak_replies::voice_here(&settings, &here).active());
    if unwanted && active(cx) {
        if let Some(entity) = global(cx) {
            entity.update(cx, |this, cx| this.fail("dropped", cx));
        }
    }
}

/// S-3: an IDE window of this device is focused (the pill hides then). A
/// window busy in an update right now is the one acting: focused.
fn ide_focused(overlay: &Overlay, cx: &mut App) -> bool {
    cx.windows()
        .into_iter()
        .filter(|window| !overlay.owns(window.window_id()))
        .any(|window| window.update(cx, |_, window, _| window.is_window_active()).unwrap_or(true))
}

impl VoiceCall {
    fn step(&mut self, event: CallEvent, cx: &mut Context<Self>) {
        if let (Some(event), Some(host)) = (wire_event(&event), self.active.as_ref().and_then(|active| active.host.as_ref())) {
            host.send_event(event);
        }
        let before = self.state.clone();
        self.state = call_step(&self.state, &event);
        // EXP-1857: a dialled call's ring stops once the host answered.
        if before.phase == CallPhase::Ringing && self.state.phase != CallPhase::Ringing && self.dialled() {
            if let Some(client) = crate::speak_replies::client(cx) {
                client.stop();
            }
        }
        // The telephones (busy) and the tray's Call follow a call's start
        // and end.
        if (before.phase == CallPhase::Hidden) != (self.state.phase == CallPhase::Hidden) {
            cx.refresh_windows();
        }
        if let Some(toast) = self.state.toast.take() {
            #[cfg(test)]
            self.toasts.push(toast.clone());
            host::notify(Notice::Error(toast), cx);
        }
        // S-3: rang and ready answered it (#EXP-1966: on Listening): the
        // device says the agent's opening, or listens (EXP-1857: a dialled
        // call's host does; only the timer starts).
        if before.phase == CallPhase::Ringing && self.state.phase == CallPhase::Listening {
            if self.dialled() {
                if let Some(active) = self.active.as_mut() {
                    active.answered_at = Some(Instant::now());
                }
            } else {
                self.answer(cx);
            }
        }
        if self.state != before {
            self.sync_pill(cx);
            self.push_pill(cx);
            cx.notify();
        }
    }

    /// S-3: the pill shows while a call runs and no IDE window is focused.
    fn sync_pill(&mut self, cx: &mut Context<Self>) {
        let wanted = self.active.is_some() && self.state.phase != CallPhase::Hidden && !ide_focused(&self.overlay, cx);
        if wanted != self.pill {
            self.pill = wanted;
            if !wanted {
                self.overlay.show(None, cx);
            }
        }
    }

    /// A dialled call runs (caller mode).
    fn dialled(&self) -> bool {
        self.active.as_ref().is_some_and(|active| active.dialled)
    }

    /// A call whose speech runs HERE: local (`host` None) or streamed in
    /// (`call_id` = the ring's server call id; #EXP-1981: None for a local
    /// call, whose call session is asked without one).
    /// `device` = this desktop's name, `fallback` = not the agent's own.
    fn begin(&mut self, agent_id: String, call_id: Option<String>, host: Option<Box<dyn CallLink>>, device: String, fallback: bool, cx: &mut Context<Self>) {
        let Some(client) = crate::speak_replies::client(cx) else { return };
        // A held push to talk and anything being said stop first.
        crate::push_to_talk::cancel_for_call(cx);
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        // #EXP-1847: the call speaks with the agent's own voice (its language,
        // its clip), prepared before the greeting.
        let language = crate::speak_replies::voice_of(&agent_id, cx).language;
        crate::speak_replies::use_voice_of(&agent_id, cx);
        let id = self.open(agent_id.clone(), host, cx);
        let remote = self.active.as_ref().is_some_and(|active| active.host.is_some());
        // #EXP-1961: the call's own session, asked while it rings.
        if let Some(job) = host::call_session(&agent_id, call_id.as_deref(), cx) {
            if let Some(active) = self.active.as_mut() {
                active.session_pending = true;
            }
            let call = id.clone();
            cx.spawn(async move |this, cx| {
                let session = cx.background_executor().spawn(async move { job() }).await;
                let _ = this.update(cx, |this, cx| {
                    if !this.is_call(&call) {
                        // #EXP-1981: the call ended while it asked: its session hears the hang-up.
                        if let Some(session) = session.as_deref() {
                            log::info!("[voice] call {call}: its session {session} came after the hang-up");
                            host::call_ended(session, cx);
                        }
                        return;
                    }
                    let mut held = Vec::new();
                    let mut agent_id = String::new();
                    if let Some(active) = this.active.as_mut() {
                        active.session_pending = false;
                        if session.is_none() {
                            // No session: the chat takes the call (no late opening to drop).
                            active.late_opening = false;
                        }
                        active.session = session;
                        held = std::mem::take(&mut active.held);
                        agent_id = active.agent_id.clone();
                    }
                    // What was said meanwhile goes where its answer is spoken.
                    for text in held {
                        this.send(agent_id.clone(), text, cx);
                    }
                    this.check_opening(cx);
                });
            })
            .detach();
        }
        self.step(CallEvent::Start { device, language: Some(language.clone()), fallback }, cx);
        // S-3: a LOCAL call rings on this device while the worker warms up;
        // a remote one rings on the caller (the host's `rang` follows).
        // EXP-1423: the worker rings until the greeting, at most `maxRingMs`.
        let ring_ms = VOICE_CALL.constants.max_ring_ms;
        client.call(id.clone(), language, remote, if remote { 0 } else { ring_ms as u32 });
        self.arm_timers(id.clone(), cx);
        // FEED-276: a clip still on its way is waited for at most
        // `hostVoice.waitMs` (then the built-in voice speaks).
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(VOICE_CALL.host_voice.wait_ms)).await;
            let _ = this.update(cx, |this, cx| {
                if this.is_call(&id) {
                    if let Some(active) = this.active.as_mut() {
                        active.voice_waited = true;
                    }
                    this.check_voice(cx);
                }
            });
        })
        .detach();
    }

    /// EXP-1857: CALLER MODE: call `agent_id` on the desktop `device`
    /// (another of the member's), streamed over the relay like web and the
    /// phones; the worker's `Dial` is the microphone (echo-cancelled) and the
    /// speaker here, and rings until the host answers.
    fn dial(&mut self, agent_id: String, device: String, fallback: bool, cx: &mut Context<Self>) {
        let Some(client) = crate::speak_replies::client(cx) else { return };
        crate::push_to_talk::cancel_for_call(cx);
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        // The greeting's language is the agent's (the host speaks it).
        let language = crate::speak_replies::voice_of(&agent_id, cx).language;
        let id = self.open(agent_id.clone(), None, cx);
        if let Some(active) = self.active.as_mut() {
            active.dialled = true;
        }
        self.step(CallEvent::Start { device, language: Some(language), fallback }, cx);
        client.dial(id.clone(), VOICE_CALL.constants.max_ring_ms as u32);
        self.arm_timers(id.clone(), cx);
        let Some(connecting) = host::host(cx).and_then(|host| host.dial_call(&agent_id, cx)) else {
            log::warn!("[voice] call {id}: no relay to dial through");
            self.fail("dropped", cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let connection = match connecting.await {
                Ok(connection) => connection,
                Err(err) => {
                    let _ = this.update(cx, |this, cx| this.dial_failed(&id, err, cx));
                    return;
                }
            };
            let (link, rx) = (connection.link, connection.events);
            let kept = this.update(cx, |this, _| match this.active.as_mut().filter(|active| active.id == id) {
                Some(active) => {
                    active.dial = Some(link);
                    true
                }
                None => {
                    // Ended while it dialled.
                    link.end();
                    false
                }
            });
            if !matches!(kept, Ok(true)) {
                return;
            }
            while let Ok(event) = rx.recv_async().await {
                if this.update(cx, |this, cx| this.on_dial(&id, event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// EXP-1857: `voice.startCall` refused: its own words when they are an
    /// availability reason (the call ends with that toast), else `dropped`.
    fn dial_failed(&mut self, id: &str, err: DialError, cx: &mut Context<Self>) {
        if !self.is_call(id) {
            return;
        }
        match err {
            DialError::Refused(message) if is_reason(&message) => {
                log::info!("[voice] call {id}: refused: {message}");
                self.close_with(false, CallEvent::End, cx);
                host::notify(Notice::Error(message), cx);
            }
            DialError::Refused(message) | DialError::Failed(message) => {
                log::warn!("[voice] call {id}: the dial failed: {message}");
                self.fail("dropped", cx);
            }
        }
    }

    /// EXP-1857: what the dialled desktop (or the relay) sent.
    fn on_dial(&mut self, id: &str, event: DialLinkEvent, cx: &mut Context<Self>) {
        if !self.is_call(id) {
            return;
        }
        match event {
            DialLinkEvent::Ready => log::info!("[voice] call {id}: the dialled desktop is on the line"),
            DialLinkEvent::Event(value) => match call_event_of_wire(&value) {
                // The host failed the call (its `call_end` follows): its toast.
                Some(CallEvent::Failed(code)) => self.fail(&code, cx),
                // #EXP-1936: a host forwards flow events only (voice-call.json
                // `wire.control.call_event`). Its `end` / `hungUp` end the
                // call for real (a step alone would hide a call whose
                // microphone stays open); `start`, `mute`, `unmute` and
                // `speaker` are this side's own.
                Some(event @ (CallEvent::End | CallEvent::HungUp(_))) => self.close_with(true, event, cx),
                Some(event) if wire_event(&event).is_some() => {
                    // Barge-in: the reply still queued here stops.
                    if event == CallEvent::SpeechStarted {
                        if let Some(client) = crate::speak_replies::client(cx) {
                            client.dial_flush(id);
                        }
                    }
                    self.step(event, cx);
                }
                _ => {}
            },
            DialLinkEvent::Audio(pcm) => {
                if let Some(client) = crate::speak_replies::client(cx) {
                    client.dial_play(id, &pcm, DIAL_DOWN_RATE);
                }
            }
            // A reason-less hang-up is silent, the relay's `idle` /
            // `max_length` toast `dropped` (voice-call.json `hangUpRules`).
            DialLinkEvent::Ended { reason } => self.close_with(false, CallEvent::HungUp(reason), cx),
            DialLinkEvent::Failed(code) => self.fail(&code, cx),
            DialLinkEvent::Dropped(reason) => {
                log::warn!("[voice] call {id}: the dialled call dropped: {reason}");
                self.fail("dropped", cx);
            }
        }
    }

    /// The call's bookkeeping (the worker's id, the clocks).
    fn open(&mut self, agent_id: String, host: Option<Box<dyn CallLink>>, cx: &mut Context<Self>) -> String {
        self.counter += 1;
        let id = format!("call-{}", self.counter);
        self.active = Some(Active {
            id: id.clone(),
            agent_id,
            host,
            dialled: false,
            dial: None,
            queued: 0,
            speaking: None,
            greeting_id: format!("{id}-greeting"),
            started: Instant::now(),
            ring: Duration::from_millis(call_ring_ms(call_ring_random())),
            listening_at: None,
            heard: false,
            answered_at: None,
            voice_preparing: false,
            voice_waited: false,
            session: None,
            session_pending: false,
            voice_ready_at: None,
            opening: None,
            late_opening: false,
            sent: false,
            opening_waited: false,
            held: Vec::new(),
        });
        self.live.update(cx, |live, _| *live = CallLive::default());
        id
    }

    /// S-3: the ring (`rang` at this call's drawn `ring`), the give-up (`maxRingMs`
    /// without `ready` fails `unavailable`) and the tick that follows the
    /// windows and the timer.
    fn arm_timers(&mut self, id: String, cx: &mut Context<Self>) {
        let ring = self.active.as_ref().map_or(Duration::ZERO, |active| active.ring);
        let max_ring = Duration::from_millis(VOICE_CALL.constants.max_ring_ms);
        let ring_id = id.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ring).await;
            let _ = this.update(cx, |this, cx| this.on_rang(&ring_id, cx));
        })
        .detach();
        let max_id = id.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(max_ring).await;
            let _ = this.update(cx, |this, cx| this.on_max_ring(&max_id, cx));
        })
        .detach();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(TICK).await;
            let running = this.update(cx, |this, cx| this.on_tick(&id, cx)).unwrap_or(false);
            if !running {
                break;
            }
        })
        .detach();
    }

    fn is_call(&self, id: &str) -> bool {
        self.active.as_ref().is_some_and(|active| active.id == id)
    }

    fn on_rang(&mut self, id: &str, cx: &mut Context<Self>) {
        // EXP-1857: a dialled call's `rang` is the host's.
        if self.is_call(id) && !self.dialled() {
            self.step(CallEvent::Rang, cx);
        }
    }

    /// `maxRingMs` passed: a voice still not ready fails the call.
    fn on_max_ring(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_call(id) && self.state.phase == CallPhase::Ringing && !self.state.ready {
            log::warn!("[voice] call {id}: the voice was not ready after the longest ring");
            self.fail("unavailable", cx);
        }
    }

    /// The tick: the pill follows the windows, the timer its second.
    /// False = the call is over (the loop ends).
    fn on_tick(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.active.as_ref().filter(|active| active.id == id) else { return false };
        let timer = answered_timer(active.answered_at);
        let was = self.pill;
        self.sync_pill(cx);
        if self.pill && !was {
            self.push_pill(cx);
        }
        let changed = self.live.update(cx, |live, cx| {
            if live.timer == timer {
                return false;
            }
            live.timer = timer;
            cx.notify();
            true
        });
        if changed {
            self.push_pill(cx);
        }
        true
    }

    /// S-3: the worker listens (`CallListening`): `ready`, and the mic
    /// check runs until the ring's end (at least [`MIC_CHECK_MIN`]).
    fn on_listening(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        let now = Instant::now();
        active.listening_at = Some(now);
        let id = active.id.clone();
        let ring_end = active.started + active.ring;
        let until = ring_end.max(now + MIC_CHECK_MIN);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(until.saturating_duration_since(Instant::now())).await;
            let _ = this.update(cx, |this, cx| this.mic_checked(&id, cx));
        })
        .detach();
        self.check_voice(cx);
    }

    /// FEED-276 (`hostVoice`): the worker listens; `ready` goes once the
    /// agent's own voice is in (its clip on disk, its Prepare done), or with
    /// the built-in voice when the clip failed or `hostVoice.waitMs` passed.
    fn check_voice(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        if active.dialled || active.listening_at.is_none() || self.state.ready || self.state.phase != CallPhase::Ringing {
            return;
        }
        let clip = crate::speak_replies::clip_of(&active.agent_id, cx);
        let voice = if clip == CallVoice::Installed && active.voice_preparing { CallVoice::Installing } else { clip };
        let waited = if active.voice_waited { VOICE_CALL.host_voice.wait_ms } else { active.started.elapsed().as_millis() as u64 };
        match call_voice_ready(voice, waited) {
            CallVoiceReady::Wait => log::info!("[voice] call {}: waiting for the agent's own voice ({voice:?})", active.id),
            CallVoiceReady::Ready => self.voice_in(cx),
            CallVoiceReady::BuiltIn => {
                log::warn!("[voice] call {}: the agent's own voice is not here ({voice:?} after {waited} ms); the built-in voice speaks", active.id);
                self.voice_in(cx);
            }
        }
    }

    /// The voice is in: `ready`, or (#EXP-1961) with a call session the
    /// opening line is waited for first (`callSession.openingCases`).
    fn voice_in(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        if active.session.is_none() && !active.session_pending {
            return self.step(CallEvent::Ready, cx);
        }
        if active.voice_ready_at.is_none() {
            active.voice_ready_at = Some(Instant::now());
            let id = active.id.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms)).await;
                let _ = this.update(cx, |this, cx| {
                    if this.is_call(&id) {
                        if let Some(active) = this.active.as_mut() {
                            active.opening_waited = true;
                        }
                        this.check_opening(cx);
                    }
                });
            })
            .detach();
        }
        self.check_opening(cx);
    }

    /// #EXP-1961: a held `ready` goes once the opening line is in, or
    /// `openingWaitMs` after the voice was (a silent pickup); a call whose
    /// session never came takes the old path at once.
    fn check_opening(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        if self.state.ready || self.state.phase != CallPhase::Ringing {
            return;
        }
        let Some(ready) = active.voice_ready_at else { return };
        if active.session.is_none() && !active.session_pending {
            return self.step(CallEvent::Ready, cx);
        }
        let wait = VOICE_CALL.call_session.opening_wait_ms;
        let ms = |at: Instant| at.saturating_duration_since(active.started).as_millis() as u64;
        let ready_ms = ms(ready);
        let opening = active.opening.as_ref().filter(|_| active.session.is_some()).map(|(at, _, _)| ms(*at));
        let now = if active.opening_waited { ready_ms + wait } else { ms(Instant::now()).min(ready_ms + wait - 1) };
        match etalk::call::call_opening(Some(ready_ms), opening, now) {
            etalk::call::CallOpening::Wait => {}
            etalk::call::CallOpening::Speak => self.step(CallEvent::Ready, cx),
            etalk::call::CallOpening::Listen => {
                log::info!("[voice] call {}: no opening line in time: picked up silently", active.id);
                self.step(CallEvent::Ready, cx);
            }
        }
    }

    /// FEED-276: the agents changed (a clip landed or failed): a ringing
    /// call prepares the agent's own voice now and looks again.
    fn on_agents(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        if active.dialled || self.state.ready || self.state.phase != CallPhase::Ringing {
            return;
        }
        let agent_id = active.agent_id.clone();
        if crate::speak_replies::use_voice_of(&agent_id, cx) {
            if let Some(active) = self.active.as_mut() {
                active.voice_preparing = true;
            }
        }
        self.check_voice(cx);
    }

    /// FEED-276: the worker confirmed a Prepare (`Voices`).
    fn on_voice_prepared(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        if std::mem::take(&mut active.voice_preparing) {
            self.check_voice(cx);
        }
    }

    /// The mic check's end: nothing reached `noSoundLevel`.
    fn mic_checked(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_call(id) && self.active.as_ref().is_some_and(|active| !active.heard) {
            log::info!("[voice] call {id}: the microphone stayed silent");
            self.step(CallEvent::NoSound, cx);
        }
    }

    /// S-3: answered: the device says the agent's opening line on the
    /// call's stream (Speak replies tracks it like a reply: its end
    /// listens), or listens at once (#EXP-1961).
    fn answer(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        active.answered_at = Some(Instant::now());
        // #EXP-1961 (#FEED-275): the agent's own opening line, or silence
        // (no fixed greeting any more, a call without its session too).
        match active.opening.clone().filter(|_| active.session.is_some()) {
            Some((_, id, text)) => {
                active.greeting_id = id.clone();
                if !crate::speak_replies::speak_greeting(id, text, cx) {
                    // Nothing can speak it: listen at once.
                    self.step(CallEvent::ReplyFinished, cx);
                }
            }
            None => {
                // #EXP-1981: a session still asked has its opening still to come, too.
                active.late_opening = active.session.is_some() || active.session_pending;
                self.step(CallEvent::ReplyFinished, cx);
            }
        }
    }

    fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        if active.dialled {
            // EXP-1857: the host hears it; the frames go on as silence.
            if let Some(dial) = active.dial.as_ref() {
                dial.set_muted(muted);
            }
            self.live.update(cx, |live, cx| {
                live.level = 0.;
                cx.notify();
            });
        } else if let Some(client) = crate::speak_replies::client(cx) {
            client.call_mute(active.id.clone(), muted);
        }
        self.step(if muted { CallEvent::Mute } else { CallEvent::Unmute }, cx);
    }

    fn hang_up(&mut self, tell_host: bool, cx: &mut Context<Self>) {
        self.close_with(tell_host, CallEvent::End, cx);
    }

    /// Hang up the worker (and the host, when told) and step `event`: End,
    /// or the caller's `call_end` as `HungUp(reason)`.
    fn close_with(&mut self, tell_host: bool, event: CallEvent, cx: &mut Context<Self>) {
        let Some(mut active) = self.active.take() else { return };
        self.send_held(&mut active, cx);
        if let Some(client) = crate::speak_replies::client(cx) {
            client.hangup(active.id.clone());
        }
        if let Some(session) = active.session.as_deref() {
            host::call_ended(session, cx);
        }
        if active.speaking.is_some() {
            crate::speak_replies::stop(cx);
        }
        if tell_host {
            if let Some(host) = active.host.as_ref() {
                host.end();
            }
            if let Some(dial) = active.dial.as_ref() {
                dial.end();
            }
        }
        self.step(event, cx);
    }

    fn fail(&mut self, code: &str, cx: &mut Context<Self>) {
        let Some(mut active) = self.active.take() else { return };
        self.send_held(&mut active, cx);
        if let Some(client) = crate::speak_replies::client(cx) {
            client.hangup(active.id.clone());
        }
        if let Some(session) = active.session.as_deref() {
            host::call_ended(session, cx);
        }
        if active.speaking.is_some() {
            crate::speak_replies::stop(cx);
        }
        // F41: a streamed caller fails with the same toast (voice-call.json
        // `call_event` failed), never the reason-less hang-up's silent end.
        if let Some(host) = active.host.as_ref() {
            host.send_event(host_failed_event(code));
            host.end();
        }
        // EXP-1857: a dialled call hangs up on its host.
        if let Some(dial) = active.dial.as_ref() {
            dial.end();
        }
        self.step(CallEvent::Failed(code.to_string()), cx);
    }

    fn on_host(&mut self, event: CallLinkEvent, cx: &mut Context<Self>) {
        use CallLinkEvent as CallHostEvent;
        if self.active.as_ref().is_none_or(|active| active.host.is_none()) {
            return;
        }
        match event {
            // S-3: the pair is up; the ring (`rang`) and the worker
            // (`ready`) answer it, never `call_ready` alone.
            CallHostEvent::Ready => log::info!("[voice] the streamed call's caller is on the line"),
            CallHostEvent::Audio(pcm) => {
                let (Some(active), Some(client)) = (self.active.as_ref(), crate::speak_replies::client(cx)) else { return };
                client.call_audio(active.id.clone(), &pcm);
            }
            // Question 9: the worker knows where an utterance begins and ends.
            CallHostEvent::Lost { ms } => {
                let (Some(active), Some(client)) = (self.active.as_ref(), crate::speak_replies::client(cx)) else { return };
                client.call_lost(active.id.clone(), ms);
            }
            CallHostEvent::Mute(muted) => self.set_muted(muted, cx),
            // The relay's `idle` / `max_length` (or the caller's socket gone)
            // fail it with the `dropped` toast; only a reason-less hang-up
            // is silent (voice-call.json `hangUpRules`).
            CallHostEvent::Ended { reason } => self.close_with(false, CallEvent::HungUp(reason), cx),
            CallHostEvent::Dropped(reason) => {
                log::warn!("[voice] the streamed call dropped: {reason}");
                self.fail("dropped", cx);
            }
        }
    }

    fn on_worker(&mut self, event: etalk::Event, cx: &mut Context<Self>) {
        match event {
            etalk::Event::CallListening { .. } => self.on_listening(cx),
            etalk::Event::Level { level, bands, .. } => {
                if let Some(active) = self.active.as_mut() {
                    if active.listening_at.is_some() && level >= VOICE_CALL.constants.no_sound_level {
                        active.heard = true;
                    }
                }
                // Only the call surfaces repaint (never every window).
                self.live.update(cx, |live, cx| {
                    live.level = level;
                    live.bands = bands.to_vec();
                    cx.notify();
                });
                self.push_pill(cx);
            }
            etalk::Event::Partial { text, .. } => self.step(CallEvent::Partial(text), cx),
            etalk::Event::SpeechStarted { .. } => {
                if let Some(host) = self.active.as_ref().and_then(|active| active.host.as_ref()) {
                    host.flush();
                }
                self.step(CallEvent::SpeechStarted, cx);
            }
            etalk::Event::Said { text, .. } => self.said(text, cx),
            // EXP-1857: a dialled call's microphone, echo-cancelled: up to
            // the host (silence while muted) and the wave.
            etalk::Event::DialAudio { pcm, .. } => {
                let Ok(samples) = etalk::decode_pcm(&pcm) else { return };
                let level = if self.state.muted { 0. } else { mic_level(&samples) };
                if let Some(dial) = self.active.as_ref().and_then(|active| active.dial.as_ref()) {
                    dial.send_audio(samples);
                }
                self.live.update(cx, |live, cx| {
                    if live.level != level {
                        live.level = level;
                        cx.notify();
                    }
                });
                self.push_pill(cx);
            }
            etalk::Event::SpeechAudio { pcm, rate, .. } => {
                let Some(host) = self.active.as_ref().and_then(|active| active.host.as_ref()) else { return };
                let Ok(samples) = etalk::decode_pcm(&pcm) else { return };
                host.send_speech(samples, rate);
            }
            // NEW-55 review: the worker died: the call cannot go on.
            etalk::Event::Exited if self.active.is_some() => {
                log::warn!("[voice] the speech worker exited during a call");
                self.fail("dropped", cx);
            }
            // S-3: a voice still downloading keeps ringing (the worker
            // opens the call after the download).
            etalk::Event::ListenFailed { reason: etalk::ListenFailure::Downloading, .. } => {}
            etalk::Event::ListenFailed { reason, detail, .. } => {
                log::warn!("[voice] the call could not listen: {detail}");
                let code = match reason {
                    etalk::ListenFailure::NoMicrophone => "no_microphone",
                    etalk::ListenFailure::NotGranted => "not_granted",
                    // NEW-57: voice-call.json `failed.meeting`.
                    etalk::ListenFailure::Meeting => "meeting",
                    // S-3: the download or the load failed.
                    etalk::ListenFailure::Unavailable => "unavailable",
                    _ => "dropped",
                };
                self.fail(code, cx);
            }
            _ => {}
        }
    }

    /// A finished utterance: a plain message into the conversation.
    fn said(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        if text.trim().is_empty() {
            return;
        }
        // S-3 (voice-call.json): words during the ring are not the call's.
        // EXP-1857 real call test: a remote caller streams from `call_ready`
        // on, so the host heard (and sent) them before the greeting.
        if self.state.phase == CallPhase::Ringing {
            log::info!("[voice] call: an utterance during the ring was dropped");
            return;
        }
        let busy = matches!(self.state.phase, CallPhase::Thinking | CallPhase::Speaking);
        let agent_id = active.agent_id.clone();
        active.sent = true;
        if busy {
            active.queued += 1;
        }
        let queued = active.queued;
        if busy {
            self.step(CallEvent::Queued { count: queued, text: text.clone() }, cx);
        } else {
            self.step(CallEvent::Said(text.clone()), cx);
        }
        self.send(agent_id, text, cx);
    }

    /// #EXP-1981: the call ended while its session was asked: what it held
    /// goes into the chat (the old path), never lost.
    fn send_held(&mut self, active: &mut Active, cx: &mut Context<Self>) {
        for text in std::mem::take(&mut active.held) {
            self.send(active.agent_id.clone(), text, cx);
        }
    }

    fn send(&mut self, agent_id: String, text: String, cx: &mut Context<Self>) {
        // #EXP-1981: the call's session is still asked: held until it answers.
        if let Some(active) = self.active.as_mut().filter(|active| active.session_pending) {
            active.held.push(text);
            return;
        }
        let Some(mut outgoing) = host::outgoing(&agent_id, &text, cx) else { return };
        // #EXP-1961: into the call's own session (sealed with the chat's tree key).
        if let Some(session) = self.active.as_ref().and_then(|active| active.session.clone()) {
            let text = match &outgoing.target {
                etalk::voice::UtteranceTarget::Session { text, .. } | etalk::voice::UtteranceTarget::Agent { text, .. } => text.clone(),
            };
            outgoing.target = etalk::voice::UtteranceTarget::Session { session_id: session, text };
        }
        let Some(job) = host::send(outgoing, cx) else { return };
        let call = self.active.as_ref().map(|active| active.id.clone()).unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { job() }).await;
            match result {
                // #EXP-1982: no word of this call can be stored (an encrypted
                // chat, no key holder here): the call ends instead of
                // listening on and swallowing every line.
                Err(err) if err.unstorable => {
                    log::warn!("[voice] call {call}: the send was refused and nothing can be stored here: {err}");
                    let _ = this.update(cx, |this, cx| {
                        if this.is_call(&call) {
                            this.fail("dropped", cx);
                        }
                    });
                }
                // The call goes on; the words are in the log, never lost
                // silently: a toast says the line did not go out.
                Err(err) => {
                    log::warn!("[voice] call: the send failed: {err}");
                    let _ = cx.update(|cx| host::notify(Notice::Error(etalk::voice::VOICE.ptt_copy.overlay_not_sent.clone()), cx));
                }
                Ok(_) => {}
            }
        })
        .detach();
    }

    /// Speak replies started or ended a reply (or the greeting): the
    /// call's Speaking follows (only for the call's own conversation).
    fn on_speaker(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        let conversation = active.session.clone().or_else(|| host::conversation(&active.agent_id, cx));
        let greeting = active.greeting_id.clone();
        let speaking = crate::speak_replies::speaking(cx)
            .filter(|speaking| speaking.message_id == greeting || Some(&speaking.session_id) == conversation.as_ref());
        let was = active.speaking.clone();
        match (was, speaking) {
            (None, Some(speaking)) => {
                if let Some(active) = self.active.as_mut() {
                    active.speaking = Some(speaking.message_id.clone());
                    // The agent took the queued lines in with this turn.
                    if speaking.message_id != greeting {
                        active.queued = 0;
                    }
                }
                self.step(CallEvent::ReplyStarted(speaking.title.clone()), cx);
            }
            (Some(_), None) => {
                if let Some(active) = self.active.as_mut() {
                    active.speaking = None;
                }
                // A barge-in already moved to Listening.
                if self.state.phase == CallPhase::Speaking {
                    self.step(CallEvent::ReplyFinished, cx);
                }
            }
            (Some(old), Some(speaking)) if old != speaking.message_id => {
                if let Some(active) = self.active.as_mut() {
                    active.speaking = Some(speaking.message_id.clone());
                }
                self.step(CallEvent::ReplyStarted(speaking.title.clone()), cx);
            }
            _ => {}
        }
    }
}

/// EXP-1857: a block's loudness for the wave (0..1, the peak with headroom).
fn mic_level(samples: &[i16]) -> f32 {
    let peak = samples.iter().map(|sample| i32::from(*sample).unsigned_abs()).max().unwrap_or(0);
    (peak as f32 / 32768. * 2.).min(1.)
}

/// EXP-1857: `message` is one of voice-call.json's availability reasons
/// (`{device}` = any name): the server refused the dial in its own words.
fn is_reason(message: &str) -> bool {
    let reasons = &VOICE_CALL.copy.reasons;
    [
        &reasons.meeting,
        &reasons.offline,
        &reasons.preparing,
        &reasons.voice_failed,
        &reasons.no_gpu,
        &reasons.busy,
        &reasons.no_desktop,
    ]
    .iter()
    .any(|reason| match reason.split_once("{device}") {
        Some((before, after)) => message.len() > before.len() + after.len() && message.starts_with(before) && message.ends_with(after),
        None => message == reason.as_str(),
    })
}

/// How long a ring may wait for the ticket before the relay gives up
/// (voice-call.json `constants.ringTimeoutMs`): a refused call waits this
/// long for the caller to be on the line ([`fail_when_on_the_line`]).
fn ring_timeout() -> Duration {
    Duration::from_millis(VOICE_CALL.constants.ring_timeout_ms)
}

/// FEED-272: the tabs' and the docked row's width cap (the chat column's).
const CHAT_COLUMN_MAX_W: f32 = 768.;

/// S-3: the in-window CALL PANEL over an agent chat's body (the agent
/// screen holds one): drawn while THIS client runs a call with that agent,
/// nothing otherwise. Esc in it ends the call; the header's telephone
/// focuses it. It repaints on the call's steps and its live level/timer,
/// never the chat around it.
pub struct CallPanel {
    agent_id: Option<String>,
    /// FEED-272: the tab on top (a call opens on `tabs.initial`).
    tab: etalk::call::CallTab,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl CallPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(call) = global(cx) {
            let live = call.read(cx).live.clone();
            subscriptions.push(cx.observe(&call, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&live, |_, _, cx| cx.notify()));
        }
        CallPanel { agent_id: None, tab: VOICE_CALL.tabs.initial, focus: cx.focus_handle(), _subscriptions: subscriptions }
    }

    /// The chat's agent (its screen resolves it).
    pub fn set_agent(&mut self, agent_id: Option<String>, cx: &mut Context<Self>) {
        if self.agent_id != agent_id {
            self.agent_id = agent_id;
            cx.notify();
        }
    }

    /// The panel shows (a call with this chat's agent runs here).
    pub fn shown(&self, cx: &App) -> bool {
        self.agent_id.as_deref().is_some_and(|agent_id| runs_for(agent_id, cx))
    }

    /// FEED-272: the call sits DOCKED over the chat (the Chat tab): the
    /// screen lays the panel out ABOVE the chat instead of over it.
    pub fn docked(&self, cx: &App) -> bool {
        self.tab == etalk::call::CallTab::Chat && self.shown(cx)
    }

    /// The tab on top.
    pub fn tab(&self) -> etalk::call::CallTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: etalk::call::CallTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }
}

impl Focusable for CallPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CallPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let face = self.shown(cx).then(|| face(cx)).flatten();
        let Some(face) = face else {
            // No call: the next one opens on `tabs.initial` again.
            self.tab = VOICE_CALL.tabs.initial;
            return div().into_any_element();
        };
        let actions = CallActions { on_mute: std::rc::Rc::new(toggle_mute), on_end: std::rc::Rc::new(end) };
        let this = cx.entity().downgrade();
        let on_tab: crate::voice_overlay::TabClick = std::rc::Rc::new(move |tab, cx| {
            this.update(cx, |panel, cx| panel.set_tab(tab, cx)).ok();
        });
        let tabs = crate::voice_overlay::call_tabs(self.tab, on_tab, cx);
        let panel_id = SharedString::from(VOICE_CALL.test_ids.panel.clone());
        let background = {
            use gpui_component::ActiveTheme as _;
            cx.theme().background
        };
        let root = div()
            .track_focus(&self.focus)
            .on_key_down(|event, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    end(cx);
                }
            });
        match self.tab {
            // FEED-272 Chat: the tabs and the pill's row docked ABOVE the
            // chat (the screen puts this panel before it), the chat with its
            // composer readable and writable under them.
            etalk::call::CallTab::Chat => root
                .flex_none()
                .w_full()
                .flex()
                .justify_center()
                .px(px(16.))
                .pt(px(12.))
                .pb(px(4.))
                .child(
                    gpui_component::v_flex()
                        .w_full()
                        .max_w(px(CHAT_COLUMN_MAX_W))
                        .gap(px(12.))
                        .child(tabs)
                        .child(crate::voice_overlay::call_pill_docked(panel_id, &face, &actions, cx)),
                )
                .into_any_element(),
            etalk::call::CallTab::Voice => root
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .bg(background)
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .justify_center()
                        .px(px(16.))
                        .pt(px(12.))
                        .child(div().w_full().max_w(px(CHAT_COLUMN_MAX_W)).child(tabs)),
                )
                .child(div().flex_1().min_h_0().child(crate::voice_overlay::call_panel(panel_id, &face, &actions, cx)))
                .into_any_element(),
        }
    }
}

/// A call as if it had started (no worker, no host), ringing, for the
/// lifecycle tests.
#[cfg(test)]
fn ring_for_test(agent_id: &str, cx: &mut App) -> String {
    let Some(entity) = global(cx) else { return String::new() };
    entity.update(cx, |this, cx| {
        let id = this.open(agent_id.into(), None, cx);
        this.step(CallEvent::Start { device: "mint".into(), language: Some("en".into()), fallback: false }, cx);
        id
    })
}

/// A call as if it had been answered, for the lifecycle tests.
#[cfg(test)]
fn start_for_test(agent_id: &str, cx: &mut App) {
    let id = ring_for_test(agent_id, cx);
    on_worker_event(etalk::Event::CallListening { id: id.clone() }, cx);
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_rang(&id, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
            install(cx);
        });
    }

    fn running(cx: &mut gpui::TestAppContext) {
        setup(cx);
        cx.update(|cx| {
            start_for_test("agent-1", cx);
            assert!(active(cx));
            assert_eq!(agent_id(cx).as_deref(), Some("agent-1"));
            assert!(runs_for("agent-1", cx));
            assert!(!runs_for("agent-2", cx));
        });
    }

    fn phase(cx: &App) -> CallPhase {
        global(cx).unwrap().read(cx).state.phase
    }

    /// NEW-55 review: a crashed speech worker ends the call and clears it.
    #[gpui::test]
    async fn a_worker_crash_ends_the_call(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            assert!(!owns(&etalk::Event::Exited, cx), "the dispatcher sends Exited itself");
            on_worker_event(etalk::Event::Exited, cx);
            assert!(!active(cx));
            assert_eq!(phase(cx), CallPhase::Hidden);
        });
    }

    /// EXP-1901: the pill's buttons run inside the pill's own window, where
    /// gpui refuses a nested update of it: Mute repaints the one pill (never
    /// a second window) and End call closes it.
    #[gpui::test]
    async fn the_pills_own_buttons_repaint_and_close_it(cx: &mut gpui::TestAppContext) {
        running(cx);
        let windows = cx.windows();
        assert_eq!(windows.len(), 1, "the pill shows");
        let pill = windows[0];
        let shown = |cx: &mut gpui::TestAppContext| {
            cx.update(|cx| global(cx).unwrap().update(cx, |this, cx| this.overlay.face(cx))).and_then(|face| match face {
                Face::Call(face) => Some(face.view.muted),
                Face::Ptt(_) => None,
            })
        };
        assert_eq!(shown(cx), Some(false));
        cx.update(|cx| pill.update(cx, |_, _, cx| toggle_mute(cx))).unwrap();
        cx.run_until_parked();
        assert_eq!(cx.windows().iter().map(|window| window.window_id()).collect::<Vec<_>>(), vec![pill.window_id()], "Mute keeps the one pill");
        assert_eq!(shown(cx), Some(true), "and repaints it");
        cx.update(|cx| pill.update(cx, |_, _, cx| end(cx))).unwrap();
        cx.run_until_parked();
        assert!(cx.windows().is_empty(), "End call closes the pill");
    }

    /// Sign-out (Speak replies' `on_session`) hangs up through `end`.
    #[gpui::test]
    async fn sign_out_hangs_up(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            end(cx);
            assert!(!active(cx));
            assert!(face(cx).is_none());
        });
    }

    /// EXP-1857: there is no voice device to move away from any more: a
    /// settings change without a meeting here keeps the call.
    #[gpui::test]
    async fn a_settings_change_keeps_the_call(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            settings_changed(cx);
            assert!(active(cx));
        });
    }

    /// A worker that exits with no call running is not the call's business.
    #[gpui::test]
    async fn an_exit_without_a_call_is_ignored(cx: &mut gpui::TestAppContext) {
        setup(cx);
        cx.update(|cx| {
            on_worker_event(etalk::Event::Exited, cx);
            assert!(!active(cx));
        });
    }

    /// S-3: a call rings first; the ring's end and the worker's
    /// `CallListening` (either order) answer it: the greeting (nothing can
    /// speak it in the test app, so it listens at once) and the timer.
    #[gpui::test]
    async fn the_ring_and_the_worker_answer_the_call(cx: &mut gpui::TestAppContext) {
        setup(cx);
        cx.update(|cx| {
            let id = ring_for_test("agent-1", cx);
            assert_eq!(phase(cx), CallPhase::Ringing);
            assert_eq!(face(cx).unwrap().view.status.as_deref(), Some(VOICE_CALL.copy.calling.as_str()));
            let event = etalk::Event::CallListening { id: id.clone() };
            assert!(owns(&event, cx));
            on_worker_event(event, cx);
            assert_eq!(phase(cx), CallPhase::Ringing, "ready without the ring's end keeps ringing");
            global(cx).unwrap().update(cx, |this, cx| this.on_rang(&id, cx));
            let entity = global(cx).unwrap();
            assert!(entity.read(cx).active.as_ref().unwrap().answered_at.is_some());
            assert_eq!(phase(cx), CallPhase::Listening);
            let face = face(cx).unwrap();
            assert_eq!(face.view.status, None, "answered: the timer");
            assert_eq!(face.timer.as_ref(), "00:00");
            assert_eq!(face.view.buttons, vec![etalk::call::CallButton::Mute, etalk::call::CallButton::End]);
        });
    }

    /// EXP-1857 (the real call test): an utterance heard during the ring
    /// is not the call's: dropped, the call still rings and shows nothing.
    #[gpui::test]
    async fn an_utterance_during_the_ring_is_dropped(cx: &mut gpui::TestAppContext) {
        setup(cx);
        cx.update(|cx| {
            let id = ring_for_test("agent-1", cx);
            on_worker_event(etalk::Event::Said { id: id.clone(), text: "status of the release".into() }, cx);
            assert_eq!(phase(cx), CallPhase::Ringing);
            assert_eq!(face(cx).unwrap().view.caption, "");
            assert_eq!(global(cx).unwrap().read(cx).active.as_ref().unwrap().queued, 0);
        });
    }

    /// S-3: the ring ending first says Getting ready; a voice still
    /// downloading keeps ringing (never dropped).
    #[gpui::test]
    async fn a_downloading_voice_keeps_ringing(cx: &mut gpui::TestAppContext) {
        setup(cx);
        cx.update(|cx| {
            let id = ring_for_test("agent-1", cx);
            global(cx).unwrap().update(cx, |this, cx| this.on_rang(&id, cx));
            assert_eq!(face(cx).unwrap().view.status.as_deref(), Some(VOICE_CALL.copy.getting_ready.as_str()));
            on_worker_event(
                etalk::Event::ListenFailed { id: id.clone(), reason: etalk::ListenFailure::Downloading, detail: "downloading".into() },
                cx,
            );
            assert!(active(cx));
            assert_eq!(phase(cx), CallPhase::Ringing);
        });
    }

    /// S-3: no `CallListening` within `maxRingMs` fails it `unavailable`;
    /// so does a failed download.
    #[gpui::test]
    async fn the_longest_ring_fails_unavailable(cx: &mut gpui::TestAppContext) {
        setup(cx);
        let unavailable = VOICE_CALL.copy.failed["unavailable"].replace("{device}", "mint");
        cx.update(|cx| {
            let id = ring_for_test("agent-1", cx);
            global(cx).unwrap().update(cx, |this, cx| this.on_max_ring(&id, cx));
            assert!(!active(cx));
            let id = ring_for_test("agent-1", cx);
            on_worker_event(etalk::Event::ListenFailed { id, reason: etalk::ListenFailure::Unavailable, detail: "load".into() }, cx);
            assert!(!active(cx));
            assert_eq!(global(cx).unwrap().read(cx).toasts, vec![unavailable.clone(), unavailable.clone()]);
        });
    }

    /// S-3: the mic check: a silent microphone says so once answered; a
    /// level at `noSoundLevel` passes it.
    #[gpui::test]
    async fn a_silent_microphone_is_reported(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            let id = global(cx).unwrap().read(cx).active.as_ref().unwrap().id.clone();
            global(cx).unwrap().update(cx, |this, cx| this.mic_checked(&id, cx));
            assert_eq!(face(cx).unwrap().view.caption, VOICE_CALL.copy.no_sound);
            end(cx);
            start_for_test("agent-1", cx);
            let id = global(cx).unwrap().read(cx).active.as_ref().unwrap().id.clone();
            let level = VOICE_CALL.constants.no_sound_level;
            on_worker_event(etalk::Event::Level { id: id.clone(), level, bands: [level; etalk::LEVEL_BANDS] }, cx);
            global(cx).unwrap().update(cx, |this, cx| this.mic_checked(&id, cx));
            assert_eq!(face(cx).unwrap().view.caption, "");
        });
    }

    /// A link to nobody (a streamed call in the tests).
    struct Detached;

    impl CallLink for Detached {
        fn send_event(&self, _: serde_json::Value) {}
        fn send_speech(&self, _: Vec<i16>, _: u32) {}
        fn flush(&self) {}
        fn end(&self) {}
    }

    /// A streamed call (a detached host socket) and the caller's `call_end`.
    fn host_ended(reason: Option<&str>, cx: &mut gpui::TestAppContext) -> Vec<String> {
        running(cx);
        cx.update(|cx| {
            let entity = global(cx).unwrap();
            entity.update(cx, |this, cx| {
                this.active.as_mut().unwrap().host = Some(Box::new(Detached));
                this.on_host(CallLinkEvent::Ended { reason: reason.map(str::to_string) }, cx);
            });
            assert!(!active(cx));
            assert_eq!(entity.read(cx).state.phase, CallPhase::Hidden);
            entity.read(cx).toasts.clone()
        })
    }

    /// #1266 review: the relay's idle timeout / max length end the call
    /// with the `dropped` toast, never silently.
    #[gpui::test]
    async fn the_relays_idle_timeout_toasts_dropped(cx: &mut gpui::TestAppContext) {
        let dropped = VOICE_CALL.copy.failed["dropped"].clone();
        assert_eq!(host_ended(Some("idle"), cx), vec![dropped]);
    }

    #[gpui::test]
    async fn the_relays_max_length_toasts_dropped(cx: &mut gpui::TestAppContext) {
        let dropped = VOICE_CALL.copy.failed["dropped"].clone();
        assert_eq!(host_ended(Some("max_length"), cx), vec![dropped]);
    }

    /// EXP-1929: a ring cancelled before the relay admitted this device
    /// (`no_such_call`, a hang-up, a link gone) starts no call; only
    /// `call_ready` does.
    #[gpui::test]
    async fn a_call_starts_only_on_the_relays_admit() {
        async fn outcome(events: Vec<CallLinkEvent>) -> bool {
            let (tx, rx) = flume::unbounded();
            for event in events {
                tx.send(event).unwrap();
            }
            drop(tx);
            admitted(&rx).await
        }
        assert!(!outcome(vec![CallLinkEvent::Dropped("no_such_call".into())]).await);
        assert!(!outcome(vec![CallLinkEvent::Ended { reason: None }]).await);
        assert!(!outcome(vec![]).await, "the link went away");
        assert!(outcome(vec![CallLinkEvent::Ready, CallLinkEvent::Dropped("later".into())]).await);
    }

    /// The caller pressing End call (no reason) ends it silently.
    #[gpui::test]
    async fn the_caller_hanging_up_is_silent(cx: &mut gpui::TestAppContext) {
        assert!(host_ended(None, cx).is_empty());
    }

    #[test]
    fn only_flow_events_go_to_the_caller() {
        assert_eq!(wire_event(&CallEvent::Said("hi".into())).unwrap()["t"], "said");
        assert_eq!(wire_event(&CallEvent::Queued { count: 2, text: "x".into() }).unwrap()["count"], 2);
        // S-3: the host's ring goes out, so the caller rings the same.
        assert_eq!(wire_event(&CallEvent::Rang).unwrap()["t"], "rang");
        assert_eq!(wire_event(&CallEvent::Ready).unwrap()["t"], "ready");
        assert_eq!(wire_event(&CallEvent::NoSound).unwrap()["t"], "noSound");
        assert!(wire_event(&CallEvent::Mute).is_none());
        assert!(wire_event(&CallEvent::End).is_none());
        assert!(wire_event(&CallEvent::Start { device: "mint".into(), language: None, fallback: true }).is_none());
    }

    /// EXP-1857: the member's desktops as this client reads them: this
    /// machine online with its own speech (first when not synced yet), the
    /// others by their doctor rows, the meeting on the voice device only.
    #[test]
    fn the_target_input_reads_this_machine_itself() {
        let row = |id: &str, online: bool, speech: Option<&str>| TalkDesktop {
            id: id.into(),
            name: format!("{id}-name"),
            online,
            speech: speech.map(str::to_string),
        };
        let rows = [row("d-mint", true, Some("ok")), row("d-mac", false, None), row("d-old", true, Some("missing"))];
        let desktops = target_desktops(Some(("d-mac", "macbook")), Some(CallSpeech::Preparing), &rows, Some("d-mint"));
        assert_eq!(desktops.iter().map(|desktop| desktop.id.as_str()).collect::<Vec<_>>(), ["d-mint", "d-mac", "d-old"], "the synced order");
        assert_eq!((desktops[1].online, desktops[1].speech), (true, Some(CallSpeech::Preparing)), "here: online, its own speech");
        assert_eq!((desktops[0].speech, desktops[0].meeting), (Some(CallSpeech::Ok), true));
        assert_eq!(desktops[2].speech, Some(CallSpeech::NoGpu));
        let unsynced = target_desktops(Some(("d-new", "new")), Some(CallSpeech::Ok), &rows, None);
        assert_eq!((unsynced[0].id.as_str(), unsynced[0].name.as_str(), unsynced[0].online), ("d-new", "new", true));
        assert_eq!(unsynced.len(), 4);
        // Danny's case: the Assistant runs on a server, this desktop speaks.
        let target = call_target(&CallTargetInput {
            here: Some("d-new"),
            agent_device: Some("d-server"),
            desktops: &unsynced,
            servers: &[],
            busy: false,
        });
        assert_eq!((target.name.as_deref(), target.local, target.fallback), (Some("new"), true, true));
        // This desktop has no GPU: the call is dialled to mint.
        let here_silent = target_desktops(Some(("d-mac", "macbook")), Some(CallSpeech::NoGpu), &rows, None);
        let target = call_target(&CallTargetInput {
            here: Some("d-mac"),
            agent_device: Some("d-server"),
            desktops: &here_silent,
            servers: &[],
            busy: false,
        });
        assert_eq!((target.device.as_deref(), target.local, target.fallback), (Some("d-mint"), false, true));
        assert_eq!(call_view(&call_step(&CallState::default(), &CallEvent::Start { device: "mint-name".into(), language: None, fallback: true }), CallPlatform::Desktop).unwrap().via.as_deref(), Some("Voice on mint-name"));
    }

    /// EXP-1857: any member desktop that speaks answers a ring, the voice
    /// device or not; a meeting refuses only where it runs.
    #[test]
    fn any_desktop_that_speaks_answers_a_ring() {
        assert_eq!(ring_answer(false, false, Some(SpeechState::Ready)), RingAnswer::Answer, "not the voice device: answers");
        assert_eq!(ring_answer(false, false, Some(SpeechState::Downloading)), RingAnswer::Answer, "rings until ready");
        assert_eq!(ring_answer(false, true, Some(SpeechState::Ready)), RingAnswer::RefuseBusy, "#EXP-1936: in a call the relay does not know");
        assert_eq!(ring_answer(false, true, None), RingAnswer::RefuseBusy);
        assert_eq!(ring_answer(false, false, Some(SpeechState::NoGpu)), RingAnswer::Ignore);
        assert_eq!(ring_answer(false, false, Some(SpeechState::Failed)), RingAnswer::Ignore);
        assert_eq!(ring_answer(false, false, None), RingAnswer::Ignore, "no speech row");
        assert_eq!(ring_answer(true, false, Some(SpeechState::Ready)), RingAnswer::RefuseMeeting);
        assert_eq!(ring_answer(true, true, Some(SpeechState::Ready)), RingAnswer::RefuseBusy, "a running call keeps it");
    }

    /// EXP-1857: the server's refusals in the fixture's words are toasted
    /// as they are; anything else is a dropped call.
    #[test]
    fn a_refusal_in_the_fixtures_words_is_a_reason() {
        assert!(is_reason("mint is offline"));
        assert!(is_reason("big desk can't speak: no usable GPU"));
        assert!(is_reason(&VOICE_CALL.copy.reasons.no_desktop));
        assert!(!is_reason(" is offline"), "no device");
        assert!(!is_reason("This call runs on this desktop"));
        assert!(!is_reason("Internal server error"));
    }

    /// A dialled desktop that records what the call did to it.
    #[derive(Clone, Default)]
    struct Recorder(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

    impl DialLink for Recorder {
        fn send_audio(&self, pcm: Vec<i16>) {
            self.0.borrow_mut().push(format!("audio {}", pcm.len()));
        }
        fn set_muted(&self, muted: bool) {
            self.0.borrow_mut().push(format!("mute {muted}"));
        }
        fn end(&self) {
            self.0.borrow_mut().push("end".into());
        }
    }

    /// A dialled call, ringing, its link connected (no worker, no relay).
    fn dialled(cx: &mut gpui::TestAppContext) -> (String, Recorder) {
        let recorder = Recorder::default();
        let id = cx.update(|cx| {
            global(cx).unwrap().update(cx, |this, cx| {
                let id = this.open("agent-1".into(), None, cx);
                let active = this.active.as_mut().unwrap();
                active.dialled = true;
                active.dial = Some(Box::new(recorder.clone()));
                this.step(CallEvent::Start { device: "mint".into(), language: Some("en".into()), fallback: true }, cx);
                id
            })
        });
        (id, recorder)
    }

    fn host_event(id: &str, event: serde_json::Value, cx: &mut App) {
        global(cx).unwrap().update(cx, |this, cx| this.on_dial(id, DialLinkEvent::Event(event), cx));
    }

    /// EXP-1857: CALLER MODE: the host's `rang` + `ready` answer it (on
    /// Listening, #EXP-1966; the host speaks), its flow events step this machine,
    /// the microphone goes up, mute and End reach the host; the via line
    /// names the desktop that speaks; Speak replies never speaks it here.
    #[gpui::test]
    async fn a_dialled_call_follows_its_host(cx: &mut gpui::TestAppContext) {
        setup(cx);
        let (id, recorder) = dialled(cx);
        cx.update(|cx| {
            assert_eq!(face(cx).unwrap().view.via.as_deref(), Some("Voice on mint"));
            assert_eq!(agent_id(cx), None, "its speech runs on mint");
            assert!(runs_for("agent-1", cx));
            global(cx).unwrap().update(cx, |this, cx| this.on_rang(&id, cx));
            assert_eq!(phase(cx), CallPhase::Ringing, "the ring is the host's");
            host_event(&id, serde_json::json!({ "t": "rang" }), cx);
            host_event(&id, serde_json::json!({ "t": "ready" }), cx);
            assert_eq!(phase(cx), CallPhase::Listening, "answered (#EXP-1966: the host speaks its opening as a reply)");
            assert!(global(cx).unwrap().read(cx).active.as_ref().unwrap().answered_at.is_some(), "the timer runs");
            host_event(&id, serde_json::json!({ "t": "replyFinished" }), cx);
            assert_eq!(phase(cx), CallPhase::Listening);
            on_worker_event(etalk::Event::DialAudio { id: id.clone(), pcm: etalk::encode_pcm(&[1000; 320]) }, cx);
            toggle_mute(cx);
            assert!(face(cx).unwrap().view.muted);
            end(cx);
            assert!(!active(cx));
        });
        assert_eq!(*recorder.0.borrow(), ["audio 320", "mute true", "end"]);
    }

    /// EXP-1857: the relay's `no_answer` (and the host's failed event)
    /// toast through `copy.failed` with the device's name.
    #[gpui::test]
    async fn a_dialled_call_that_nobody_answers_toasts(cx: &mut gpui::TestAppContext) {
        setup(cx);
        let (id, recorder) = dialled(cx);
        cx.update(|cx| {
            global(cx).unwrap().update(cx, |this, cx| this.on_dial(&id, DialLinkEvent::Failed("no_answer".into()), cx));
            assert!(!active(cx));
            assert_eq!(global(cx).unwrap().read(cx).toasts, vec![VOICE_CALL.copy.failed["no_answer"].replace("{device}", "mint")]);
        });
        assert_eq!(*recorder.0.borrow(), ["end"]);
        let (id, _) = dialled(cx);
        cx.update(|cx| {
            host_event(&id, serde_json::json!({ "t": "failed", "code": "unavailable" }), cx);
            assert!(!active(cx));
        });
    }

    use crate::test_support::{settings, Rig, AGENT};

    /// A local call through the real `start`, still ringing: its worker id.
    fn rung(cx: &mut gpui::TestAppContext) -> String {
        cx.update(|cx| {
            start(AGENT.into(), cx);
            assert_eq!(phase(cx), CallPhase::Ringing);
            global(cx).unwrap().read(cx).active.as_ref().unwrap().id.clone()
        })
    }

    /// The worker listens and the longest drawn ring passed: answered.
    fn answered(rig: &Rig, id: &str, cx: &mut gpui::TestAppContext) {
        rig.worker(etalk::Event::CallListening { id: id.into() }, cx);
        let constants = &VOICE_CALL.constants;
        cx.executor().advance_clock(Duration::from_millis(constants.min_ring_ms + constants.ring_jitter_ms));
        cx.run_until_parked();
    }

    fn finished(rig: &Rig, id: &str, cx: &mut gpui::TestAppContext) {
        rig.worker(etalk::Event::Finished { id: id.into(), stopped: false }, cx);
    }

    /// #EXP-1961 (#FEED-275): a call with its own session: no fixed
    /// greeting; the pickup waits for the agent's own opening line (written
    /// during the ring, never said before the pickup), says it, sends the
    /// utterances into the call session, speaks its replies and tells the
    /// server the hang-up.
    #[gpui::test]
    async fn a_call_session_opens_with_the_agents_own_line(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        let id = rung(cx);
        cx.run_until_parked();
        rig.requests();
        rig.worker(etalk::Event::CallListening { id: id.clone() }, cx);
        assert!(cx.update(|cx| !global(cx).unwrap().read(cx).state.ready), "ready waits for the opening line");
        rig.call_message("o1", "Hi Danny, the build is green.", 1, cx);
        assert!(rig.spoken().is_empty(), "never said during the ring");
        answered(&rig, &id, cx);
        assert_eq!(rig.spoken(), ["o1"]);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Speaking);
        finished(&rig, "o1", cx);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening);
        rig.worker(etalk::Event::Said { id: id.clone(), text: "thanks".into() }, cx);
        assert_eq!(rig.call_log().0, ["call-s"]);
        rig.message("chat", "agent", "A reply in the chat.", 2, cx);
        assert!(rig.spoken().is_empty(), "the chat's replies are not the call's");
        rig.call_message("r1", "Anything else?", 3, cx);
        assert_eq!(rig.spoken(), ["r1"]);
        cx.update(end);
        assert_eq!(rig.call_log().1, ["call-s"]);
    }

    /// #EXP-1961: no opening line `openingWaitMs` after the voice: a silent
    /// pickup on Listening, and the late line is never spoken.
    #[gpui::test]
    async fn a_call_session_without_an_opening_picks_up_silently(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        let id = rung(cx);
        cx.run_until_parked();
        answered(&rig, &id, cx);
        cx.executor().advance_clock(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening);
        rig.requests();
        rig.call_message("o1", "Hello!", 1, cx);
        assert!(rig.spoken().is_empty(), "a late opening is not spoken");
        rig.call_message("r2", "Still there?", 2, cx);
        assert_eq!(rig.spoken(), ["r2"]);
    }

    /// #EXP-1936: the call and the speaker together: #EXP-1961 no fixed
    /// greeting (a call without its own session listens at once); an
    /// utterance goes out and waits; the agent's reply is spoken and the
    /// call follows it; a card-only row is not.
    #[gpui::test]
    async fn a_call_speaks_its_replies_and_no_fixed_greeting(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        let id = rung(cx);
        assert!(matches!(rig.requests().last(), Some(etalk::Request::Call { remote: false, .. })));
        answered(&rig, &id, cx);
        assert!(rig.spoken().is_empty(), "no fixed greeting");
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening);
        rig.worker(etalk::Event::Said { id: id.clone(), text: "status of the release".into() }, cx);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Thinking);
        rig.row(crate::test_support::Row { id: "card".into(), author_kind: "agent", body: "A plan.".into(), has_card: true, created_at: "2026-11-17T10:00:04.000Z".into(), session_id: None }, cx);
        assert!(rig.spoken().is_empty(), "a card is shown, never spoken");
        rig.message("r1", "agent", "All green. Ship it.", 5, cx);
        assert_eq!(rig.spoken(), ["r1"]);
        assert_eq!(cx.update(|cx| (phase(cx), face(cx).unwrap().view.caption)), (CallPhase::Speaking, "All green.".to_string()));
        finished(&rig, "r1", cx);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening);
        assert!(rig.notices().is_empty());
    }

    /// FEED-276: the agent's clip, still downloading when the call starts.
    fn fetching_voice() -> crate::host::AgentVoice {
        crate::host::AgentVoice {
            voice: etalk::VoiceChoice { language: "en".into(), ..Default::default() },
            clip: CallVoice::Fetching,
            ..Default::default()
        }
    }

    /// FEED-276 (`hostVoice`): a call whose agent's clip still downloads
    /// keeps ringing after the ring's end; the clip lands, its Prepare goes
    /// out, and only the worker's `Voices` answers it: the greeting is the
    /// agent's own voice, spoken after that Prepare.
    #[gpui::test]
    async fn the_greeting_waits_for_the_agents_own_voice(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.agent_voice(fetching_voice(), cx);
        let id = rung(cx);
        rig.requests();
        rig.worker(etalk::Event::CallListening { id: id.clone() }, cx);
        cx.executor().advance_clock(Duration::from_millis(VOICE_CALL.host_voice.wait_ms - 1));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Ringing, "the clip is still on its way");
        assert!(rig.spoken().is_empty(), "nothing in the built-in voice");
        let own = etalk::VoiceChoice { language: "en".into(), file: "/data/voice-clips/c-1.wav".into(), transcript: "Hi.".into() };
        rig.agent_voice(crate::host::AgentVoice { voice: own.clone(), clip: CallVoice::Installed, ..Default::default() }, cx);
        let requests = rig.requests();
        assert!(requests.iter().any(|request| matches!(request, etalk::Request::Prepare { voice } if *voice == own)), "{requests:?}");
        assert!(!requests.iter().any(|request| matches!(request, etalk::Request::Speak { .. })), "its Prepare is not confirmed yet");
        assert_eq!(cx.update(|cx| (phase(cx), global(cx).unwrap().read(cx).state.ready)), (CallPhase::Ringing, false));
        rig.worker(etalk::Event::Voices { voices: vec!["en".into()] }, cx);
        assert!(cx.update(|cx| global(cx).unwrap().read(cx).state.ready), "its Prepare is done: ready");
        cx.executor().advance_clock(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening, "answered (#EXP-1961: no fixed greeting)");
    }

    /// FEED-276: a clip that never lands is waited for `hostVoice.waitMs`,
    /// then the built-in voice answers; a failed one answers at once.
    #[gpui::test]
    async fn a_clip_that_does_not_arrive_falls_back_to_the_built_in_voice(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.agent_voice(fetching_voice(), cx);
        let id = rung(cx);
        answered(&rig, &id, cx);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening, "the bound passed within the longest ring: answered");
        cx.update(end);
        rig.agent_voice(crate::host::AgentVoice { clip: CallVoice::Failed, ..fetching_voice() }, cx);
        let id = rung(cx);
        rig.worker(etalk::Event::CallListening { id: id.clone() }, cx);
        cx.executor().advance_clock(Duration::from_millis(VOICE_CALL.constants.min_ring_ms + VOICE_CALL.constants.ring_jitter_ms));
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Listening);
    }

    /// #EXP-1992: the call session's opening line lands after the ring's
    /// earliest answer, while `ready` waits for it: the pickup says it (the
    /// app panicked here: the pickup updated the speaker from inside its
    /// own update).
    #[gpui::test]
    async fn an_opening_after_the_rings_end_is_said_at_the_pickup(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        let id = rung(cx);
        cx.run_until_parked();
        rig.worker(etalk::Event::CallListening { id: id.clone() }, cx);
        cx.update(|cx| global(cx).unwrap().update(cx, |this, cx| this.on_rang(&id, cx)));
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Ringing, "rang: it waits for the opening line");
        rig.requests();
        rig.call_message("o1", "Hi Danny, the build is green.", 1, cx);
        assert_eq!(rig.spoken(), ["o1"]);
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Speaking);
    }

    /// #EXP-1936: while the call still rings no reply is the call's: the
    /// answer to a message typed before it is not read aloud (a Speak would
    /// end the ring tone, and the greeting would cut it off).
    #[gpui::test]
    async fn a_reply_during_the_ring_is_not_the_calls(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        let id = rung(cx);
        rig.message("typed", "agent", "The answer to what you typed.", 5, cx);
        assert!(rig.spoken().is_empty(), "nothing is said during the ring");
        assert_eq!(cx.update(|cx| (agent_id(cx), crate::speak_replies::speaking(cx))), (None, None));
        assert_eq!(cx.update(|cx| phase(cx)), CallPhase::Ringing);
        answered(&rig, &id, cx);
        assert!(rig.spoken().is_empty(), "the typed answer stays unsaid");
        assert_eq!(cx.update(|cx| agent_id(cx)).as_deref(), Some(AGENT), "answered: its replies are the call's");
    }

    /// #EXP-1936: a settings change that takes the worker away (the
    /// Assistant removed on another client) ends the call like a worker
    /// that died: a requested shutdown sends no `Exited`.
    #[gpui::test]
    async fn a_settings_change_that_takes_the_worker_ends_the_call(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        let id = rung(cx);
        answered(&rig, &id, cx);
        cx.update(|cx| crate::set_settings(etalk::voice::VoiceSettings { agent_id: None, ..settings() }, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(!active(cx), "no call without a worker");
            assert_eq!(global(cx).unwrap().read(cx).toasts, vec![VOICE_CALL.copy.failed["dropped"].clone()]);
        });
    }

    /// #EXP-1936: a ring that reaches a desktop already in a call (a local
    /// one, which the relay does not know) is answered `busy`: joined, the
    /// `failed` event once the caller is on, then hung up. The running call
    /// goes on.
    #[gpui::test]
    async fn a_ring_during_a_call_is_refused_as_busy(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rung(cx);
        let (caller, link) = rig.incoming_call();
        caller.send(CallLinkEvent::Ready).unwrap();
        cx.update(|cx| on_ring("c-2".into(), AGENT.into(), cx));
        cx.run_until_parked();
        assert_eq!(*link.borrow(), [format!("event {}", host_failed_event("busy")), "end".to_string()]);
        cx.update(|cx| assert!(active(cx) && phase(cx) == CallPhase::Ringing, "the call here goes on"));
    }

    /// #EXP-1981: a rung call asks for its call session with the RING'S
    /// call id (it was an empty one, a 400, on every call), once the relay
    /// admitted this desktop: a sealed call only after its first frame
    /// opened, when its key source has adopted (or created) the chat. Its
    /// utterances go into that session and its hang-up is told. A call run
    /// here asks with no call id at all.
    #[gpui::test]
    async fn a_rung_call_asks_for_its_session_with_the_rings_call_id(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        let (caller, _link) = rig.incoming_call();
        cx.update(|cx| on_ring("c-1".into(), AGENT.into(), cx));
        cx.run_until_parked();
        assert!(rig.session_asks().is_empty(), "nothing before the caller is on the line");
        caller.send(CallLinkEvent::Ready).unwrap();
        cx.run_until_parked();
        assert_eq!(rig.session_asks(), [(AGENT.to_string(), Some("c-1".to_string()))]);
        let id = cx.update(|cx| {
            let this = global(cx).unwrap().read(cx);
            let active = this.active.as_ref().unwrap();
            assert_eq!(active.session.as_deref(), Some("call-s"));
            active.id.clone()
        });
        answered(&rig, &id, cx);
        cx.executor().advance_clock(Duration::from_millis(VOICE_CALL.call_session.opening_wait_ms));
        cx.run_until_parked();
        rig.worker(etalk::Event::Said { id: id.clone(), text: "hello".into() }, cx);
        assert_eq!(rig.call_log().0, ["call-s"], "the utterance goes into the call session");
        caller.send(CallLinkEvent::Ended { reason: None }).unwrap();
        cx.run_until_parked();
        assert_eq!(rig.call_log().1, ["call-s"], "the hang-up is told");
        rung(cx);
        cx.run_until_parked();
        assert_eq!(rig.session_asks(), [(AGENT.to_string(), None)], "a local call names no call id");
    }

    /// #EXP-1981: a call that ends while its session is still asked: the
    /// session that answers afterwards hears the hang-up (it would otherwise
    /// read as running and the next call would resume it).
    #[gpui::test]
    async fn a_session_that_answers_after_the_hang_up_hears_it(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        cx.update(|cx| {
            start(AGENT.into(), cx);
            end(cx);
        });
        cx.run_until_parked();
        assert_eq!(rig.session_asks(), [(AGENT.to_string(), None)]);
        assert_eq!(rig.call_log().1, ["call-s"]);
    }

    /// #EXP-1982: a ring the host refused (a clear call into an encrypted
    /// chat, no key holder here) never begins: no call, no worker, no call
    /// session; the caller gets `failed {unavailable}` once it is on the
    /// line, then the hang-up.
    #[gpui::test]
    async fn a_refused_ring_fails_unavailable_and_never_begins(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        rig.with_call_session("call-s");
        rig.requests();
        let (caller, link) = rig.incoming_refused_call("unavailable");
        cx.update(|cx| on_ring("c-1".into(), AGENT.into(), cx));
        cx.run_until_parked();
        caller.send(CallLinkEvent::Ready).unwrap();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert_eq!(*link.borrow(), [format!("event {}", host_failed_event("unavailable")), "end".to_string()]);
        cx.update(|cx| assert!(!active(cx), "the call never began"));
        assert!(rig.session_asks().is_empty(), "no call session for it");
        assert!(rig.requests().iter().all(|request| !matches!(request, etalk::Request::Call { .. })), "nothing listens");
    }

    /// #EXP-1982 (backstop): a send refused while this desktop has no key
    /// holder can never land, nor can any later one: the call ends
    /// `dropped` (the caller's toast) instead of listening on. Any other
    /// failed send keeps the call and toasts the line not sent here.
    #[gpui::test]
    async fn a_send_that_can_never_be_stored_ends_the_call(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        let (caller, link) = rig.incoming_call();
        cx.update(|cx| on_ring("c-1".into(), AGENT.into(), cx));
        cx.run_until_parked();
        caller.send(CallLinkEvent::Ready).unwrap();
        cx.run_until_parked();
        let id = cx.update(|cx| global(cx).unwrap().read(cx).active.as_ref().unwrap().id.clone());
        answered(&rig, &id, cx);
        rig.fail_sends(crate::host::SendError { message: "HTTP 400: not sent".into(), refused: true, unstorable: false });
        rig.worker(etalk::Event::Said { id: id.clone(), text: "hello".into() }, cx);
        cx.update(|cx| assert!(active(cx), "a refusal another desktop could pass: the call goes on"));
        link.borrow_mut().clear();
        rig.fail_sends(crate::host::SendError { message: "HTTP 400: no key".into(), refused: true, unstorable: true });
        rig.worker(etalk::Event::Said { id: id.clone(), text: "anyone there?".into() }, cx);
        cx.update(|cx| assert!(!active(cx), "the call ended"));
        let sent = link.borrow().clone();
        let failed = sent.iter().position(|line| *line == format!("event {}", host_failed_event("dropped")));
        assert!(failed.is_some_and(|at| sent[at + 1..] == ["end".to_string()]), "the caller's toast, then the hang-up: {sent:?}");
    }

    /// #EXP-1941: the ring names its agent to the host (the ticket vouches
    /// for it), a refusal joins refuse-only (no key source), and a call id
    /// that got its ticket is not answered again.
    #[gpui::test]
    async fn a_ring_names_its_agent_and_a_call_id_is_answered_once(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(settings(), cx);
        let (caller, link) = rig.incoming_call();
        cx.update(|cx| on_ring("c-1".into(), AGENT.into(), cx));
        cx.run_until_parked();
        assert_eq!(rig.connects(), [("c-1".to_string(), AGENT.to_string(), false)]);
        caller.send(CallLinkEvent::Ready).unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert!(active(cx), "the rung call began"));
        // A ring during it is refused, refuse-only.
        rig.incoming_call();
        cx.update(|cx| on_ring("c-2".into(), AGENT.into(), cx));
        cx.run_until_parked();
        assert_eq!(rig.connects(), [("c-2".to_string(), AGENT.to_string(), true)]);
        // The caller hangs up; the relay rings the same vouched id again.
        caller.send(CallLinkEvent::Ended { reason: None }).unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert!(!active(cx)));
        assert!(link.borrow().iter().all(|line| line != "end"), "the caller ended it");
        rig.incoming_call();
        cx.update(|cx| on_ring("c-1".into(), AGENT.into(), cx));
        cx.run_until_parked();
        assert!(rig.connects().is_empty(), "no ticket is asked for a call id answered before");
        cx.update(|cx| assert!(!active(cx)));
    }

    /// #EXP-1936 (voice-call.json `wire.control.call_event`): the dialling
    /// desktop steps only what a host forwards. `start`, `mute`, `unmute`
    /// and `speaker` are this side's own; `end` and `hungUp` end the call
    /// for real (never a hidden call with an open microphone).
    #[gpui::test]
    async fn a_dialling_desktop_steps_only_what_a_host_forwards(cx: &mut gpui::TestAppContext) {
        setup(cx);
        let (id, recorder) = dialled(cx);
        cx.update(|cx| {
            host_event(&id, serde_json::json!({ "t": "rang" }), cx);
            host_event(&id, serde_json::json!({ "t": "ready" }), cx);
            let before = global(cx).unwrap().read(cx).state.clone();
            assert_eq!(before.phase, CallPhase::Listening);
            for foreign in [
                serde_json::json!({ "t": "start", "device": "elsewhere" }),
                serde_json::json!({ "t": "mute" }),
                serde_json::json!({ "t": "unmute" }),
                serde_json::json!({ "t": "speaker", "on": true }),
            ] {
                host_event(&id, foreign.clone(), cx);
                assert_eq!(global(cx).unwrap().read(cx).state, before, "{foreign} is not the host's to send");
            }
            host_event(&id, serde_json::json!({ "t": "end" }), cx);
            assert!(!active(cx), "`end` ends the call");
            assert_eq!(phase(cx), CallPhase::Hidden);
        });
        assert_eq!(*recorder.0.borrow(), ["end"], "and hangs up on the host");
        let (id, recorder) = dialled(cx);
        cx.update(|cx| {
            host_event(&id, serde_json::json!({ "t": "hungUp", "reason": "idle" }), cx);
            assert!(!active(cx), "`hungUp` ends the call");
            assert_eq!(global(cx).unwrap().read(cx).toasts, vec![VOICE_CALL.copy.failed["dropped"].clone()]);
        });
        assert_eq!(*recorder.0.borrow(), ["end"]);
    }

    /// F41: a host that fails the call names the code before `call_end`.
    #[test]
    fn a_failed_call_tells_the_caller_its_code() {
        assert_eq!(host_failed_event("unavailable"), serde_json::json!({ "t": "failed", "code": "unavailable" }));
        assert_eq!(host_failed_event("meeting")["t"], "failed");
    }
}
