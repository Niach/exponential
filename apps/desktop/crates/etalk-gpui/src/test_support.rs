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
    AgentVoice, Brand, CallConnection, CallLink, CallLinkEvent, Notice, Outgoing, SendJob, Sent, SignedIn, Surface, TalkAgent, TalkDevice, TalkHost, TalkIcon, TalkMessage, VoiceWrite,
};

/// This desktop, its one agent and the member's conversation with it.
pub(crate) const HERE: &str = "d-here";
pub(crate) const AGENT: &str = "a1";
pub(crate) const CONVERSATION: &str = "s-1";
/// EXP-1859: a team agent with its own voice, and the member's chat with it.
pub(crate) const TEAM_AGENT: &str = "a-team";
pub(crate) const TEAM_CONVERSATION: &str = "s-team";
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
    /// #EXP-1961: the call's own session (None = the conversation).
    pub session_id: Option<&'static str>,
}

#[derive(Default)]
struct State {
    rows: Vec<Row>,
    notices: Vec<Notice>,
    /// The streamed call the next `connect_call` answers with.
    call: Option<CallConnection>,
    /// What `connect_call` was asked: (call id, agent id, refuse only).
    connects: Vec<(String, String, bool)>,
    /// #EXP-1938: computer use holds the person's input.
    input_held: bool,
    /// FEED-276: the agent's own voice (None = the member's).
    voice: Option<AgentVoice>,
    /// #EXP-1961: the call session `voice.callSession` answers (None = a host without).
    call_session: Option<&'static str>,
    /// #EXP-1961: the sends' targets and the hang-ups told.
    sends: Vec<String>,
    ended: Vec<String>,
    /// #EXP-1992: every send: where it went (None = to the agent) and its text.
    sent: Vec<(Option<String>, String)>,
    /// EXP-1859: the sends' `clientMessageId`s.
    keys: Vec<Option<String>>,
    /// #EXP-1981: what `voice.callSession` was asked: (agent id, call id).
    session_asks: Vec<(String, Option<String>)>,
    /// EXP-1859: the agent's Runs on device, and whether it is online (its
    /// agent is then listened for by name).
    runs_on: Option<(&'static str, bool)>,
    /// #EXP-1982: every send fails with this.
    send_fails: Option<crate::host::SendError>,
}

/// A streamed call's link that writes down what the host did with it.
struct LinkLog(Rc<RefCell<Vec<String>>>);

impl CallLink for LinkLog {
    fn send_event(&self, event: serde_json::Value) {
        self.0.borrow_mut().push(format!("event {event}"));
    }
    fn send_speech(&self, _: Vec<i16>, _: u32) {}
    fn flush(&self) {
        self.0.borrow_mut().push("flush".into());
    }
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
        let device_id = self.state.borrow().runs_on.map(|(device, _)| device.to_string());
        (agent_id == AGENT).then(|| TalkAgent { name: "Studio".into(), title: "Studio".into(), team_id: "t1".into(), device_id, ..TalkAgent::default() })
    }
    fn desktops(&self, _: &App) -> Vec<crate::host::TalkDesktop> {
        let runs_on = self.state.borrow().runs_on;
        runs_on.map(|(id, online)| crate::host::TalkDesktop { id: id.into(), name: id.into(), online, speech: Some("ok".into()) }).into_iter().collect()
    }
    fn wake_listeners(&self, _: &VoiceSettings, cx: &App) -> Vec<etalk::WakeListener> {
        if self.state.borrow().runs_on.is_none() {
            return Vec::new();
        }
        vec![etalk::WakeListener { agent_id: AGENT.into(), phrase: "Hey Studio".into(), language: "en".into(), dial: crate::voice_call::dials(AGENT, cx) }]
    }
    fn agent_voice(&self, agent_id: &str, _: &VoiceSettings, _: &App) -> Option<AgentVoice> {
        if agent_id == TEAM_AGENT {
            let voice = etalk::VoiceChoice { language: "en".into(), file: "/data/team.wav".into(), transcript: "Hi.".into() };
            return Some(AgentVoice { voice, ..Default::default() });
        }
        (agent_id == AGENT).then(|| self.state.borrow().voice.clone()).flatten()
    }
    fn conversation(&self, agent_id: &str, _: &App) -> Option<String> {
        match agent_id {
            AGENT => Some(CONVERSATION.to_string()),
            TEAM_AGENT => Some(TEAM_CONVERSATION.to_string()),
            _ => None,
        }
    }
    fn messages(&self, _: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool {
        for row in &self.state.borrow().rows {
            let message = VoiceMessage {
                author_kind: Some(row.author_kind),
                body: Some(&row.body),
                has_card: row.has_card,
                author_session_id: None,
                session_id: Some(row.session_id.unwrap_or(CONVERSATION)),
                created_at: Some(&row.created_at),
            };
            visit(&TalkMessage { id: &row.id, message });
        }
        true
    }
    fn call_session(&self, agent_id: &str, call_id: Option<&str>, _: &App) -> Option<crate::host::CallSessionJob> {
        self.state.borrow_mut().session_asks.push((agent_id.to_string(), call_id.map(str::to_string)));
        let session = self.state.borrow().call_session?;
        Some(Box::new(move || Some(session.to_string())))
    }
    fn call_ended(&self, session_id: &str, _: &App) {
        self.state.borrow_mut().ended.push(session_id.to_string());
    }
    fn send(&self, outgoing: Outgoing, _: &App) -> Option<SendJob> {
        if let etalk::voice::UtteranceTarget::Session { session_id, .. } = &outgoing.target {
            self.state.borrow_mut().sends.push(session_id.clone());
        }
        let sent = match &outgoing.target {
            etalk::voice::UtteranceTarget::Session { session_id, text } => (Some(session_id.clone()), text.clone()),
            etalk::voice::UtteranceTarget::Agent { text, .. } => (None, text.clone()),
        };
        let session_id = sent.0.clone().unwrap_or_else(|| CONVERSATION.to_string());
        self.state.borrow_mut().sent.push(sent);
        self.state.borrow_mut().keys.push(outgoing.client_message_id.clone());
        if let Some(err) = self.state.borrow().send_fails.clone() {
            return Some(Box::new(move || Err(err)));
        }
        Some(Box::new(move || Ok(Sent { session_id, message_id: Some(SENT.into()) })))
    }
    fn write(&self, _: VoiceWrite, _: Box<dyn FnOnce(&mut App)>, _: &mut App) -> bool {
        false
    }
    fn connect_call(&self, call_id: &str, agent_id: &str, refuse_only: bool, _: &mut App) -> Option<Task<Result<CallConnection, String>>> {
        let mut state = self.state.borrow_mut();
        state.connects.push((call_id.into(), agent_id.into(), refuse_only));
        state.call.take().map(|call| Task::ready(Ok(call)))
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
        self.row(Row { id: id.into(), author_kind, body: body.into(), has_card: false, created_at: format!("2026-11-17T10:00:{second:02}.000Z"), session_id: None }, cx);
    }

    /// #EXP-1961: the next call gets its own session `session`.
    pub(crate) fn with_call_session(&self, session: &'static str) {
        self.state.borrow_mut().call_session = Some(session);
    }

    /// #EXP-1992: the next call gets `session` as its own (None = a host without).
    pub(crate) fn set_call_session(&self, session: Option<&'static str>) {
        self.state.borrow_mut().call_session = session;
    }

    /// #EXP-1992: every send since the last look: its session (None = to
    /// the agent) and its text.
    pub(crate) fn sent(&self) -> Vec<(Option<String>, String)> {
        std::mem::take(&mut self.state.borrow_mut().sent)
    }

    /// EXP-1859: the sends' `clientMessageId`s since the last look.
    pub(crate) fn keys(&self) -> Vec<Option<String>> {
        std::mem::take(&mut self.state.borrow_mut().keys)
    }

    /// #EXP-1992: the agent speaks with the member's own voice again.
    pub(crate) fn member_voice(&self, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().voice = None;
        cx.update(crate::agents_changed);
        cx.run_until_parked();
    }

    /// #EXP-1961: the agent's reply in the call's own session.
    pub(crate) fn call_message(&self, id: &str, body: &str, second: u32, cx: &mut gpui::TestAppContext) {
        let session_id = self.state.borrow().call_session;
        self.row(Row { id: id.into(), author_kind: "agent", body: body.into(), has_card: false, created_at: format!("2026-11-17T10:00:{second:02}.000Z"), session_id }, cx);
    }

    /// EXP-1859: the agent runs on `device` (`online`), and the devices changed.
    pub(crate) fn runs_on(&self, device: &'static str, online: bool, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().runs_on = Some((device, online));
        cx.update(crate::devices_changed);
    }

    /// #EXP-1982: every send from now on fails with `err`.
    pub(crate) fn fail_sends(&self, err: crate::host::SendError) {
        self.state.borrow_mut().send_fails = Some(err);
    }

    /// #EXP-1981: what `voice.callSession` was asked since the last look.
    pub(crate) fn session_asks(&self) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut self.state.borrow_mut().session_asks)
    }

    /// #EXP-1961: the sends' session targets and the hang-ups told, since the last look.
    pub(crate) fn call_log(&self) -> (Vec<String>, Vec<String>) {
        let mut state = self.state.borrow_mut();
        (std::mem::take(&mut state.sends), std::mem::take(&mut state.ended))
    }

    pub(crate) fn row(&self, row: Row, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().rows.push(row);
        cx.update(crate::messages_changed);
        cx.run_until_parked();
    }

    /// EXP-1859: row `id` opens (its chat's key came).
    pub(crate) fn open_row(&self, id: &str, body: &str, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().rows.iter_mut().find(|row| row.id == id).expect("a row").body = body.into();
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
        self.state.borrow_mut().call = Some(CallConnection { link: Box::new(LinkLog(log.clone())), events, refuse: None });
        (tx, log)
    }

    /// #EXP-1982: the relay rings this desktop next and the host joins only
    /// to fail it with `code`.
    pub(crate) fn incoming_refused_call(&self, code: &'static str) -> (flume::Sender<CallLinkEvent>, Rc<RefCell<Vec<String>>>) {
        let (tx, log) = self.incoming_call();
        if let Some(call) = self.state.borrow_mut().call.as_mut() {
            call.refuse = Some(code);
        }
        (tx, log)
    }

    /// FEED-276: the agent's own voice changes (a clip landed or failed):
    /// the host tells eTalk, as `talk_host` does.
    pub(crate) fn agent_voice(&self, voice: AgentVoice, cx: &mut gpui::TestAppContext) {
        self.state.borrow_mut().voice = Some(voice);
        cx.update(crate::agents_changed);
        cx.run_until_parked();
    }

    /// What `connect_call` was asked since the last look.
    pub(crate) fn connects(&self) -> Vec<(String, String, bool)> {
        std::mem::take(&mut self.state.borrow_mut().connects)
    }

    /// #EXP-1938: computer use takes (or lets go of) the person's input.
    pub(crate) fn hold_input(&self, held: bool) {
        self.state.borrow_mut().input_held = held;
    }

    pub(crate) fn notices(&self) -> Vec<Notice> {
        self.state.borrow().notices.clone()
    }
}
