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
//! in the worker (barge-in), and THE capsule (`push_to_talk::overlay`,
//! [`Face::Call`]) shows each step with Mute and End call. A streamed call
//! plays nothing here: the worker's reply audio goes to the caller, and
//! every flow event follows it as `call_event`.

use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};
use sync::Store;

use domain::voice::UtteranceTarget;
use domain::voice_call::{call_step, call_unavailable, call_view, CallAvailabilityInput, CallEvent, CallPhase, CallPlatform, CallSpeech, CallState};

use crate::push_to_talk::overlay::{Face, Overlay};

/// The call running now.
struct Active {
    /// The worker's call id.
    id: String,
    agent_id: String,
    /// A streamed call's socket (None = this device's own microphone).
    host: Option<steer::call_host::CallHost>,
    /// Utterances said while the agent's turn ran (the queue bar's share).
    queued: usize,
    /// The reply being spoken (its message id).
    speaking: Option<String>,
}

pub(crate) struct VoiceCall {
    state: CallState,
    active: Option<Active>,
    overlay: Overlay,
    level: f32,
    bands: Vec<f32>,
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
    let entity = cx.new(|cx: &mut Context<VoiceCall>| {
        if let Some(speaker) = crate::speak_replies::entity(cx) {
            cx.observe(&speaker, |this: &mut VoiceCall, _, cx| this.on_speaker(cx)).detach();
        }
        VoiceCall {
            state: CallState::default(),
            active: None,
            overlay: Overlay::default(),
            level: 0.,
            bands: Vec::new(),
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

/// The wave's level now (the overlay reads it each frame).
pub(crate) fn level(cx: &App) -> (f32, Vec<f32>) {
    global(cx).map(|entity| {
        let this = entity.read(cx);
        (this.level, this.bands.clone())
    })
    .unwrap_or_default()
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
/// (voice-call.json `availabilityCases`, the desktop's view).
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
        speak_replies: crate::speak_replies::settings(cx).speak_replies,
        online,
        here,
        speech,
        busy: active(cx),
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

/// The capsule's lead pill.
pub(crate) fn toggle_mute(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| {
            let muted = !this.state.muted;
            this.set_muted(muted, cx);
        });
    }
}

/// End call (the capsule's primary pill, sign-out, a settings change).
pub(crate) fn end(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.hang_up(true, cx));
    }
}

/// The relay rang this device (`call_ring` on the control socket): a phone
/// or web caller streams a call to `agent_id`. Busy or unable to speak =
/// no answer (the relay tells the caller).
pub(crate) fn on_ring(call_id: String, agent_id: String, cx: &mut App) {
    let (_, _, here) = voice_device(cx);
    if !here || active(cx) || crate::speak_replies::speech_state(cx) != Some(coding::device_doctor::SpeechState::Ready) {
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

/// A speech-worker event that belongs to the call (routed here before
/// push to talk and Speak replies see it).
pub(crate) fn owns(event: &talk::Event, cx: &App) -> bool {
    let Some(entity) = global(cx) else { return false };
    let this = entity.read(cx);
    let Some(active) = this.active.as_ref() else { return false };
    match event {
        talk::Event::Said { id, .. } | talk::Event::SpeechStarted { id } => *id == active.id,
        talk::Event::Level { id, .. } | talk::Event::Partial { id, .. } | talk::Event::ListenFailed { id, .. } => *id == active.id,
        talk::Event::SpeechAudio { .. } => active.host.is_some(),
        _ => false,
    }
}

pub(crate) fn on_worker_event(event: talk::Event, cx: &mut App) {
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
        speak_replies: crate::speak_replies::settings(cx).speak_replies,
        online,
        here,
        speech: Some(CallSpeech::Ok),
        busy: false,
    })
}

/// The flow event as the caller receives it (`call_event`).
fn wire_event(event: &CallEvent) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match event {
        CallEvent::Partial(text) => json!({ "t": "partial", "text": text }),
        CallEvent::Said(text) => json!({ "t": "said", "text": text }),
        CallEvent::SpeechStarted => json!({ "t": "speechStarted" }),
        CallEvent::Queued { count, text } => json!({ "t": "queued", "count": count, "text": text }),
        CallEvent::ReplyStarted(text) => json!({ "t": "replyStarted", "text": text }),
        CallEvent::ReplyFinished => json!({ "t": "replyFinished" }),
        _ => return None,
    })
}

impl VoiceCall {
    fn step(&mut self, event: CallEvent, cx: &mut Context<Self>) {
        if let (Some(event), Some(host)) = (wire_event(&event), self.active.as_ref().and_then(|active| active.host.as_ref())) {
            host.send_event(event);
        }
        let was_hidden = self.state.phase == CallPhase::Hidden;
        self.state = call_step(&self.state, &event);
        // The telephones (busy) and the tray's Call follow a call's start
        // and end.
        if was_hidden != (self.state.phase == CallPhase::Hidden) {
            cx.refresh_windows();
        }
        if let Some(toast) = self.state.toast.take() {
            #[cfg(test)]
            self.toasts.push(toast.clone());
            crate::toast::show_in_active_window(crate::toast::Toast::error(toast), cx);
        }
        let view = call_view(&self.state);
        self.overlay.show(view.map(Face::Call), cx);
        cx.notify();
    }

    fn begin(&mut self, agent_id: String, host: Option<steer::call_host::CallHost>, cx: &mut Context<Self>) {
        let Some(client) = crate::speak_replies::client(cx) else { return };
        let (device, _, _) = voice_device(cx);
        // A held push to talk and anything being said stop first.
        crate::push_to_talk::signed_out(cx);
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        self.counter += 1;
        let id = format!("call-{}", self.counter);
        let remote = host.is_some();
        let language = crate::speak_replies::settings(cx).language().to_string();
        self.active = Some(Active { id: id.clone(), agent_id, host, queued: 0, speaking: None });
        self.level = 0.;
        self.bands.clear();
        self.step(CallEvent::Start { device: device.unwrap_or_default() }, cx);
        client.call(id, language, remote);
        // A local call is connected at once; a streamed one on `call_ready`
        // (which may already have come: the caller waits on the relay).
        if !remote {
            self.step(CallEvent::Connected, cx);
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
        if let Some(host) = active.host.as_ref() {
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
            CallHostEvent::Ready => self.step(CallEvent::Connected, cx),
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

    fn on_worker(&mut self, event: talk::Event, cx: &mut Context<Self>) {
        match event {
            talk::Event::Level { level, bands, .. } => {
                self.level = level;
                self.bands = bands.to_vec();
                self.overlay.refresh(cx);
            }
            talk::Event::Partial { text, .. } => self.step(CallEvent::Partial(text), cx),
            talk::Event::SpeechStarted { .. } => {
                if let Some(host) = self.active.as_ref().and_then(|active| active.host.as_ref()) {
                    host.flush();
                }
                self.step(CallEvent::SpeechStarted, cx);
            }
            talk::Event::Said { text, .. } => self.said(text, cx),
            talk::Event::SpeechAudio { pcm, rate, .. } => {
                let Some(host) = self.active.as_ref().and_then(|active| active.host.as_ref()) else { return };
                let Ok(samples) = talk::decode_pcm(&pcm) else { return };
                host.send_speech(samples, rate);
            }
            // NEW-55 review: the worker died: the call cannot go on.
            talk::Event::Exited if self.active.is_some() => {
                log::warn!("[voice] the speech worker exited during a call");
                self.fail("dropped", cx);
            }
            talk::Event::ListenFailed { reason, detail, .. } => {
                log::warn!("[voice] the call could not listen: {detail}");
                let code = match reason {
                    talk::ListenFailure::NoMicrophone => "no_microphone",
                    talk::ListenFailure::NotGranted => "not_granted",
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
        // #NEW-79: "yes", "take two", "reject one" answer the conversation's
        // waiting proposal cards, as with push to talk.
        if let Some(answers) = conversation.as_deref().and_then(|session_id| crate::push_to_talk::card_answers(session_id, &text, cx)) {
            let Some(trpc) = crate::queries::trpc_client(cx) else { return };
            cx.spawn(async move |_, cx| {
                let result = cx.background_executor().spawn(async move { crate::push_to_talk::post_card_answers(&trpc, &answers) }).await;
                if let Err(err) = result {
                    log::warn!("[voice] call: answering a card failed: {err}");
                    let _ = cx.update(|cx| {
                        crate::toast::show_in_active_window(
                            crate::toast::Toast::error(domain::voice::VOICE.ptt_copy.overlay_not_sent.clone()),
                            cx,
                        )
                    });
                }
            })
            .detach();
            return;
        }
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

    /// Speak replies started or ended a reply: the call's Speaking follows
    /// (only for the call's own conversation).
    fn on_speaker(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_ref() else { return };
        let conversation = crate::session_data::my_conversation(&active.agent_id, cx).map(|row| row.id);
        let speaking = crate::speak_replies::speaking(cx).filter(|speaking| Some(&speaking.session_id) == conversation.as_ref());
        let was = active.speaking.clone();
        match (was, speaking) {
            (None, Some(speaking)) => {
                if let Some(active) = self.active.as_mut() {
                    active.speaking = Some(speaking.message_id.clone());
                    // The agent took the queued lines in with this turn.
                    active.queued = 0;
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
#[allow(dead_code)]
fn ring_timeout() -> Duration {
    Duration::from_millis(domain::voice_call::VOICE_CALL.constants.ring_timeout_ms)
}

/// A call as if it had connected (no worker, no host), for the lifecycle
/// tests.
#[cfg(test)]
fn start_for_test(agent_id: &str, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        this.active = Some(Active { id: "call-test".into(), agent_id: agent_id.into(), host: None, queued: 0, speaking: None });
        this.step(CallEvent::Start { device: "mint".into() }, cx);
        this.step(CallEvent::Connected, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            install(cx);
            start_for_test("agent-1", cx);
            assert!(active(cx));
            assert_eq!(agent_id(cx).as_deref(), Some("agent-1"));
        });
    }

    /// NEW-55 review: a crashed speech worker ends the call and clears it.
    #[gpui::test]
    async fn a_worker_crash_ends_the_call(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            assert!(!owns(&talk::Event::Exited, cx), "the dispatcher sends Exited itself");
            on_worker_event(talk::Event::Exited, cx);
            assert!(!active(cx));
            assert_eq!(global(cx).unwrap().read(cx).state.phase, CallPhase::Hidden);
        });
    }

    /// Sign-out (Speak replies' `on_session`) hangs up through `end`.
    #[gpui::test]
    async fn sign_out_hangs_up(cx: &mut gpui::TestAppContext) {
        running(cx);
        cx.update(|cx| {
            end(cx);
            assert!(!active(cx));
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
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            install(cx);
            on_worker_event(talk::Event::Exited, cx);
            assert!(!active(cx));
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
        let dropped = domain::voice_call::VOICE_CALL.copy.failed["dropped"].clone();
        assert_eq!(host_ended(Some("idle"), cx), vec![dropped]);
    }

    #[gpui::test]
    async fn the_relays_max_length_toasts_dropped(cx: &mut gpui::TestAppContext) {
        let dropped = domain::voice_call::VOICE_CALL.copy.failed["dropped"].clone();
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
        assert!(wire_event(&CallEvent::Mute).is_none());
        assert!(wire_event(&CallEvent::End).is_none());
        assert!(wire_event(&CallEvent::Start { device: "mint".into() }).is_none());
    }
}
