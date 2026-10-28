//! The OS backend behind the tools: what one desktop can do, in the input
//! units of its OS. One implementation per OS; the hub opens one per tool
//! call ([`open`]: an X connection, a no-op elsewhere), so a connection that
//! died takes no state with it.
//!
//! Everything here is FOREGROUND: the pointer the person sees moves, the
//! active window is the one that gets the keys. Coordinates are the OS's
//! own (X11 and Windows: physical pixels of the virtual screen; macOS:
//! points of the main display); the frame the agent sees is `tools.rs`'s.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::keys::Key;

/// One RGBA8 frame of the screen, the OS's full resolution.
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One display (monitor) of the screen, in screen units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayInfo {
    /// #FEED-161: stable across reconnects and reorders, per OS
    /// (`fixtures/work-screen.json` DISPLAYS: macOS the display UUID,
    /// Windows the monitor's device interface path, X11 `edid:`/`output:`);
    /// unique within one list ([`finish_ids`]), at most [`ID_MAX`] chars.
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub name: String,
}

impl DisplayInfo {
    /// The area (screen units²) a rect shares with this display.
    pub(crate) fn overlap(&self, x: i32, y: i32, width: u32, height: u32) -> u64 {
        let span = |start: i32, len: u32, own: i32, own_len: u32| {
            let lo = i64::from(start.max(own));
            let hi = (i64::from(start) + i64::from(len)).min(i64::from(own) + i64::from(own_len));
            (hi - lo).max(0) as u64
        };
        span(x, width, self.x, self.width) * span(y, height, self.y, self.height)
    }
}

/// The longest display id (`work-screen.json` DISPLAYS).
pub(crate) const ID_MAX: usize = 128;

/// #FEED-161: every id at most [`ID_MAX`] chars, and twins (two monitors
/// without a serial) told apart by `#2`, `#3`, … in the OS's order.
pub(crate) fn finish_ids(displays: &mut [DisplayInfo]) {
    let clip = |id: &str, room: usize| id.chars().take(room).collect::<String>();
    let mut seen: Vec<String> = Vec::with_capacity(displays.len());
    for display in displays.iter_mut() {
        let base = clip(&display.id, ID_MAX);
        let mut id = base.clone();
        let mut n = 1;
        while seen.contains(&id) {
            n += 1;
            let suffix = format!("#{n}");
            id = format!("{}{suffix}", clip(&base, ID_MAX - suffix.len()));
        }
        seen.push(id.clone());
        display.id = id;
    }
}

/// FNV-1a 64: a hash that stays the same across builds (X11's `edid:` id).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// One top-level window, in screen units.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowInfo {
    pub id: u64,
    pub title: String,
    /// The owning application (WM_CLASS, the bundle's name, the exe stem).
    pub app: String,
    pub pid: Option<u32>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub active: bool,
    /// NEW-59: a fullscreen app's window, by the OS's rule
    /// (`fixtures/work-screen.json` FULLSCREEN): macOS = an app's layer-0
    /// window (not the Dock's, Finder's or the Window Server's) whose bounds
    /// equal a display's; Windows = a non-tool window (not the desktop or
    /// the taskbar) covering its monitor; X11 = `_NET_WM_STATE_FULLSCREEN`.
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Left,
    Middle,
    Right,
}

/// Released on drop. Dropping from the lock's own thread (inside `on_esc`)
/// must not join that thread.
pub(crate) trait LockGuard: Send {}

pub(crate) trait Backend: Send {
    /// Every display, the primary marked; never empty.
    fn displays(&self) -> Result<Vec<DisplayInfo>, String>;
    /// `display`'s pixels now; any resolution, the frame fits it.
    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String>;
    fn cursor_position(&self) -> Result<(i32, i32), String>;
    /// Top first, the focused one `active`, each marked `fullscreen` (one
    /// window-list read: the work-screen gate reads it on every call).
    fn list_windows(&self) -> Result<Vec<WindowInfo>, String>;
    /// Raise + focus `id` (a `WindowInfo::id`).
    fn activate_window(&self, id: u64) -> Result<(), String>;
    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String>;
    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String>;
    /// Wheel notches at the pointer: `dy > 0` scrolls the content down (the
    /// wheel towards the person), `dx > 0` scrolls right.
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String>;
    /// Press or release one key. A character that needs Shift on this
    /// layout gets it from the backend.
    fn key(&self, key: Key, down: bool) -> Result<(), String>;
    /// Type one character (press + release), whatever the layout.
    fn type_char(&self, ch: char) -> Result<(), String>;
    /// Take the person's keyboard and pointer: nothing they do reaches any
    /// window, our own input still lands; `on_esc` fires on their Esc.
    /// `Err` = the OS refused (another grab, a missing permission).
    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String>;
}

/// Why this machine cannot drive anything, if so (no display, Wayland):
/// read without connecting (the doctor line).
pub(crate) fn unsupported() -> Option<String> {
    #[cfg(target_os = "linux")]
    return linux::unsupported();
    #[cfg(target_os = "macos")]
    return macos::unsupported();
    #[cfg(target_os = "windows")]
    return windows::unsupported();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Some("Computer use is not available on this OS.".to_string())
}

/// Every OS permission computer use needs here and whether it is granted,
/// read WITHOUT prompting.
pub(crate) fn permissions() -> Vec<(crate::Permission, bool)> {
    #[cfg(target_os = "macos")]
    return macos::permissions();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// Ask the OS for every permission NOW (macOS: Screen Recording +
/// Accessibility dialogs); returns the state afterwards.
pub(crate) fn request_permissions() -> Vec<(crate::Permission, bool)> {
    #[cfg(target_os = "macos")]
    return macos::request_permissions();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// NEW-58: the app id of process `pid` where the OS names it cheaply (the
/// macOS bundle id); `None` elsewhere.
pub(crate) fn app_id(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    return macos::app_id(pid);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        None
    }
}

/// NEW-59: the native pixel size of the display at this rect (screen
/// units); the rect's own size where the OS already counts pixels.
pub(crate) fn pixel_size(x: i32, y: i32, width: u32, height: u32) -> (u32, u32) {
    #[cfg(target_os = "macos")]
    return macos::pixel_size(x, y, width, height);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y);
        (width, height)
    }
}

/// Open the backend for one call.
/// [`crate::screen_layout`]: X11 only, where the toolkit's display is the
/// whole screen.
#[cfg(target_os = "linux")]
pub(crate) fn screen_layout() -> Option<crate::ScreenLayout> {
    linux::screen_layout()
}

pub(crate) fn open() -> Result<Box<dyn Backend>, String> {
    if let Some(reason) = unsupported() {
        return Err(reason);
    }
    #[cfg(target_os = "linux")]
    return linux::open();
    #[cfg(target_os = "macos")]
    return macos::open();
    #[cfg(target_os = "windows")]
    return windows::open();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Err("Computer use is not available on this OS.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: &str) -> DisplayInfo {
        DisplayInfo { id: id.into(), x: 0, y: 0, width: 100, height: 100, primary: false, name: String::new() }
    }

    /// #FEED-161: twins get `#2`, `#3` in the OS's order; every id fits 128.
    #[test]
    fn display_ids_are_unique_and_at_most_128_chars() {
        let long = "x".repeat(200);
        let mut displays = [display("edid:a"), display("edid:a"), display("output:DP-1"), display("edid:a"), display(&long), display(&long)];
        finish_ids(&mut displays);
        let ids: Vec<&str> = displays.iter().map(|display| display.id.as_str()).collect();
        assert_eq!(&ids[..4], ["edid:a", "edid:a#2", "output:DP-1", "edid:a#3"]);
        assert_eq!(ids[4], "x".repeat(128));
        assert_eq!(ids[5], format!("{}#2", "x".repeat(126)));
    }

    #[test]
    fn the_overlap_is_the_shared_area() {
        let display = DisplayInfo { x: 100, ..display("a") };
        assert_eq!(display.overlap(150, 50, 100, 100), 50 * 50);
        assert_eq!(display.overlap(0, 0, 100, 100), 0);
        assert_eq!(display.overlap(-1000, -1000, 5000, 5000), 100 * 100);
    }

    #[test]
    fn the_stable_hash_is_fnv1a() {
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
