//! #NEW-54 (voice plan V3): push to talk on the voice device's IDE
//! (`fixtures/voice.json` PUSH TO TALK, `pttFlowCases`, `utteranceCases`).
//!
//! Only on the member's voice device with a key bound (S-3: no key = push
//! to talk off, there is no toggle) does the IDE hold the shortcut
//! ([`hotkey`]); while Settings → Voice records a new key the grab is let go
//! ([`set_recording`]) so the old binding cannot fire. A key-down asks the speech worker to Listen (which stops
//! any speech: barge-in) unless the tray's Mute is on; the release ends the
//! Listen and the worker answers Heard or Empty. Heard text, trimmed, goes
//! out as a plain message through `sessions.send`: into the member's
//! conversation with the voice agent, or to the agent while there is none
//! yet; a failed send keeps the text in that chat's composer
//! ([`take_unsent`]). The agent's reply row ends Thinking; `speak_replies`
//! reads it, and the overlay ([`overlay`], THE capsule in a never-focused
//! window) shows every step, from [`etalk::voice::ptt_step`].
//!
//! #1238 review carry: a running Listen is cancelled (CancelListen) on
//! sign-out, on a voice-device change, and on any settings change that
//! turns push to talk off or changes the agent or the language.
//!
//! #NEW-56 (voice plan V7): with the wake word on (and the tray's Mute off)
//! the worker is handed the phrase (`etalk::voice::wake_phrase`) and keeps
//! the microphone open for it; a heard phrase arrives as `Woke`, which is a
//! key-down for one turn: the same capsule, the same Heard or Empty, the
//! same send. The worker ends that turn itself; the tray draws the open
//! microphone from `WakeListening`.
//!
//! S-3 (voice.json WAKE DETAIL): [`wake_state`] says why the wake word is or
//! is not listening (the worker's `WakeListening` and its reason, the tray's
//! Mute, a call, a meeting) for the Wake word row and the doctor's `wake`.

pub(crate) mod hotkey;
pub(crate) mod overlay;

use std::collections::HashMap;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};
use sync::{Collection, Store};

use domain::rows::MessageRow;
use etalk::voice::{ptt_overlay, ptt_step, PttEvent, PttOverlay, PttPhase, PttState, UtteranceTarget, VoiceSettings};

pub(crate) use overlay::activity as overlay_activity;

/// What THIS device listens for (None = nothing listens here).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Armed {
    /// The push-to-talk shortcut (None = push to talk is off).
    shortcut: Option<String>,
    agent_id: String,
    language: String,
    /// #NEW-56: the wake word's stored phrase (None = the wake word is off;
    /// empty = `Hey {agent}`).
    wake_phrase: Option<String>,
}

fn armed_for(settings: &VoiceSettings, device_id: &str) -> Option<Armed> {
    let settings = settings.clone().clear_on_none();
    if settings.voice_device_id.as_deref() != Some(device_id) {
        return None;
    }
    // S-3: push to talk is on iff a key is bound.
    let shortcut = settings.push_to_talk().then(|| settings.ptt_shortcut.clone());
    let wake_phrase = settings.wake_word.then(|| settings.wake_phrase.clone());
    if shortcut.is_none() && wake_phrase.is_none() {
        return None;
    }
    Some(Armed { shortcut, agent_id: settings.agent_id.clone()?, language: settings.language().to_string(), wake_phrase })
}

/// NEW-78 / S-3: THIS device listens: it is the member's voice device with
/// an agent and a bound key or the wake word (after voice.json
/// `clearOnNone`). Mute shows then, and the doctor carries `microphone`.
pub(crate) fn listens_here(settings: &VoiceSettings, device_id: &str) -> bool {
    armed_for(settings, device_id).is_some()
}

/// The send this utterance made.
#[derive(Clone, Debug)]
struct Sent {
    session_id: String,
    message_id: Option<String>,
}

pub(crate) struct PushToTalk {
    device_id: String,
    client: Option<etalk::SpeechClient>,
    armed: Option<Armed>,
    grab: Option<hotkey::Grab>,
    status: hotkey::Status,
    tx: flume::Sender<hotkey::HotkeyEvent>,
    /// The key is down (any backend; repeats are ignored).
    held: bool,
    state: PttState,
    /// The Listen running now.
    utterance: Option<String>,
    counter: u64,
    /// Bumped by every new key-down: an older send's answer is ignored.
    generation: u64,
    sent: Option<Sent>,
    reply_id: Option<String>,
    level: f32,
    bands: Vec<f32>,
    hide_generation: u64,
    /// Text a failed send left for its chat's composer, by agent id.
    unsent: HashMap<String, String>,
    /// DEV-ONLY: the pill's agent name under `EXP_DEV_PTT_OVERLAY`.
    dev_agent: Option<String>,
    /// #NEW-56: the worker answered Ready and has not exited since.
    worker_ready: bool,
    /// #NEW-56: the wake phrase and language the worker was last handed
    /// (None = off).
    wake_sent: Option<(String, String)>,
    /// #NEW-56: the worker holds the microphone open for the wake word.
    wake_listening: bool,
    /// S-3: why the worker's last `WakeListening` said the microphone is not
    /// open (None = no failure reported).
    wake_reason: Option<etalk::ListenFailure>,
    /// S-3: Settings → Voice records a new key: the grab is let go.
    recording: bool,
    /// #NEW-56: the utterance in flight is a wake turn.
    woken: bool,
    /// #NEW-56: the tray's Mute, mirrored here: the tray calls in from its
    /// own update, where reading it would panic.
    muted: bool,
    overlay: overlay::Overlay,
}

struct PushToTalkGlobal(Entity<PushToTalk>);

impl Global for PushToTalkGlobal {}

fn global(cx: &App) -> Option<Entity<PushToTalk>> {
    cx.try_global::<PushToTalkGlobal>().map(|global| global.0.clone())
}

/// The watcher (the tray observes it).
pub(crate) fn entity(cx: &App) -> Option<Entity<PushToTalk>> {
    global(cx)
}

/// Install ONCE, after `speak_replies::install` (it shares that worker).
pub fn install(cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let Some(speaker) = crate::speak_replies::entity(cx) else { return };
    let messages = store.collections().messages.clone();
    let agents = store.collections().agents.clone();
    let device_id = steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
    let client = crate::speak_replies::client(cx);
    let (tx, rx) = flume::unbounded::<hotkey::HotkeyEvent>();
    let entity = cx.new(|cx: &mut Context<PushToTalk>| {
        cx.observe(&messages, |this: &mut PushToTalk, collection, cx| this.on_messages(&collection, cx)).detach();
        // The voice ending (or Stop) ends Speaking.
        cx.observe(&speaker, |this: &mut PushToTalk, _, cx| this.on_speaker(cx)).detach();
        // #NEW-56: `Hey {agent}` needs the agent's name.
        cx.observe(&agents, |this: &mut PushToTalk, _, cx| this.sync_wake(cx)).detach();
        PushToTalk {
            device_id,
            client,
            armed: None,
            grab: None,
            status: hotkey::Status::Idle,
            tx,
            held: false,
            state: PttState::default(),
            utterance: None,
            counter: 0,
            generation: 0,
            sent: None,
            reply_id: None,
            level: 0.,
            bands: Vec::new(),
            hide_generation: 0,
            unsent: HashMap::new(),
            dev_agent: None,
            worker_ready: false,
            wake_sent: None,
            wake_listening: false,
            wake_reason: None,
            recording: false,
            woken: false,
            muted: false,
            overlay: overlay::Overlay::default(),
        }
    });
    // DEV-ONLY (packages/shots/src/desktop-env.md): `EXP_DEV_PTT_OVERLAY=
    // <phase>|<line>|<agent>` opens the overlay window in that state, so a
    // capture needs no microphone, worker or sign-in.
    if let Ok(spec) = std::env::var("EXP_DEV_PTT_OVERLAY") {
        entity.update(cx, |this, cx| this.dev_overlay(&spec, cx));
    }
    cx.set_global(PushToTalkGlobal(entity.clone()));
    // #1244 review: the focused-window fallback gets no key-up once the
    // window the key is held in closes (gpui delivers none); the hold ends
    // here, or the worker's MAX_HOLD would send two minutes of the room.
    let closing = entity.downgrade();
    cx.on_window_closed(move |cx, window_id| {
        if let Some(entity) = closing.upgrade() {
            entity.update(cx, |this, cx| {
                if !this.overlay.owns(window_id) {
                    this.focus_lost(cx);
                }
            });
        }
    })
    .detach();
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            let _ = cx.update(|cx| entity.update(cx, |this, cx| this.on_hotkey(event, cx)));
        }
    })
    .detach();
    let settings = crate::speak_replies::settings(cx);
    settings_changed(&settings, cx);
}

/// The settings changed (a heartbeat, a save, a tray toggle): called by
/// `speak_replies::set_settings` BEFORE the worker may be shut down, so a
/// running Listen is cancelled first.
pub(crate) fn settings_changed(settings: &VoiceSettings, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        let armed = armed_for(settings, &this.device_id);
        this.arm(armed, cx);
    });
}

/// Sign-out or another account: drop the Listen, let the shortcut go.
pub(crate) fn signed_out(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| this.arm(None, cx));
}

/// `etalk::Event`s that belong to listening.
pub(crate) fn is_listen_event(event: &etalk::Event) -> bool {
    matches!(
        event,
        etalk::Event::Level { .. }
            | etalk::Event::Partial { .. }
            | etalk::Event::Heard { .. }
            | etalk::Event::Empty { .. }
            | etalk::Event::ListenFailed { .. }
            | etalk::Event::Woke { .. }
            | etalk::Event::WakeListening { .. }
    )
}

/// #NEW-56: the worker came up (`Ready`) or went away (`Exited`). Called
/// inside the speaker's update, so it never reads the speaker.
pub(crate) fn worker_changed(ready: bool, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        this.worker_ready = ready;
        // A new worker knows no wake word; one that exited holds no microphone.
        this.wake_sent = None;
        this.wake_listening = false;
        this.wake_reason = None;
        this.sync_wake(cx);
        cx.notify();
    });
}

/// #NEW-56: the tray's Mute changed: the wake word lets the microphone go
/// (and a wake turn in flight ends) or takes it again.
pub(crate) fn mute_changed(muted: bool, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        this.muted = muted;
        this.sync_wake(cx);
    });
}

/// #NEW-56: the wake word holds the microphone open (the tray's dot).
pub(crate) fn wake_listening(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).wake_listening) || *DEV_WAKE_LISTENING
}

/// S-3: the wake word's state on THIS device (voice.json `wakeDetail`
/// `state`): the tray's Mute, a call or a meeting holding the microphone,
/// the worker's `WakeListening` and its reason; None = not reported yet.
pub(crate) fn wake_state(cx: &App) -> Option<etalk::voice::WakeState> {
    use etalk::voice::WakeState;
    if wake_listening(cx) {
        return Some(WakeState::Listening);
    }
    let entity = global(cx)?;
    let this = entity.read(cx);
    if this.muted {
        return Some(WakeState::Muted);
    }
    if crate::voice_call::active(cx) {
        return Some(WakeState::MicBusy);
    }
    if crate::meeting::state(cx).running {
        return Some(WakeState::Meeting);
    }
    this.wake_reason.map(|reason| match reason {
        etalk::ListenFailure::NoMicrophone => WakeState::NoMicrophone,
        etalk::ListenFailure::NotGranted => WakeState::NotGranted,
        etalk::ListenFailure::Downloading => WakeState::Downloading,
        etalk::ListenFailure::Meeting => WakeState::Meeting,
        etalk::ListenFailure::Unavailable | etalk::ListenFailure::Failed => WakeState::Unavailable,
    })
}

/// S-3: the Wake word row's trailing detail on THIS device, which is the
/// voice device (`etalk::voice::wake_detail` over [`wake_state`]); the
/// doctor's `wake` row carries the same text to every other client.
pub(crate) fn own_wake_detail(settings: &VoiceSettings, device_name: &str, cx: &App) -> Option<String> {
    etalk::voice::wake_detail(&etalk::voice::WakeDetailContext {
        wake_word: settings.wake_word,
        agent: settings.agent_id.is_some(),
        online: true,
        device: device_name,
        state: wake_state(cx),
    })
}

/// S-3: Settings → Voice's recorder started (`true`) or ended: the grab is
/// let go while it records, so the old binding cannot fire, and taken again
/// after.
pub(crate) fn set_recording(recording: bool, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        if this.recording == recording {
            return;
        }
        this.recording = recording;
        if recording && this.held {
            this.key_up(cx);
        }
        this.regrab();
        cx.notify();
    });
}

/// DEV-ONLY (packages/shots/src/desktop-env.md): `EXP_DEV_WAKE_LISTENING=1`
/// reads as if the speech worker held the wake word's microphone open (the
/// tray dot, the rail's voice button dot, the Wake word row's Listening), so
/// a capture needs no microphone or worker.
static DEV_WAKE_LISTENING: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("EXP_DEV_WAKE_LISTENING").is_ok_and(|value| value == "1"));

/// A listening event from the worker.
pub(crate) fn on_worker_event(event: etalk::Event, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_worker(event, cx));
    }
}

/// How the shortcut is held (the tray item and the Shortcut row).
pub(crate) fn status(cx: &App) -> hotkey::Status {
    global(cx).map(|entity| entity.read(cx).status.clone()).unwrap_or_default()
}

/// The shortcut is not held system-wide here: the shell's key listener
/// stands in while an IDE window is focused.
fn focused_fallback(cx: &App) -> Option<hotkey::Shortcut> {
    let entity = global(cx)?;
    let this = entity.read(cx);
    let shortcut = this.armed.as_ref()?.shortcut.as_deref()?;
    matches!(this.status, hotkey::Status::FocusedOnly(_)).then(|| hotkey::Shortcut::parse(shortcut)).flatten()
}

/// The shell's capture listener: a key-down or key-up while an IDE window is
/// focused. `true` = it was the push-to-talk key (consumed).
pub(crate) fn on_focused_key(down: bool, keystroke: &gpui::Keystroke, cx: &mut App) -> bool {
    let Some(shortcut) = focused_fallback(cx) else { return false };
    let hit = if down { shortcut.matches_keystroke(keystroke) } else { shortcut.key_matches(&keystroke.key) };
    if !hit {
        return false;
    }
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_hotkey(if down { hotkey::HotkeyEvent::Down } else { hotkey::HotkeyEvent::Up }, cx));
    }
    true
}

/// #1244 review: an IDE window lost focus (the shell's activation observer).
/// In the focused-window fallback that is the key-up gpui never delivers:
/// the hold ends and the Listen with it. The global backends see the real
/// release and are untouched.
pub(crate) fn on_window_deactivated(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.focus_lost(cx));
    }
}

/// A failed send's text for the chat of session `session_id` (the member's
/// conversation with that agent), taken once by its composer.
pub(crate) fn take_unsent(session_id: &str, cx: &mut App) -> Option<String> {
    let entity = global(cx)?;
    if entity.read(cx).unsent.is_empty() {
        return None;
    }
    let me = crate::queries::active_account(cx)?.user_id;
    let agent_id = Store::try_global(cx)?
        .collections()
        .sessions
        .read(cx)
        .iter()
        .find(|row| row.id == session_id && row.is_root() && row.user_id.as_deref() == Some(me.as_str()))?
        .agent_id
        .clone()?;
    entity.update(cx, |this, _| this.unsent.remove(&agent_id))
}

/// The overlay's pill: Stop while speaking, Unmute while muted.
pub(crate) fn on_pill(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    let phase = entity.read(cx).state.phase;
    match phase {
        PttPhase::Speaking => crate::speak_replies::stop(cx),
        PttPhase::Muted => {
            crate::voice_tray::set_muted(false, cx);
            entity.update(cx, |this, cx| this.step(PttEvent::Hide, cx));
        }
        _ => {}
    }
}

impl PushToTalk {
    fn arm(&mut self, armed: Option<Armed>, cx: &mut Context<Self>) {
        if armed == self.armed {
            return;
        }
        let before = self.armed.take();
        // Carry (a): push to talk off, another device (armed gone), another
        // agent or language cancels the utterance in flight.
        let cancel = match (&before, &armed) {
            (Some(_), None) => true,
            (Some(old), Some(new)) => old.agent_id != new.agent_id || old.language != new.language,
            _ => false,
        };
        if cancel {
            self.cancel(cx);
        }
        let shortcut = |armed: &Option<Armed>| armed.as_ref().and_then(|armed| armed.shortcut.clone());
        let regrab = shortcut(&before) != shortcut(&armed);
        self.armed = armed;
        if regrab {
            // F43: a key held through the change ends its utterance (the
            // regrab forgets the hold, so no key-up would come).
            if self.held {
                self.key_up(cx);
            }
            self.regrab();
        }
        self.sync_wake(cx);
        cx.notify();
    }

    /// Drop the grab (letting the shortcut go) and take the armed one again,
    /// unless Settings → Voice records a new key.
    fn regrab(&mut self) {
        self.grab = None;
        self.held = false;
        self.status = hotkey::Status::Idle;
        let shortcut = self.armed.as_ref().and_then(|armed| armed.shortcut.clone()).filter(|_| !self.recording);
        if let Some(shortcut) = shortcut {
            self.grab = Some(hotkey::grab(&shortcut, self.tx.clone()));
        }
    }

    /// #NEW-56: hand the worker the wake word this device wants now (the
    /// phrase on the voice device with the wake word on, the worker up and
    /// Mute off; else off). Taking it back ends a wake turn in flight.
    fn sync_wake(&mut self, cx: &mut Context<Self>) {
        let wanted = match self.armed.as_ref() {
            Some(armed) if self.worker_ready && !self.muted => armed.wake_phrase.as_ref().and_then(|phrase| {
                let settings = VoiceSettings { wake_phrase: phrase.clone(), ..Default::default() };
                let phrase = etalk::voice::wake_phrase(&settings, self.agent_label(cx).as_deref())?;
                Some((phrase, armed.language.clone()))
            }),
            _ => None,
        };
        if wanted.is_none() && self.woken && self.utterance.is_some() {
            self.cancel(cx);
        }
        if wanted == self.wake_sent {
            return;
        }
        let Some(client) = self.client.as_ref() else { return };
        match wanted.as_ref() {
            Some((phrase, language)) => client.wake(phrase.clone(), language.clone()),
            None => client.wake("", "en"),
        }
        if wanted.is_none() && (self.wake_listening || self.wake_reason.is_some()) {
            self.wake_listening = false;
            self.wake_reason = None;
            cx.notify();
        }
        self.wake_sent = wanted;
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.utterance.take() {
            if let Some(client) = self.client.as_ref() {
                client.cancel_listen(id);
            }
        }
        self.held = false;
        self.woken = false;
        self.sent = None;
        self.reply_id = None;
        self.generation += 1;
        self.step(PttEvent::Cancel, cx);
    }

    fn on_hotkey(&mut self, event: hotkey::HotkeyEvent, cx: &mut Context<Self>) {
        match event {
            hotkey::HotkeyEvent::Status(status) => {
                if status != self.status {
                    self.status = status;
                    cx.notify();
                    // The tray item and Settings → Voice read it.
                    cx.refresh_windows();
                }
            }
            hotkey::HotkeyEvent::Down => self.key_down(cx),
            hotkey::HotkeyEvent::Up => self.key_up(cx),
        }
    }

    fn key_down(&mut self, cx: &mut Context<Self>) {
        if self.held || self.recording {
            return;
        }
        let Some(armed) = self.armed.clone() else { return };
        // NEW-55: a call holds the microphone; the key waits for End call.
        if crate::voice_call::active(cx) {
            return;
        }
        // NEW-57: a meeting holds it too: the key flushes its queue (`ptt`).
        if crate::meeting::state(cx).running {
            crate::meeting::flush_now(cx);
            return;
        }
        self.held = true;
        self.woken = false;
        self.generation += 1;
        self.sent = None;
        self.reply_id = None;
        if crate::voice_tray::muted(cx) {
            // The microphone never opens.
            self.step(PttEvent::KeyDown { muted: true }, cx);
            return;
        }
        let Some(client) = self.client.clone() else { return };
        if let Some(old) = self.utterance.take() {
            client.cancel_listen(old);
        }
        // Barge-in: the worker silences the voice; the Dock banner goes too.
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        self.counter += 1;
        let id = format!("ptt-{}", self.counter);
        client.listen(id.clone(), armed.language);
        self.utterance = Some(id);
        self.level = 0.;
        self.bands.clear();
        self.step(PttEvent::KeyDown { muted: false }, cx);
    }

    fn key_up(&mut self, cx: &mut Context<Self>) {
        if !self.held {
            return;
        }
        self.held = false;
        if let (Some(id), Some(client)) = (self.utterance.as_ref(), self.client.as_ref()) {
            client.end_listen(id.clone());
        }
        self.step(PttEvent::KeyUp, cx);
    }

    /// The window the key is held in deactivated or closed: a key-up while
    /// the shortcut is held only through a focused window.
    fn focus_lost(&mut self, cx: &mut Context<Self>) {
        if self.held && matches!(self.status, hotkey::Status::FocusedOnly(_)) {
            self.key_up(cx);
        }
    }

    fn on_worker(&mut self, event: etalk::Event, cx: &mut Context<Self>) {
        let ours = |id: &str, utterance: &Option<String>| utterance.as_deref() == Some(id);
        match event {
            etalk::Event::Level { id, level, bands } if ours(&id, &self.utterance) => {
                self.level = level;
                self.bands = bands.to_vec();
                self.overlay.refresh(cx);
            }
            etalk::Event::Partial { id, text } if ours(&id, &self.utterance) => self.step(PttEvent::Partial(text), cx),
            etalk::Event::Woke { id } => self.woke(id, cx),
            etalk::Event::WakeListening { listening, reason } => {
                if let Some(reason) = reason {
                    log::info!("[voice] wake word: the microphone is not open ({reason:?})");
                }
                let reason = reason.filter(|_| !listening);
                if listening != self.wake_listening || reason != self.wake_reason {
                    self.wake_listening = listening;
                    self.wake_reason = reason;
                    cx.notify();
                    // The Wake word row and the doctor's `wake` read it.
                    cx.refresh_windows();
                }
            }
            etalk::Event::Heard { id, text } if ours(&id, &self.utterance) => {
                self.utterance = None;
                self.woken = false;
                self.step(PttEvent::Heard(text.clone()), cx);
                if self.state.phase == PttPhase::Heard {
                    self.send(text, cx);
                }
            }
            etalk::Event::Empty { id } if ours(&id, &self.utterance) => {
                self.utterance = None;
                self.woken = false;
                self.step(PttEvent::Empty, cx);
            }
            etalk::Event::ListenFailed { id, reason, detail } if ours(&id, &self.utterance) => {
                self.utterance = None;
                self.woken = false;
                log::info!("[voice] push to talk: listening failed ({reason:?}): {detail}");
                self.step(PttEvent::ListenFailed(listen_failure(reason)), cx);
            }
            _ => {}
        }
    }

    /// #NEW-56: the worker heard the wake phrase: a key-down for one turn
    /// (its microphone is already open; the worker ends the turn itself).
    fn woke(&mut self, id: String, cx: &mut Context<Self>) {
        // A held key's Listen replaces the turn in the worker.
        if self.held {
            return;
        }
        // A Woke in flight when the member muted or turned the wake word
        // off (or push to talk disarmed) never becomes a turn.
        if self.wake_sent.is_none() || self.muted || self.armed.is_none() {
            if let Some(client) = self.client.as_ref() {
                client.cancel_listen(id);
            }
            return;
        }
        if let Some(old) = self.utterance.take() {
            if let Some(client) = self.client.as_ref() {
                client.cancel_listen(old);
            }
        }
        self.generation += 1;
        self.sent = None;
        self.reply_id = None;
        // The worker silenced the voice (barge-in); the Dock banner goes too.
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        self.utterance = Some(id);
        self.woken = true;
        self.level = 0.;
        self.bands.clear();
        self.step(PttEvent::KeyDown { muted: false }, cx);
    }

    fn send(&mut self, heard: String, cx: &mut Context<Self>) {
        let Some(armed) = self.armed.clone() else { return };
        let conversation = crate::session_data::my_conversation(&armed.agent_id, cx).map(|row| row.id);
        let team_id = Store::try_global(cx)
            .and_then(|store| {
                store.collections().agents.read(cx).iter().find(|row| row.id == armed.agent_id).and_then(|row| row.team_id.clone())
            })
            .unwrap_or_default();
        let Some(target) = etalk::voice::utterance_input(&heard, &team_id, Some(&armed.agent_id), conversation.as_deref()) else {
            return;
        };
        let (input, text) = match target {
            UtteranceTarget::Session { session_id, text } => (api::sessions::SendInput::message(&session_id, &text, Vec::new()), text),
            UtteranceTarget::Agent { team_id, agent_id, text } => {
                (crate::chat_launch::agent_send_input(&team_id, &agent_id, &text, Vec::new()), text)
            }
        };
        let generation = self.generation;
        let agent_id = armed.agent_id.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            self.send_failed(agent_id, text, cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::sessions::send(&trpc, &input) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(output) => {
                        // S-3: the reply to a spoken turn is read aloud.
                        crate::speak_replies::voice_turn(&output.session_id, cx);
                        this.sent = Some(Sent { session_id: output.session_id, message_id: output.message_id });
                        this.step(PttEvent::Sent, cx);
                    }
                    Err(err) => {
                        log::warn!("[voice] push to talk: the send failed: {err}");
                        this.send_failed(agent_id, text, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// Not sent: the overlay says so and the text waits in the chat's composer.
    fn send_failed(&mut self, agent_id: String, text: String, cx: &mut Context<Self>) {
        let kept = match self.unsent.remove(&agent_id) {
            Some(older) => format!("{older}\n\n{text}"),
            None => text,
        };
        self.unsent.insert(agent_id, kept);
        self.step(PttEvent::SendFailed, cx);
        cx.refresh_windows();
    }

    /// Thinking ends when the agent's reply row lands in the conversation:
    /// its own message (the sent row) first, then the agent's next final
    /// reply after it.
    fn on_messages(&mut self, collection: &Entity<Collection<MessageRow>>, cx: &mut Context<Self>) {
        if self.state.phase != PttPhase::Thinking {
            return;
        }
        let Some(sent) = self.sent.clone() else { return };
        let Some(store) = Store::try_global(cx) else { return };
        let sessions = domain::message_conversation::conversation_sessions(store.collections().conversations.read(cx).iter());
        let reply = {
            let rows = collection.read(cx);
            let in_session = |row: &&MessageRow| {
                domain::message_conversation::session_of(row, &sessions).unwrap_or(row.session_id.as_str()) == sent.session_id
            };
            let sent_at = sent
                .message_id
                .as_deref()
                .and_then(|id| rows.iter().find(|row| row.id == id))
                .and_then(|row| row.created_at.as_deref().and_then(etalk::voice::parse_instant));
            let Some(sent_at) = sent_at else { return };
            rows.iter()
                .filter(in_session)
                .filter(|row| {
                    row.author_kind.as_deref() == Some("agent")
                        && row.author_session_id.is_none()
                        && !row.card.as_ref().is_some_and(|card| !card.is_null())
                        && row.body.as_deref().is_some_and(|body| !body.trim().is_empty())
                })
                .filter_map(|row| Some((row.created_at.as_deref().and_then(etalk::voice::parse_instant)?, row)))
                .filter(|(at, _)| *at > sent_at)
                .min_by_key(|(at, _)| *at)
                .map(|(_, row)| (row.id.clone(), row.body.clone().unwrap_or_default()))
        };
        let Some((id, body)) = reply else { return };
        let speaking = crate::speak_replies::speaking(cx).is_some_and(|speaking| speaking.message_id == id);
        self.reply_id = Some(id);
        self.step(PttEvent::Replied { body, speaking }, cx);
    }

    fn on_speaker(&mut self, cx: &mut Context<Self>) {
        if self.state.phase != PttPhase::Speaking {
            return;
        }
        let speaking = crate::speak_replies::speaking(cx);
        if speaking.is_none_or(|speaking| Some(speaking.message_id) != self.reply_id) {
            self.step(PttEvent::SpeakFinished, cx);
        }
    }

    fn agent_label(&self, cx: &App) -> Option<String> {
        if let Some(agent) = self.dev_agent.clone() {
            return Some(agent);
        }
        let armed = self.armed.as_ref()?;
        let settings = VoiceSettings { agent_id: Some(armed.agent_id.clone()), ..Default::default() };
        crate::voice_tray::agent_label(&settings, cx)
    }

    /// The capsule now (None = hidden).
    fn overlay_now(&self, cx: &App) -> Option<PttOverlay> {
        if self.state.phase == PttPhase::Hidden {
            return None;
        }
        ptt_overlay(&self.state, self.agent_label(cx).as_deref())
    }

    fn step(&mut self, event: PttEvent, cx: &mut Context<Self>) {
        let next = ptt_step(&self.state, &event);
        if next == self.state && !matches!(event, PttEvent::KeyDown { .. }) {
            return;
        }
        self.state = next;
        let view = self.overlay_now(cx);
        self.hide_generation += 1;
        if let Some(after) = view.as_ref().and_then(|view| view.hide_after_ms) {
            let generation = self.hide_generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(after)).await;
                let _ = this.update(cx, |this, cx| {
                    if this.hide_generation == generation {
                        this.step(PttEvent::Hide, cx);
                    }
                });
            })
            .detach();
        }
        self.overlay.show(view.map(overlay::Face::Ptt), cx);
        cx.notify();
    }
}

impl PushToTalk {
    fn dev_overlay(&mut self, spec: &str, cx: &mut Context<Self>) {
        let mut parts = spec.splitn(3, '|');
        let (Some(phase), line, agent) = (parts.next().and_then(PttPhase::from_key), parts.next(), parts.next()) else {
            log::warn!("[voice] EXP_DEV_PTT_OVERLAY: want <phase>|<line>|<agent>, got {spec:?}");
            return;
        };
        self.state = PttState { phase, line: line.unwrap_or_default().to_string() };
        self.dev_agent = agent.map(str::to_string);
        if phase == PttPhase::Listening {
            self.level = 0.7;
            self.bands = vec![0.9, 0.6, 1., 0.5, 0.7, 0.4, 0.6, 0.3];
        }
        let view = self.overlay_now(cx);
        self.overlay.show(view.map(overlay::Face::Ptt), cx);
    }
}

fn listen_failure(reason: etalk::ListenFailure) -> etalk::voice::ListenFailure {
    use etalk::voice::ListenFailure as Domain;
    match reason {
        etalk::ListenFailure::NoMicrophone => Domain::NoMicrophone,
        etalk::ListenFailure::NotGranted => Domain::NotGranted,
        etalk::ListenFailure::Downloading => Domain::Downloading,
        etalk::ListenFailure::Unavailable => Domain::Unavailable,
        // NEW-57: only a Call is refused for a meeting (a Listen mid-meeting
        // is a flush trigger answered Empty); never reaches push to talk.
        etalk::ListenFailure::Failed | etalk::ListenFailure::Meeting => Domain::Failed,
    }
}

/// The wave's level and bands now (the overlay reads them each frame).
pub(crate) fn level(cx: &App) -> (f32, Vec<f32>) {
    global(cx).map(|entity| {
        let this = entity.read(cx);
        (this.level, this.bands.clone())
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> VoiceSettings {
        VoiceSettings {
            voice_device_id: Some("d-mint".into()),
            agent_id: Some("a1".into()),
            ptt_shortcut: "ctrl+space".into(),
            ..Default::default()
        }
    }

    #[test]
    fn only_the_voice_device_with_push_to_talk_is_armed() {
        assert_eq!(
            armed_for(&settings(), "d-mint"),
            Some(Armed { shortcut: Some("ctrl+space".into()), agent_id: "a1".into(), language: "en".into(), wake_phrase: None })
        );
        assert_eq!(armed_for(&settings(), "d-mac"), None, "another device");
        // S-3: no key bound = push to talk off.
        assert_eq!(armed_for(&VoiceSettings { ptt_shortcut: String::new(), ..settings() }, "d-mint"), None);
        assert_eq!(armed_for(&VoiceSettings { agent_id: None, ..settings() }, "d-mint"), None, "clearOnNone");
        // #NEW-56: the wake word alone arms the device, without a shortcut.
        let wake = VoiceSettings { ptt_shortcut: String::new(), wake_word: true, wake_phrase: "Computer".into(), ..settings() };
        let armed = armed_for(&wake, "d-mint").unwrap();
        assert_eq!((armed.shortcut, armed.wake_phrase.as_deref()), (None, Some("Computer")));
        assert_eq!(armed_for(&VoiceSettings { agent_id: None, ..wake }, "d-mint"), None, "clearOnNone");
        let german = VoiceSettings { language: "de".into(), ..settings() };
        assert_eq!(armed_for(&german, "d-mint").unwrap().language, "de");
    }

    #[test]
    fn every_listen_event_is_routed_here() {
        assert!(is_listen_event(&etalk::Event::Empty { id: "u".into() }));
        assert!(is_listen_event(&etalk::Event::Heard { id: "u".into(), text: "x".into() }));
        assert!(is_listen_event(&etalk::Event::Woke { id: "wake-1".into() }));
        assert!(is_listen_event(&etalk::Event::WakeListening { listening: true, reason: None }));
        assert!(!is_listen_event(&etalk::Event::Ready { protocol: etalk::PROTOCOL }));
        assert!(!is_listen_event(&etalk::Event::Finished { id: "m".into(), stopped: false }));
    }
}
