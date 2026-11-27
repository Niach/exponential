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
    Ask { text: String, reply: flume::Sender<Result<(), String>> },
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
            Request::Page { .. } | Request::Search { .. } | Request::Ask { .. } if !crate::ede_component::is_component() => {
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
            Request::Ask { text, .. } => {
                let (reply, answer) = flume::bounded(1);
                if tx.send(Event::Ask { text, reply }).is_err() {
                    return Reply::Refused { message: "the app is quitting".into() };
                }
                return match answer.recv_timeout(FOREGROUND_TIMEOUT) {
                    Ok(Ok(())) => Reply::Done,
                    Ok(Err(message)) => Reply::Refused { message },
                    Err(_) => Reply::Refused { message: "the app did not answer".into() },
                };
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
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            cx.update(|cx| handle(event, cx));
        }
    })
    .detach();
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
        Event::Ask { text, reply } => {
            let _ = reply.send(ask(&text, cx));
        }
    }
}

/// #EDE-258: `text` to the viewer's Assistant, as its chat's composer sends
/// it (`sessions.send({agentId})`, sealed like any send); no window opens.
fn ask(text: &str, cx: &mut App) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("nothing to send".into());
    }
    let assistant = crate::session_data::my_assistant_anywhere(None, cx).ok_or("no Assistant")?;
    if assistant.team_id.is_none() {
        return Err("the Assistant has no team".into());
    }
    crate::chat_actions::send_to_agent(&assistant.id, text, cx);
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
