//! EXP-1196: computer use for coding runs, through the cua driver
//! (trycua/cua, MIT), linked. The host process (the IDE or the
//! `exponential` daemon) serves ONE loopback MCP endpoint whose tools are
//! cua's own (`list_windows`, `get_window_state`, `click`, `type_text`,
//! `verify_state`, ...); the launcher hands a run a [`Grant`] when the
//! device's Computer use switch is on.
//!
//! The platform runtime lives in a private worker, this same executable
//! re-run as `__private-worker` ([`worker`]): cua's topology for a GUI host.
//! It inherits the host's OS permissions (TCC's responsibility chain), owns
//! the agent cursor's main thread, and a crash in it never takes the host
//! down. The worker runs while the switch is on and the hub has had a
//! reason to start it; [`shutdown`] stops it when the switch turns off.
//!
//! Unrestricted by decision: the switch is the one gate. No app blocklist,
//! no approval cards, nothing on top of what cua itself refuses.

#[cfg(feature = "cua")]
mod driver;
mod server;
pub mod worker;

#[cfg(feature = "cua")]
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
    /// Nothing to drive here (a display-less Linux box).
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

#[cfg(feature = "cua")]
struct Host {
    hub: Arc<server::Hub>,
    driver: Arc<driver::Driver>,
    url: String,
}

/// The process-wide host, started on the first grant or prepare. An `Err`
/// (no free port, no runtime) is remembered: it will not change under a
/// running process.
#[cfg(feature = "cua")]
static HOST: OnceLock<Result<Host, String>> = OnceLock::new();

#[cfg(feature = "cua")]
fn host() -> Result<&'static Host, String> {
    HOST.get_or_init(|| {
        let driver = Arc::new(driver::Driver::new(&host_bundle_id())?);
        let hub = Arc::new(server::Hub::new(driver.clone()));
        let url = server::serve(hub.clone())?;
        log::info!("[computer] serving computer use on {url}");
        Ok(Host { hub, driver, url })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Why computer use is not in this binary at all, if so.
const NOT_LINKED: Option<&str> = if cfg!(feature = "cua") {
    None
} else {
    Some("Not available in this build (the Linux daemon); use the desktop app.")
};

/// The advisory host identity cua echoes in its diagnostics (never a trust
/// signal): the executable's name under the app's reverse domain.
#[cfg(feature = "cua")]
fn host_bundle_id() -> String {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "host".to_string());
    format!("at.exponential.{stem}")
}

/// Why this machine cannot drive anything at all, if so.
fn unsupported() -> Option<String> {
    if let Some(reason) = NOT_LINKED {
        return Some(reason.to_string());
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Some("No display session (neither DISPLAY nor WAYLAND_DISPLAY).".to_string());
    }
    None
}

/// Admit the run `session_id`; a grant it held before stops working.
/// `label` is what the agent cursor's badge shows for the run (the issue's
/// identifier, the action's name).
pub fn grant(session_id: &str, label: &str) -> Result<Grant, String> {
    if let Some(reason) = unsupported() {
        return Err(reason);
    }
    #[cfg(feature = "cua")]
    {
        let host = host()?;
        Ok(Grant { url: host.url.clone(), token: host.hub.grant(session_id, label) })
    }
    #[cfg(not(feature = "cua"))]
    unreachable!("{session_id} {label}")
}

/// The run ended: its token is dead and its session closed.
pub fn revoke(session_id: &str) {
    #[cfg(feature = "cua")]
    if let Some(Ok(host)) = HOST.get() {
        host.hub.revoke(session_id);
    }
    #[cfg(not(feature = "cua"))]
    let _ = session_id;
}

/// Every OS permission computer use needs here and whether it is granted,
/// read WITHOUT prompting (the device doctor's rows). Empty where the OS
/// asks for none up front (Windows, X11; a Wayland desktop's portals ask
/// per session, inside cua).
pub fn permissions() -> Vec<(Permission, bool)> {
    #[cfg(all(target_os = "macos", feature = "cua"))]
    {
        let status = cua_driver_sdk::current_mac_os_permission_status();
        vec![
            (Permission::ScreenRecording, status.screen_recording),
            (Permission::Accessibility, status.accessibility),
        ]
    }
    #[cfg(not(all(target_os = "macos", feature = "cua")))]
    {
        Vec::new()
    }
}

fn readiness_of(permissions: &[(Permission, bool)]) -> Readiness {
    if let Some(reason) = unsupported() {
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
/// (macOS Screen Recording + Accessibility, attributed to this app, which
/// the worker then inherits) and bring the worker up, so neither interrupts
/// a run. Blocks while a dialog waits: off the UI thread, see
/// [`prepare_in_background`].
pub fn prepare() -> Readiness {
    #[cfg(all(target_os = "macos", feature = "cua"))]
    let permissions = {
        let status = cua_driver_sdk::request_mac_os_permissions();
        vec![
            (Permission::ScreenRecording, status.screen_recording),
            (Permission::Accessibility, status.accessibility),
        ]
    };
    #[cfg(not(all(target_os = "macos", feature = "cua")))]
    let permissions = permissions();
    let readiness = readiness_of(&permissions);
    #[cfg(feature = "cua")]
    if readiness == Readiness::Ready {
        if let Err(reason) = host().and_then(|host| host.driver.start()) {
            log::warn!("[computer] {reason}");
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
            Readiness::Ready => log::info!("[computer] computer use is ready"),
            Readiness::MissingPermission(reason) | Readiness::Unsupported(reason) => {
                log::warn!("[computer] computer use is not ready: {reason}")
            }
        }
        PREPARING.store(false, std::sync::atomic::Ordering::Release);
    });
}

/// The switch turned off: every run's grant ends and the worker stops (its
/// agent cursor with it). Grants after this start it again.
pub fn shutdown() {
    #[cfg(feature = "cua")]
    if let Some(Ok(host)) = HOST.get() {
        host.hub.revoke_all();
        host.driver.stop();
    }
}

/// Whether the worker runs right now.
pub fn running() -> bool {
    #[cfg(feature = "cua")]
    return matches!(HOST.get(), Some(Ok(host)) if host.driver.is_live());
    #[cfg(not(feature = "cua"))]
    false
}
