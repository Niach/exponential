//! EXP-1273: computer use for coding runs, FOREGROUND only, on a thin
//! backend of our own ([`backend`]): the agent drives the person's real
//! pointer and keyboard through the OS input APIs (XTest, CGEvent,
//! SendInput), sees the screen as a fitted screenshot ([`tools`]), and the
//! person's own input is locked while it works (XInput2 slave grabs, a
//! CGEventTap, low-level hooks) with Esc as the abort ([`host`]). No
//! background mode, no second pointer, no accessibility tree: pixels only.
//!
//! The host process (the IDE or the `exponential` daemon) serves ONE
//! loopback MCP endpoint ([`server`]); the launcher hands a run a [`Grant`]
//! when the device's Computer use switch is on. Unrestricted by decision:
//! the switch is the one gate.
//!
//! gpui-free: the IDE's banner ("Computer use in progress · Esc to abort")
//! follows [`set_presence_observer`]; the daemon only logs.

mod backend;
mod host;
mod keys;
mod server;
mod tools;

use std::sync::{Arc, OnceLock};

pub use server::SERVER_NAME;

/// The env var the run's token rides in; the MCP config only ever says
/// `Bearer ${EXP_COMPUTER_TOKEN}`.
pub const TOKEN_ENV: &str = "EXP_COMPUTER_TOKEN";

/// What a run needs to reach the endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Grant({}, <redacted>)", self.url)
    }
}

/// Whether this machine can do computer use right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// An OS permission the person has to grant; the text names it.
    MissingPermission(String),
    /// Nothing to drive here (a display-less box, a Wayland session).
    Unsupported(String),
}

/// One OS permission computer use needs on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// macOS Screen Recording.
    ScreenRecording,
    /// macOS Accessibility.
    Accessibility,
}

impl Permission {
    /// The wire key (`device-doctor.json` labels).
    pub fn key(self) -> &'static str {
        match self {
            Permission::ScreenRecording => "screen_recording",
            Permission::Accessibility => "accessibility",
        }
    }
}

struct Live {
    hub: Arc<server::Hub>,
    host: Arc<host::Host>,
    url: String,
}

/// The process-wide endpoint, started on the first grant. An `Err` (no free
/// port) is remembered: it will not change under a running process.
static LIVE: OnceLock<Result<Live, String>> = OnceLock::new();

fn live() -> Result<&'static Live, String> {
    LIVE.get_or_init(|| {
        let host = host::Host::new();
        let hub = Arc::new(server::Hub::new(host.clone()));
        let url = server::serve(hub.clone())?;
        log::info!("[computer] serving computer use on {url}");
        if let Some(presence) = PRESENCE.get() {
            host.set_presence(presence.clone());
        }
        Ok(Live { hub, host, url })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// The IDE's banner, registered before the endpoint exists.
static PRESENCE: OnceLock<host::Presence> = OnceLock::new();

/// Admit the run `session_id`; a grant it held before stops working.
/// `label` is what the banner shows for the run (the issue's identifier,
/// the action's name).
pub fn grant(session_id: &str, label: &str) -> Result<Grant, String> {
    if let Some(reason) = backend::unsupported() {
        return Err(reason);
    }
    let live = live()?;
    Ok(Grant { url: live.url.clone(), token: live.hub.grant(session_id, label) })
}

/// The run ended: its token is dead, its abort state gone, the lock
/// released when it was the last user.
pub fn revoke(session_id: &str) {
    if let Some(Ok(live)) = LIVE.get() {
        live.hub.revoke(session_id);
    }
}

/// The run's turn began (the person's next message): an Esc that aborted
/// it is answered, its calls work again.
pub fn turn_started(session_id: &str) {
    if let Some(Ok(live)) = LIVE.get() {
        live.host.turn_started(session_id);
    }
}

/// What the host does when the person's Esc aborts `session_id`'s computer
/// use: the engine's Cancel of the running turn. A run's grant precedes its
/// engine, so an endpoint that is not up yet has no run to hook: nothing
/// starts here.
pub fn on_abort(session_id: &str, hook: Arc<dyn Fn() + Send + Sync>) {
    if let Some(Ok(live)) = LIVE.get() {
        live.host.set_hook(session_id, hook);
    }
}

/// Who hears the lock engage (`Some(label)`) and release (`None`): the
/// IDE's banner. Called at once with the current state.
pub fn set_presence_observer(observer: impl Fn(Option<String>) + Send + Sync + 'static) {
    let presence: host::Presence = Arc::new(observer);
    if PRESENCE.set(presence.clone()).is_err() {
        log::warn!("[computer] a presence observer was already set");
        return;
    }
    match LIVE.get() {
        Some(Ok(live)) => live.host.set_presence(presence),
        _ => presence(None),
    }
}

/// Every OS permission computer use needs here and whether it is granted,
/// read WITHOUT prompting (the device doctor's rows). Empty where the OS
/// asks for none up front (Windows, X11).
pub fn permissions() -> Vec<(Permission, bool)> {
    backend::permissions()
}

fn readiness_of(permissions: &[(Permission, bool)]) -> Readiness {
    if let Some(reason) = backend::unsupported() {
        return Readiness::Unsupported(reason);
    }
    match permissions.iter().find(|(_, granted)| !granted) {
        Some((Permission::ScreenRecording, _)) => {
            Readiness::MissingPermission("Screen Recording is not granted.".to_string())
        }
        Some((Permission::Accessibility, _)) => {
            Readiness::MissingPermission("Accessibility is not granted.".to_string())
        }
        None => Readiness::Ready,
    }
}

/// Whether this machine can do computer use right now, without prompting
/// and without starting anything (the doctor line).
pub fn readiness() -> Readiness {
    readiness_of(&permissions())
}

/// The switch is on: ask the OS for every permission computer use needs NOW
/// (macOS Screen Recording + Accessibility) and prove the backend opens, so
/// nothing interrupts a run. Blocks while a dialog waits: off the UI
/// thread, see [`prepare_in_background`].
pub fn prepare() -> Readiness {
    let readiness = readiness_of(&backend::request_permissions());
    if readiness == Readiness::Ready {
        match backend::open() {
            Ok(backend) => match backend.displays() {
                Ok(displays) => log::info!("[computer] computer use is ready ({} display(s))", displays.len()),
                Err(reason) => log::warn!("[computer] the screen does not answer: {reason}"),
            },
            Err(reason) => log::warn!("[computer] {reason}"),
        }
    }
    readiness
}

/// [`prepare`] on its own thread, once at a time (a local toggle and its
/// synced echo must not stack two dialogs).
pub fn prepare_in_background() {
    static PREPARING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if PREPARING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(|| {
        match prepare() {
            Readiness::Ready => {}
            Readiness::MissingPermission(reason) | Readiness::Unsupported(reason) => {
                log::warn!("[computer] computer use is not ready: {reason}")
            }
        }
        PREPARING.store(false, std::sync::atomic::Ordering::Release);
    });
}

/// The switch turned off: every run's grant ends and the person's input is
/// theirs again. Grants after this work again.
pub fn shutdown() {
    if let Some(Ok(live)) = LIVE.get() {
        live.hub.revoke_all();
        live.host.shutdown();
    }
}

/// Whether an agent drives the screen right now (the person's input is
/// locked).
pub fn running() -> bool {
    matches!(LIVE.get(), Some(Ok(live)) if live.host.engaged())
}
