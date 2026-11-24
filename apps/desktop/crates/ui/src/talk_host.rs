//! #EXP-1422: Exponential as eTalk's host. The voice tray, push to talk, the
//! overlay, the call UI, spoken replies and meeting mode live in
//! `etalk-gpui` (shared with serverless hosts like spezl); this is the
//! one place they reach Exponential: the synced store (messages, agents,
//! sessions, devices), the server (`sessions.send`, `voice.*`), the relay's
//! call host (`steer::call_host`), the device doctor, the toasts and the
//! shell window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use etalk::call::CallVoice;
use etalk::meeting::{AudioApp, DesktopAudioCapability};
use etalk::voice::{UtteranceTarget, VoiceMessage, VoiceSettings};
use etalk_gpui::{
    AgentVoice, Brand, CallConnection, CallLink, CallLinkEvent, DialConnection, DialError, DialLink, DialLinkEvent, Notice, Outgoing, SendError,
    SendJob, Sent, SignedIn, Surface, TalkAgent, TalkDesktop, TalkDevice, TalkHost, TalkIcon, TalkMessage, VoiceWrite,
};
use gpui::{App, Task};
use gpui_component::Icon;
use sync::Store;

use crate::icons::{registry, ExpIcon};

pub use etalk_gpui::voice_tray::open_ide;

/// The tray's icon: the app icon `desktop_integration` installs on Linux.
const TRAY_ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/logo-white.svg");

struct ExponentialHost {
    data_dir: PathBuf,
    /// #EXP-1847: the agents' voice clips, fetched once into the data dir.
    clips: Rc<ClipCache>,
}

/// #EXP-1847 (agent-settings.json `voice`): an agent's uploaded clip is
/// fetched ONCE from `/api/attachments/{clipId}` into
/// `{data_dir}/voice-clips/{clipId}.wav` and spoken from there. A clip
/// still on its way, or one that failed (logged once, not retried until a
/// restart), is the built-in voice.
struct ClipCache {
    dir: PathBuf,
    state: RefCell<HashMap<String, ClipState>>,
    /// A landed clip's id (the voice re-resolves on the main thread).
    landed: flume::Sender<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipState {
    Fetching,
    Failed,
}

/// `clip_id` as a file name (an attachment id is a uuid; anything else is
/// never a path).
fn clip_file(dir: &Path, clip_id: &str) -> Option<PathBuf> {
    let safe = !clip_id.is_empty() && clip_id.len() <= 64 && clip_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    safe.then(|| dir.join(format!("{clip_id}.wav")))
}

impl ClipCache {
    /// The clip's cached file, or (FEED-276) why it is not here: `Fetching`
    /// (the first ask starts its fetch) or `Failed` (an unusable id, no
    /// transport, a fetch that failed).
    fn path(&self, clip_id: &str, cx: &App) -> Result<PathBuf, CallVoice> {
        let file = clip_file(&self.dir, clip_id).ok_or(CallVoice::Failed)?;
        if file.is_file() {
            return Ok(file);
        }
        match self.state.borrow().get(clip_id) {
            Some(ClipState::Fetching) => return Err(CallVoice::Fetching),
            Some(ClipState::Failed) => return Err(CallVoice::Failed),
            None => {}
        }
        let Some(transport) = crate::queries::http_attachment_transport(cx) else { return Err(CallVoice::Failed) };
        self.state.borrow_mut().insert(clip_id.to_string(), ClipState::Fetching);
        let (id, dir, landed) = (clip_id.to_string(), self.dir.clone(), self.landed.clone());
        let _ = std::thread::Builder::new().name("voice-clip".into()).spawn(move || {
            use crate::markdown::AttachmentTransport as _;
            let result = transport.fetch(&format!("/api/attachments/{id}")).and_then(|bytes| {
                std::fs::create_dir_all(&dir)?;
                let temp = dir.join(format!("{id}.wav.part"));
                std::fs::write(&temp, &bytes)?;
                std::fs::rename(&temp, &file)?;
                Ok(())
            });
            match result {
                Ok(()) => {
                    let _ = landed.send(id);
                }
                Err(err) => {
                    log::warn!("[ui] voice clip {id} could not be fetched; the built-in voice speaks: {err}");
                    let _ = landed.send(format!("!{id}"));
                }
            }
        });
        Err(CallVoice::Fetching)
    }

    /// A fetch ended (`!id` = it failed).
    fn ended(&self, message: &str) {
        match message.strip_prefix('!') {
            Some(id) => {
                self.state.borrow_mut().insert(id.to_string(), ClipState::Failed);
            }
            None => {
                self.state.borrow_mut().remove(message);
            }
        }
    }
}

/// #EXP-1847: what the worker is prepared with for a resolved agent voice:
/// its language; a clip = its local file (an attachment once `cached` has
/// it, the built-in voice until then) with its transcript; else the
/// built-in voice. FEED-276: `clip` says where the clip is (a call's
/// `ready` waits while it is `Fetching`).
pub(crate) fn voice_inputs(resolved: &domain::agent_settings::ResolvedAgentVoice, cached: impl Fn(&str) -> Result<PathBuf, CallVoice>) -> AgentVoice {
    use domain::agent_settings::AgentVoiceClip;
    let (file, clip) = match &resolved.clip {
        Some(AgentVoiceClip::Attachment(id)) => match cached(id) {
            Ok(path) => (Some(path.to_string_lossy().into_owned()), CallVoice::Installed),
            Err(status) => (None, status),
        },
        Some(AgentVoiceClip::Path(path)) => (Some(path.clone()), CallVoice::Installed),
        None => (None, CallVoice::None),
    };
    let (file, transcript) = match file {
        Some(file) => (file, resolved.transcript.clone()),
        None => (String::new(), String::new()),
    };
    AgentVoice { voice: etalk::VoiceChoice { language: resolved.language.clone(), file, transcript }, wake_phrase: resolved.wake_phrase.clone(), clip }
}

/// Install eTalk with Exponential as its host: ONCE, after `Store::open`, the
/// `AuthContext` and the steer subsystem, before the session bootstrap. The
/// store's messages, session and agents feed it from here.
pub fn install(cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let Some(data_dir) = cx.try_global::<crate::session::AuthContext>().map(|auth| auth.data_dir.clone()) else { return };
    let collections = store.collections();
    let (messages, agents, state) = (collections.messages.clone(), collections.agents.clone(), store.state());
    let (landed, clips_rx) = flume::unbounded::<String>();
    let clips = Rc::new(ClipCache { dir: data_dir.join("voice-clips"), state: RefCell::default(), landed });
    etalk_gpui::init(cx, Box::new(ExponentialHost { data_dir, clips: clips.clone() }));
    // #EXP-1847: a clip that landed (or failed) re-resolves the voices.
    cx.spawn(async move |cx| {
        while let Ok(message) = clips_rx.recv_async().await {
            clips.ended(&message);
            let _ = cx.update(etalk_gpui::agents_changed);
        }
    })
    .detach();
    cx.observe(&messages, |_, cx| etalk_gpui::messages_changed(cx)).detach();
    // A sign-out or another account ends the voice (and the worker).
    cx.observe(&state, |_, cx| etalk_gpui::session_changed(cx)).detach();
    // The agents' names label the tray's Call and the wake phrase.
    cx.observe(&agents, |_, cx| etalk_gpui::agents_changed(cx)).detach();
}

/// NEW-67: new voice settings arrived (a heartbeat, or this device's own
/// Settings → Voice save): eTalk adopts them; NEW-78 / NEW-55: when push to
/// talk or Speak replies moved here or away, the doctor's voice rows follow
/// now (the refresh nudges the beat), not a beat later.
pub(crate) fn set_voice_settings(settings: VoiceSettings, cx: &mut App) {
    let rows = crate::device_readiness::voice_rows(cx);
    etalk_gpui::set_settings(settings, cx);
    if crate::device_readiness::voice_rows(cx) != rows {
        refresh_doctor(cx);
    }
}

fn refresh_doctor(cx: &mut App) {
    if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
        crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
    }
}

/// THIS device's `speech` doctor row (None = no row).
pub(crate) fn speech_state(cx: &App) -> Option<coding::device_doctor::SpeechState> {
    etalk_gpui::speak_replies::speech_state(cx).map(|state| match state {
        etalk_gpui::SpeechState::Ready => coding::device_doctor::SpeechState::Ready,
        etalk_gpui::SpeechState::Downloading => coding::device_doctor::SpeechState::Downloading,
        etalk_gpui::SpeechState::NoGpu => coding::device_doctor::SpeechState::NoGpu,
        etalk_gpui::SpeechState::Failed => coding::device_doctor::SpeechState::Failed,
    })
}

/// A failed push-to-talk send's text for the chat of session `session_id`
/// (the member's conversation with that agent), taken once by its composer.
pub(crate) fn take_unsent(session_id: &str, cx: &mut App) -> Option<String> {
    if !etalk_gpui::push_to_talk::has_unsent(cx) {
        return None;
    }
    let me = crate::queries::active_account(cx)?.user_id;
    let agent_id = Store::try_global(cx)?
        .collections()
        .sessions
        .read(cx)
        .iter()
        .find(|row| {
            row.id == session_id
                && row.is_root()
                && (row.user_id.as_deref() == Some(me.as_str()) || crate::session_data::is_group_chat_root(row, cx))
        })?
        .agent_id
        .clone()?;
    etalk_gpui::push_to_talk::take_unsent(&agent_id, cx)
}

/// A send's answer the batch can never pass (4xx: the server refused it).
/// A 401 (an expired token, `Unauthorized` or a bare `Http 401`) is a failed
/// send, never a refusal: the batch waits for the refreshed token.
fn refused(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status, .. } if (400..500).contains(status) && !matches!(*status, 401 | 408 | 429))
}

/// `settled` = the send's tree was known here (an unsynced one's 400 may be
/// its lookup failing: the next send decides).
fn send_error(err: api::ApiError, settled: bool) -> SendError {
    let refused = refused(&err);
    // #EXP-1982: no key holder here: a refusal is for every word of a call.
    SendError { refused, unstorable: refused && settled && coding::sealing::key_holder_missing(), message: err.to_string() }
}

/// `outgoing` as `sessions.send`'s input.
fn send_input(outgoing: Outgoing) -> api::sessions::SendInput {
    let mut input = match outgoing.target {
        UtteranceTarget::Session { session_id, text } => api::sessions::SendInput::message(&session_id, &text, Vec::new()),
        UtteranceTarget::Agent { team_id, agent_id, text } => crate::chat_launch::agent_send_input(&team_id, &agent_id, &text, Vec::new()),
    };
    if outgoing.meeting {
        input = input.meeting();
    }
    if outgoing.client_message_id.is_some() {
        input.client_message_id = outgoing.client_message_id;
    }
    input
}

/// The relay's call host socket as eTalk's link.
struct SteerLink(steer::call_host::CallHost);

impl CallLink for SteerLink {
    fn send_event(&self, event: serde_json::Value) {
        self.0.send_event(event);
    }

    fn send_speech(&self, pcm: Vec<i16>, rate: u32) {
        self.0.send_speech(pcm, rate);
    }

    fn flush(&self) {
        self.0.flush();
    }

    fn end(&self) {
        self.0.end();
    }
}

/// EXP-1857: the relay's caller socket as eTalk's dial link.
struct SteerDial(steer::call_caller::CallCaller);

impl DialLink for SteerDial {
    fn send_audio(&self, pcm: Vec<i16>) {
        self.0.send_audio(pcm);
    }

    fn set_muted(&self, muted: bool) {
        self.0.set_muted(muted);
    }

    fn end(&self) {
        self.0.end();
    }

    fn fail(&self) {
        self.0.fail();
    }
}

fn dial_event(event: steer::call_caller::CallCallerEvent) -> DialLinkEvent {
    use steer::call_caller::CallCallerEvent;
    match event {
        CallCallerEvent::Ready => DialLinkEvent::Ready,
        CallCallerEvent::Event(event) => DialLinkEvent::Event(event),
        CallCallerEvent::Audio(pcm) => DialLinkEvent::Audio(pcm),
        CallCallerEvent::Ended { reason } => DialLinkEvent::Ended { reason },
        CallCallerEvent::Failed(code) => DialLinkEvent::Failed(code),
        CallCallerEvent::Dropped(reason) => DialLinkEvent::Dropped(reason),
    }
}

/// EXP-1857: `voice.startCall`'s refusal: the server's own words for a
/// precondition (an availability reason), anything else a failure.
fn dial_error(err: api::ApiError) -> DialError {
    match err {
        api::ApiError::Http { status: 412, message } => DialError::Refused(message),
        other => DialError::Failed(other.to_string()),
    }
}

/// EXP-1857: a doctor's `speech` row state (group voice; None = no row).
fn doctor_speech(doctor: Option<&serde_json::Value>) -> Option<String> {
    doctor?
        .get("items")?
        .as_array()?
        .iter()
        .find(|item| item["key"] == coding::device_doctor::KEY_SPEECH && item["group"] == "voice")?
        .get("state")?
        .as_str()
        .map(str::to_string)
}

/// EXP-1857: the member's OWN desktops (`user_id` = `me`), newest
/// `lastSeenAt` first, as a call's target reads them.
fn own_desktops(rows: &[&domain::rows::DeviceRow], me: &str, now_ms: i64) -> Vec<TalkDesktop> {
    own_devices(rows, me, now_ms, |kind| kind.is_none_or(|kind| kind == "desktop"))
}

/// EXP-1858: the member's own servers (CLI daemons), newest first.
fn own_servers(rows: &[&domain::rows::DeviceRow], me: &str, now_ms: i64) -> Vec<TalkDesktop> {
    own_devices(rows, me, now_ms, |kind| kind == Some("server"))
}

fn own_devices(rows: &[&domain::rows::DeviceRow], me: &str, now_ms: i64, kind: impl Fn(Option<&str>) -> bool) -> Vec<TalkDesktop> {
    let mut rows: Vec<&domain::rows::DeviceRow> = rows
        .iter()
        .copied()
        .filter(|row| row.user_id.as_deref() == Some(me) && kind(row.kind.as_deref()) && row.device_id.is_some())
        .collect();
    let seen = |row: &domain::rows::DeviceRow| row.last_seen_at.as_deref().and_then(crate::comments::parse_epoch).unwrap_or(0);
    rows.sort_by_key(|row| std::cmp::Reverse(seen(row)));
    rows.into_iter()
        .map(|row| {
            let id = row.device_id.clone().unwrap_or_default();
            TalkDesktop {
                name: row.label.clone().filter(|label| !label.is_empty()).unwrap_or_else(|| id.clone()),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                speech: doctor_speech(row.doctor.as_ref()),
                id,
            }
        })
        .collect()
}

/// #EXP-1941: how a rung desktop joins the call socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostJoin {
    /// Today's wire: the server did not mark the call sealed, or this
    /// desktop joins only to refuse (a clear `failed` works before any key).
    Clear,
    /// Told `seal`: with this install's key holder as the key source.
    Sealed,
    /// Told `seal` and no node here: the call ends `no_key`. Never clear.
    SealedNoKey,
}

fn host_join(told_seal: bool, refuse_only: bool, has_node: bool) -> HostJoin {
    if told_seal && !refuse_only {
        return if has_node { HostJoin::Sealed } else { HostJoin::SealedNoKey };
    }
    HostJoin::Clear
}

/// #EXP-1982: a clear call this desktop answers (not one it joins only to
/// refuse) into a chat it can store none of its words in while it has no
/// key holder (`unstorable`, asked only then) is refused at the ring with
/// `unavailable` ("{device} couldn't get its voice ready"), never heard.
/// This desktop seals every send outside a clear tree (`sealing::plan_send`,
/// a Local account aside): a keyed chat, a keyless one and no chat yet alike.
fn clear_refusal(join: HostJoin, refuse_only: bool, holder_missing: bool, unstorable: impl FnOnce() -> bool) -> Option<&'static str> {
    (join == HostJoin::Clear && !refuse_only && holder_missing && unstorable()).then_some("unavailable")
}

fn link_event(event: steer::call_host::CallHostEvent) -> CallLinkEvent {
    use steer::call_host::CallHostEvent;
    match event {
        CallHostEvent::Ready => CallLinkEvent::Ready,
        CallHostEvent::Audio(pcm) => CallLinkEvent::Audio(pcm),
        CallHostEvent::Lost { ms } => CallLinkEvent::Lost { ms },
        CallHostEvent::Mute(muted) => CallLinkEvent::Mute(muted),
        CallHostEvent::Ended { reason } => CallLinkEvent::Ended { reason },
        CallHostEvent::Dropped(reason) => CallLinkEvent::Dropped(reason),
    }
}

/// The voice device's desktop audio as its synced doctor reports it
/// (`doctor.desktopAudio`; absent = `none` and `[]`).
pub(crate) fn desktop_audio(device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
    let Some(store) = Store::try_global(cx) else { return Default::default() };
    let devices = store.collections().devices.read(cx);
    let audio = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))
        .and_then(|row| row.doctor.as_ref())
        .and_then(|doctor| doctor.get("desktopAudio"))
        .and_then(|audio| serde_json::from_value::<coding::device_doctor::DoctorDesktopAudio>(audio.clone()).ok());
    audio.map(|audio| (audio.capability, audio.apps)).unwrap_or_default()
}

/// #EXP-2069 (`chat-express.json` `plainProseCases`): Voice reads an agent
/// row's PROSE only: its Express blocks are views, never read aloud (a
/// block-only reply has nothing to say). `None` = the body as it is.
fn spoken_prose(row: &domain::rows::MessageRow) -> Option<String> {
    let body = row.body.as_deref()?;
    (row.author_kind.as_deref() == Some("agent") && domain::chat_express::has_express_block(body))
        .then(|| domain::chat_express::plain_prose(body))
}

/// One row to the voice surfaces, its body read as [`spoken_prose`].
fn visit_talk(row: &domain::rows::MessageRow, visit: &mut dyn FnMut(&TalkMessage<'_>)) {
    let prose = spoken_prose(row);
    visit(&talk_message(row, prose.as_deref().or(row.body.as_deref())));
}

fn talk_message<'a>(row: &'a domain::rows::MessageRow, body: Option<&'a str>) -> TalkMessage<'a> {
    TalkMessage {
        id: &row.id,
        message: VoiceMessage {
            author_kind: row.author_kind.as_deref(),
            body,
            has_card: row.card.as_ref().is_some_and(|card| !card.is_null()),
            author_session_id: row.author_session_id.as_deref(),
            session_id: domain::message_session::message_session(row),
            created_at: row.created_at.as_deref(),
        },
    }
}

impl TalkHost for ExponentialHost {
    fn brand(&self) -> Brand {
        Brand { name: "Exponential", tray_icon_svg: TRAY_ICON_SVG, app_id: crate::undock::CHANNEL_APP_ID, tray_id: "exponential-voice" }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn input_held(&self, _: &App) -> bool {
        crate::computer_indicator::input_held()
    }

    fn device_id(&self) -> String {
        steer::persistent_device_id(&self.data_dir)
    }

    fn account(&self, cx: &App) -> Option<String> {
        match Store::try_global(cx)?.session(cx) {
            sync::SessionPhase::Synced { account_id } => Some(account_id),
            _ => None,
        }
    }

    fn accounts(&self, cx: &App) -> SignedIn {
        let auth = crate::session::AuthContext::global(cx);
        let all = auth.auth.signed_in_accounts().into_iter().map(|account| account.id).collect();
        SignedIn { active: crate::queries::active_account(cx).map(|account| account.id), all }
    }

    fn agent(&self, agent_id: &str, cx: &App) -> Option<TalkAgent> {
        let agent = crate::session_data::agent_def(agent_id, cx)?;
        Some(TalkAgent {
            name: agent.name.clone().unwrap_or_default(),
            title: crate::agent_screen::agent_def_name(&agent),
            team_id: agent.team_id.clone().unwrap_or_default(),
            icon: agent.icon.clone(),
            color: agent.color.clone(),
            device_id: agent.device_id.clone().filter(|id| !id.is_empty()),
        })
    }

    /// #EXP-1847: the agent's own voice (agent-settings.json `voice`
    /// `agentVoiceOf`) against the member's settings.
    fn agent_voice(&self, agent_id: &str, settings: &VoiceSettings, cx: &App) -> Option<AgentVoice> {
        let agent = crate::session_data::agent_def(agent_id, cx)?;
        let member = domain::agent_settings::MemberVoice {
            language: settings.language(),
            voice_file: &settings.voice_file,
            voice_transcript: &settings.voice_transcript,
            wake_phrase: &settings.wake_phrase,
        };
        let resolved = domain::agent_settings::agent_voice_of(Some(&agent.role), agent.name.as_deref().unwrap_or_default(), &agent.voice, &member);
        Some(voice_inputs(&resolved, |clip_id| self.clips.path(clip_id, cx)))
    }

    /// EXP-1859 (agent-settings.json `voice.wake`): who the voice device
    /// listens for: `wakeListeners` over the synced agents.
    fn wake_listeners(&self, settings: &VoiceSettings, cx: &App) -> Vec<etalk::WakeListener> {
        let Some(store) = Store::try_global(cx) else { return Vec::new() };
        let rows = store.collections().agents.read(cx);
        let agents: Vec<domain::agent_settings::WakeAgent<'_>> = rows.iter().map(domain::agent_settings::WakeAgent::of).collect();
        let input = domain::agent_settings::WakeSettings {
            voice_device_id: settings.voice_device_id.as_deref(),
            agent_id: settings.agent_id.as_deref(),
            wake_word: settings.wake_word,
            wake_agent_ids: &settings.wake_agent_ids,
            member: domain::agent_settings::MemberVoice {
                language: settings.language(),
                voice_file: &settings.voice_file,
                voice_transcript: &settings.voice_transcript,
                wake_phrase: &settings.wake_phrase,
            },
        };
        domain::agent_settings::wake_listeners(&input, &agents)
            .into_iter()
            .map(|listener| etalk::WakeListener { agent_id: listener.agent_id, phrase: listener.phrase, language: listener.language, dial: false })
            .collect()
    }

    fn conversation(&self, agent_id: &str, cx: &App) -> Option<String> {
        crate::session_data::my_conversation(agent_id, cx).map(|row| row.id)
    }

    fn messages(&self, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool {
        self.messages_since(None, cx, visit).0
    }

    /// #PERF-9: the collection's change log narrows a look to the rows
    /// written since `since` (every row when it no longer reaches back).
    fn messages_since(&self, since: Option<u64>, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> (bool, Option<u64>) {
        let Some(store) = Store::try_global(cx) else { return (false, None) };
        let rows = store.collections().messages.read(cx);
        match since.and_then(|since| rows.changed_since(since)) {
            Some(keys) => keys.into_iter().filter_map(|key| rows.get_key(key)).for_each(|row| visit_talk(row, visit)),
            None => rows.iter().for_each(|row| visit_talk(row, visit)),
        }
        (rows.is_ready(), Some(rows.revision()))
    }

    /// #PERF-9: one conversation's rows through the group index.
    fn session_messages(&self, session_id: &str, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) {
        let Some(store) = Store::try_global(cx) else { return };
        store.collections().messages.read(cx).in_group(session_id).for_each(|row| visit_talk(row, visit));
    }

    fn send(&self, outgoing: Outgoing, cx: &App) -> Option<SendJob> {
        let trpc = crate::queries::trpc_client(cx)?;
        let input = send_input(outgoing);
        let plan = crate::sealing::plan_send(&input, cx);
        let settled = !matches!(plan.target, crate::sealing::SealTarget::Unsynced(_));
        Some(Box::new(move || {
            crate::sealing::send_blocking(&trpc, &input, &plan)
                .map(|output| Sent { session_id: output.session_id, message_id: output.message_id })
                .map_err(|err| send_error(err, settled))
        }))
    }

    fn call_session(&self, agent_id: &str, call_id: Option<&str>, cx: &App) -> Option<etalk_gpui::CallSessionJob> {
        let trpc = crate::queries::trpc_client(cx)?;
        // #EXP-1981: a streamed call names the ring's id; one run here, none.
        let (agent_id, call_id) = (agent_id.to_string(), call_id.map(str::to_string));
        Some(Box::new(move || match api::voice::voice_call_session(&trpc, &agent_id, call_id.as_deref()) {
            Ok(session) => session.map(|session| session.session_id),
            Err(err) => {
                log::info!("[voice] no call session ({err}): the chat takes the call");
                None
            }
        }))
    }

    fn call_ended(&self, session_id: &str, cx: &App) {
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        let session_id = session_id.to_string();
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = api::voice::voice_call_ended(&trpc, &session_id) {
                    log::warn!("[voice] the call's end did not reach the server: {err}");
                }
            })
            .detach();
    }

    fn write(&self, write: VoiceWrite, done: Box<dyn FnOnce(&mut App)>, cx: &mut App) -> bool {
        let Some(trpc) = crate::queries::trpc_client(cx) else { return false };
        let (verb, noun) = match write {
            VoiceWrite::WakeWord(_) => ("save", "the voice settings"),
            VoiceWrite::StartMeeting => ("start", "meeting mode"),
            VoiceWrite::StopMeeting(_) => ("stop", "meeting mode"),
        };
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match write {
                        VoiceWrite::WakeWord(on) => {
                            api::voice::voice_update(&trpc, &api::voice::VoiceUpdate { wake_word: Some(on), ..Default::default() })
                        }
                        VoiceWrite::StartMeeting => api::voice::voice_start_meeting(&trpc),
                        VoiceWrite::StopMeeting(reason) => api::voice::voice_stop_meeting(&trpc, reason),
                    }
                })
                .await;
            cx.update(|cx| {
                match result {
                    // The speaker adopts it (the tray and the meeting host follow).
                    Ok(stored) => set_voice_settings(stored, cx),
                    Err(err) => crate::action_error::failed_action_err(verb, noun, &err, cx),
                }
                done(cx);
            });
        })
        .detach();
        true
    }

    fn connect_call(&self, call_id: &str, agent_id: &str, refuse_only: bool, cx: &mut App) -> Option<Task<Result<CallConnection, String>>> {
        let trpc = Arc::new(crate::queries::trpc_client(cx)?);
        let runtime = crate::steer_wiring::runtime(cx)?;
        let (call_id, agent_id) = (call_id.to_string(), agent_id.to_string());
        // EXP-1857: the ticket names THIS desktop (any of the member's may be
        // rung). #EXP-1941: and the ring's agent: the ticket vouches for both.
        let device_id = self.device_id();
        let node = self.account(cx).and_then(|account| crate::enet_wiring::node(&account, cx));
        // #EXP-1982: a Local desktop's trees are clear by nature.
        let local = crate::queries::active_account(cx).is_some_and(|account| account.is_local());
        Some(cx.spawn(async move |cx| {
            let (asking, ids) = (Arc::clone(&trpc), (call_id.clone(), device_id.clone(), agent_id.clone()));
            // #FEED-268: how long the ring waited for its ticket (the relay
            // gives up on a ring nobody joins in `ringTimeoutMs`).
            let (log_call_id, asked) = (call_id.clone(), std::time::Instant::now());
            let ticket = cx
                .background_executor()
                .spawn(async move { api::voice::voice_call_host_ticket_for(&asking, &ids.0, &ids.1, &ids.2) })
                .await
                .map_err(|err| err.to_string())?;
            log::info!("[voice] call {log_call_id}: the host ticket came after {} ms (sealed {})", asked.elapsed().as_millis(), ticket.seal);
            let join = host_join(ticket.seal, refuse_only, node.is_some());
            let asking = Arc::clone(&trpc);
            let agent = agent_id.clone();
            let refuse = cx
                .background_executor()
                .spawn(async move {
                    clear_refusal(join, refuse_only, !local && coding::sealing::key_holder_missing(), || {
                        // Plaintext lands only in a clear tree; anything else
                        // (keyed, keyless, no chat yet) wants a key here.
                        api::sessions::send_target(&asking, Some(&agent), None).is_ok_and(|target| !(target.clear_tree && target.root_id.is_some()))
                    })
                })
                .await;
            let sealing = match join {
                HostJoin::Clear => None,
                // Told `seal` without a node: no key holder, the call ends `no_key`. Never clear.
                join => {
                    let keys: Arc<dyn steer::call_keys::CallKeySource> = match node.filter(|_| join == HostJoin::Sealed) {
                        Some(node) => Arc::new(steer::call_keys::HostKeys {
                            ops: Arc::new(steer::call_keys::InstallKeyOps { trpc, node }),
                            call_id: call_id.clone(),
                            agent_id: agent_id.clone(),
                            device_id: device_id.clone(),
                            // The IDE's send asks the server where an agent
                            // send lands (#FEED-250): the adopted root is its answer.
                            adopted: None,
                        }),
                        None => Arc::new(steer::call_keys::FixedKeySource { key: None, delay: std::time::Duration::ZERO }),
                    };
                    Some(steer::call_keys::CallSealing { call_id, agent_id, device_id, caller_target: None, keys })
                }
            };
            let (tx, events) = flume::unbounded::<CallLinkEvent>();
            let host = steer::call_host::connect_sealed(&runtime, ticket.url, &log_call_id, sealing, move |event| {
                let _ = tx.send(link_event(event));
            });
            Ok(CallConnection { link: Box::new(SteerLink(host)), events, refuse })
        }))
    }

    /// EXP-1857: `voice.startCall` as a desktop (this one the first
    /// fallback), then the relay's caller socket.
    fn dial_call(&self, agent_id: &str, cx: &mut App) -> Option<Task<Result<DialConnection, DialError>>> {
        let trpc = Arc::new(crate::queries::trpc_client(cx)?);
        let runtime = crate::steer_wiring::runtime(cx)?;
        let (agent_id, device_id) = (agent_id.to_string(), self.device_id());
        let node = self.account(cx).and_then(|account| crate::enet_wiring::node(&account, cx));
        Some(cx.spawn(async move |cx| {
            let (asking, agent) = (Arc::clone(&trpc), agent_id.clone());
            let has_node = node.is_some();
            let (started, before) = cx
                .background_executor()
                .spawn(async move {
                    // #EXP-1941: a caller that can seal claims it. A key holder
                    // that is still starting is waited for first (bounded, the
                    // wait the chat's writers use); only a desktop that can
                    // never hold keys dials without the claim.
                    let claim = has_node && coding::sealing::sealer().is_ok();
                    // Its own answer on root and epoch, before it dials.
                    let before = claim.then(|| api::sessions::send_target(&asking, Some(&agent), None).ok()).flatten();
                    let started = api::voice::voice_start_call_claiming(&asking, &agent, &device_id, claim)?;
                    Ok::<_, api::ApiError>((started, before))
                })
                .await
                .map_err(dial_error)?;
            log::info!("[voice] call {}: dialling {} (fallback {}, sealed {})", started.call_id, started.device, started.fallback, started.seal);
            // The server's one boolean: the claim AND the target's cap.
            let sealing = started.seal.then(|| {
                let before = before.and_then(|target| {
                    Some(steer::call_keys::CallTarget { root_id: target.root_id?, epoch: target.epoch, rotation_due: target.rotation_due })
                });
                let keys: Arc<dyn steer::call_keys::CallKeySource> = match node {
                    Some(node) => Arc::new(steer::call_keys::CallerKeys { ops: Arc::new(steer::call_keys::InstallKeyOps { trpc, node }), agent_id: agent_id.clone(), before: before.clone() }),
                    None => Arc::new(steer::call_keys::FixedKeySource { key: None, delay: std::time::Duration::ZERO }),
                };
                steer::call_keys::CallSealing {
                    call_id: started.call_id.clone(),
                    agent_id: agent_id.clone(),
                    // The HOST's device, as the server answered it.
                    device_id: started.device_id.clone(),
                    caller_target: before.and_then(|target| Some((target.root_id, target.epoch?))),
                    keys,
                }
            });
            let (tx, events) = flume::unbounded::<DialLinkEvent>();
            let caller = steer::call_caller::connect_sealed(&runtime, started.url, &started.call_id, sealing, move |event| {
                let _ = tx.send(dial_event(event));
            });
            Ok(DialConnection { link: Box::new(SteerDial(caller)), events })
        }))
    }

    fn desktops(&self, cx: &App) -> Vec<TalkDesktop> {
        let Some(store) = Store::try_global(cx) else { return Vec::new() };
        let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else { return Vec::new() };
        let devices = store.collections().devices.read(cx);
        let rows: Vec<&domain::rows::DeviceRow> = devices.iter().collect();
        own_desktops(&rows, &me, chrono::Utc::now().timestamp_millis())
    }

    fn servers(&self, cx: &App) -> Vec<TalkDesktop> {
        let Some(store) = Store::try_global(cx) else { return Vec::new() };
        let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else { return Vec::new() };
        let devices = store.collections().devices.read(cx);
        let rows: Vec<&domain::rows::DeviceRow> = devices.iter().collect();
        own_servers(&rows, &me, chrono::Utc::now().timestamp_millis())
    }

    fn device(&self, device_id: &str, cx: &App) -> Option<TalkDevice> {
        let store = Store::try_global(cx)?;
        let devices = store.collections().devices.read(cx);
        let row = devices.iter().find(|row| row.device_id.as_deref() == Some(device_id))?;
        Some(TalkDevice {
            label: row.label.clone(),
            online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), chrono::Utc::now().timestamp_millis()),
        })
    }

    fn desktop_audio(&self, device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
        desktop_audio(device_id, cx)
    }

    fn icon(&self, icon: TalkIcon<'_>) -> Icon {
        match icon {
            TalkIcon::Brand => Icon::from(ExpIcon::Logo),
            TalkIcon::Concept(name) => Icon::new(registry::concept_by_name(name).unwrap_or(registry::VOICE_CALL)),
            TalkIcon::Agent { icon, color } => crate::icons::agent_icon(Some(icon), color),
        }
    }

    fn open(&self, surface: Surface, cx: &mut App) {
        match surface {
            Surface::Window => {
                if let Some(window) = crate::os_notifications::shell_window(cx) {
                    let _ = window.update(cx, |_, window, _| window.activate_window());
                    return;
                }
                crate::window_hooks::request_shell_window(cx);
            }
            Surface::VoiceSettings => crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx),
            Surface::MeetingConsent => crate::meeting_mode::request_consent(cx),
        }
    }

    fn notify(&self, notice: Notice, cx: &mut App) {
        match notice {
            Notice::Error(text) => crate::toast::show_in_active_window(crate::toast::Toast::error(text), cx),
            Notice::Failed { verb, noun, reason } => crate::action_error::failed_action(verb, noun, reason, cx),
            Notice::MeetingAutoStopped { hours } => crate::meeting_mode::notify_auto_stopped(hours, cx),
        }
    }

    fn readiness_changed(&self, cx: &mut App) {
        refresh_doctor(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-2069 (`plainProseCases`): an agent row reaches Voice as its
    /// prose (every fixture case); a person's text, a pasted tag, stays.
    #[test]
    fn voice_reads_an_agent_rows_prose_only() {
        let row = |author: &str, body: &str| domain::rows::MessageRow {
            id: "m1".to_string(),
            author_kind: Some(author.to_string()),
            body: Some(body.to_string()),
            ..Default::default()
        };
        let fixture = domain::chat_express::fixture();
        for case in fixture["plainProseCases"].as_array().expect("plainProseCases") {
            let text = case["text"].as_str().unwrap();
            let agent = row("agent", text);
            let mut heard = Vec::new();
            visit_talk(&agent, &mut |message| heard.push(message.message.body.map(str::to_string)));
            assert_eq!(heard, vec![Some(case["expected"].as_str().unwrap().to_string())], "{}", case["name"]);
            let person = row("user", text);
            let mut heard = Vec::new();
            visit_talk(&person, &mut |message| heard.push(message.message.body.map(str::to_string)));
            assert_eq!(heard, vec![Some(text.to_string())], "{}", case["name"]);
        }
    }

    /// #EXP-1941: a host told `seal` joins with a key source; a refusal
    /// joins without one, whatever the ticket says; a call the server did
    /// not mark sealed is today's; told `seal` without a node is never clear.
    #[test]
    fn a_host_told_seal_joins_with_a_key_source_and_a_refusal_without() {
        assert_eq!(host_join(true, false, true), HostJoin::Sealed);
        assert_eq!(host_join(true, true, true), HostJoin::Clear, "refuse only: a clear `failed`");
        assert_eq!(host_join(false, false, true), HostJoin::Clear);
        assert_eq!(host_join(true, false, false), HostJoin::SealedNoKey, "no key holder: no_key, never clear");
    }

    /// #EXP-1982: only a clear call this desktop answers, into a chat it
    /// cannot write in clear, while no key holder runs or comes, is refused
    /// `unavailable`; the chat is asked about only then.
    #[test]
    fn a_clear_call_into_an_encrypted_chat_without_a_key_holder_is_refused() {
        assert_eq!(clear_refusal(HostJoin::Clear, false, true, || true), Some("unavailable"));
        assert_eq!(clear_refusal(HostJoin::Clear, false, true, || false), None, "a clear tree takes plaintext");
        assert_eq!(clear_refusal(HostJoin::Clear, false, false, || panic!("not asked")), None, "a key holder seals");
        assert_eq!(clear_refusal(HostJoin::Clear, true, true, || panic!("not asked")), None, "already refused busy or meeting");
        assert_eq!(clear_refusal(HostJoin::Sealed, false, true, || panic!("not asked")), None, "a sealed call keeps its own path");
    }

    /// #EXP-1847: a resolved voice → the worker's prepare inputs: the clip's
    /// cached file with its transcript, the built-in voice while it is not
    /// here, a member path as is.
    #[test]
    fn a_resolved_voice_prepares_its_clip_once_it_is_here() {
        use domain::agent_settings::{agent_voice_of, AgentVoice as Stored, MemberVoice};
        let member = MemberVoice { language: "de", voice_file: "/home/me/me.wav", voice_transcript: "Hallo.", wake_phrase: "Hey Jarvis" };
        let own = Stored { language: Some("en".into()), clip_id: Some("c-1".into()), clip_name: Some("sloppy.wav".into()), transcript: Some("Hi.".into()), wake_phrase: Some("Hey Sloppy".into()) };
        let resolved = agent_voice_of(Some("agent"), "Sloppy", &own, &member);
        let here = voice_inputs(&resolved, |id| if id == "c-1" { Ok(PathBuf::from("/data/voice-clips/c-1.wav")) } else { Err(CallVoice::Failed) });
        assert_eq!(here.voice, etalk::VoiceChoice { language: "en".into(), file: "/data/voice-clips/c-1.wav".into(), transcript: "Hi.".into() });
        assert_eq!(here.wake_phrase, "Hey Sloppy");
        assert_eq!(here.clip, CallVoice::Installed);
        let fetching = voice_inputs(&resolved, |_| Err(CallVoice::Fetching));
        assert_eq!(fetching.voice, etalk::VoiceChoice { language: "en".into(), file: String::new(), transcript: String::new() });
        assert_eq!(fetching.clip, CallVoice::Fetching, "FEED-276: a call waits for it");
        assert_eq!(voice_inputs(&resolved, |_| Err(CallVoice::Failed)).clip, CallVoice::Failed);
        let assistant = agent_voice_of(Some("assistant"), "Bro", &Stored::default(), &member);
        let path = voice_inputs(&assistant, |_| Err(CallVoice::Fetching));
        assert_eq!(path.voice, etalk::VoiceChoice { language: "de".into(), file: "/home/me/me.wav".into(), transcript: "Hallo.".into() });
        assert_eq!(path.wake_phrase, "Hey Jarvis");
        assert_eq!(path.clip, CallVoice::Installed, "a member path is on disk");
        let team = agent_voice_of(Some("agent"), "Sloppy", &Stored::default(), &member);
        let team = voice_inputs(&team, |_| Err(CallVoice::Fetching));
        assert_eq!(team.voice, etalk::VoiceChoice { language: "de".into(), ..Default::default() });
        assert_eq!(team.clip, CallVoice::None, "no clip: the built-in voice is its own");
        assert_eq!(clip_file(Path::new("/d"), "c-1"), Some(PathBuf::from("/d/c-1.wav")));
        assert_eq!(clip_file(Path::new("/d"), "../x"), None);
    }

    /// EXP-1857: the member's own desktops, newest first, with their
    /// doctor's `speech` row; servers and other members' machines never.
    #[test]
    fn a_calls_desktops_are_the_members_own_newest_first() {
        let now = chrono::Utc::now();
        let at = |ago: i64| (now - chrono::Duration::seconds(ago)).to_rfc3339();
        let row = |device: &str, user: &str, kind: &str, ago: i64, speech: Option<&str>| domain::rows::DeviceRow {
            user_id: Some(user.into()),
            device_id: Some(device.into()),
            label: Some(format!("{device}-name")),
            kind: Some(kind.into()),
            last_seen_at: Some(at(ago)),
            doctor: speech.map(|state| serde_json::json!({ "checkedAt": "x", "items": [{ "key": "speech", "group": "voice", "state": state }] })),
            ..serde_json::from_value(serde_json::json!({ "id": device })).unwrap()
        };
        let rows = [
            row("d-old", "me", "desktop", 9_000, Some("ok")),
            row("d-server", "me", "server", 1, None),
            row("d-mint", "me", "desktop", 5, Some("missing")),
            row("d-theirs", "them", "desktop", 1, Some("ok")),
        ];
        let refs: Vec<&domain::rows::DeviceRow> = rows.iter().collect();
        let desktops = own_desktops(&refs, "me", now.timestamp_millis());
        assert_eq!(desktops.iter().map(|desktop| desktop.id.as_str()).collect::<Vec<_>>(), ["d-mint", "d-old"]);
        assert_eq!((desktops[0].name.as_str(), desktops[0].online, desktops[0].speech.as_deref()), ("d-mint-name", true, Some("missing")));
        assert!(!desktops[1].online, "seen long ago");
        // EXP-1858: the member's own servers, apart (only the agent's counts).
        let servers = own_servers(&refs, "me", now.timestamp_millis());
        assert_eq!(servers.iter().map(|server| (server.id.as_str(), server.online)).collect::<Vec<_>>(), [("d-server", true)]);
        assert_eq!(dial_error(api::ApiError::Http { status: 412, message: "mint is offline".into() }), DialError::Refused("mint is offline".into()));
        assert!(matches!(dial_error(api::ApiError::Http { status: 500, message: "x".into() }), DialError::Failed(_)));
    }

    /// meeting.json FLUSH: a refusal (4xx) drops the batch, never requeued;
    /// a transport error or 5xx retries.
    #[test]
    fn a_4xx_is_a_refusal_a_401_or_transport_error_retries() {
        let http = |status| api::ApiError::Http { status, message: "x".into() };
        assert!(refused(&http(400)) && refused(&http(403)) && refused(&http(413)));
        assert!(!refused(&http(408)) && !refused(&http(429)) && !refused(&http(500)) && !refused(&http(503)));
        assert!(!refused(&http(401)) && !refused(&api::ApiError::Unauthorized), "an expired token retries");
        assert!(!refused(&api::ApiError::Transport { message: "down".into(), offline: true }));
        assert!(send_error(http(413), true).refused && !send_error(http(503), true).refused);
        // #EXP-1982: never unstorable for an unsynced tree or a retry.
        assert!(!send_error(http(400), false).unstorable && !send_error(http(503), true).unstorable);
    }

    /// F21: every meeting part names its `clientMessageId`.
    #[test]
    fn a_meeting_part_sends_its_client_message_id() {
        let target = UtteranceTarget::Session { session_id: "s1".into(), text: "hi".into() };
        let input = send_input(Outgoing { target: target.clone(), meeting: true, client_message_id: Some("0b7c".into()) });
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["clientMessageId"], "0b7c");
        assert_eq!(json["meeting"], true);
        let plain = serde_json::to_value(send_input(Outgoing { target, meeting: false, client_message_id: None })).unwrap();
        assert!(plain.get("clientMessageId").is_none());
        assert_eq!(plain, serde_json::to_value(api::sessions::SendInput::message("s1", "hi", Vec::new())).unwrap());
    }
}
