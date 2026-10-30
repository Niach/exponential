//! #FEED-162: what screen memory (`fixtures/screen-memory.json`) reads of
//! the screen beyond [`crate::ScreenView`], from the SAME window-list read:
//! every on-screen window on every display it overlaps (top first, its rect
//! in that display's pixels), so a denylisted window that is not on top is
//! masked; whether this machine has a window list at all (Wayland: no, so
//! the Screen memory row hides); and one live screenshot of a display (the
//! fresh capture after a resume where the OS stream only delivers changes).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::backend::{self, DisplayInfo, WindowInfo};
use crate::work_screen;
use crate::{ScreenRect, ScreenView};

/// One window as it lies on one display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleWindow {
    /// 1-based ([`crate::displays`] order).
    pub display: u32,
    /// That display's size in physical pixels: the frame `rect` is cut
    /// from must have exactly this size.
    pub display_size: (u32, u32),
    pub app: String,
    /// The macOS bundle id where known.
    pub app_id: Option<String>,
    pub title: String,
    /// Its rect within the display, physical pixels, clipped to it.
    pub rect: ScreenRect,
}

/// [`ScreenView`] plus every window, from one window-list read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenLook {
    pub view: ScreenView,
    /// Top first; a window on two displays is listed once per display.
    pub windows: Vec<VisibleWindow>,
    /// #FEED-164: the [`display_epoch`] of the display set this reading
    /// numbered its displays by.
    pub epoch: u64,
}

/// Whether this machine lists windows (cheap, no connection): `false` on
/// Wayland and without a display, where screen memory can never capture.
pub fn window_list_supported() -> bool {
    backend::unsupported().is_none()
}

const DISPLAYS_TTL: Duration = Duration::from_secs(3);
type Displays = Vec<(DisplayInfo, (u32, u32))>;
/// The display set, when it was read and under which epoch.
static DISPLAYS: Mutex<Option<(Instant, u64, Displays)>> = Mutex::new(None);
/// pid → app id, cleared with the display cache (a reused pid lives 3 s).
static APP_IDS: Mutex<Option<HashMap<u32, Option<String>>>> = Mutex::new(None);
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// #FEED-164 (`screen-memory.json` DISPLAY SET): bumped whenever someone
/// sees the monitor set change; a reading and a frame of different epochs
/// may number their displays differently.
pub fn display_epoch() -> u64 {
    EPOCH.load(Ordering::Acquire)
}

/// #FEED-164: the monitor set changed (a capturer saw a hot-plug): the
/// next [`screen_look`] re-reads the displays instead of waiting out the
/// cache, under a new [`display_epoch`].
pub fn displays_changed() {
    EPOCH.fetch_add(1, Ordering::AcqRel);
}

/// The screen now; `None` without a display or a window list.
pub fn screen_look() -> Option<ScreenLook> {
    let backend = backend::open().ok()?;
    let (displays, epoch) = {
        let mut cache = DISPLAYS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let epoch = display_epoch();
        match cache.as_ref().filter(|(at, read_in, _)| *read_in == epoch && at.elapsed() < DISPLAYS_TTL) {
            Some((_, _, displays)) => (displays.clone(), epoch),
            None => {
                let displays: Displays = backend
                    .displays()
                    .ok()?
                    .into_iter()
                    .map(|display| {
                        let pixels = backend::pixel_size(display.x, display.y, display.width, display.height);
                        (display, pixels)
                    })
                    .collect();
                // Our own re-read saw the change first: the capturer's
                // numbering is stale too.
                let epoch = match cache.as_ref() {
                    Some((_, read_in, before)) if *read_in == epoch && *before != displays => {
                        displays_changed();
                        display_epoch()
                    }
                    _ => epoch,
                };
                *cache = Some((Instant::now(), epoch, displays.clone()));
                *APP_IDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                (displays, epoch)
            }
        }
    };
    let windows = backend.list_windows().ok()?;
    let app_id = |pid: u32| {
        let mut ids = APP_IDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        ids.get_or_insert_with(HashMap::new).entry(pid).or_insert_with(|| backend::app_id(pid)).clone()
    };
    Some(ScreenLook { epoch, ..look_of(&displays, &windows, work_screen::picked().as_deref(), app_id) })
}

/// [`screen_look`] off one display set and one window list (top first).
pub(crate) fn look_of(displays: &[(DisplayInfo, (u32, u32))], windows: &[WindowInfo], picked: Option<&str>, app_id: impl Fn(u32) -> Option<String>) -> ScreenLook {
    let view = work_screen::view_of(displays, windows, picked, &app_id);
    let mut visible = Vec::new();
    for window in windows {
        for (index, (display, pixels)) in displays.iter().enumerate() {
            if let Some(rect) = work_screen::rect_in_pixels(display, *pixels, window) {
                visible.push(VisibleWindow {
                    display: index as u32 + 1,
                    display_size: *pixels,
                    app: window.app.clone(),
                    app_id: window.pid.and_then(&app_id),
                    title: window.title.clone(),
                    rect,
                });
            }
        }
    }
    ScreenLook { view, windows: visible, epoch: 0 }
}

/// Display `number`'s pixels now (RGBA8, width, height), the OS's live
/// read (GDI on Windows, the X root on X11).
pub fn screenshot(number: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let backend = backend::open()?;
    let displays = backend.displays()?;
    let display = number.checked_sub(1).and_then(|index| displays.get(index as usize)).ok_or("No such display.")?;
    let image = backend.screenshot(display)?;
    Ok((image.width, image.height, image.rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: &str, x: i32, width: u32) -> (DisplayInfo, (u32, u32)) {
        (DisplayInfo { id: id.into(), x, y: 0, width, height: 1000, primary: x == 0, name: String::new() }, (width * 2, 2000))
    }

    fn window(app: &str, x: i32, width: u32, active: bool) -> WindowInfo {
        WindowInfo { id: 1, title: format!("{app} window"), app: app.into(), pid: Some(7), x, y: 100, width, height: 300, active, fullscreen: false }
    }

    /// Every window, top first, once per display it overlaps, in that
    /// display's pixels; app ids for all of them.
    #[test]
    fn every_window_is_listed_on_every_display_it_overlaps() {
        let displays = [display("a", 0, 1000), display("b", 1000, 1000)];
        let windows = [window("Zed", 100, 400, true), window("Passwords", 900, 300, false), window("Off", 5000, 10, false)];
        let look = look_of(&displays, &windows, None, |_| Some("com.apple.Passwords".into()));
        let rows: Vec<(u32, &str, ScreenRect)> = look.windows.iter().map(|window| (window.display, window.app.as_str(), window.rect)).collect();
        assert_eq!(
            rows,
            [
                (1, "Zed", ScreenRect { x: 200, y: 200, width: 800, height: 600 }),
                (1, "Passwords", ScreenRect { x: 1800, y: 200, width: 200, height: 600 }),
                (2, "Passwords", ScreenRect { x: 0, y: 200, width: 400, height: 600 }),
            ]
        );
        assert!(look.windows.iter().all(|window| window.display_size == (2000, 2000) && window.app_id.as_deref() == Some("com.apple.Passwords")));
        assert_eq!(look.view.focused.as_ref().map(|window| window.app.as_str()), Some("Zed"));
    }
}
