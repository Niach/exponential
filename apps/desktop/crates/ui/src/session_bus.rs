//! #EDE-227: the holder's end of the session bus ([`api::bus`]): the shell's
//! links with the launch token of the press, its holder commands, and a
//! component's request that a standalone app hand the login over.
//!
//! The bus is bound by whoever holds the app's single-instance lock (`app`
//! decides that before gpui starts); this installs its server once the
//! foreground is up. Requests are answered on the bus's threads; what they
//! ask is done here, on the foreground.

use api::bus::{Peer, Reply, Request, Role};
use gpui::App;

/// What the bus asks of the foreground.
enum Event {
    Open { link: Option<String>, activation_token: Option<String> },
    Holder(api::holder::Command),
    Handover,
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
    }
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
