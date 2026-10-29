//! #EXP-1422: eTalk's gpui side, shared by Exponential and spezl: the voice
//! tray ([`voice_tray`]), push to talk and the wake word ([`push_to_talk`],
//! the overlay capsule [`voice_overlay`]), caller mode ([`voice_call`]),
//! spoken replies ([`speak_replies`]) and meeting mode ([`meeting`]), over
//! ONE speech worker (`etalk::SpeechClient`).
//!
//! The app plugs in through [`TalkHost`] (and a streamed call through
//! [`CallLink`]), installed once by [`init`]; what the app knows changes
//! arrive through [`messages_changed`], [`session_changed`],
//! [`agents_changed`] and [`set_settings`].

pub mod bar;
mod host;
pub mod meeting;
pub mod push_to_talk;
pub mod speak_replies;
pub mod voice_call;
pub mod voice_overlay;
pub mod voice_tray;

pub use host::{
    Brand, CallConnection, CallLink, CallLinkEvent, Notice, Outgoing, SendError, SendJob, Sent, SignedIn, Surface, TalkAgent,
    TalkDevice, TalkHost, TalkIcon, TalkMessage, VoiceWrite,
};
pub use speak_replies::{set_settings, SpeechState};

use gpui::App;

/// Install `host` and every surface, in the order they lean on each other:
/// the speaker (it owns the worker), push to talk, the call, the meeting
/// host, the tray. ONCE, before the first window.
pub fn init(cx: &mut App, host: Box<dyn TalkHost>) {
    host::set(host, cx);
    speak_replies::install(cx);
    push_to_talk::install(cx);
    voice_call::install(cx);
    meeting::init(cx);
    voice_tray::install(cx);
}

/// The host's messages changed: a reply to speak, the end of a turn.
pub fn messages_changed(cx: &mut App) {
    speak_replies::on_messages(cx);
    push_to_talk::on_messages(cx);
    meeting::on_messages(cx);
}

/// The signed-in account changed (or signed out).
pub fn session_changed(cx: &mut App) {
    speak_replies::on_session(cx);
    meeting::on_session(cx);
}

/// The host's agents changed (their names label the tray and the wake phrase).
pub fn agents_changed(cx: &mut App) {
    push_to_talk::on_agents(cx);
    voice_tray::on_agents(cx);
}
