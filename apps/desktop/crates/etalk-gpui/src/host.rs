//! #EXP-1422: what eTalk's surfaces need from the app they run in, as plain
//! data. Exponential implements [`TalkHost`] over its synced store, its
//! server and its relay (`ui/src/talk_host.rs`); a host without a server
//! (spezl) answers from its one local agent session.

use std::path::PathBuf;
use std::rc::Rc;

use etalk::meeting::{AudioApp, DesktopAudioCapability, MeetingStopReason};
use etalk::voice::{UtteranceTarget, VoiceMessage};
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
    /// The member's conversation with the agent (None = none yet).
    fn conversation(&self, agent_id: &str, cx: &App) -> Option<String>;
    /// Visit every message row the host holds; answers whether they have
    /// synced (talk treats the first synced rows as history).
    fn messages(&self, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool;
    /// Prepare a send (None = signed out).
    fn send(&self, outgoing: Outgoing, cx: &App) -> Option<SendJob>;
    /// Store a settings change; the host hands the stored settings to
    /// [`crate::set_settings`] (or says the write failed), then runs `done`.
    /// False = not signed in (nothing happens, `done` never runs).
    fn write(&self, write: VoiceWrite, done: Box<dyn FnOnce(&mut App)>, cx: &mut App) -> bool;
    /// Answer a streamed call ringing here (None = no relay or not signed
    /// in; Err = refused).
    fn connect_call(&self, call_id: &str, cx: &mut App) -> Option<Task<Result<CallConnection, String>>>;
    /// A device of the member's (None = unknown).
    fn device(&self, device_id: &str, cx: &App) -> Option<TalkDevice>;
    /// The device's desktop audio as it reports it (meeting mode's who).
    fn desktop_audio(&self, device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>);
    fn icon(&self, icon: TalkIcon<'_>) -> Icon;
    fn open(&self, surface: Surface, cx: &mut App);
    fn notify(&self, notice: Notice, cx: &mut App);
    /// Talk's readiness changed (the speech state, the listening device):
    /// a host with a device doctor reports it again.
    fn readiness_changed(&self, cx: &mut App);
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
