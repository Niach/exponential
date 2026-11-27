//! #EDE-227: the holder's end of the session bus ([`api::bus`]): the shell's
//! links with the launch token of the press, its holder commands, and a
//! component's request that a standalone app hand the login over.
//!
//! The bus is bound by whoever holds the app's single-instance lock (`app`
//! decides that before gpui starts); this installs its server once the
//! foreground is up. Requests are answered on the bus's threads; what they
//! ask is done here, on the foreground.
//!
//! #EDE-258 (eDE mode): the start menu's feeds are answered on the bus
//! thread from [`crate::ede_feeds::feed_board`]; a page opens by its window
//! key, an issue search and an Assistant line wait for the foreground's
//! answer (under [`api::bus::CALL_TIMEOUT`]).
//!
//! #EDE-239: an Assistant line lands in the Assistant's ONE window (its
//! page `agent:<id>`, focused or opened, with the press's token); the
//! taskbar's session area stops a chat (`stop`) and answers its waiting
//! card in place (`answer`), as the IDE's Stop and card buttons do.
//!
//! #EDE-267: the shell's voice tray: `voice` is answered on the bus thread
//! from [`crate::ede_voice::voice_board`] (its own board), `voice-act` on the
//! foreground ([`crate::ede_voice::act`]).

use std::time::Duration;

use api::bus::{Peer, Reply, Request, Role};
use gpui::App;

/// How long a bus thread waits for the foreground's answer (a client gives
/// up at [`api::bus::CALL_TIMEOUT`]).
const FOREGROUND_TIMEOUT: Duration = Duration::from_millis(1200);

/// What the bus asks of the foreground.
enum Event {
    Open { link: Option<String>, activation_token: Option<String> },
    Holder(api::holder::Command),
    Handover,
    /// #EDE-258: the page of a window key.
    Page { key: String, activation_token: Option<String> },
    /// #EDE-258: the issues for a query, answered on `reply`.
    Search { query: String, reply: flume::Sender<Vec<serde_json::Value>> },
    /// #EDE-258: a line to the viewer's Assistant; `Err` = why not.
    Ask { text: String, activation_token: Option<String>, reply: flume::Sender<Result<(), String>> },
    /// #EDE-239: stop a chat's working sessions; `Err` = why not.
    Stop { root: String, reply: flume::Sender<Result<(), String>> },
    /// #EDE-239: answer a waiting card in place.
    Answer { input: api::sessions::AnswerInput, reply: flume::Sender<Result<(), String>> },
    /// #EDE-267: one of the voice tray's acts.
    VoiceAct { action: String, item: Option<String>, reply: flume::Sender<Result<(), String>> },
}

/// Binds the bus in the app's data directory and serves it for the life of
/// the process. A bind that fails leaves the link socket alone in charge.
pub fn install(role: Role, cx: &mut App) {
    let (tx, rx) = flume::unbounded::<Event>();
    let path = api::bus::socket_path(&api::default_data_dir());
    let served = api::bus::serve(&path, move |peer: &Peer, request| {
        let event = match request {
            Request::Hello => return Reply::Hello { version: api::bus::VERSION, pid: std::process::id(), role },
            Request::Open { link, activation_token } => Event::Open { link, activation_token },
            Request::Holder { command } => Event::Holder(command),
            Request::Handover if role == Role::Component => return Reply::Refused { message: "the session's component holds the login".into() },
            Request::Handover => Event::Handover,
            // #EDE-237: what the holder's windows are, held on this bus
            // thread until they change.
            Request::Windows { after } => {
                let (generation, windows) = crate::ede_windows::window_board().wait(after, api::bus::WINDOWS_WAIT);
                return Reply::Windows { generation, app_id: crate::ede_windows::app_id(), windows };
            }
            // #EDE-258: the start menu's feeds, held like the windows.
            Request::Feeds { after } => {
                let (generation, feeds) = crate::ede_feeds::feed_board().wait(after, api::bus::FEEDS_WAIT);
                return Reply::Feeds { generation, feeds };
            }
            // #EDE-267: the voice tray, held on its own board.
            Request::Voice { after } => {
                let (generation, voice) = crate::ede_voice::voice_board().wait(after, api::bus::VOICE_WAIT);
                return Reply::Voice { generation, voice };
            }
            Request::Page { .. } | Request::Search { .. } | Request::Ask { .. } | Request::Stop { .. } | Request::Answer { .. } | Request::VoiceAct { .. }
                if !crate::ede_component::is_component() =>
            {
                return Reply::Refused { message: "only eDE's component opens pages".into() };
            }
            Request::Page { key, .. } if crate::ede_windows::screen_of_key(&key).is_none() => {
                return Reply::Refused { message: format!("no page {key}") };
            }
            Request::Page { key, activation_token } => Event::Page { key, activation_token },
            Request::Search { query } => {
                if query.trim().is_empty() {
                    return Reply::Search { issues: Vec::new() };
                }
                let (reply, answer) = flume::bounded(1);
                if tx.send(Event::Search { query, reply }).is_err() {
                    return Reply::Refused { message: "the app is quitting".into() };
                }
                return match answer.recv_timeout(FOREGROUND_TIMEOUT) {
                    Ok(issues) => Reply::Search { issues },
                    Err(_) => Reply::Refused { message: "the search took too long".into() },
                };
            }
            Request::Ask { text, activation_token } => {
                let (reply, answer) = flume::bounded(1);
                return answered(tx.send(Event::Ask { text, activation_token, reply }).is_ok(), &answer);
            }
            Request::Stop { root } => {
                let (reply, answer) = flume::bounded(1);
                return answered(tx.send(Event::Stop { root, reply }).is_ok(), &answer);
            }
            Request::Answer { message_id, decision, answer: text } => {
                let input = api::sessions::AnswerInput { answers: text.as_ref().map(|text| serde_json::json!([text])), message_id, decision, answer: text, ..Default::default() };
                let (reply, answer) = flume::bounded(1);
                return answered(tx.send(Event::Answer { input, reply }).is_ok(), &answer);
            }
            Request::VoiceAct { action, .. } if !api::bus::VOICE_ACTIONS.contains(&action.as_str()) => {
                return Reply::Refused { message: format!("no voice action {action}") };
            }
            Request::VoiceAct { action, item } => {
                let (reply, answer) = flume::bounded(1);
                return answered(tx.send(Event::VoiceAct { action, item, reply }).is_ok(), &answer);
            }
        };
        log::info!("[bus] a request of pid {}", peer.pid);
        match tx.send(event) {
            Ok(()) => Reply::Done,
            Err(_) => Reply::Refused { message: "the app is quitting".into() },
        }
    });
    match served {
        Ok(()) => log::info!("[bus] serving {} as the {role:?}", path.display()),
        Err(err) => {
            log::warn!("[bus] {}: {err}", path.display());
            return;
        }
    }
    // #EDE-258: eDE mode publishes the start menu's feeds.
    crate::ede_feeds::install(cx);
    // #EDE-267: and the voice tray the shell's status icon shows.
    crate::ede_voice::install(cx);
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            cx.update(|cx| handle(event, cx));
        }
    })
    .detach();
}

/// The bus's reply to a foreground `answer` (`sent` = the event reached it).
fn answered(sent: bool, answer: &flume::Receiver<Result<(), String>>) -> Reply {
    if !sent {
        return Reply::Refused { message: "the app is quitting".into() };
    }
    match answer.recv_timeout(FOREGROUND_TIMEOUT) {
        Ok(Ok(())) => Reply::Done,
        Ok(Err(message)) => Reply::Refused { message },
        Err(_) => Reply::Refused { message: "the app did not answer".into() },
    }
}

fn handle(event: Event, cx: &mut App) {
    match event {
        Event::Open { link, activation_token } => open(link, activation_token, cx),
        Event::Holder(command) => crate::session_holder::run_command(command, cx),
        Event::Handover => {
            log::info!("[bus] eDE's component takes the login over: quitting");
            cx.quit();
        }
        Event::Page { key, activation_token } => {
            if let Some(token) = activation_token.filter(|token| !token.is_empty()) {
                gpui_linux::hand_activation_token(token);
            }
            cx.activate(true);
            crate::ede_windows::open_key(&key, cx);
        }
        Event::Search { query, reply } => {
            let _ = reply.send(crate::ede_feeds::search(&query, cx));
        }
        Event::Ask { text, activation_token, reply } => {
            let _ = reply.send(ask(&text, activation_token, cx));
        }
        Event::Stop { root, reply } => {
            let _ = reply.send(stop(&root, cx));
        }
        Event::Answer { input, reply } => {
            let _ = reply.send(answer(input, cx));
        }
        Event::VoiceAct { action, item, reply } => {
            let _ = reply.send(crate::ede_voice::act(&action, item.as_deref(), cx));
        }
    }
}

/// #EDE-239: the IDE's Stop on the chat `root`: every working session of
/// its tree (`steer_viewer::stop_sessions`, its toasts on failure).
fn stop(root: &str, cx: &mut App) -> Result<(), String> {
    let working: Vec<String> = crate::session_data::tree_of_root(root, cx).iter().filter(|session| crate::session_data::session_is_working(session)).map(|session| session.id.clone()).collect();
    if working.is_empty() {
        return Err("nothing is working in that chat".into());
    }
    crate::steer_viewer::stop_sessions(working, cx);
    Ok(())
}

/// #EDE-239: a waiting card answered in place (`session_chat::answer_card`:
/// once per card, sealed like the card's own buttons).
fn answer(input: api::sessions::AnswerInput, cx: &mut App) -> Result<(), String> {
    if input.decision.is_none() && input.answer.as_deref().is_none_or(|text| text.trim().is_empty()) {
        return Err("no answer".into());
    }
    if let Some(decision) = input.decision.as_deref().filter(|decision| !matches!(*decision, "approved" | "rejected")) {
        return Err(format!("no decision {decision}"));
    }
    crate::session_chat::answer_card(input, cx);
    Ok(())
}

/// #EDE-258: `text` to the viewer's Assistant, as its chat's composer sends
/// it (`sessions.send({agentId})`, sealed like any send). #EDE-239: the
/// Assistant's one window comes up on it (focused, else opened; it takes the
/// keyboard with the press's token).
fn ask(text: &str, activation_token: Option<String>, cx: &mut App) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("nothing to send".into());
    }
    let assistant = crate::session_data::my_assistant_anywhere(None, cx).ok_or("no Assistant")?;
    if assistant.team_id.is_none() {
        return Err("the Assistant has no team".into());
    }
    crate::chat_actions::send_to_agent(&assistant.id, text, cx);
    if let Some(token) = activation_token.filter(|token| !token.is_empty()) {
        gpui_linux::hand_activation_token(token);
    }
    cx.activate(true);
    crate::ede_windows::open_key(&format!("agent:{}", assistant.id), cx);
    Ok(())
}

/// A link (or none: a window) lands in a window that comes to the front
/// with the press's token.
fn open(link: Option<String>, activation_token: Option<String>, cx: &mut App) {
    if let Some(token) = activation_token.filter(|token| !token.is_empty()) {
        gpui_linux::hand_activation_token(token);
    }
    cx.activate(true);
    // #EDE-237: in eDE mode the link lands on its page's own window.
    if crate::ede_component::is_component() {
        crate::ede_windows::open_from_bus(link, cx);
        return;
    }
    match crate::os_notifications::shell_window(cx) {
        Some(window) => {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        None => {
            crate::window_hooks::request_shell_window(cx);
        }
    }
    if let Some(link) = link {
        crate::handle_open_urls(vec![link], cx);
    }
}
