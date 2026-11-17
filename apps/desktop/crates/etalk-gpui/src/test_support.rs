//! #EXP-1936: the surfaces wired together for a gpui test: a [`TalkHost`]
//! that answers from memory, the speaker over a `SpeechClient` that records
//! its requests (no worker is ever spawned), push to talk (no key is
//! grabbed under test) and the call. The worker's events are handed in
//! through [`Rig::worker`], the way the reader thread's do in the app.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use etalk::meeting::{AudioApp, DesktopAudioCapability};
use etalk::voice::{VoiceMessage, VoiceSettings};
use gpui::{App, Task};

use crate::host::{
    Brand, CallConnection, CallLink, CallLinkEvent, Notice, Outgoing, SendJob, Sent, SignedIn, Surface, TalkAgent, TalkDevice, TalkHost, TalkIcon, TalkMessage, VoiceWrite,
};

/// This desktop, its one agent and the member's conversation with it.
pub(crate) const HERE: &str = "d-here";
pub(crate) const AGENT: &str = "a1";
pub(crate) const CONVERSATION: &str = "s-1";
/// The row every send answers with (a test adds it to the messages itself).
pub(crate) const SENT: &str = "m-sent";

/// One message row the host holds.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub id: String,
    pub author_kind: &'static str,
    pub body: String,
    pub has_card: bool,
    pub created_at: String,
}

#[derive(Default)]
struct State {
    rows: Vec<Row>,
    notices: Vec<Notice>,
    /// The streamed call the next `connect_call` answers with.
    call: Option<CallConnection>,
    /// #EXP-1938: computer use holds the person's input.
    input_held: bool,
}

/// A streamed call's link that writes down what the host did with it.
struct LinkLog(Rc<RefCell<Vec<String>>>);

impl CallLink for LinkLog {
    fn send_event(&self, event: serde_json::Value) {
        self.0.borrow_mut().push(format!("event {event}"));
    }
    fn send_speech(&self, _: Vec<i16>, _: u32) {}
    fn flush(&self) {}
    fn end(&self) {
        self.0.borrow_mut().push("end".into());
    }
}

struct TestHost {
    state: Rc<RefCell<State>>,
    data_dir: PathBuf,
}

impl TalkHost for TestHost {
    fn brand(&self) -> Brand {
        Brand { name: "Test", tray_icon_svg: b"", app_id: "test", tray_id: "test" }
    }
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }
    fn device_id(&self) -> String {
        HERE.into()
    }
    fn account(&self, _: &App) -> Option<String> {
        Some("me".into())
    }
    fn accounts(&self, _: &App) -> SignedIn {
        SignedIn { active: Some("me".into()), all: vec!["me".into()] }
    }
    fn agent(&self, agent_id: &str, _: &App) -> Option<TalkAgent> {
        (agent_id == AGENT).then(|| TalkAgent { name: "Studio".into(), title: "Studio".into(), team_id: "t1".into(), ..TalkAgent::default() })
    }
    fn conversation(&self, agent_id: &str, _: &App) -> Option<String> {
        (agent_id == AGENT).then(|| CONVERSATION.to_string())
    }
    fn messages(&self, _: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool {
        for row in &self.state.borrow().rows {
            let message = VoiceMessage {
                author_kind: Some(row.author_kind),
                body: Some(&row.body),
                has_card: row.has_card,
                author_session_id: None,
                session_id: Some(CONVERSATION),
                created_at: Some(&row.created_at),
            };
            visit(&TalkMessage { id: &row.id, message });
        }
        true
    }
    fn send(&self, _: Outgoing, _: &App) -> Option<SendJob> {
        Some(Box::new(|| Ok(Sent { session_id: CONVERSATION.into(), message_id: Some(SENT.into()) })))
    }
    fn write(&self, _: VoiceWrite, _: Box<dyn FnOnce(&mut App)>, _: &mut App) -> bool {
        false
    }
    fn connect_call(&self, _: &str, _: &mut App) -> Option<Task<Result<CallConnection, String>>> {
        self.state.borrow_mut().call.take().map(|call| Task::ready(Ok(call)))
    }
    fn device(&self, _: &str, _: &App) -> Option<TalkDevice> {
        None
    }
    fn desktop_audio(&self, _: &str, _: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
        (DesktopAudioCapability::None, Vec::new())
    }
    fn icon(&self, _: TalkIcon<'_>) -> gpui_component::Icon {
        gpui_component::Icon::empty()
    }
    fn open(&self, _: Surface, _: &mut App) {}
    fn notify(&self, notice: Notice, _: &mut App) {
        self.state.borrow_mut().notices.push(notice);
    }
    fn readiness_changed(&self, _: &mut App) {}
    fn input_held(&self, _: &App) -> bool {
        self.state.borrow().input_held
    }
}

/// The installed surfaces, from the outside.
pub(crate) struct Rig {
    state: Rc<RefCell<State>>,
    requests: Arc<Mutex<Vec<etalk::Request>>>,
}

/// The member's settings on this desktop: an Assistant, push to talk bound,
/// and this desktop the voice device.
pub(crate) fn settings() -> VoiceSettings {
    VoiceSettings { voice_device_id: Some(HERE.into()), agent_id: Some(AGENT.into()), ptt_shortcut: "ctrl+space".into(), ..Default::default() }
}

impl Rig {
    /// Host, speaker, push to talk and the call, with `settings` delivered,
    /// the worker up and speaking English, and the messages armed (empty).
    pub(crate) fn install(settings: VoiceSettings, cx: &mut gpui::TestAppContext) -> Rig {
        let state = Rc::new(RefCell::new(State::default()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let data_dir = std::env::temp_dir().join(format!("etalk-gpui-test-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        let rig = Rig { state: state.clone(), requests: requests.clone() };
        cx.update(|cx| {
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
            crate::host::set(Box::new(TestHost { state, data_dir: data_dir.clone() }), cx);
            let client = etalk::SpeechClient::recording(data_dir.clone(), move |request| requests.lock().unwrap().push(request.clone()));
            crate::speak_replies::install_with(Some(client), &data_dir, cx);
            crate::push_to_talk::install(cx);
            crate::voice_call::install(cx);
            crate::set_settings(settings, cx);
            crate::speak_replies::dispatch(etalk::Event::Ready { protocol: etalk::PROTOCOL }, cx);
            crate::speak_replies::dispatch(etalk::Event::Voices { voices: vec!["en".into()] }, cx);
            crate::messages_changed(cx);
        });
        cx.run_until_parked();
        rig
    }

    /// One event of the (absent) speech worker.
    pub(crate) fn worker(&self, event: etalk::Event, cx: &mut gpui::TestAppContext) {
        cx.update(|cx| crate::speak_replies::dispatch(event, cx));
        cx.run_until_parked();
    }

    /// A row lands in the conversation (`created_at` = seconds after 10:00).
    pub(crate) fn message(&self, id: &str, author_kind: &'static str, body: &str, second: u32, cx: &mut gpui::TestAppContext) {
        self.row(Row { id: id.into(), author_kind, body: body.into(), has_card: false, created_at: format!("2026-11-17T10:00:{second:02}.000Z") }, cx);
    }

    pub(crate) fn row(&self, row: Row, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().rows.push(row);
        cx.update(crate::messages_changed);
        cx.run_until_parked();
    }

    /// What the worker was asked since the last look.
    pub(crate) fn requests(&self) -> Vec<etalk::Request> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }

    /// The ids the worker was asked to say since the last look.
    pub(crate) fn spoken(&self) -> Vec<String> {
        self.requests().into_iter().filter_map(|request| if let etalk::Request::Speak { id, .. } = request { Some(id) } else { None }).collect()
    }

    /// The relay rings this desktop next: what the caller's side sends, and
    /// what the host did with the link.
    pub(crate) fn incoming_call(&self) -> (flume::Sender<CallLinkEvent>, Rc<RefCell<Vec<String>>>) {
        let (tx, events) = flume::unbounded();
        let log = Rc::new(RefCell::new(Vec::new()));
        self.state.borrow_mut().call = Some(CallConnection { link: Box::new(LinkLog(log.clone())), events });
        (tx, log)
    }

    /// #EXP-1938: computer use takes (or lets go of) the person's input.
    pub(crate) fn hold_input(&self, held: bool) {
        self.state.borrow_mut().input_held = held;
    }

    pub(crate) fn notices(&self) -> Vec<Notice> {
        self.state.borrow().notices.clone()
    }
}
