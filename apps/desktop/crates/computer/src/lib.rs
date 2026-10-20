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

/// A rectangle of the screen in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// EXP-1273: where the IDE's banner goes on X11. The toolkit's X11 display
/// is the whole X screen (every monitor side by side) in logical pixels,
/// so centring on it lands on the seam between two monitors; the primary
/// monitor and the panel-free work area come from the server instead, in
/// physical pixels, and [`ScreenLayout::banner_origin`] scales them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLayout {
    /// The root's size (all monitors).
    pub root: (u32, u32),
    /// The primary monitor.
    pub primary: ScreenRect,
    /// What the panels leave of the root (`_NET_WORKAREA`), when published.
    pub work_area: Option<ScreenRect>,
}

impl ScreenLayout {
    /// The top-left of a `width` x `height` (logical) banner floating
    /// `gap` above the bottom centre of the primary monitor's visible
    /// area, in the toolkit's logical pixels, given the root's logical
    /// width (its physical width over the scale factor). `None` when the
    /// numbers make no sense.
    pub fn banner_origin(&self, logical_root_width: f32, width: f32, height: f32, gap: f32) -> Option<(f32, f32)> {
        if logical_root_width <= 0. || self.root.0 == 0 {
            return None;
        }
        let scale = self.root.0 as f32 / logical_root_width;
        let primary = self.primary;
        let mut bottom = primary.y + primary.height as i32;
        if let Some(area) = self.work_area {
            let area_bottom = area.y + area.height as i32;
            if area_bottom > primary.y && area_bottom < bottom {
                bottom = area_bottom;
            }
        }
        let x = (primary.x as f32 + (primary.width as f32 - width * scale) / 2.) / scale;
        let y = bottom as f32 / scale - height - gap;
        Some((x, y))
    }
}

/// The screen layout the banner is placed by (X11 only; `None` elsewhere
/// and without a display).
pub fn screen_layout() -> Option<ScreenLayout> {
    #[cfg(target_os = "linux")]
    {
        backend::screen_layout()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_centres_on_the_primary_monitor_above_the_panel() {
        // Two 5120x2880 monitors side by side at scale 2, a 60 px panel at
        // the bottom: the root is 10240x2880 physical, 5120 logical wide.
        let layout = ScreenLayout {
            root: (10240, 2880),
            primary: ScreenRect { x: 0, y: 0, width: 5120, height: 2880 },
            work_area: Some(ScreenRect { x: 0, y: 0, width: 10240, height: 2820 }),
        };
        let (x, y) = layout.banner_origin(5120., 560., 64., 16.).unwrap();
        assert_eq!((x, y), ((2560. - 560.) / 2., 2820. / 2. - 64. - 16.));
        // The primary on the right, no work area: its own bottom.
        let right = ScreenLayout {
            root: (10240, 2880),
            primary: ScreenRect { x: 5120, y: 0, width: 5120, height: 2880 },
            work_area: None,
        };
        assert_eq!(right.banner_origin(5120., 560., 64., 16.).unwrap(), (2560. + 1000., 1440. - 80.));
        // A work area that ends above the primary (a panel on another
        // monitor) is ignored; nonsense widths give nothing.
        let elsewhere = ScreenLayout { work_area: Some(ScreenRect { x: 0, y: -100, width: 1, height: 50 }), ..right };
        assert_eq!(elsewhere.banner_origin(5120., 560., 64., 16.), right.banner_origin(5120., 560., 64., 16.));
        assert_eq!(right.banner_origin(0., 560., 64., 16.), None);
    }
}
