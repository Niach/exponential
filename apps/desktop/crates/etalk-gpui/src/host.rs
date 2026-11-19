//! #EXP-1422: what eTalk's surfaces need from the app they run in, as plain
//! data. Exponential implements [`TalkHost`] over its synced store, its
//! server and its relay (`ui/src/talk_host.rs`); a host without a server
//! (spezl) answers from its one local agent session.

use std::path::PathBuf;
use std::rc::Rc;

use etalk::meeting::{AudioApp, DesktopAudioCapability, MeetingStopReason};
use etalk::voice::{UtteranceTarget, VoiceMessage, VoiceSettings};
use gpui::{App, Global, Task};
use gpui_component::Icon;

/// What the host brands the tray and the floating windows with.
#[derive(Clone, Copy, Debug)]
pub struct Brand {
    /// The tray's title and tooltip.
    pub name: &'static str,
    /// The app icon the tray rasterizes: a 100-unit square SVG with a white
    /// disc (the dots and rings are drawn into its corner).
    pub tray_icon_svg: &'static [u8],
    /// The X11/Wayland app id of the overlay and call pill windows.
    pub app_id: &'static str,
    /// The Linux StatusNotifierItem id of the tray.
    pub tray_id: &'static str,
}

/// The accounts signed in on this machine (the speech data cleanup).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignedIn {
    /// The account the settings arriving now belong to.
    pub active: Option<String>,
    /// Every account signed in here.
    pub all: Vec<String>,
}

/// One agent a voice can talk to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TalkAgent {
    /// The bare name (`Studio`): the tray's Call, the wake phrase, the pill.
    pub name: String,
    /// The call surfaces' name for it.
    pub title: String,
    /// Its team (an utterance to an agent with no conversation yet names it).
    pub team_id: String,
    /// Its icon and `#rrggbb` colour (the call's avatar).
    pub icon: Option<String>,
    pub color: Option<String>,
    /// EXP-1857: its Runs on device (a call rings it when it is one of the
    /// member's own desktops that speaks).
    pub device_id: Option<String>,
}

/// EXP-1857: one of the member's OWN desktops as a call's target reads it
/// (voice-call.json `callTarget` `desktops`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TalkDesktop {
    pub id: String,
    /// Its name (the id when it has none).
    pub name: String,
    /// Online by its presence.
    pub online: bool,
    /// Its doctor row `speech` state (`ok`, `off`, `missing`, `error`, ...;
    /// None = no row), read through `etalk::call::call_speech_of`.
    pub speech: Option<String>,
}

/// #EXP-1847 (agent-settings.json `voice`): the voice one agent speaks
/// with on this device, resolved against the member's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentVoice {
    /// What the worker is prepared with: the language, a LOCAL clip path
    /// (empty = the built-in voice) and its transcript.
    pub voice: etalk::VoiceChoice,
    /// The wake phrase (empty = `Hey {agent}`).
    pub wake_phrase: String,
    /// FEED-276: where the agent's own clip is (`Fetching` = `voice` is
    /// the built-in one until it lands; a call's `ready` waits for it).
    pub clip: etalk::call::CallVoice,
}

impl AgentVoice {
    /// The member's own voice (a host without per-agent voices).
    pub fn member(settings: &VoiceSettings) -> Self {
        Self {
            voice: etalk::VoiceChoice {
                language: settings.language().to_string(),
                file: settings.voice_file.clone(),
                transcript: settings.voice_transcript.clone(),
            },
            wake_phrase: settings.wake_phrase.clone(),
            clip: etalk::call::CallVoice::None,
        }
    }
}

/// One message row of the host's conversations, borrowed while talk reads it.
#[derive(Clone, Debug)]
pub struct TalkMessage<'a> {
    pub id: &'a str,
    pub message: VoiceMessage<'a>,
}

/// An utterance (or a meeting part) on its way to the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub target: UtteranceTarget,
    /// A meeting batch (`meeting: true`).
    pub meeting: bool,
    /// The id a retried meeting part keeps (the receiver dedupes it).
    pub client_message_id: Option<String>,
}

/// What a send answered: the conversation and the sent row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sent {
    pub session_id: String,
    pub message_id: Option<String>,
}

/// A send that did not go through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendError {
    pub message: String,
    /// The receiver refused it for good (Exponential: a 4xx other than 401,
    /// 408, 429): a meeting batch is dropped, never retried.
    pub refused: bool,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// A prepared send, run on a background thread.
pub type SendJob = Box<dyn FnOnce() -> Result<Sent, SendError> + Send>;

/// #EXP-1961: a prepared `voice.callSession` (its session id, None = none).
pub type CallSessionJob = Box<dyn FnOnce() -> Option<String> + Send>;

/// A change to the member's voice settings the host stores; the stored
/// settings come back through [`crate::set_settings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceWrite {
    /// The tray's Wake word item.
    WakeWord(bool),
    /// THE consent dialog's Start.
    StartMeeting,
    /// Stop at once (None = by hand).
    StopMeeting(Option<MeetingStopReason>),
}

/// The voice device as this client knows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TalkDevice {
    /// Its name (None = the id stands in).
    pub label: Option<String>,
    /// It is online (this machine always is).
    pub online: bool,
}

/// The icons the surfaces draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TalkIcon<'a> {
    /// The brand mark (the capsule's wave, the Assistant's avatar).
    Brand,
    /// An icon concept by name (`voice-call.json` `icons`); an unknown one
    /// is the host's telephone.
    Concept(&'a str),
    /// An agent's icon in its colour.
    Agent { icon: &'a str, color: Option<&'a str> },
}

/// A host surface talk asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Bring a window forward, or open one when every window is closed.
    Window,
    /// The voice settings.
    VoiceSettings,
    /// A window with THE meeting consent dialog.
    MeetingConsent,
}

/// Something the member is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// An error toast with these words.
    Error(String),
    /// A failed action (`Could not {verb} {noun}`), with its reason.
    Failed { verb: &'static str, noun: &'static str, reason: Option<String> },
    /// A meeting stopped itself after `hours` (toast + OS notification).
    MeetingAutoStopped { hours: i64 },
}

/// What a streamed call's caller sends (the relay's call host socket).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallLinkEvent {
    /// The caller is connected.
    Ready,
    /// The caller's audio: mono 16-bit PCM.
    Audio(Vec<i16>),
    /// #EXP-1941: this many ms of the caller's audio were lost before the
    /// next audio (a sealed call's socket counts it).
    Lost { ms: u32 },
    Mute(bool),
    /// The caller (or the relay) hung up, with its reason.
    Ended { reason: Option<String> },
    /// The link failed or closed without a hang-up.
    Dropped(String),
}

/// A streamed call's link back to its caller.
pub trait CallLink: 'static {
    /// A flow event (`call_event`, JSON).
    fn send_event(&self, event: serde_json::Value);
    /// Reply audio for the caller: mono 16-bit PCM at `rate` Hz.
    fn send_speech(&self, pcm: Vec<i16>, rate: u32);
    /// Barge-in: drop what is buffered.
    fn flush(&self);
    /// Hang up.
    fn end(&self);
}

/// A connected streamed call.
pub struct CallConnection {
    pub link: Box<dyn CallLink>,
    pub events: flume::Receiver<CallLinkEvent>,
}

/// EXP-1857: what the desktop a DIALLED call streams to sends back (the
/// relay's caller socket).
#[derive(Clone, Debug, PartialEq)]
pub enum DialLinkEvent {
    /// `call_ready`: the host joined.
    Ready,
    /// `call_event`: one flow event the host stepped (its JSON).
    Event(serde_json::Value),
    /// The host's reply: mono 16-bit PCM at [`DIAL_DOWN_RATE`], de-jittered.
    Audio(Vec<i16>),
    /// The host (or the relay) hung up, with its reason.
    Ended { reason: Option<String> },
    /// The relay refused or ended the call with a code (`no_answer`, `busy`).
    Failed(String),
    /// The link failed or closed without a hang-up.
    Dropped(String),
}

/// EXP-1857: the rate of [`DialLinkEvent::Audio`] (voice-call.json `downRate`).
pub const DIAL_DOWN_RATE: u32 = 24_000;

/// EXP-1857: a dialled call's link to the desktop that speaks.
pub trait DialLink: 'static {
    /// The microphone: mono 16-bit PCM at 16 kHz (`etalk::CALL_RATE`).
    fn send_audio(&self, pcm: Vec<i16>);
    /// `call_mute` (the frames go on as silence).
    fn set_muted(&self, muted: bool);
    /// Hang up (`call_end`).
    fn end(&self);
}

/// EXP-1857: a dialled call, connected.
pub struct DialConnection {
    pub link: Box<dyn DialLink>,
    pub events: flume::Receiver<DialLinkEvent>,
}

/// EXP-1857: why a dial did not connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialError {
    /// The server refused it, in its own words (an availability reason).
    Refused(String),
    /// Anything else (the network, a 500).
    Failed(String),
}

/// The app eTalk runs in. Installed once with [`crate::init`]; every method
/// runs on the main thread except a [`SendJob`].
pub trait TalkHost: 'static {
    fn brand(&self) -> Brand;
    /// Where talk keeps its device-local files (the speech data, the tray's
    /// switches, the meeting log, the pill's position).
    fn data_dir(&self) -> PathBuf;
    /// This machine's id (the member's voice device is one of them).
    fn device_id(&self) -> String;
    /// The account signed in and synced now (None = signed out).
    fn account(&self, cx: &App) -> Option<String>;
    /// Every account signed in on this machine.
    fn accounts(&self, cx: &App) -> SignedIn;
    /// An agent by id.
    fn agent(&self, agent_id: &str, cx: &App) -> Option<TalkAgent>;
    /// #EXP-1847: the voice `agent_id` speaks with here, resolved against
    /// the member's `settings` (None = the member's own: a host without
    /// per-agent voices, or an unknown agent). A clip still on its way is
    /// the built-in voice (FEED-276: `clip` = `Fetching`, a call waits for
    /// it); the host calls [`crate::agents_changed`] once it landed or
    /// failed.
    fn agent_voice(&self, agent_id: &str, settings: &VoiceSettings, cx: &App) -> Option<AgentVoice> {
        let _ = (agent_id, settings, cx);
        None
    }
    /// The member's conversation with the agent (None = none yet).
    fn conversation(&self, agent_id: &str, cx: &App) -> Option<String>;
    /// Visit every message row the host holds; answers whether they have
    /// synced (talk treats the first synced rows as history).
    fn messages(&self, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool;
    /// #PERF-9: [`Self::messages`] narrowed to the rows changed since
    /// `since`, a cursor an earlier look returned (None = every row; a host
    /// may always visit more). Answers whether they have synced, and the
    /// cursor for the next look. The default visits every row.
    fn messages_since(&self, since: Option<u64>, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> (bool, Option<u64>) {
        let _ = since;
        (self.messages(cx, visit), None)
    }
    /// #PERF-9: the rows of one session (a conversation), any order. The
    /// default filters every row.
    fn session_messages(&self, session_id: &str, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) {
        self.messages(cx, &mut |row| {
            if row.message.session_id == Some(session_id) {
                visit(row);
            }
        });
    }
    /// Prepare a send (None = signed out).
    fn send(&self, outgoing: Outgoing, cx: &App) -> Option<SendJob>;
    /// #EXP-1961 (voice-call.json `callSession`): ask for the call's own
    /// session while it rings (`voice.callSession`); the job answers its id
    /// (None = no call session: the call goes to the chat). None = a host
    /// without call sessions.
    fn call_session(&self, agent_id: &str, call_id: Option<&str>, cx: &App) -> Option<CallSessionJob> {
        let _ = (agent_id, call_id, cx);
        None
    }
    /// #EXP-1961: the hang-up of a call with its own session (`voice.callEnded`).
    fn call_ended(&self, session_id: &str, cx: &App) {
        let _ = (session_id, cx);
    }
    /// Store a settings change; the host hands the stored settings to
    /// [`crate::set_settings`] (or says the write failed), then runs `done`.
    /// False = not signed in (nothing happens, `done` never runs).
    fn write(&self, write: VoiceWrite, done: Box<dyn FnOnce(&mut App)>, cx: &mut App) -> bool;
    /// Answer a streamed call ringing here (None = no relay or not signed
    /// in; Err = refused). EXP-1857: the host names THIS device in its
    /// ticket (any of the member's desktops may be rung). #EXP-1941: and the
    /// ring's `agent_id`: the relay writes the ring, the server's ticket
    /// vouches for its call id and its agent, and tells the call's mode. A
    /// host told `seal` joins with a key source. `refuse_only` = it joins
    /// only to fail the call (`busy`, `meeting`): without a key source, a
    /// clear `failed` works before any key exists.
    fn connect_call(&self, call_id: &str, agent_id: &str, refuse_only: bool, cx: &mut App) -> Option<Task<Result<CallConnection, String>>>;
    /// EXP-1857: dial `agent_id` on ANOTHER of the member's desktops (the
    /// server picks the same target the telephone did): None = no relay,
    /// no server or not signed in.
    fn dial_call(&self, agent_id: &str, cx: &mut App) -> Option<Task<Result<DialConnection, DialError>>> {
        let _ = (agent_id, cx);
        None
    }
    /// A device of the member's (None = unknown).
    fn device(&self, device_id: &str, cx: &App) -> Option<TalkDevice>;
    /// EXP-1857: the member's OWN desktops, newest `lastSeenAt` first (a
    /// host without other devices: none).
    fn desktops(&self, cx: &App) -> Vec<TalkDesktop> {
        let _ = cx;
        Vec::new()
    }
    /// EXP-1858: the member's OWN servers (CLI daemons), newest first: a
    /// call rings one only when the agent runs on it.
    fn servers(&self, cx: &App) -> Vec<TalkDesktop> {
        let _ = cx;
        Vec::new()
    }
    /// The device's desktop audio as it reports it (meeting mode's who).
    fn desktop_audio(&self, device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>);
    fn icon(&self, icon: TalkIcon<'_>) -> Icon;
    fn open(&self, surface: Surface, cx: &mut App);
    fn notify(&self, notice: Notice, cx: &mut App);
    /// Talk's readiness changed (the speech state, the listening device):
    /// a host with a device doctor reports it again.
    fn readiness_changed(&self, cx: &mut App);
    /// #EXP-1938: the person's keyboard and pointer are held (computer use
    /// drives this desktop): a key that arrives now is an agent's, never the
    /// person's. True from before the hold's first synthetic key.
    fn input_held(&self, _cx: &App) -> bool {
        false
    }
}

struct HostGlobal(Rc<dyn TalkHost>);

impl Global for HostGlobal {}

pub(crate) fn set(host: Box<dyn TalkHost>, cx: &mut App) {
    cx.set_global(HostGlobal(Rc::from(host)));
}

/// The installed host (None before [`crate::init`]: the styleguide's tests).
pub(crate) fn host(cx: &App) -> Option<Rc<dyn TalkHost>> {
    cx.try_global::<HostGlobal>().map(|global| global.0.clone())
}

pub(crate) fn data_dir(cx: &App) -> Option<PathBuf> {
    host(cx).map(|host| host.data_dir())
}

pub(crate) fn device_id(cx: &App) -> Option<String> {
    host(cx).map(|host| host.device_id())
}

pub(crate) fn account(cx: &App) -> Option<String> {
    host(cx)?.account(cx)
}

pub(crate) fn agent(agent_id: &str, cx: &App) -> Option<TalkAgent> {
    host(cx)?.agent(agent_id, cx)
}

/// #EXP-1847: the voice `agent_id` speaks with here (the member's when the
/// host has none for it, or no agent is named).
pub(crate) fn agent_voice(agent_id: Option<&str>, settings: &VoiceSettings, cx: &App) -> AgentVoice {
    agent_id
        .and_then(|agent_id| host(cx)?.agent_voice(agent_id, settings, cx))
        .unwrap_or_else(|| AgentVoice::member(settings))
}

/// #EXP-1847: the wake phrase `agent_id` listens for, from the member's
/// stored phrase `stored` (the Assistant's own wins; empty = `Hey {agent}`).
pub(crate) fn wake_phrase_of(agent_id: Option<&str>, stored: &str, cx: &App) -> String {
    let member = VoiceSettings { wake_phrase: stored.to_string(), ..Default::default() };
    agent_voice(agent_id, &member, cx).wake_phrase
}

/// #EXP-1847: the language `agent_id` is heard (and speaks) in, from the
/// member's `member_language`: an English agent is never heard as German.
pub(crate) fn language_of(agent_id: &str, member_language: &str, cx: &App) -> String {
    let member = VoiceSettings { language: member_language.to_string(), ..Default::default() };
    agent_voice(Some(agent_id), &member, cx).voice.language
}

/// #EXP-1961: see [`TalkHost::call_session`].
pub(crate) fn call_session(agent_id: &str, call_id: Option<&str>, cx: &App) -> Option<CallSessionJob> {
    host(cx)?.call_session(agent_id, call_id, cx)
}

/// #EXP-1961: see [`TalkHost::call_ended`].
pub(crate) fn call_ended(session_id: &str, cx: &App) {
    if let Some(host) = host(cx) {
        host.call_ended(session_id, cx);
    }
}

pub(crate) fn conversation(agent_id: &str, cx: &App) -> Option<String> {
    host(cx)?.conversation(agent_id, cx)
}

pub(crate) fn notify(notice: Notice, cx: &mut App) {
    if let Some(host) = host(cx) {
        host.notify(notice, cx);
    }
}

pub(crate) fn open(surface: Surface, cx: &mut App) {
    if let Some(host) = host(cx) {
        host.open(surface, cx);
    }
}

pub(crate) fn icon(icon: TalkIcon<'_>, cx: &App) -> Icon {
    host(cx).map(|host| host.icon(icon)).unwrap_or_else(Icon::empty)
}

/// #EXP-1938: [`TalkHost::input_held`] (false without a host).
pub(crate) fn input_held(cx: &App) -> bool {
    host(cx).is_some_and(|host| host.input_held(cx))
}

pub(crate) fn readiness_changed(cx: &mut App) {
    if let Some(host) = host(cx) {
        host.readiness_changed(cx);
    }
}

/// `agent_id`'s utterance `text` as a send (None = nothing to send).
pub(crate) fn outgoing(agent_id: &str, text: &str, cx: &App) -> Option<Outgoing> {
    let conversation = conversation(agent_id, cx);
    let team_id = agent(agent_id, cx).map(|agent| agent.team_id).unwrap_or_default();
    let target = etalk::voice::utterance_input(text, &team_id, Some(agent_id), conversation.as_deref())?;
    Some(Outgoing { target, meeting: false, client_message_id: None })
}

/// Prepare `outgoing` (None = signed out, or no host).
pub(crate) fn send(outgoing: Outgoing, cx: &App) -> Option<SendJob> {
    host(cx)?.send(outgoing, cx)
}

/// Epoch milliseconds now.
pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
