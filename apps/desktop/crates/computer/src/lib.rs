//! EXP-1196: computer use for coding runs. The host process (the IDE or the
//! `exponential` daemon) serves ONE loopback MCP endpoint; the launcher hands
//! a run a [`Grant`] when the device's Computer use switch is on, and the
//! agent then sees `screenshot`, `click`, `type`, `key`, `scroll`,
//! `list_windows`, `focus_window` and `read_ui`.
//!
//! In-process on purpose: the OS permissions (Screen Recording,
//! Accessibility) belong to the host app the person granted them to, one
//! turn lock serializes every run's input, and nothing has to locate a
//! helper binary.
//!
//! Safety is [`guard`]: a pause while the person is at the keyboard and the
//! driving stamp the hosts turn into an indicator. No app blocklist: the
//! device switch is the one gate (unrestricted by decision).

pub mod backend;
pub mod guard;
mod server;

#[cfg(all(unix, not(target_os = "macos")))]
mod atspi;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop;
#[cfg(test)]
mod fake;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(unix, not(target_os = "macos")))]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub use backend::Readiness;
pub use server::SERVER_NAME;

/// The env var the run's token rides in; the MCP config only ever says
/// `Bearer ${EXP_COMPUTER_TOKEN}`.
pub const TOKEN_ENV: &str = "EXP_COMPUTER_TOKEN";

/// How long after its last input action a run still counts as driving.
pub const DRIVING_WINDOW: Duration = Duration::from_secs(4);

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

struct Host {
    hub: Arc<server::Hub>,
    url: String,
}

/// The process-wide host, started on the first grant. An `Err` (no backend
/// for this session type, no free port) is remembered: it will not change
/// under a running process.
static HOST: OnceLock<Result<Host, String>> = OnceLock::new();

fn host() -> Result<&'static Host, String> {
    HOST.get_or_init(|| {
        let hub = Arc::new(server::Hub::new(guard::Guard::new(backend::platform()?)));
        let url = server::serve(hub.clone())?;
        log::info!("[computer] serving computer use on {url}");
        Ok(Host { hub, url })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Admit the run `session_id`; a grant it held before stops working.
pub fn grant(session_id: &str) -> Result<Grant, String> {
    let host = host()?;
    Ok(Grant { url: host.url.clone(), token: host.hub.grant(session_id) })
}

/// The run ended: its token is dead.
pub fn revoke(session_id: &str) {
    if let Some(Ok(host)) = HOST.get() {
        host.hub.revoke(session_id);
    }
}

/// Whether this machine can do computer use right now, without prompting
/// and without starting the server (the doctor line).
pub fn readiness() -> Readiness {
    match backend::platform() {
        Ok(backend) => backend.readiness(false),
        Err(reason) => Readiness::Unsupported(reason),
    }
}

/// Ask the OS for every permission computer use needs NOW (macOS Screen
/// Recording + Accessibility, the Wayland remote-desktop and screenshot
/// dialogs), so none interrupts a run. The hosts call it when the device's
/// switch turns on, locally or synced in, and at start while it is on.
/// Blocks while a dialog waits: off the UI thread, see
/// [`prepare_in_background`]. Goes through the live host (starting it), so
/// a Wayland session it opens is the one the runs then drive.
pub fn prepare() -> Readiness {
    match host() {
        Ok(host) => host.hub.guard.prepare(),
        Err(reason) => Readiness::Unsupported(reason),
    }
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
            Readiness::Ready => log::info!("[computer] computer use is ready"),
            Readiness::MissingPermission(reason) | Readiness::Unsupported(reason) => {
                log::warn!("[computer] computer use is not ready: {reason}")
            }
        }
        PREPARING.store(false, std::sync::atomic::Ordering::Release);
    });
}

/// The session id of the run that acted within [`DRIVING_WINDOW`].
pub fn driving() -> Option<String> {
    let (at, session_id) = HOST.get()?.as_ref().ok()?.hub.driving()?;
    (at.elapsed() < DRIVING_WINDOW).then_some(session_id)
}

/// Re-read the keyboard layout a chord's character keys resolve against.
/// The host calls this ON ITS MAIN THREAD, at start and now and then after
/// (the person may switch layouts); anywhere else, and on every OS but
/// macOS, it does nothing. See `macos::refresh_key_layout` for why the
/// thread matters.
pub fn refresh_key_layout() {
    #[cfg(target_os = "macos")]
    macos::refresh_key_layout();
}

type FirstActionHook = Box<dyn Fn(&str) + Send + Sync>;
static FIRST_ACTION: Mutex<Vec<FirstActionHook>> = Mutex::new(Vec::new());

/// Call `hook(session_id)` on each run's FIRST input action (the daemon's
/// "an agent is driving this computer" notice).
pub fn on_first_action(hook: impl Fn(&str) + Send + Sync + 'static) {
    FIRST_ACTION.lock().unwrap().push(Box::new(hook));
}

pub(crate) fn fire_first_action(session_id: &str) {
    for hook in FIRST_ACTION.lock().unwrap().iter() {
        hook(session_id);
    }
}
