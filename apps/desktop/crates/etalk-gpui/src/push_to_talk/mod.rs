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

pub mod hotkey;
pub mod overlay;

use std::collections::HashMap;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Global};

use etalk::voice::{ptt_overlay, ptt_step, PttEvent, PttOverlay, PttPhase, PttState, VoiceSettings};

use crate::host::{self, DialLink, DialLinkEvent, DIAL_DOWN_RATE};

pub use overlay::activity as overlay_activity;

/// #EXP-1938: how long a focus change may take to name its new window.
const FOCUS_SETTLE: Duration = Duration::from_millis(50);

/// EXP-1859: a dialled turn holds at most this much audio before its link
/// is up (talk-listen.json `maxTurnMs`).
const MAX_TURN_SECONDS: usize = 60;

/// EXP-1859: how long before its `said` reached the caller the host may
/// have sent a dialled turn (the reply is the first after that).
const SENT_SLACK_MS: i64 = 2_000;

/// EXP-1859: what bounds a dialled turn whatever its host does (an older one
/// answers it as an ordinary call). Not `said` this long after `call_ready`
/// (talk-listen.json `maxTurnMs` + a margin for a cold host): heard here.
const TURN_SAID_WITHIN: Duration = Duration::from_secs(MAX_TURN_SECONDS as u64 + 15);
/// After `said`: no reply this much past the host's own `turnReplyWaitMs`.
const TURN_REPLY_MARGIN: Duration = Duration::from_secs(10);
/// The longest spoken reply.
const TURN_REPLY_AT_MOST: Duration = Duration::from_secs(300);

/// EXP-1859: how a dialled turn ends on the caller.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// The person (pill, key, Mute, Wake word off, sign-out): reason-less `call_end`.
    Cancelled,
    /// This side failed (its microphone, its worker): `call_end {dropped}`.
    Failed,
    /// The host ended it: nothing to tell.
    HungUp,
}

/// EXP-1859: the newest reply in the member's conversation with `agent_id`
/// (what Speaking shows while a dialled turn's reply plays).
fn newest_reply(agent_id: &str, cx: &App) -> Option<String> {
    let session_id = host::conversation(agent_id, cx)?;
    let host = host::host(cx)?;
    let mut newest: Option<(i64, String)> = None;
    host.session_messages(&session_id, cx, &mut |row| {
        let message = &row.message;
        let reply = message.author_kind == Some("agent")
            && message.author_session_id.is_none()
            && !message.has_card
            && message.body.is_some_and(|body| !body.trim().is_empty());
        if let Some(at) = message.created_at.and_then(etalk::voice::parse_instant).filter(|_| reply) {
            if newest.as_ref().is_none_or(|(newest, _)| at > *newest) {
                newest = Some((at, message.body.unwrap_or_default().to_string()));
            }
        }
    });
    newest.map(|(_, body)| body)
}

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
    /// EXP-1859: the team agents switched on to listen too (empty while
    /// the wake word is off here).
    wake_agent_ids: Vec<String>,
}

fn armed_for(settings: &VoiceSettings, device_id: &str) -> Option<Armed> {
    let settings = settings.clone().clear_on_none();
    // S-3: push to talk is on iff a key is bound. EXP-1857: it listens on
    // the desktop you're at (every desktop of the member's).
    let shortcut = settings.push_to_talk().then(|| settings.ptt_shortcut.clone());
    // The wake word keeps `clearOnNone` and the voice device.
    let voice_device = settings.voice_device_id.as_deref() == Some(device_id);
    let wake_phrase = (settings.wake_word && voice_device).then(|| settings.wake_phrase.clone());
    if shortcut.is_none() && wake_phrase.is_none() {
        return None;
    }
    let wake_agent_ids = if wake_phrase.is_some() { settings.wake_agent_ids.clone() } else { Vec::new() };
    Some(Armed { shortcut, agent_id: settings.agent_id.clone()?, language: settings.language().to_string(), wake_phrase, wake_agent_ids })
}

/// NEW-78 / S-3: THIS device listens: an agent and a bound key (EXP-1857:
/// on any desktop of the member's) or the wake word on the voice device
/// (after voice.json `clearOnNone`). Mute shows then, and the doctor
/// carries `microphone`.
pub fn listens_here(settings: &VoiceSettings, device_id: &str) -> bool {
    armed_for(settings, device_id).is_some()
}

/// The send this utterance made.
#[derive(Clone, Debug)]
struct Sent {
    session_id: String,
    message_id: Option<String>,
    /// EXP-1859: a dialled turn's host sent it (no row id here): the reply
    /// is the first after this instant (epoch ms).
    at: Option<i64>,
}

/// EXP-1859 (voice-call.json): a wake turn DIALLED to the agent's own
/// device as a TURN call. The worker streams the turn (`DialAudio`) and
/// plays the host's reply; the capsule follows the call (`turnCapsuleCases`).
struct TurnDial {
    /// The worker's wake turn id (its Dial id too).
    id: String,
    agent_id: String,
    link: Option<Box<dyn DialLink>>,
    /// The turn's audio until `call_ready` (sealed: until the seal is open):
    /// before it the link drops what it is handed.
    held: Vec<i16>,
    /// `call_ready` came (the fallback timer no longer applies).
    ready: bool,
    /// The host stepped `ready`: a turn host does at once, cold too (an older
    /// build rings first: it answered an ordinary call).
    answered: bool,
    /// The host's `said` came: from here a failure only ends it, and nothing
    /// of the microphone leaves.
    said: bool,
    /// The host's reply plays.
    replying: bool,
}

pub struct PushToTalk {
    device_id: String,
    client: Option<etalk::SpeechClient>,
    armed: Option<Armed>,
    grab: Option<hotkey::Grab>,
    status: hotkey::Status,
    /// EXP-1901: bumped by every regrab; a dropped grab's events are ignored.
    grab_generation: u64,
    /// The key is down (any backend; repeats are ignored).
    held: bool,
    /// #EXP-1938: the window that was active at the key-down (the IDE
    /// window the focused-window fallback hears the release in).
    held_in: Option<gpui::WindowId>,
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
    /// (None = off). EXP-1859: and every listener (empty = that one phrase).
    wake_sent: Option<(String, String, Vec<etalk::WakeListener>)>,
    /// #NEW-56: the worker holds the microphone open for the wake word.
    wake_listening: bool,
    /// S-3: why the worker's last `WakeListening` said the microphone is not
    /// open (None = no failure reported).
    wake_reason: Option<etalk::ListenFailure>,
    /// S-3: Settings → Voice records a new key: the grab is let go.
    recording: bool,
    /// #NEW-56: the utterance in flight is a wake turn.
    woken: bool,
    /// EXP-1859: the agent whose phrase opened the last wake turn (None =
    /// the voice agent: a push-to-talk turn, or a wake that named nobody).
    /// Its turn goes into the member's chat with THAT agent.
    turn_agent: Option<String>,
    /// EXP-1859: the wake turn dialled to the agent's own device.
    turn_dial: Option<TurnDial>,
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
pub fn entity(cx: &App) -> Option<Entity<PushToTalk>> {
    global(cx)
}

/// Install ONCE, after `speak_replies::install` (it shares that worker).
/// The host's messages (the reply that ends Thinking) and its agents
/// (#NEW-56: `Hey {agent}` needs the agent's name) arrive through
/// [`crate::messages_changed`] and [`crate::agents_changed`].
pub(crate) fn install(cx: &mut App) {
    let Some(device_id) = host::device_id(cx) else { return };
    let Some(speaker) = crate::speak_replies::entity(cx) else { return };
    let client = crate::speak_replies::client(cx);
    let entity = start(device_id, client, Some(speaker), cx);
    // DEV-ONLY (packages/shots/src/desktop-env.md): `EXP_DEV_PTT_OVERLAY=
    // <phase>|<line>|<agent>` opens the overlay window in that state, so a
    // capture needs no microphone, worker or sign-in.
    if let Ok(spec) = std::env::var("EXP_DEV_PTT_OVERLAY") {
        entity.update(cx, |this, cx| this.dev_overlay(&spec, cx));
    }
    let settings = crate::speak_replies::settings(cx);
    settings_changed(&settings, cx);
}

/// The watcher, its global and its window-close observer (EXP-1901:
/// [`install`] without the host, so a gpui test can drive it).
fn start(
    device_id: String,
    client: Option<etalk::SpeechClient>,
    speaker: Option<Entity<crate::speak_replies::SpeakReplies>>,
    cx: &mut App,
) -> Entity<PushToTalk> {
    let entity = cx.new(|cx: &mut Context<PushToTalk>| {
        // The voice ending (or Stop) ends Speaking.
        if let Some(speaker) = speaker.as_ref() {
            cx.observe(speaker, |this: &mut PushToTalk, _, cx| this.on_speaker(cx)).detach();
        }
        PushToTalk {
            device_id,
            client,
            armed: None,
            grab: None,
            status: hotkey::Status::Idle,
            grab_generation: 0,
            held: false,
            held_in: None,
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
            turn_agent: None,
            turn_dial: None,
            muted: false,
            overlay: overlay::Overlay::default(),
        }
    });
    cx.set_global(PushToTalkGlobal(entity.clone()));
    // #1244 review: the focused-window fallback gets no key-up once the
    // window the key is held in closes (gpui delivers none); the hold ends
    // here, or the worker's MAX_HOLD would send two minutes of the room.
    let closing = entity.downgrade();
    cx.on_window_closed(move |cx, window_id| {
        // EXP-1901: gpui runs this inline when a window closes, and the
        // capsule closes from inside the watcher's own update (the hide, a
        // wider reopen): updating the watcher here would be a second lease
        // (a panic), so it waits for that update's end.
        let closing = closing.clone();
        cx.defer(move |cx| {
            if let Some(entity) = closing.upgrade() {
                entity.update(cx, |this, cx| {
                    if !this.overlay.closed(window_id) && !this.overlay.owns(window_id) {
                        this.focus_lost(cx);
                    }
                });
            }
        });
    })
    .detach();
    entity
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

/// The host's messages changed: the reply that ends Thinking.
pub(crate) fn on_messages(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_messages(cx));
    }
}

/// #NEW-56: the host's agents changed (`Hey {agent}` names one).
pub(crate) fn on_agents(cx: &mut App) {
    sync(cx);
}

/// The wake word handed to the worker again when what it wants changed.
pub(crate) fn sync(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.sync_wake(cx));
    }
}

/// Sign-out or another account: drop the Listen, let the shortcut go.
pub fn signed_out(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| this.arm(None, cx));
}

/// #EXP-1936: a call starts: it takes the microphone, so the utterance in
/// flight is dropped. The binding and the wake word stay as they are (a
/// key-down returns while a call runs, the worker holds the wake word back
/// itself): both work again the moment the call ends.
pub(crate) fn cancel_for_call(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| this.cancel(cx));
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
            // EXP-1859: a dialled wake turn's audio (a call's own is the call's).
            | etalk::Event::DialAudio { .. }
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
        if ready {
            // The listeners' `dial` reads the speaker: after its update.
            cx.defer(sync);
        } else {
            this.sync_wake(cx);
        }
        // EXP-1859: a dialled turn's audio went with it.
        let dialled = !ready && this.end_turn_dial(Ending::Failed, cx).is_some();
        // #EXP-1936: the worker went away with a Listen open (the key held,
        // or released and waiting): no answer will come, so it fails now.
        if !ready && (this.utterance.take().is_some() || dialled) {
            this.woken = false;
            this.step(PttEvent::ListenFailed(etalk::voice::ListenFailure::Failed), cx);
        }
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
pub fn wake_listening(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).wake_listening) || *DEV_WAKE_LISTENING
}

/// S-3: the wake word's state on THIS device (voice.json `wakeDetail`
/// `state`): the tray's Mute, a call or a meeting holding the microphone,
/// the worker's `WakeListening` and its reason; None = not reported yet.
pub fn wake_state(cx: &App) -> Option<etalk::voice::WakeState> {
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
pub fn own_wake_detail(settings: &VoiceSettings, device_name: &str, cx: &App) -> Option<String> {
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
pub fn set_recording(recording: bool, cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        if this.recording == recording {
            return;
        }
        this.recording = recording;
        if recording && this.held {
            this.key_up(cx);
        }
        this.regrab(cx);
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
pub fn status(cx: &App) -> hotkey::Status {
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
pub fn on_focused_key(down: bool, keystroke: &gpui::Keystroke, cx: &mut App) -> bool {
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
///
/// #EXP-1938: unless the focus only went to the capsule this very hold
/// opened (a Wayland compositor without wlr-layer-shell gives the capsule a
/// plain window, and focuses it): the capsule hears the release then
/// ([`overlay`]), and its own deactivation comes back here.
pub fn on_window_deactivated(cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.window_deactivated(cx));
    }
}

/// #EXP-1938: computer use took the person's keyboard and pointer
/// ([`host::TalkHost::input_held`] is true by now). A push-to-talk key the
/// person holds at that moment never gets its release: the hold floats
/// their keyboard, so the X11 grab thread hears no KeyRelease and the
/// microphone would stay open until the worker's MAX_HOLD. The hold ends
/// here, as a release; a system-wide grab is taken anew, which forgets the
/// thread's held key and ends the X server's activated grab with it.
pub fn input_taken(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    entity.update(cx, |this, cx| {
        if !this.held {
            return;
        }
        this.key_up(cx);
        if this.status == hotkey::Status::Global {
            let status = std::mem::take(&mut this.status);
            this.regrab(cx);
            // The same shortcut on the same desktop: the tray and Settings
            // keep reading it as held until the new grab answers.
            this.status = status;
        }
    });
}

/// A failed send left text waiting for a chat's composer.
pub fn has_unsent(cx: &App) -> bool {
    global(cx).is_some_and(|entity| !entity.read(cx).unsent.is_empty())
}

/// A failed send's text for the chat with `agent_id` (the member's
/// conversation with that agent), taken once by its composer.
pub fn take_unsent(agent_id: &str, cx: &mut App) -> Option<String> {
    let entity = global(cx)?;
    entity.update(cx, |this, _| this.unsent.remove(agent_id))
}

/// The overlay's pill: Stop while speaking, Unmute while muted.
pub fn on_pill(cx: &mut App) {
    let Some(entity) = global(cx) else { return };
    let phase = entity.read(cx).state.phase;
    match phase {
        // EXP-1859: a dialled turn's reply plays on its call.
        PttPhase::Speaking if entity.read(cx).turn_dial.is_some() => entity.update(cx, |this, cx| {
            this.end_turn_dial(Ending::Cancelled, cx);
            this.step(PttEvent::SpeakFinished, cx);
        }),
        PttPhase::Speaking => crate::speak_replies::stop(cx),
        PttPhase::Muted => {
            crate::voice_tray::set_muted(false, cx);
            entity.update(cx, |this, cx| this.step(PttEvent::Hide, cx));
        }
        // EXP-1859: a click cancels a wake turn, local (while it is heard:
        // nothing is sent) or dialled (its call ends).
        _ => entity.update(cx, |this, cx| {
            if this.turn_dial.is_some() || this.woken && this.utterance.is_some() {
                this.cancel(cx);
            }
        }),
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
            self.regrab(cx);
        }
        self.sync_wake(cx);
        cx.notify();
    }

    /// Drop the grab (letting the shortcut go) and take the armed one again,
    /// unless Settings → Voice records a new key.
    fn regrab(&mut self, cx: &mut Context<Self>) {
        self.grab = None;
        self.held = false;
        self.status = hotkey::Status::Idle;
        self.grab_generation += 1;
        let shortcut = self.armed.as_ref().and_then(|armed| armed.shortcut.clone()).filter(|_| !self.recording);
        if let Some(shortcut) = shortcut {
            let tx = self.grab_events(cx);
            // #EXP-1936: a test never holds a key of the machine it runs on.
            self.grab = (!cfg!(test)).then(|| hotkey::grab(&shortcut, tx));
        }
    }

    /// EXP-1901: the channel of the grab taken now. A grab's thread answers
    /// on its own time, so what a dropped one still says (a late status, a
    /// key) ends here: it would overwrite what is held now.
    fn grab_events(&mut self, cx: &mut Context<Self>) -> flume::Sender<hotkey::HotkeyEvent> {
        let (tx, rx) = flume::unbounded::<hotkey::HotkeyEvent>();
        let generation = self.grab_generation;
        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv_async().await {
                let live = this.update(cx, |this, cx| {
                    let live = this.grab_generation == generation;
                    if live {
                        this.on_hotkey(event, cx);
                    }
                    live
                });
                if !matches!(live, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        tx
    }

    /// #NEW-56: hand the worker the wake word this device wants now (the
    /// phrase on the voice device with the wake word on, the worker up and
    /// Mute off; else off). Taking it back ends a wake turn in flight.
    fn sync_wake(&mut self, cx: &mut Context<Self>) {
        let wanted = match self.armed.as_ref() {
            Some(armed) if self.worker_ready && !self.muted => armed.wake_phrase.as_ref().and_then(|phrase| {
                // #EXP-1847: the voice agent's own phrase (the Assistant's) wins.
                let phrase = host::wake_phrase_of(Some(&armed.agent_id), phrase, cx);
                let settings = VoiceSettings { wake_phrase: phrase, ..Default::default() };
                let phrase = etalk::voice::wake_phrase(&settings, self.label_of(armed.agent_id.clone(), cx).as_deref())?;
                // #EXP-1847: the wake turn is heard in the voice agent's language.
                let language = host::language_of(&armed.agent_id, &armed.language, cx);
                // EXP-1859: every agent switched on listens too (the host's
                // `wakeListeners`, the voice agent first; none = that phrase).
                let member = VoiceSettings {
                    voice_device_id: Some(self.device_id.clone()),
                    agent_id: Some(armed.agent_id.clone()),
                    wake_word: true,
                    wake_phrase: armed.wake_phrase.clone().unwrap_or_default(),
                    wake_agent_ids: armed.wake_agent_ids.clone(),
                    language: armed.language.clone(),
                    ..Default::default()
                };
                let listeners = host::wake_listeners(&member, cx);
                Some(match listeners.first() {
                    Some(first) => (first.phrase.clone(), first.language.clone(), listeners),
                    None => (phrase, language, listeners),
                })
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
            Some((phrase, language, listeners)) => client.wake_for(phrase.clone(), language.clone(), listeners.clone()),
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
        // EXP-1859: a dialled turn's call ends reason-less (nothing is sent
        // before the host's `said`).
        self.end_turn_dial(Ending::Cancelled, cx);
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
        // #EXP-1938: while computer use holds the person's input the chord
        // can only be an agent's synthetic keystroke (X11's XTEST passes a
        // passive grab, RegisterHotKey and Carbon fire for injected keys):
        // it opens no microphone and stops no speech. The grab still
        // swallows it on every OS, so the driven app does not get it.
        if host::input_held(cx) {
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
        self.held_in = cx.active_window().map(|window| window.window_id());
        self.woken = false;
        // The key talks to the voice agent (push to talk stays with it).
        self.turn_agent = None;
        self.generation += 1;
        self.sent = None;
        self.reply_id = None;
        if crate::voice_tray::muted(cx) {
            // The microphone never opens.
            self.step(PttEvent::KeyDown { muted: true }, cx);
            return;
        }
        let Some(client) = self.client.clone() else { return };
        // EXP-1859: the key replaces a dialled wake turn (its call ends first).
        self.end_turn_dial(Ending::Cancelled, cx);
        if let Some(old) = self.utterance.take() {
            client.cancel_listen(old);
        }
        // Barge-in: the worker silences the voice; the Dock banner goes too.
        if crate::speak_replies::speaking(cx).is_some() {
            crate::speak_replies::stop(cx);
        }
        self.counter += 1;
        let id = format!("ptt-{}", self.counter);
        // #EXP-1847: the turn is heard in the voice agent's own language.
        client.listen(id.clone(), host::language_of(&armed.agent_id, &armed.language, cx));
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

    /// #EXP-1938: a window deactivated mid-hold in the focused-window
    /// fallback. The window that takes the focus is known only once the
    /// focus has settled (Wayland: leave, then enter), so the hold is
    /// judged [`FOCUS_SETTLE`] later: it goes on while the capsule's own
    /// window or the window the key went down in has the focus (both hear
    /// the release), and ends anywhere else.
    fn window_deactivated(&mut self, cx: &mut Context<Self>) {
        if !self.held || !matches!(self.status, hotkey::Status::FocusedOnly(_)) {
            return;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FOCUS_SETTLE).await;
            let _ = this.update(cx, |this, cx| {
                let active = cx.active_window().map(|window| window.window_id());
                let hears_the_release = active.is_some_and(|active| this.overlay.owns(active) || this.held_in == Some(active));
                if this.generation == generation && !hears_the_release {
                    this.focus_lost(cx);
                }
            });
        })
        .detach();
    }

    fn on_worker(&mut self, event: etalk::Event, cx: &mut Context<Self>) {
        let ours = |id: &str, utterance: &Option<String>| utterance.as_deref() == Some(id);
        match event {
            etalk::Event::Level { id, level, bands } if ours(&id, &self.utterance) => {
                self.level = level;
                self.bands = bands.to_vec();
                self.overlay.refresh(self.level, &self.bands, cx);
            }
            etalk::Event::Partial { id, text } if ours(&id, &self.utterance) => self.step(PttEvent::Partial(text), cx),
            etalk::Event::Woke { id, agent_id } => self.woke(id, agent_id, cx),
            etalk::Event::DialAudio { id, pcm } => self.turn_audio(&id, &pcm, cx),
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
                // EXP-1859: a dialled turn's microphone closed: its call ends.
                self.end_turn_dial(Ending::Failed, cx);
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
    fn woke(&mut self, id: String, agent_id: Option<String>, cx: &mut Context<Self>) {
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
        self.end_turn_dial(Ending::Cancelled, cx);
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
        // EXP-1859: a listener whose call runs on another device: a turn call.
        let dial = agent_id.as_deref().and_then(|agent| {
            let listeners = &self.wake_sent.as_ref()?.2;
            listeners.iter().find(|listener| listener.agent_id == agent && listener.dial).map(|listener| listener.phrase.clone())
        });
        self.utterance = Some(id.clone());
        self.woken = true;
        // EXP-1859: the turn is the woken agent's.
        self.turn_agent = agent_id.clone();
        self.level = 0.;
        self.bands.clear();
        self.step(PttEvent::KeyDown { muted: false }, cx);
        if let (Some(phrase), Some(agent_id)) = (dial, agent_id) {
            self.dial_turn(id, agent_id, phrase, cx);
        }
    }

    /// EXP-1859: dial wake turn `id` to `agent_id`'s own device (`voice.startCall`
    /// `mode: turn`). It becomes the LOCAL turn (the worker's Fallback, from
    /// the phrase's start) when the dial is refused, when `call_ready` has not
    /// come within `turnDialMs`, or on a failure before the host's `said`.
    fn dial_turn(&mut self, id: String, agent_id: String, phrase: String, cx: &mut Context<Self>) {
        self.turn_dial = Some(TurnDial { id: id.clone(), agent_id: agent_id.clone(), link: None, held: Vec::new(), ready: false, answered: false, said: false, replying: false });
        let Some(connecting) = host::host(cx).and_then(|host| host.dial_turn(&agent_id, &phrase, cx)) else {
            log::info!("[voice] wake turn {id}: nothing to dial through: heard here");
            return self.fall_back(&id, cx);
        };
        log::info!("[voice] wake turn {id}: dialling the agent's own device");
        // The worker counts its wake turns from 1 again after a restart: what
        // an older turn left behind never reaches this one.
        let generation = self.generation;
        let waiting = id.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(etalk::call::VOICE_CALL.constants.turn_dial_ms)).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.turn_dial.as_ref().is_some_and(|turn| turn.id == waiting && !turn.ready) {
                    log::info!("[voice] wake turn {waiting}: not connected in time: heard here");
                    this.fall_back(&waiting, cx);
                }
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let connection = match connecting.await {
                Ok(connection) => connection,
                Err(err) => {
                    let _ = this.update(cx, |this, cx| {
                        log::info!("[voice] wake turn {id}: the dial was refused ({err:?}): heard here");
                        if this.generation == generation {
                            this.turn_failed(&id, cx);
                        }
                    });
                    return;
                }
            };
            let (link, rx) = (connection.link, connection.events);
            let kept = this.update(cx, |this, _| match this.turn_dial.as_mut().filter(|turn| turn.id == id && this.generation == generation) {
                Some(turn) => {
                    turn.link = Some(link);
                    true
                }
                None => {
                    // Ended (or fell back) while it dialled.
                    link.end();
                    false
                }
            });
            if !matches!(kept, Ok(true)) {
                return;
            }
            while let Ok(event) = rx.recv_async().await {
                let current = this.update(cx, |this, cx| {
                    if this.generation == generation {
                        this.on_turn_dial(&id, event, cx);
                    }
                    this.generation == generation
                });
                if !current.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    /// EXP-1859: the dialled turn's audio: up to its host (held until the
    /// link is up, at most the longest turn) and the capsule's wave.
    fn turn_audio(&mut self, id: &str, pcm: &str, cx: &mut Context<Self>) {
        let Some(turn) = self.turn_dial.as_mut().filter(|turn| turn.id == id) else { return };
        let Ok(samples) = etalk::decode_pcm(pcm) else { return };
        let peak = samples.iter().map(|sample| i32::from(*sample).unsigned_abs()).max().unwrap_or(0);
        self.level = (peak as f32 / 32768. * 2.).min(1.);
        // After `said` the microphone's own samples never reach the link.
        let samples = if turn.said { vec![0; samples.len()] } else { samples };
        match turn.link.as_ref() {
            Some(link) if turn.ready => link.send_audio(samples),
            _ => {
                let room = (etalk::CALL_RATE as usize * MAX_TURN_SECONDS).saturating_sub(turn.held.len());
                turn.held.extend(samples.into_iter().take(room));
            }
        }
        self.overlay.refresh(self.level, &[], cx);
    }

    /// EXP-1859: what the dialled turn's host (or the relay) sent.
    fn on_turn_dial(&mut self, id: &str, event: DialLinkEvent, cx: &mut Context<Self>) {
        let Some(turn) = self.turn_dial.as_mut().filter(|turn| turn.id == id) else { return };
        match event {
            DialLinkEvent::Ready => {
                log::info!("[voice] wake turn {id}: the agent's device is on the line");
                turn.ready = true;
                // What was said until now first, in order; live audio follows.
                if let Some(link) = turn.link.as_ref().filter(|_| !turn.held.is_empty()) {
                    link.send_audio(std::mem::take(&mut turn.held));
                }
                self.bound_turn(id, TURN_SAID_WITHIN, cx);
                let (waiting, generation) = (id.to_string(), self.generation);
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(etalk::call::VOICE_CALL.constants.turn_dial_ms)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.turn_dial.as_ref().is_some_and(|turn| turn.id == waiting && this.generation == generation && !turn.answered && !turn.said) {
                            log::info!("[voice] wake turn {waiting}: its host rings (an ordinary call): heard here");
                            this.fall_back(&waiting, cx);
                        }
                    });
                })
                .detach();
            }
            DialLinkEvent::Audio(pcm) => {
                if let Some(client) = self.client.as_ref() {
                    client.dial_play(id, &pcm, DIAL_DOWN_RATE);
                }
            }
            DialLinkEvent::Event(value) => match etalk::call::call_event_of_wire(&value) {
                Some(etalk::call::CallEvent::Failed(code)) => {
                    log::info!("[voice] wake turn {id}: its host failed it ({code})");
                    self.turn_failed(id, cx);
                }
                Some(etalk::call::CallEvent::HungUp(Some(reason))) => {
                    log::info!("[voice] wake turn {id}: ended ({reason})");
                    self.turn_failed(id, cx);
                }
                Some(etalk::call::CallEvent::End | etalk::call::CallEvent::HungUp(None)) => self.turn_hung_up(id, cx),
                Some(etalk::call::CallEvent::SpeechStarted) => {
                    // Barge-in: the reply still queued here stops.
                    if let Some(client) = self.client.as_ref() {
                        client.dial_flush(id);
                    }
                }
                // A turn host says nothing of a reply before `said`, and ONE
                // utterance: this one runs an ordinary call (an older build
                // ignores the ring's mode). Before `said` the turn is heard here.
                Some(etalk::call::CallEvent::ReplyStarted(_) | etalk::call::CallEvent::ReplyFinished | etalk::call::CallEvent::NoSound) if !turn.said => {
                    log::info!("[voice] wake turn {id}: its host runs an ordinary call: heard here");
                    self.turn_failed(id, cx);
                }
                Some(etalk::call::CallEvent::Said(_)) if turn.said => {
                    log::info!("[voice] wake turn {id}: its host heard a second utterance: ended");
                    self.turn_failed(id, cx);
                }
                Some(event) => {
                    let sent = turn.said;
                    let mut bound = None;
                    match &event {
                        etalk::call::CallEvent::Said(_) => {
                            turn.said = true;
                            // The turn is over: the frames go on as silence
                            // (the relay's idle clock), never the microphone.
                            if let Some(link) = turn.link.as_ref() {
                                link.set_muted(true);
                            }
                            bound = Some(Duration::from_millis(etalk::call::VOICE_CALL.constants.turn_reply_wait_ms) + TURN_REPLY_MARGIN);
                        }
                        etalk::call::CallEvent::Ready => turn.answered = true,
                        etalk::call::CallEvent::ReplyStarted(_) if !turn.replying => {
                            turn.replying = true;
                            bound = Some(TURN_REPLY_AT_MOST);
                        }
                        _ => {}
                    }
                    let newest = match &event {
                        etalk::call::CallEvent::ReplyStarted(first) => {
                            let agent_id = turn.agent_id.clone();
                            newest_reply(&agent_id, cx).unwrap_or_else(|| first.clone())
                        }
                        _ => String::new(),
                    };
                    for step in etalk::call::turn_capsule(&etalk::call::TurnCallEvent::Flow(event), sent, &newest) {
                        self.step(step, cx);
                    }
                    if let Some(bound) = bound {
                        self.bound_turn(id, bound, cx);
                    }
                }
                None => {}
            },
            DialLinkEvent::Ended { reason: None } => self.turn_hung_up(id, cx),
            DialLinkEvent::Ended { reason: Some(reason) } => {
                log::info!("[voice] wake turn {id}: ended ({reason})");
                self.turn_failed(id, cx);
            }
            DialLinkEvent::Failed(code) => {
                log::info!("[voice] wake turn {id}: the relay refused it ({code})");
                self.turn_failed(id, cx);
            }
            DialLinkEvent::Dropped(reason) => {
                log::info!("[voice] wake turn {id}: the link dropped ({reason})");
                self.turn_failed(id, cx);
            }
        }
    }

    /// EXP-1859: the dialled turn ends unless it moved on `within` this long.
    fn bound_turn(&mut self, id: &str, within: Duration, cx: &mut Context<Self>) {
        let stage = |this: &Self, id: &str| this.turn_dial.as_ref().filter(|turn| turn.id == id).map(|turn| (turn.said, turn.replying));
        let (id, armed, generation) = (id.to_string(), stage(self, id), self.generation);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(within).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && armed.is_some() && stage(this, &id) == armed {
                    log::info!("[voice] wake turn {id}: its host did not go on in time: ended");
                    this.turn_failed(&id, cx);
                }
            });
        })
        .detach();
    }

    /// EXP-1859: the dialled turn failed: before the host's `said` it is the
    /// local turn from here (nothing said is lost); after it the call only
    /// ends (the message is sent, the reply lands in the chat).
    fn turn_failed(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(said) = self.turn_dial.as_ref().filter(|turn| turn.id == id).map(|turn| turn.said) else { return };
        if !said {
            return self.fall_back(id, cx);
        }
        // The far side's failure (no reason of this side's, #EXP-2017).
        let agent_id = self.end_turn_dial(Ending::Cancelled, cx);
        self.after_turn_sent(agent_id, cx);
    }

    /// EXP-1859: the host hung up (reason-less): its turn is over.
    fn turn_hung_up(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(said) = self.turn_dial.as_ref().filter(|turn| turn.id == id).map(|turn| turn.said) else { return };
        let agent_id = self.end_turn_dial(Ending::HungUp, cx);
        for step in etalk::call::turn_capsule(&etalk::call::TurnCallEvent::Flow(etalk::call::CallEvent::HungUp(None)), said, "") {
            self.step(step, cx);
        }
        if said {
            self.after_turn_sent(agent_id, cx);
        }
    }

    /// EXP-1859: a dialled turn that is not answered becomes the local turn:
    /// the worker hears what it kept from the phrase's start.
    fn fall_back(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(turn) = self.turn_dial.take_if(|turn| turn.id == id) else { return };
        if let Some(link) = turn.link.as_ref() {
            link.end();
        }
        if let Some(client) = self.client.as_ref() {
            client.fallback(id.to_string());
        }
        cx.notify();
    }

    /// EXP-1859: the dialled turn ends here: the worker's turn hangs up, and
    /// the host hears it (`Cancelled`: reason-less, `Failed`: this side's
    /// failure; `HungUp`: the host ended it). Answers its agent.
    fn end_turn_dial(&mut self, ending: Ending, cx: &mut Context<Self>) -> Option<String> {
        let turn = self.turn_dial.take()?;
        if let Some(client) = self.client.as_ref() {
            client.hangup(turn.id.clone());
        }
        if let Some(link) = turn.link.as_ref() {
            match ending {
                Ending::Cancelled => link.end(),
                Ending::Failed => link.fail(),
                Ending::HungUp => {}
            }
        }
        // The worker answers a hung-up turn with nothing.
        if self.utterance.as_deref() == Some(turn.id.as_str()) {
            self.utterance = None;
            self.woken = false;
        }
        cx.notify();
        Some(turn.agent_id)
    }

    /// EXP-1859: the call ended after its host sent the turn: Thinking (if
    /// it still shows) ends when the reply lands in the chat.
    fn after_turn_sent(&mut self, agent_id: Option<String>, cx: &mut Context<Self>) {
        let Some(session_id) = agent_id.and_then(|agent| host::conversation(&agent, cx)) else { return };
        // A reply cut off by the call's end is no longer spoken.
        self.step(PttEvent::SpeakFinished, cx);
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64);
        // The host sent it a moment before its `said` reached here.
        self.sent = Some(Sent { session_id, message_id: None, at: Some(at - SENT_SLACK_MS) });
        self.on_messages(cx);
    }

    fn send(&mut self, heard: String, cx: &mut Context<Self>) {
        let Some(armed) = self.armed.clone() else { return };
        // EXP-1859: a wake turn goes to the agent whose phrase was heard.
        let turn_agent = self.turn_agent.clone().unwrap_or_else(|| armed.agent_id.clone());
        let Some(outgoing) = host::outgoing(&turn_agent, &heard, cx) else {
            return;
        };
        let text = match &outgoing.target {
            etalk::voice::UtteranceTarget::Session { text, .. } | etalk::voice::UtteranceTarget::Agent { text, .. } => text.clone(),
        };
        let generation = self.generation;
        let agent_id = turn_agent;
        let Some(job) = host::send(outgoing, cx) else {
            self.send_failed(agent_id, text, cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { job() }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(output) => {
                        // S-3: the reply to a spoken turn is read aloud.
                        crate::speak_replies::voice_turn(&output.session_id, &agent_id, cx);
                        this.sent = Some(Sent { session_id: output.session_id, message_id: output.message_id, at: None });
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
    fn on_messages(&mut self, cx: &mut Context<Self>) {
        if self.state.phase != PttPhase::Thinking {
            return;
        }
        let Some(sent) = self.sent.clone() else { return };
        let Some(host) = host::host(cx) else { return };
        // One pass: the sent row's instant, and the agent's final replies
        // in the conversation (the earliest after it wins).
        let mut sent_at = sent.at.map(Some);
        let mut replies: Vec<(i64, String, String)> = Vec::new();
        // #PERF-9: the conversation's rows only (was every message held).
        host.session_messages(&sent.session_id, cx, &mut |row| {
            let message = &row.message;
            if sent.message_id.as_deref() == Some(row.id) && sent_at.is_none() {
                sent_at = Some(message.created_at.and_then(etalk::voice::parse_instant));
            }
            let reply = message.session_id.unwrap_or("") == sent.session_id
                && message.author_kind == Some("agent")
                && message.author_session_id.is_none()
                && !message.has_card
                && message.body.is_some_and(|body| !body.trim().is_empty());
            if let Some(at) = message.created_at.and_then(etalk::voice::parse_instant).filter(|_| reply) {
                replies.push((at, row.id.to_string(), message.body.unwrap_or_default().to_string()));
            }
        });
        let Some(Some(sent_at)) = sent_at else { return };
        let reply = replies.into_iter().filter(|(at, _, _)| *at > sent_at).min_by_key(|(at, _, _)| *at).map(|(_, id, body)| (id, body));
        let Some((id, body)) = reply else { return };
        let speaking = crate::speak_replies::speaking(cx).is_some_and(|speaking| speaking.message_id == id);
        self.reply_id = Some(id);
        self.step(PttEvent::Replied { body, speaking }, cx);
    }

    fn on_speaker(&mut self, cx: &mut Context<Self>) {
        // EXP-1859: a dialled turn's reply is not this speaker's.
        if self.state.phase != PttPhase::Speaking || self.turn_dial.is_some() {
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
        // EXP-1859: a wake turn's capsule names the woken agent.
        self.label_of(self.turn_agent.clone().unwrap_or_else(|| armed.agent_id.clone()), cx)
    }

    fn label_of(&self, agent_id: String, cx: &App) -> Option<String> {
        let settings = VoiceSettings { agent_id: Some(agent_id), ..Default::default() };
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
        if self.state.phase == PttPhase::Listening {
            // A capsule opened (again) mid-hold draws the level at once.
            self.overlay.refresh(self.level, &self.bands, cx);
        }
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
    fn push_to_talk_is_armed_on_every_desktop_the_wake_word_on_the_voice_device() {
        assert_eq!(
            armed_for(&settings(), "d-mint"),
            Some(Armed { shortcut: Some("ctrl+space".into()), agent_id: "a1".into(), language: "en".into(), wake_phrase: None, wake_agent_ids: Vec::new() })
        );
        // EXP-1857: push to talk listens on the desktop you're at.
        assert_eq!(armed_for(&settings(), "d-mac").unwrap().shortcut.as_deref(), Some("ctrl+space"), "another desktop");
        let nowhere = VoiceSettings { voice_device_id: None, ..settings() };
        assert!(armed_for(&nowhere, "d-mac").is_some(), "no voice device at all (an Assistant on a server)");
        // S-3: no key bound = push to talk off.
        assert_eq!(armed_for(&VoiceSettings { ptt_shortcut: String::new(), ..settings() }, "d-mint"), None);
        assert_eq!(armed_for(&VoiceSettings { agent_id: None, ..settings() }, "d-mint"), None, "clearOnNone");
        // #NEW-56: the wake word alone arms the device, without a shortcut.
        let wake = VoiceSettings { ptt_shortcut: String::new(), wake_word: true, wake_phrase: "Computer".into(), ..settings() };
        let armed = armed_for(&wake, "d-mint").unwrap();
        assert_eq!((armed.shortcut, armed.wake_phrase.as_deref()), (None, Some("Computer")));
        assert_eq!(armed_for(&wake, "d-mac"), None, "the wake word stays on the voice device");
        let both = VoiceSettings { wake_word: true, wake_phrase: "Computer".into(), ..settings() };
        assert_eq!(armed_for(&both, "d-mac").unwrap().wake_phrase, None);
        // EXP-1859: the switched-on agents arm with the wake word, only on the voice device.
        let more = VoiceSettings { wake_agent_ids: vec!["a2".into()], ..both.clone() };
        assert_eq!(armed_for(&more, "d-mint").unwrap().wake_agent_ids, ["a2"]);
        assert!(armed_for(&more, "d-mac").unwrap().wake_agent_ids.is_empty());
        assert_eq!(armed_for(&VoiceSettings { agent_id: None, ..wake }, "d-mint"), None, "clearOnNone");
        let german = VoiceSettings { language: "de".into(), ..settings() };
        assert_eq!(armed_for(&german, "d-mint").unwrap().language, "de");
    }

    fn watcher(cx: &mut gpui::TestAppContext) -> Entity<PushToTalk> {
        cx.update(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
            start("d-mint".into(), None, None, cx)
        })
    }

    fn step(entity: &Entity<PushToTalk>, event: PttEvent, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| entity.update(cx, |this, cx| this.step(event, cx)));
    }

    /// EXP-1901: the capsule's hide closes its window from inside the
    /// watcher's own update; the window-close observer must not update the
    /// watcher again (gpui panics on the second lease).
    #[gpui::test]
    async fn the_capsule_hides_and_its_window_is_gone(cx: &mut gpui::TestAppContext) {
        let entity = watcher(cx);
        step(&entity, PttEvent::KeyDown { muted: true }, cx);
        assert_eq!(cx.windows().len(), 1, "the capsule shows");
        cx.executor().advance_clock(Duration::from_millis(1500));
        cx.run_until_parked();
        assert!(cx.windows().is_empty(), "the capsule is gone");
        assert_eq!(cx.update(|cx| entity.read(cx).state.phase), PttPhase::Hidden);
    }

    /// EXP-1901 (#1244 review kept): the capsule opening again, wider, for a
    /// longer line mid-hold is not the window the key is held in; an IDE
    /// window closing is, in the focused-window fallback.
    #[gpui::test]
    async fn a_wider_capsule_keeps_the_hold_an_ide_window_closing_ends_it(cx: &mut gpui::TestAppContext) {
        let entity = watcher(cx);
        let ide = cx.add_window(|_, _| gpui::Empty);
        cx.update(|cx| {
            entity.update(cx, |this, cx| {
                this.status = hotkey::Status::FocusedOnly(etalk::voice::ShortcutReason::NoPortal);
                this.held = true;
                this.step(PttEvent::KeyDown { muted: false }, cx);
            })
        });
        let capsule = |cx: &mut gpui::TestAppContext| cx.windows().into_iter().find(|window| window.window_id() != ide.window_id());
        let narrow = capsule(cx).expect("the capsule shows");
        step(&entity, PttEvent::Partial("a line long enough that the capsule has to open again, wider than before".into()), cx);
        cx.run_until_parked();
        let wide = capsule(cx).expect("the capsule shows again");
        assert_ne!(narrow.window_id(), wide.window_id(), "it opened again");
        assert_eq!(cx.windows().len(), 2, "one capsule");
        assert!(cx.update(|cx| entity.read(cx).held), "the hold goes on");
        cx.update(|cx| ide.update(cx, |_, window, _| window.remove_window())).unwrap();
        cx.run_until_parked();
        assert!(!cx.update(|cx| entity.read(cx).held), "the IDE window closing is the key-up");
    }

    /// EXP-1901: Unmute is a click inside the capsule's own window; gpui
    /// refuses a nested update of that window, so the close runs after it.
    #[gpui::test]
    async fn unmute_on_the_capsule_closes_it(cx: &mut gpui::TestAppContext) {
        let entity = watcher(cx);
        step(&entity, PttEvent::KeyDown { muted: true }, cx);
        let capsule = cx.windows()[0];
        cx.update(|cx| capsule.update(cx, |_, _, cx| on_pill(cx))).unwrap();
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| entity.read(cx).state.phase), PttPhase::Hidden);
        assert!(cx.windows().is_empty(), "the capsule is gone");
    }

    /// EXP-1901: a dropped grab's late answer (its thread reports on its own
    /// time) never overwrites the status of what is held now.
    #[gpui::test]
    async fn a_dropped_grabs_late_status_is_ignored(cx: &mut gpui::TestAppContext) {
        let entity = watcher(cx);
        let stale = cx.update(|cx| {
            entity.update(cx, |this, cx| {
                let stale = this.grab_events(cx);
                // Nothing armed: the regrab drops the grab and takes none.
                this.regrab(cx);
                stale
            })
        });
        stale.send(hotkey::HotkeyEvent::Status(hotkey::Status::Global)).unwrap();
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| status(cx)), hotkey::Status::Idle, "nothing is held");
        // The grab taken now still reports.
        let live = cx.update(|cx| entity.update(cx, |this, cx| this.grab_events(cx)));
        live.send(hotkey::HotkeyEvent::Status(hotkey::Status::Global)).unwrap();
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| status(cx)), hotkey::Status::Global);
    }

    use crate::test_support::{Rig, AGENT, SENT};

    fn key(down: bool, cx: &mut gpui::TestAppContext) {
        let event = if down { hotkey::HotkeyEvent::Down } else { hotkey::HotkeyEvent::Up };
        cx.update(|cx| global(cx).unwrap().update(cx, |this, cx| this.on_hotkey(event, cx)));
        cx.run_until_parked();
    }

    fn phase(cx: &mut gpui::TestAppContext) -> PttPhase {
        cx.update(|cx| global(cx).unwrap().read(cx).state.phase)
    }

    /// #EXP-1936: Stop on the capsule (the speaker's `stop`) ends Speaking:
    /// the capsule follows the speaker, which has to say that it stopped.
    #[gpui::test]
    async fn stop_on_the_capsule_ends_speaking(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        key(true, cx);
        key(false, cx);
        rig.worker(etalk::Event::Heard { id: "ptt-1".into(), text: "status of the release".into() }, cx);
        assert_eq!(phase(cx), PttPhase::Thinking);
        rig.message(SENT, "user", "status of the release", 1, cx);
        rig.message("r1", "agent", "All green. Ship it.", 5, cx);
        assert_eq!(rig.spoken(), ["r1"], "a voice turn's reply is read aloud");
        assert_eq!(phase(cx), PttPhase::Speaking);
        cx.update(on_pill);
        cx.run_until_parked();
        assert_eq!(rig.requests(), [etalk::Request::Stop]);
        assert_eq!(phase(cx), PttPhase::Spoken, "the capsule left Speaking");
        assert_eq!(cx.update(|cx| crate::speak_replies::speaking(cx)), None);
        // The worker's answer to the Stop finds nothing left to end.
        rig.worker(etalk::Event::Finished { id: "r1".into(), stopped: true }, cx);
        assert_eq!(phase(cx), PttPhase::Spoken);
    }

    /// #EXP-1936: a call takes the microphone, not the binding: the held
    /// utterance is dropped, the key stays grabbed and the worker keeps its
    /// wake word, so both work again the moment the call ends.
    #[gpui::test]
    async fn a_call_leaves_push_to_talk_and_the_wake_word_armed(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let armed = |cx: &mut gpui::TestAppContext| {
            cx.update(|cx| {
                let entity = global(cx).unwrap();
                let this = entity.read(cx);
                (this.armed.clone(), this.grab_generation, this.wake_sent.clone())
            })
        };
        let before = armed(cx);
        assert!(before.0.is_some() && before.2.is_some(), "armed, the wake word handed over");
        key(true, cx);
        assert_eq!(phase(cx), PttPhase::Listening);
        rig.requests();
        cx.update(|cx| crate::voice_call::start(AGENT.into(), cx));
        let requests = rig.requests();
        assert!(requests.contains(&etalk::Request::CancelListen { id: "ptt-1".into() }), "the held utterance is dropped: {requests:?}");
        assert!(!requests.iter().any(|request| matches!(request, etalk::Request::Wake { .. })), "the wake word stays: {requests:?}");
        assert_eq!(phase(cx), PttPhase::Hidden);
        assert_eq!(armed(cx), before, "armed through the call");
        cx.update(crate::voice_call::end);
        assert_eq!(armed(cx), before, "and after it");
    }

    /// EXP-1859: a dialled wake turn's link that records what the turn did to it.
    #[derive(Clone, Default)]
    struct TurnLink(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

    impl DialLink for TurnLink {
        fn send_audio(&self, pcm: Vec<i16>) {
            self.0.borrow_mut().push(format!("audio {}", pcm.len()));
        }
        fn set_muted(&self, muted: bool) {
            self.0.borrow_mut().push(format!("mute {muted}"));
        }
        fn end(&self) {
            self.0.borrow_mut().push("end".into());
        }
        fn fail(&self) {
            self.0.borrow_mut().push("end dropped".into());
        }
    }

    /// The wake word with `a-ops` as a dial listener (its call runs elsewhere).
    fn dial_listener(rig: &Rig, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            global(cx).unwrap().update(cx, |this, _| {
                let listener = etalk::WakeListener { agent_id: "a-ops".into(), phrase: "Hey Ops".into(), language: "en".into(), dial: true };
                this.wake_sent = Some(("Hey Ops".into(), "en".into(), vec![listener]));
            })
        });
        rig.requests();
    }

    /// EXP-1859: a woken dialled turn, its call connected to `link`.
    fn dialled_turn(rig: &Rig, link: &TurnLink, cx: &mut gpui::TestAppContext) {
        dial_listener(rig, cx);
        rig.worker(etalk::Event::Woke { id: "wake-1".into(), agent_id: Some("a-ops".into()) }, cx);
        cx.update(|cx| {
            global(cx).unwrap().update(cx, |this, _| {
                let turn = TurnDial { id: "wake-1".into(), agent_id: "a-ops".into(), link: Some(Box::new(link.clone())), held: Vec::new(), ready: true, answered: false, said: false, replying: false };
                this.turn_dial = Some(turn);
            })
        });
        assert_eq!(phase(cx), PttPhase::Listening, "the wake opens the capsule like a key-down");
        rig.requests();
    }

    fn turn_event(event: DialLinkEvent, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| global(cx).unwrap().update(cx, |this, cx| this.on_turn_dial("wake-1", event, cx)));
        cx.run_until_parked();
    }

    /// EXP-1859: a dial listener's wake with no way to dial (no relay here)
    /// is the local turn at once: the worker's Fallback, the capsule listening.
    #[gpui::test]
    async fn a_dialled_wake_turn_that_cannot_dial_is_heard_here(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        dial_listener(&rig, cx);
        rig.worker(etalk::Event::Woke { id: "wake-1".into(), agent_id: Some("a-ops".into()) }, cx);
        assert!(rig.requests().contains(&etalk::Request::Fallback { id: "wake-1".into() }));
        assert_eq!(phase(cx), PttPhase::Listening, "it keeps listening");
        cx.update(|cx| assert!(global(cx).unwrap().read(cx).turn_dial.is_none()));
        // A local turn's Heard ends it as any wake turn (sent to the woken agent).
        rig.worker(etalk::Event::Heard { id: "wake-1".into(), text: "Status?".into() }, cx);
        assert_ne!(phase(cx), PttPhase::Listening);
    }

    /// EXP-1859 (`turnCapsuleCases`): the dialled turn's audio goes up, the
    /// call's events drive the capsule, the host's reason-less end hangs the
    /// worker's turn up; nothing tells the host twice.
    #[gpui::test]
    async fn a_dialled_wake_turn_follows_its_call(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        let pcm = etalk::encode_pcm(&[1000i16; 320]);
        rig.worker(etalk::Event::DialAudio { id: "wake-1".into(), pcm }, cx);
        assert_eq!(*link.0.borrow(), ["audio 320"]);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "rang" })), cx);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "partial", "text": "what's new" })), cx);
        assert_eq!(phase(cx), PttPhase::Listening);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "said", "text": "What's new?" })), cx);
        assert_eq!(phase(cx), PttPhase::Thinking, "heard and sent at once");
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "replyStarted", "text": "Two issues." })), cx);
        assert_eq!(phase(cx), PttPhase::Speaking);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "replyFinished" })), cx);
        assert_eq!(phase(cx), PttPhase::Spoken);
        turn_event(DialLinkEvent::Ended { reason: None }, cx);
        assert!(rig.requests().contains(&etalk::Request::Hangup { id: "wake-1".into() }));
        assert_eq!(*link.0.borrow(), ["audio 320", "mute true"], "the host hung up: nothing to tell it");
        cx.update(|cx| assert!(global(cx).unwrap().read(cx).turn_dial.is_none()));
    }

    /// EXP-1859: a click on the pill cancels a LOCAL wake turn while it is
    /// heard: the Listen dropped, the capsule hidden, a late Heard unsent.
    #[gpui::test]
    async fn the_pill_cancels_a_local_wake_turn(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        rig.worker(etalk::Event::Woke { id: "wake-1".into(), agent_id: None }, cx);
        assert_eq!(phase(cx), PttPhase::Listening);
        rig.requests();
        cx.update(on_pill);
        assert_eq!(rig.requests(), [etalk::Request::CancelListen { id: "wake-1".into() }]);
        assert_eq!(phase(cx), PttPhase::Hidden);
        rig.worker(etalk::Event::Heard { id: "wake-1".into(), text: "Status?".into() }, cx);
        assert!(rig.sent().is_empty(), "nothing is sent");
        // A held key's Listen is no wake turn: the pill leaves it.
        key(true, cx);
        rig.requests();
        cx.update(on_pill);
        assert!(rig.requests().is_empty());
        assert_eq!(phase(cx), PttPhase::Listening);
    }

    /// EXP-1859: the pill ends a dialled turn in every phase; the speaker's
    /// own changes never end its Speaking.
    #[gpui::test]
    async fn the_pill_cancels_a_dialled_wake_turn(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        cx.update(on_pill);
        assert!(rig.requests().contains(&etalk::Request::Hangup { id: "wake-1".into() }));
        assert_eq!((phase(cx), link.0.borrow().clone()), (PttPhase::Hidden, vec!["end".to_string()]));
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "said", "text": "Status?" })), cx);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "replyStarted", "text": "Green." })), cx);
        assert_eq!(phase(cx), PttPhase::Speaking, "before the speaker changes");
        rig.worker(etalk::Event::Voices { voices: vec!["en".into()] }, cx);
        assert_eq!(phase(cx), PttPhase::Speaking, "after Voices");
        cx.update(|cx| crate::speak_replies::entity(cx).unwrap().update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
        assert_eq!(phase(cx), PttPhase::Speaking);
        cx.update(on_pill);
        assert_eq!((phase(cx), link.0.borrow().clone()), (PttPhase::Spoken, vec!["mute true".to_string(), "end".to_string()]));
    }

    /// EXP-1859: an agent's device that comes online makes its wake turn a
    /// dialled one; a change that moves nothing sends nothing. The worker's
    /// Ready (inside the speaker's update) reads the target without a panic.
    #[gpui::test]
    async fn the_devices_decide_where_a_wake_turn_runs(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let dials = |cx: &mut gpui::TestAppContext| {
            cx.run_until_parked();
            let requests = rig.requests();
            let wakes: Vec<bool> = requests
                .iter()
                .filter_map(|request| if let etalk::Request::Wake { listeners, .. } = request { Some(listeners.iter().any(|listener| listener.dial)) } else { None })
                .collect();
            wakes
        };
        rig.requests();
        rig.runs_on("d-other", false, cx);
        assert_eq!(dials(cx), [false], "its device is offline: the turn runs here");
        rig.runs_on("d-other", true, cx);
        assert_eq!(dials(cx), [true], "it came online");
        rig.runs_on("d-other", true, cx);
        assert!(dials(cx).is_empty(), "nothing moved");
        rig.worker(etalk::Event::Ready { protocol: etalk::PROTOCOL }, cx);
        assert_eq!(dials(cx), [true], "a new worker is told again");
    }

    /// EXP-1859: whatever its host does, a dialled turn ends: an ordinary
    /// call's host (an older build) is noticed and the turn heard here, one
    /// that never says it too, a second `said` or a missing reply ends it.
    /// After `said` nothing of the microphone leaves.
    #[gpui::test]
    async fn nothing_a_host_does_keeps_a_dialled_turn_open(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let event = |value: serde_json::Value, cx: &mut gpui::TestAppContext| turn_event(DialLinkEvent::Event(value), cx);
        let gone = |cx: &mut gpui::TestAppContext| cx.update(|cx| global(cx).unwrap().read(cx).turn_dial.is_none());
        let fallback = etalk::Request::Fallback { id: "wake-1".into() };
        let hangup = etalk::Request::Hangup { id: "wake-1".into() };
        let fresh = |cx: &mut gpui::TestAppContext| {
            let link = TurnLink::default();
            dialled_turn(&rig, &link, cx);
            turn_event(DialLinkEvent::Ready, cx);
            link
        };
        // An older host: its ring answers, then it picks up like a call.
        for old in [serde_json::json!({ "t": "noSound" }), serde_json::json!({ "t": "replyFinished" }), serde_json::json!({ "t": "replyStarted", "text": "Hi." })] {
            let link = fresh(cx);
            event(serde_json::json!({ "t": "rang" }), cx);
            event(serde_json::json!({ "t": "ready" }), cx);
            assert!(!gone(cx));
            event(old, cx);
            assert_eq!(rig.requests(), [fallback.clone()]);
            assert_eq!(*link.0.borrow(), ["end"]);
            assert_eq!(phase(cx), PttPhase::Listening, "heard here");
        }
        // It only rings: noticed within turnDialMs.
        let dial = Duration::from_millis(etalk::call::VOICE_CALL.constants.turn_dial_ms);
        let link = fresh(cx);
        event(serde_json::json!({ "t": "rang" }), cx);
        cx.executor().advance_clock(dial + Duration::from_millis(100));
        cx.run_until_parked();
        assert!(gone(cx));
        assert_eq!(rig.requests(), [fallback.clone()]);
        assert_eq!(*link.0.borrow(), ["end"]);
        // Never said (the timers the turns above left behind end nothing).
        let link = fresh(cx);
        event(serde_json::json!({ "t": "ready" }), cx);
        cx.executor().advance_clock(TURN_SAID_WITHIN - Duration::from_secs(1));
        cx.run_until_parked();
        assert!(!gone(cx));
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        assert!(gone(cx));
        assert_eq!(rig.requests(), [fallback]);
        assert_eq!(*link.0.borrow(), ["end"]);
        // A second said; no word of the microphone after the first.
        let link = fresh(cx);
        event(serde_json::json!({ "t": "said", "text": "Status?" }), cx);
        assert_eq!(*link.0.borrow(), ["mute true"]);
        event(serde_json::json!({ "t": "said", "text": "And more." }), cx);
        assert_eq!(rig.requests(), [hangup.clone()]);
        assert_eq!(link.0.borrow().last().map(String::as_str), Some("end"));
        // No reply, and a reply without an end.
        for replying in [false, true] {
            let link = fresh(cx);
            event(serde_json::json!({ "t": "said", "text": "Status?" }), cx);
            if replying {
                event(serde_json::json!({ "t": "replyStarted", "text": "Green." }), cx);
                cx.executor().advance_clock(Duration::from_secs(100));
                cx.run_until_parked();
                assert!(!gone(cx), "a long reply plays on");
            }
            cx.executor().advance_clock(if replying { TURN_REPLY_AT_MOST } else { Duration::from_secs(71) });
            cx.run_until_parked();
            assert!(gone(cx));
            assert_eq!(rig.requests(), [hangup.clone()]);
            assert_eq!(link.0.borrow().last().map(String::as_str), Some("end"));
        }
    }

    /// EXP-1859: until `call_ready` the turn's audio is held, not handed to
    /// the link (it would drop it); then ALL of it goes first, in order, then
    /// the live audio. A cancel during the wait sends none of it.
    #[gpui::test]
    async fn a_dialled_turns_audio_waits_for_the_host(cx: &mut gpui::TestAppContext) {
        #[derive(Clone, Default)]
        struct Samples(std::rc::Rc<std::cell::RefCell<Vec<i16>>>);
        impl DialLink for Samples {
            fn send_audio(&self, pcm: Vec<i16>) {
                self.0.borrow_mut().extend(pcm);
            }
            fn set_muted(&self, _: bool) {}
            fn end(&self) {}
            fn fail(&self) {}
        }
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let said = |from: i16, cx: &mut gpui::TestAppContext| {
            let pcm: Vec<i16> = (from..from + 320).collect();
            rig.worker(etalk::Event::DialAudio { id: "wake-1".into(), pcm: etalk::encode_pcm(&pcm) }, cx);
        };
        for cancel in [false, true] {
            let link = Samples::default();
            dial_listener(&rig, cx);
            rig.worker(etalk::Event::Woke { id: "wake-1".into(), agent_id: Some("a-ops".into()) }, cx);
            cx.update(|cx| {
                global(cx).unwrap().update(cx, |this, _| {
                    this.turn_dial = Some(TurnDial { id: "wake-1".into(), agent_id: "a-ops".into(), link: None, held: Vec::new(), ready: false, answered: false, said: false, replying: false });
                })
            });
            rig.requests();
            said(0, cx);
            cx.update(|cx| global(cx).unwrap().update(cx, |this, _| this.turn_dial.as_mut().unwrap().link = Some(Box::new(link.clone()))));
            said(320, cx);
            assert!(link.0.borrow().is_empty(), "nothing before call_ready");
            assert_eq!(phase(cx), PttPhase::Listening);
            if cancel {
                cx.update(on_pill);
                turn_event(DialLinkEvent::Ready, cx);
                said(640, cx);
                assert!(link.0.borrow().is_empty(), "cancelled: nothing leaves");
                continue;
            }
            turn_event(DialLinkEvent::Ready, cx);
            said(640, cx);
            turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "ready" })), cx);
            assert_eq!(*link.0.borrow(), (0..960).collect::<Vec<i16>>(), "all of it, in order");
            // A cold host: no timer of this side ends the wait.
            cx.executor().advance_clock(Duration::from_secs(20));
            cx.run_until_parked();
            assert_eq!(phase(cx), PttPhase::Listening);
            assert!(!rig.requests().iter().any(|request| matches!(request, etalk::Request::Fallback { .. } | etalk::Request::Hangup { .. })));
            turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "said", "text": "Status?" })), cx);
            assert_eq!(phase(cx), PttPhase::Thinking);
            cx.update(on_pill);
            rig.requests();
        }
    }

    /// EXP-1859: a failure before the host's `said` falls back to the local
    /// turn; after it the call only ends; the key replaces a dialled turn
    /// (its call ends reason-less first).
    #[gpui::test]
    async fn a_dialled_wake_turn_falls_back_fails_and_is_cancelled(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "failed", "code": "no_answer" })), cx);
        assert_eq!(rig.requests(), [etalk::Request::Fallback { id: "wake-1".into() }]);
        assert_eq!(phase(cx), PttPhase::Listening, "the capsule keeps listening");
        assert_eq!(std::mem::take(&mut *link.0.borrow_mut()), ["end"]);
        // After `said`, a failure ends it: the message is sent.
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        turn_event(DialLinkEvent::Event(serde_json::json!({ "t": "said", "text": "Status?" })), cx);
        turn_event(DialLinkEvent::Dropped("gone".into()), cx);
        assert_eq!(rig.requests(), [etalk::Request::Hangup { id: "wake-1".into() }]);
        assert_eq!(phase(cx), PttPhase::Thinking, "Thinking until the reply lands in the chat");
        assert_eq!(*link.0.borrow(), ["mute true", "end"], "the far side's failure");
        // The key replaces a dialled turn.
        let link = TurnLink::default();
        dialled_turn(&rig, &link, cx);
        key(true, cx);
        let requests = rig.requests();
        assert_eq!(requests.first(), Some(&etalk::Request::Hangup { id: "wake-1".into() }), "{requests:?}");
        assert!(requests.iter().any(|request| matches!(request, etalk::Request::Listen { .. })), "{requests:?}");
        assert_eq!(*link.0.borrow(), ["end"], "reason-less");

    /// EXP-1859: a local wake turn of a TEAM agent: into ITS chat, its reply
    /// spoken, the capsule Speaking, then Replied.
    #[gpui::test]
    async fn a_team_agents_local_wake_turn_speaks_its_reply(cx: &mut gpui::TestAppContext) {
        use crate::test_support::{Row, TEAM_AGENT, TEAM_CONVERSATION, SENT};
        let rig = Rig::install(VoiceSettings { wake_word: true, ..crate::test_support::settings() }, cx);
        rig.worker(etalk::Event::Woke { id: "wake-1".into(), agent_id: Some(TEAM_AGENT.into()) }, cx);
        rig.worker(etalk::Event::Heard { id: "wake-1".into(), text: "Status?".into() }, cx);
        assert_eq!(rig.sent(), [(Some(TEAM_CONVERSATION.to_string()), "Status?".to_string())]);
        assert_eq!(phase(cx), PttPhase::Thinking);
        let row = |id: &str, author_kind, second: u32| Row { id: id.into(), author_kind, body: "All green.".into(), has_card: false, created_at: format!("2026-11-17T10:00:{second:02}.000Z"), session_id: Some(TEAM_CONVERSATION) };
        rig.row(row(SENT, "user", 5), cx);
        rig.row(row("r1", "agent", 6), cx);
        assert_eq!(rig.spoken(), ["r1"]);
        assert_eq!(phase(cx), PttPhase::Speaking);
        rig.worker(etalk::Event::Finished { id: "r1".into(), stopped: false }, cx);
        assert_eq!(phase(cx), PttPhase::Spoken);
    }

    /// #EXP-1936: the speech worker died while the key was held: no Heard,
    /// Empty or ListenFailed will ever come, so the capsule fails now.
    #[gpui::test]
    async fn a_worker_lost_mid_hold_fails_the_capsule(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        key(true, cx);
        assert_eq!(phase(cx), PttPhase::Listening);
        rig.worker(etalk::Event::EXITED, cx);
        assert_eq!(phase(cx), PttPhase::Failed(etalk::voice::ListenFailure::Failed));
        rig.requests();
        key(false, cx);
        assert!(!rig.requests().iter().any(|request| matches!(request, etalk::Request::EndListen { .. })), "nothing left to end");
        assert_eq!(phase(cx), PttPhase::Failed(etalk::voice::ListenFailure::Failed));
    }

    fn ends(requests: &[etalk::Request]) -> usize {
        requests.iter().filter(|request| matches!(request, etalk::Request::EndListen { .. })).count()
    }

    fn set_status(status: hotkey::Status, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| global(cx).unwrap().update(cx, |this, cx| this.on_hotkey(hotkey::HotkeyEvent::Status(status), cx)));
    }

    /// #EXP-1938: while computer use holds the person's input the chord is
    /// an agent's synthetic keystroke: no microphone, no capsule, no
    /// barge-in. The person's own press works again after the hold.
    #[gpui::test]
    async fn an_agents_keystroke_never_fires_push_to_talk(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        rig.requests();
        rig.hold_input(true);
        key(true, cx);
        assert_eq!(phase(cx), PttPhase::Hidden, "no capsule over the screen the agent works on");
        assert!(cx.windows().is_empty());
        key(false, cx);
        assert_eq!(rig.requests(), [], "the microphone never opened");
        rig.hold_input(false);
        key(true, cx);
        assert_eq!(phase(cx), PttPhase::Listening, "the person's own press");
    }

    /// #EXP-1938: computer use takes the keyboard while the person holds
    /// the key: its release never arrives (X11 floats the keyboard), so the
    /// hold ends with the take and the system-wide grab is taken anew.
    #[gpui::test]
    async fn a_key_held_into_a_computer_use_hold_is_released(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        set_status(hotkey::Status::Global, cx);
        let grabs = |cx: &mut gpui::TestAppContext| cx.update(|cx| global(cx).unwrap().read(cx).grab_generation);
        // Nothing held: the hold changes nothing.
        let before = grabs(cx);
        cx.update(input_taken);
        assert_eq!(grabs(cx), before);
        key(true, cx);
        assert_eq!(phase(cx), PttPhase::Listening);
        rig.requests();
        rig.hold_input(true);
        cx.update(input_taken);
        cx.run_until_parked();
        assert_eq!(rig.requests(), [etalk::Request::EndListen { id: "ptt-1".into() }], "the microphone closes with the take");
        assert!(!cx.update(|cx| global(cx).unwrap().read(cx).held));
        assert_eq!(grabs(cx), before + 1, "the grab that still holds the key is let go");
        assert_eq!(cx.update(|cx| status(cx)), hotkey::Status::Global, "the shortcut reads as held throughout");
        // The release that comes late, or never, ends nothing twice.
        key(false, cx);
        assert_eq!(rig.requests(), []);
    }

    /// The capsule of a hold taken in `ide` through the focused-window
    /// fallback (no portal), and the chord.
    fn focused_hold(ide: gpui::AnyWindowHandle, cx: &mut gpui::TestAppContext) -> (gpui::AnyWindowHandle, gpui::Keystroke) {
        set_status(hotkey::Status::FocusedOnly(etalk::voice::ShortcutReason::NoPortal), cx);
        cx.update(|cx| ide.update(cx, |_, window, _| window.activate_window())).unwrap();
        cx.run_until_parked();
        let chord = gpui::Keystroke::parse("ctrl-space").unwrap();
        assert!(cx.update(|cx| on_focused_key(true, &chord, cx)), "the shell's listener takes the key");
        cx.run_until_parked();
        assert_eq!(phase(cx), PttPhase::Listening);
        let capsule = cx.update(|cx| global(cx).unwrap().read(cx).overlay.window_id()).expect("the capsule shows");
        let capsule = cx.windows().into_iter().find(|window| window.window_id() == capsule).unwrap();
        (capsule, chord)
    }

    /// #EXP-1938: a compositor that gives the capsule a plain window
    /// focuses it (Wayland without wlr-layer-shell): the IDE window's
    /// deactivation is not the key-up, and the release reaches the capsule.
    #[gpui::test]
    async fn the_capsule_taking_the_focus_keeps_a_focused_window_hold(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        let ide = cx.add_window(|_, _| gpui::Empty);
        let (capsule, chord) = focused_hold(ide.into(), cx);
        rig.requests();
        cx.update(|cx| capsule.update(cx, |_, window, _| window.activate_window())).unwrap();
        // The shell's activation observer, for the IDE window.
        cx.update(on_window_deactivated);
        cx.executor().advance_clock(FOCUS_SETTLE * 4);
        cx.run_until_parked();
        assert_eq!(ends(&rig.requests()), 0, "the capsule opening is no key-up");
        assert_eq!(phase(cx), PttPhase::Listening);
        let mut in_capsule = gpui::VisualTestContext::from_window(capsule, cx);
        in_capsule.simulate_event(gpui::KeyUpEvent { keystroke: chord });
        cx.run_until_parked();
        assert_eq!(ends(&rig.requests()), 1, "the release in the capsule ends the hold");
    }

    /// #EXP-1938 (#1244 review kept): the focus going anywhere else is the
    /// key-up, from the IDE window or from a capsule that had it.
    #[gpui::test]
    async fn a_focused_window_hold_ends_when_the_focus_leaves(cx: &mut gpui::TestAppContext) {
        let rig = Rig::install(crate::test_support::settings(), cx);
        let ide = cx.add_window(|_, _| gpui::Empty);
        let other = cx.add_window(|_, _| gpui::Empty);
        let (capsule, _) = focused_hold(ide.into(), cx);
        rig.requests();
        cx.update(|cx| capsule.update(cx, |_, window, _| window.activate_window())).unwrap();
        cx.update(on_window_deactivated);
        cx.executor().advance_clock(FOCUS_SETTLE * 4);
        cx.run_until_parked();
        assert_eq!(ends(&rig.requests()), 0);
        // No call from the shell here: the capsule's own window says it.
        cx.update(|cx| other.update(cx, |_, window, _| window.activate_window())).unwrap();
        cx.run_until_parked();
        cx.executor().advance_clock(FOCUS_SETTLE * 4);
        cx.run_until_parked();
        assert_eq!(ends(&rig.requests()), 1, "another window took the focus from the capsule");
        assert!(!cx.update(|cx| global(cx).unwrap().read(cx).held));
    }

    #[test]
    fn every_listen_event_is_routed_here() {
        assert!(is_listen_event(&etalk::Event::Empty { id: "u".into() }));
        assert!(is_listen_event(&etalk::Event::Heard { id: "u".into(), text: "x".into() }));
        assert!(is_listen_event(&etalk::Event::Woke { id: "wake-1".into(), agent_id: None }));
        assert!(is_listen_event(&etalk::Event::WakeListening { listening: true, reason: None }));
        assert!(!is_listen_event(&etalk::Event::Ready { protocol: etalk::PROTOCOL }));
        assert!(!is_listen_event(&etalk::Event::Finished { id: "m".into(), stopped: false }));
    }
}
