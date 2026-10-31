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
pub mod work_screen;
pub mod visible;

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

    /// The primary monitor's width in the toolkit's logical pixels (the
    /// room the banner may take, EXP-1372). `None` when the numbers make
    /// no sense.
    pub fn primary_logical_width(&self, logical_root_width: f32) -> Option<f32> {
        if logical_root_width <= 0. || self.root.0 == 0 {
            return None;
        }
        Some(self.primary.width as f32 * logical_root_width / self.root.0 as f32)
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
    server: emcp::Server,
    host: Arc<host::Host>,
}

/// The process-wide endpoint, started on the first grant. An `Err` (no free
/// port) is remembered: it will not change under a running process.
static LIVE: OnceLock<Result<Live, String>> = OnceLock::new();

fn live() -> Result<&'static Live, String> {
    LIVE.get_or_init(|| {
        let host = host::Host::new();
        let server = server::serve(host.clone())?;
        log::info!("[computer] serving computer use on {}", server.url());
        if let Some(presence) = PRESENCE.get() {
            host.set_presence(presence.clone());
        }
        Ok(Live { server, host })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// The IDE's banner, registered before the endpoint exists.
static PRESENCE: OnceLock<host::Presence> = OnceLock::new();

/// NEW-59: one display as the device reports it (`doctor.displays`) and as
/// the overlay and the banner place by: its stable `id` (#FEED-161, the
/// stored Work screen value), the 1-based `number` in the OS's order
/// (list_windows' index + 1, the label only), the rect in the OS's screen
/// units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub id: String,
    pub number: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// NEW-59: the screen as the work-screen rules read it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenState {
    pub displays: Vec<Display>,
    /// 0-based indexes of the displays holding a fullscreen window.
    pub fullscreen: Vec<usize>,
    /// 0-based indexes agents may drive ([`work_screen::work_displays`]).
    pub work: Vec<usize>,
}

/// NEW-59: the device's Work screen setting (`launch_defaults.computerUseDisplayId`,
/// the picked [`Display::id`]; `None` = Auto). The hub resolves it to the
/// display carrying that id on every call (#FEED-161).
pub fn set_work_display(id: Option<String>) {
    work_screen::set_picked(id);
}

/// #FEED-164 TWINS UPGRADE (`work-screen.json` `legacyIdCases`): the id
/// among `ids` (the reported displays, in order) a stored Work screen pick
/// names: itself, or for an id stored before twins carried their connector
/// (`base`, `base#n`) the twin the old scheme named; `None` = not connected.
pub fn picked_display_id<'a>(ids: &[&'a str], stored: &str) -> Option<&'a str> {
    work_screen::picked_index(ids, stored).map(|index| ids[index])
}

/// NEW-59: the displays for the doctor (`doctor.displays`). Empty without a
/// display (a server, Wayland) or when the backend does not open.
pub fn displays() -> Vec<Display> {
    work_screen::displays()
}

/// NEW-59: `display`'s size in physical pixels (the doctor's `displays`
/// and the Work screen labels): macOS reports [`Display`] in points, so a
/// Retina panel reads its native pixels here; elsewhere the rect already is
/// pixels.
pub fn pixel_size(display: &Display) -> (u32, u32) {
    backend::pixel_size(display.x, display.y, display.width, display.height)
}

/// NEW-58: the focused window as screen memory reads it: the owning app
/// (its name and, where the OS names it cheaply, its id: the macOS bundle
/// id), its title, the 1-based display it overlaps most (#FEED-161; the
/// [`displays`] order) and its rect within that display in PHYSICAL pixels (the frame's
/// own pixels), clipped to the display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusedWindow {
    pub app: String,
    /// The app's id (macOS bundle id); `None` where the OS has none to hand.
    pub app_id: Option<String>,
    pub title: String,
    pub display: u32,
    pub rect: ScreenRect,
}

/// NEW-58: the top window of one display (the first of the window list
/// that overlaps it most).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopWindow {
    /// 1-based.
    pub display: u32,
    pub app: String,
    /// The app's id (macOS bundle id), like [`FocusedWindow::app_id`]; the
    /// denylist matches it (#FEED-164).
    pub app_id: Option<String>,
    pub title: String,
}

/// NEW-58: what screen memory reads on every poll, from ONE window-list
/// read: the focused window (`None` = the focus is on no listed window or
/// off every display), the work displays (1-based,
/// [`work_screen::work_displays`]) and each display's top window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenView {
    pub focused: Option<FocusedWindow>,
    pub work: Vec<u32>,
    pub tops: Vec<TopWindow>,
}

/// NEW-58: the screen as screen memory reads it now; `None` without a
/// display or a window list (Wayland, a refused list). One window-list read
/// (a few ms, the display set cached): polled every `capture.focusPollMs`.
pub fn screen_view() -> Option<ScreenView> {
    work_screen::screen_view()
}

/// NEW-58: the focused window now ([`screen_view`]'s).
pub fn focused_window() -> Option<FocusedWindow> {
    screen_view()?.focused
}

/// NEW-59: the displays, their fullscreen windows and the work displays now;
/// `None` without a display. Reads the OS (a few ms): call it when the
/// overlay opens, not per frame.
pub fn screen_state() -> Option<ScreenState> {
    work_screen::screen_state()
}

/// NEW-59: the display the computer-use banner floats on: the engaged run's
/// current display (the work screen), else `None` (the primary).
pub fn banner_display() -> Option<Display> {
    work_screen::banner_display()
}

/// Admit the run `session_id`; a grant it held before stops working.
/// `label` is what the banner shows for the run (the issue's identifier,
/// the action's name).
pub fn grant(session_id: &str, label: &str) -> Result<Grant, String> {
    if let Some(reason) = backend::unsupported() {
        return Err(reason);
    }
    let live = live()?;
    Ok(Grant { url: live.server.url().to_string(), token: live.server.grant(session_id, label) })
}

/// The run ended: its token is dead, its abort state gone, the lock
/// released when it was the last user.
pub fn revoke(session_id: &str) {
    if let Some(Ok(live)) = LIVE.get() {
        live.server.revoke(session_id);
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
        live.server.revoke_all();
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

    #[test]
    fn the_banners_room_is_the_primary_monitor() {
        // EXP-1372: two 5K monitors at scale 2 = 2560 logical each, not
        // the 5120 of the whole X screen.
        let layout = ScreenLayout {
            root: (10240, 2880),
            primary: ScreenRect { x: 5120, y: 0, width: 5120, height: 2880 },
            work_area: None,
        };
        assert_eq!(layout.primary_logical_width(5120.), Some(2560.));
        // A 1920 monitor beside a 2560 one at scale 1.
        let mixed = ScreenLayout { root: (4480, 1440), primary: ScreenRect { x: 0, y: 0, width: 1920, height: 1080 }, ..layout };
        assert_eq!(mixed.primary_logical_width(4480.), Some(1920.));
        assert_eq!(layout.primary_logical_width(0.), None);
    }
}
