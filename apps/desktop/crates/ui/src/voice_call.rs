//! NEW-55 (voice V4): CALLER MODE on the voice device's IDE
//! (`fixtures/voice-call.json`; the machine is `domain::voice_call`).
//!
//! A call = a conversation with ONE agent by voice. It starts from the
//! telephone in the agent chat's header (`agent_screen`), from the tray's
//! `Call {agent}`, or, streamed from a phone or web, from the relay's
//! `call_ring` on this device's control socket (the CALL HOST,
//! `steer::call_host`). The speech worker listens without a key (`Call`):
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
//! with `ring_ms`, a local call's ring tone on its one output stream) and
//! confirms with `CallListening` (= `Ready`); the ring's `ringMs` timer =
//! `Rang`; both = answered: the device says `callGreeting(language)`
//! through the worker's `Speak` ([`speak_replies::speak_greeting`], never an
//! agent message), its end listens. No `CallListening` within `maxRingMs`
//! (or a failed download) fails it `unavailable`; a voice still downloading
//! keeps ringing. MIC CHECK: no `Level` ≥ `noSoundLevel` from
//! `CallListening` until the ring's end (at least 1.5 s of listening) steps
//! `NoSound`. The surfaces: the in-window CALL PANEL over the agent chat's
//! body ([`CallPanel`]) and, while no IDE window is focused, the FLOATING
//! PILL (`push_to_talk::overlay`, [`Face::Call`]); both read [`face`]. The
//! live level and the timer repaint only them ([`CallLive`]).
//!
//! [`speak_replies::speak_greeting`]: crate::speak_replies::speak_greeting

use std::time::{Duration, Instant};

use gpui::{
    div, App, AppContext as _, Context, Entity, FocusHandle, Focusable, Global, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window,
};
use sync::Store;

use domain::voice::UtteranceTarget;
use domain::voice_call::{
    call_step, call_timer, call_unavailable, call_view, CallAvailabilityInput, CallEvent, CallPhase, CallPlatform, CallSpeech, CallState,
    VOICE_CALL,
};

use crate::push_to_talk::overlay::{Face, Overlay};
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
    /// A streamed call's socket (None = this device's own microphone).
    host: Option<steer::call_host::CallHost>,
    /// Utterances said while the agent's turn ran (the queue bar's share).
    queued: usize,
    /// The reply being spoken (its message id, or the greeting's).
    speaking: Option<String>,
    /// S-3: the greeting's `Speak` id.
    greeting_id: String,
    started: Instant,
    /// S-3: when the worker confirmed it listens.
    listening_at: Option<Instant>,
    /// S-3: a `Level` reached `noSoundLevel` since `CallListening`.
    heard: bool,
    /// S-3: when rang + ready answered it (the timer counts from here).
    answered_at: Option<Instant>,
}

/// S-3: what changes many times a second (the microphone's level, the
/// timer's second): only the call surfaces observe it.
#[derive(Default)]
pub(crate) struct CallLive {
    level: f32,
    bands: Vec<f32>,
    timer: String,
}

pub(crate) struct VoiceCall {
    state: CallState,
    active: Option<Active>,
    overlay: Overlay,
    live: Entity<CallLive>,
    /// The pill is open (no IDE window focused).
    pill: bool,
    counter: u64,
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
pub fn install(cx: &mut App) {
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
            #[cfg(test)]
            toasts: Vec::new(),
        }
    });
    cx.set_global(VoiceCallGlobal(entity));
}

/// The controller (the tray observes it).
pub(crate) fn entity(cx: &App) -> Option<Entity<VoiceCall>> {
    global(cx)
}

/// A call runs on this device.
pub(crate) fn active(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).active.is_some())
}

/// The agent of the running call (Speak replies speaks its conversation).
pub(crate) fn agent_id(cx: &App) -> Option<String> {
    global(cx).and_then(|entity| entity.read(cx).active.as_ref().map(|active| active.agent_id.clone()))
}

/// S-3: THIS client runs a call with `agent_id` (its telephone shows the
/// panel instead of dialing).
pub(crate) fn runs_for(agent_id: &str, cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).active.as_ref().is_some_and(|active| active.agent_id == agent_id))
}

/// S-3: what the call surfaces show now (None = no call).
pub(crate) fn face(cx: &App) -> Option<CallFace> {
    global(cx)?.read(cx).face_now(cx)
}

impl VoiceCall {
    /// [`face`] from inside the controller (the pill is handed it).
    fn face_now(&self, cx: &App) -> Option<CallFace> {
        let active = self.active.as_ref()?;
        let view = call_view(&self.state, CallPlatform::Desktop)?;
        let agent = crate::session_data::agent_def(&active.agent_id, cx);
        let live = self.live.read(cx);
        let (level, bands) = match view.activity {
            "listening" => (live.level, live.bands.clone()),
            // The reply's own loudness is not reported: a steady wave.
            "speaking" => (0.5, Vec::new()),
            _ => (0., Vec::new()),
        };
        Some(CallFace {
            agent: SharedString::from(agent.as_ref().map(crate::agent_screen::agent_def_name).unwrap_or_default()),
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

/// The member's voice device as THIS client sees it: its name, whether it
/// is online and whether it is this machine.
fn voice_device(cx: &App) -> (Option<String>, bool, bool) {
    let settings = crate::speak_replies::settings(cx);
    let Some(device_id) = settings.voice_device_id.clone() else { return (None, false, false) };
    let own = steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
    let row = Store::try_global(cx).and_then(|store| {
        store.collections().devices.read(cx).iter().find(|row| row.device_id.as_deref() == Some(device_id.as_str())).cloned()
    });
    let name = row.as_ref().and_then(|row| row.label.clone()).unwrap_or_else(|| device_id.clone());
    let here = device_id == own;
    // This machine is online by definition; another one by its presence.
    let online = here || row.as_ref().is_some_and(|row| crate::device_settings::row_is_online(row.last_seen_at.as_deref(), chrono::Utc::now().timestamp_millis()));
    (Some(name), online, here)
}

/// None = the telephone is enabled; Some = disabled, with the reason
/// (voice-call.json `availabilityCases`, the desktop's view). S-3: a voice
/// still downloading rings until it is ready.
pub(crate) fn unavailable(cx: &App) -> Option<String> {
    let (device, online, here) = voice_device(cx);
    let speech = crate::speak_replies::speech_state(cx).map(|state| match state {
        coding::device_doctor::SpeechState::Ready => CallSpeech::Ok,
        coding::device_doctor::SpeechState::Downloading => CallSpeech::Preparing,
        coding::device_doctor::SpeechState::NoGpu => CallSpeech::NoGpu,
    });
    call_unavailable(&CallAvailabilityInput {
        platform: CallPlatform::Desktop,
        device: device.as_deref(),
        online,
        here,
        speech,
        busy: active(cx),
        // #NEW-57: a meeting runs on the voice device.
        meeting: crate::speak_replies::settings(cx).meeting(),
    })
}

/// The telephone (header) or the tray's Call: call `agent_id` here.
pub(crate) fn start(agent_id: String, cx: &mut App) {
    if let Some(reason) = unavailable(cx) {
        crate::toast::show_in_active_window(crate::toast::Toast::error(reason), cx);
        return;
    }
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| this.begin(agent_id, None, cx));
}

/// The surfaces' mute button.
pub(crate) fn toggle_mute(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| {
            let muted = !this.state.muted;
            this.set_muted(muted, cx);
        });
    }
}

/// End call / Cancel (the surfaces' red button, Esc, the tray, sign-out,
/// a settings change).
pub(crate) fn end(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.hang_up(true, cx));
    }
}

/// The relay rang this device (`call_ring` on the control socket): a phone
/// or web caller streams a call to `agent_id`. Busy or unable to speak =
/// no answer (the relay tells the caller). S-3: a voice still downloading
/// answers too (the call rings until it is ready).
pub(crate) fn on_ring(call_id: String, agent_id: String, cx: &mut App) {
    let (_, _, here) = voice_device(cx);
    // #NEW-57: a call reaching the device mid-meeting fails with
    // `failed.meeting` (the caller's telephone should have been off).
    if here && !active(cx) && crate::speak_replies::settings(cx).meeting() {
        refuse_for_meeting(call_id, cx);
        return;
    }
    let speech = crate::speak_replies::speech_state(cx);
    let can_speak = matches!(speech, Some(coding::device_doctor::SpeechState::Ready | coding::device_doctor::SpeechState::Downloading));
    if !here || active(cx) || !can_speak {
        log::info!("[voice] call {call_id}: not answered here");
        return;
    }
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    let Some(runtime) = crate::steer_wiring::runtime(cx) else { return };
    let Some(entity) = global(cx) else { return };
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        let ticket = cx.background_executor().spawn(async move { api::voice::voice_call_host_ticket(&trpc, &call_id) }).await;
        let ticket = match ticket {
            Ok(ticket) => ticket,
            Err(err) => {
                log::warn!("[voice] the call host ticket was refused: {err}");
                return;
            }
        };
        let (tx, rx) = flume::unbounded::<steer::call_host::CallHostEvent>();
        let host = steer::call_host::connect(&runtime, ticket.url, move |event| {
            let _ = tx.send(event);
        });
        let started = weak.update(cx, |this, cx| {
            if this.active.is_some() {
                host.end();
                return false;
            }
            this.begin(agent_id, Some(host), cx);
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

/// The `call_event` a host sends before hanging up on a call it failed
/// (voice-call.json `wire.control`): the caller steps it as `failed`.
fn host_failed_event(code: &str) -> serde_json::Value {
    serde_json::json!({ "t": "failed", "code": code })
}

/// #NEW-57: join the call only to fail it as `meeting` (a `failed` flow
/// event the caller steps), then hang up.
fn refuse_for_meeting(call_id: String, cx: &mut App) {
    log::info!("[voice] call {call_id}: refused, a meeting runs");
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    let Some(runtime) = crate::steer_wiring::runtime(cx) else { return };
    cx.spawn(async move |cx| {
        let ticket = cx.background_executor().spawn(async move { api::voice::voice_call_host_ticket(&trpc, &call_id) }).await;
        let Ok(ticket) = ticket else { return };
        let (tx, rx) = flume::unbounded::<steer::call_host::CallHostEvent>();
        let host = steer::call_host::connect(&runtime, ticket.url, move |event| {
            let _ = tx.send(event);
        });
        let started = std::time::Instant::now();
        while started.elapsed() < ring_timeout() {
            match rx.try_recv() {
                Ok(steer::call_host::CallHostEvent::Ready) => {
                    host.send_event(host_failed_event("meeting"));
                    break;
                }
                Ok(steer::call_host::CallHostEvent::Ended { .. } | steer::call_host::CallHostEvent::Dropped(_))
                | Err(flume::TryRecvError::Disconnected) => break,
                Ok(_) | Err(flume::TryRecvError::Empty) => {
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                }
            }
        }
        host.end();
    })
    .detach();
}

/// A speech-worker event that belongs to the call (routed here before
/// push to talk and Speak replies see it).
pub(crate) fn owns(event: &etalk::Event, cx: &App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let this = entity.read(cx);
    let Some(active) = this.active.as_ref() else { return false };
    match event {
        etalk::Event::Said { id, .. } | etalk::Event::SpeechStarted { id } | etalk::Event::CallListening { id } => *id == active.id,
        etalk::Event::Level { id, .. } | etalk::Event::Partial { id, .. } | etalk::Event::ListenFailed { id, .. } => *id == active.id,
        etalk::Event::SpeechAudio { .. } => active.host.is_some(),
        _ => false,
    }
}

pub(crate) fn on_worker_event(event: etalk::Event, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_worker(event, cx));
    }
}

/// Settings changed or the account went away: a call that can no longer
/// run here ends.
pub(crate) fn settings_changed(cx: &mut App) {
    if active(cx) && unavailable_ignoring_busy(cx).is_some() {
        end(cx);
    }
}

fn unavailable_ignoring_busy(cx: &App) -> Option<String> {
    let (device, online, here) = voice_device(cx);
    call_unavailable(&CallAvailabilityInput {
        platform: CallPlatform::Desktop,
        device: device.as_deref(),
        online,
        here,
        speech: Some(CallSpeech::Ok),
        busy: false,
        meeting: crate::speak_replies::settings(cx).meeting(),
    })
}

/// The flow event as the caller receives it (`call_event`): S-3 the ring's
/// `rang`, the worker's `ready` and the mic check's `noSound` too, so caller
/// and host show the same ring.
fn wire_event(event: &CallEvent) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match event {
        CallEvent::Rang => json!({ "t": "rang" }),
        CallEvent::Ready => json!({ "t": "ready" }),
        CallEvent::NoSound => json!({ "t": "noSound" }),
        CallEvent::Partial(text) => json!({ "t": "partial", "text": text }),
        CallEvent::Said(text) => json!({ "t": "said", "text": text }),
        CallEvent::SpeechStarted => json!({ "t": "speechStarted" }),
        CallEvent::Queued { count, text } => json!({ "t": "queued", "count": count, "text": text }),
        CallEvent::ReplyStarted(text) => json!({ "t": "replyStarted", "text": text }),
        CallEvent::ReplyFinished => json!({ "t": "replyFinished" }),
        _ => return None,
    })
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
        // The telephones (busy) and the tray's Call follow a call's start
        // and end.
        if (before.phase == CallPhase::Hidden) != (self.state.phase == CallPhase::Hidden) {
            cx.refresh_windows();
        }
        if let Some(toast) = self.state.toast.take() {
            #[cfg(test)]
            self.toasts.push(toast.clone());
            crate::toast::show_in_active_window(crate::toast::Toast::error(toast), cx);
        }
        // S-3: rang and ready answered it: the device says its greeting.
        if before.phase == CallPhase::Ringing && self.state.phase == CallPhase::Speaking {
            self.answer(cx);
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

    fn begin(&mut self, agent_id: String, host: Option<steer::call_host::CallHost>, cx: &mut Context<Self>) {
        let Some(client) = crate::speak_replies::client(cx) else { return };
        let (device, _, _) = voice_device(cx);
        // A held push to talk and anything being said stop first.
        crate::push_to_talk::signed_out(cx);
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        let id = self.open(agent_id, host, cx);
        let remote = self.active.as_ref().is_some_and(|active| active.host.is_some());
        let language = crate::speak_replies::settings(cx).language().to_string();
        self.step(CallEvent::Start { device: device.unwrap_or_default(), language: Some(language.clone()) }, cx);
        // S-3: a LOCAL call rings on this device while the worker warms up;
        // a remote one rings on the caller (the host's `rang` follows).
        let ring_ms = VOICE_CALL.constants.ring_ms;
        client.call(id.clone(), language, remote, if remote { 0 } else { ring_ms as u32 });
        self.arm_timers(id, cx);
    }

    /// The call's bookkeeping (the worker's id, the clocks).
    fn open(&mut self, agent_id: String, host: Option<steer::call_host::CallHost>, cx: &mut Context<Self>) -> String {
        self.counter += 1;
        let id = format!("call-{}", self.counter);
        self.active = Some(Active {
            id: id.clone(),
            agent_id,
            host,
            queued: 0,
            speaking: None,
            greeting_id: format!("{id}-greeting"),
            started: Instant::now(),
            listening_at: None,
            heard: false,
            answered_at: None,
        });
        self.live.update(cx, |live, _| *live = CallLive::default());
        id
    }

    /// S-3: the ring (`rang` at `ringMs`), the give-up (`maxRingMs`
    /// without `ready` fails `unavailable`) and the tick that follows the
    /// windows and the timer.
    fn arm_timers(&mut self, id: String, cx: &mut Context<Self>) {
        let ring = Duration::from_millis(VOICE_CALL.constants.ring_ms);
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
        if self.is_call(id) {
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
        let ring_end = active.started + Duration::from_millis(VOICE_CALL.constants.ring_ms);
        let until = ring_end.max(now + MIC_CHECK_MIN);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(until.saturating_duration_since(Instant::now())).await;
            let _ = this.update(cx, |this, cx| this.mic_checked(&id, cx));
        })
        .detach();
        self.step(CallEvent::Ready, cx);
    }

    /// The mic check's end: nothing reached `noSoundLevel`.
    fn mic_checked(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_call(id) && self.active.as_ref().is_some_and(|active| !active.heard) {
            log::info!("[voice] call {id}: the microphone stayed silent");
            self.step(CallEvent::NoSound, cx);
        }
    }

    /// S-3: answered: the device says its greeting on the call's stream
    /// (Speak replies tracks it like a reply: its end listens).
    fn answer(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_mut() else { return };
        active.answered_at = Some(Instant::now());
        let greeting_id = active.greeting_id.clone();
        if !crate::speak_replies::speak_greeting(greeting_id, self.state.greeting.clone(), cx) {
            // Nothing can speak it: listen at once.
            self.step(CallEvent::ReplyFinished, cx);
        }
    }

    fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        if let Some(client) = crate::speak_replies::client(cx) {
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
        let Some(active) = self.active.take() else { return };
        if let Some(client) = crate::speak_replies::client(cx) {
            client.hangup(active.id.clone());
        }
        if active.speaking.is_some() {
            crate::speak_replies::stop(cx);
        }
        if tell_host {
            if let Some(host) = active.host.as_ref() {
                host.end();
            }
        }
        self.step(event, cx);
    }

    fn fail(&mut self, code: &str, cx: &mut Context<Self>) {
        let Some(active) = self.active.take() else { return };
        if let Some(client) = crate::speak_replies::client(cx) {
            client.hangup(active.id.clone());
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
        self.step(CallEvent::Failed(code.to_string()), cx);
    }

    fn on_host(&mut self, event: steer::call_host::CallHostEvent, cx: &mut Context<Self>) {
        use steer::call_host::CallHostEvent;
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
        let busy = matches!(self.state.phase, CallPhase::Thinking | CallPhase::Speaking);
        let agent_id = active.agent_id.clone();
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

    fn send(&mut self, agent_id: String, text: String, cx: &mut Context<Self>) {
        let conversation = crate::session_data::my_conversation(&agent_id, cx).map(|row| row.id);
        let team_id = Store::try_global(cx)
            .and_then(|store| store.collections().agents.read(cx).iter().find(|row| row.id == agent_id).and_then(|row| row.team_id.clone()))
            .unwrap_or_default();
        let Some(target) = domain::voice::utterance_input(&text, &team_id, Some(&agent_id), conversation.as_deref()) else { return };
        let input = match target {
            UtteranceTarget::Session { session_id, text } => api::sessions::SendInput::message(&session_id, &text, Vec::new()),
            UtteranceTarget::Agent { team_id, agent_id, text } => crate::chat_launch::agent_send_input(&team_id, &agent_id, &text, Vec::new()),
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |_, cx| {
            let result = cx.background_executor().spawn(async move { api::sessions::send(&trpc, &input) }).await;
            if let Err(err) = result {
                // The call goes on; the words are in the log, never lost
                // silently: a toast says the line did not go out.
                log::warn!("[voice] call: the send failed: {err}");
                let _ = cx.update(|cx| {
                    crate::toast::show_in_active_window(
                        crate::toast::Toast::error(domain::voice::VOICE.ptt_copy.overlay_not_sent.clone()),
                        cx,
                    )
                });
            }
        })
        .detach();
    }

    /// Speak replies started or ended a reply (or the greeting): the
    /// call's Speaking follows (only for the call's own conversation).
    fn on_speaker(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        let conversation = crate::session_data::my_conversation(&active.agent_id, cx).map(|row| row.id);
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

/// How long a ring may wait for the ticket before the relay gives up
/// (voice-call.json `constants.ringTimeoutMs`), for the log.
fn ring_timeout() -> Duration {
    Duration::from_millis(VOICE_CALL.constants.ring_timeout_ms)
}

/// S-3: the in-window CALL PANEL over an agent chat's body (the agent
/// screen holds one): drawn while THIS client runs a call with that agent,
/// nothing otherwise. Esc in it ends the call; the header's telephone
/// focuses it. It repaints on the call's steps and its live level/timer,
/// never the chat around it.
pub(crate) struct CallPanel {
    agent_id: Option<String>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl CallPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(call) = global(cx) {
            let live = call.read(cx).live.clone();
            subscriptions.push(cx.observe(&call, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&live, |_, _, cx| cx.notify()));
        }
        CallPanel { agent_id: None, focus: cx.focus_handle(), _subscriptions: subscriptions }
    }

    /// The chat's agent (its screen resolves it).
    pub(crate) fn set_agent(&mut self, agent_id: Option<String>, cx: &mut Context<Self>) {
        if self.agent_id != agent_id {
            self.agent_id = agent_id;
            cx.notify();
        }
    }

    /// The panel shows (a call with this chat's agent runs here).
    pub(crate) fn shown(&self, cx: &App) -> bool {
        self.agent_id.as_deref().is_some_and(|agent_id| runs_for(agent_id, cx))
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
        let Some(face) = face else { return div().into_any_element() };
        let actions = CallActions { on_mute: std::rc::Rc::new(toggle_mute), on_end: std::rc::Rc::new(end) };
        div()
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_key_down(|event, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    end(cx);
                }
            })
            .child(crate::voice_overlay::call_panel(SharedString::from(VOICE_CALL.test_ids.panel.clone()), &face, &actions, cx))
            .into_any_element()
    }
}

/// A call as if it had started (no worker, no host), ringing, for the
/// lifecycle tests.
#[cfg(test)]
fn ring_for_test(agent_id: &str, cx: &mut App) -> String {
    let Some(entity) = global(cx) else { return String::new() };
    entity.update(cx, |this, cx| {
        let id = this.open(agent_id.into(), None, cx);
        this.step(CallEvent::Start { device: "mint".into(), language: Some("en".into()) }, cx);
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
            theme::init(cx);
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

    /// A settings change that leaves no voice device here (another device
    /// picked, or none) ends the call.
    #[gpui::test]
    async fn a_voice_device_change_hangs_up(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            assert!(unavailable_ignoring_busy(cx).is_some(), "no voice device in the test app");
            settings_changed(cx);
            assert!(!active(cx));
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
            assert_eq!(face.view.buttons, vec![domain::voice_call::CallButton::Mute, domain::voice_call::CallButton::End]);
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

    /// A streamed call (a detached host socket) and the caller's `call_end`.
    fn host_ended(reason: Option<&str>, cx: &mut gpui::TestAppContext) -> Vec<String> {
        running(cx);
        cx.update(|cx| {
            let entity = global(cx).unwrap();
            entity.update(cx, |this, cx| {
                this.active.as_mut().unwrap().host = Some(steer::call_host::CallHost::detached());
                this.on_host(steer::call_host::CallHostEvent::Ended { reason: reason.map(str::to_string) }, cx);
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
        assert!(wire_event(&CallEvent::Start { device: "mint".into(), language: None }).is_none());
    }

    /// F41: a host that fails the call names the code before `call_end`.
    #[test]
    fn a_failed_call_tells_the_caller_its_code() {
        assert_eq!(host_failed_event("unavailable"), serde_json::json!({ "t": "failed", "code": "unavailable" }));
        assert_eq!(host_failed_event("meeting")["t"], "failed");
    }
}
