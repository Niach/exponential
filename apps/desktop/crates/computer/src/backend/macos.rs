//! macOS = Quartz events, through core-graphics.
//!
//! Input is CGEvents posted at the HID tap: exactly where the person's own
//! mouse and keyboard enter, so every app takes it. Keys are the ANSI
//! (US) virtual keycodes; a character with no key there is typed as a
//! unicode string on a blank keystroke. Held modifiers ride on every event
//! as its flags, the way Cocoa and Chrome read shortcuts. Windows come
//! from the window server's list (CGWindowListCopyWindowInfo) and are
//! activated through AppKit, the exact window raised over Accessibility.
//!
//! The lock is an active event tap at the HID level on its own thread:
//! every keyboard and pointer event passes it, ours (tagged with
//! [`MAGIC`] in the event source's user data) go through, the person's are
//! swallowed; their Esc fires `on_esc`. Needs Accessibility; the screenshot
//! needs Screen Recording. Coordinates are points of the global display
//! space (origin top-left of the main display, y down).

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::mach_port::{CFMachPort, CFMachPortInvalidate, CFMachPortRef};
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::access::ScreenCaptureAccess;
use core_graphics::display::{CGDirectDisplayID, CGDisplay};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGKeyCode, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID,
};
use foreign_types::ForeignType;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

use crate::backend::{Backend, Button, DisplayInfo, Image, LockGuard, WindowInfo};
use crate::keys::{Key, Named};
use crate::Permission;

/// Our tag in every event we post ("EXP1"): the lock lets these through.
const MAGIC: i64 = 0x4558_5031;

/// kVK_Escape: the person's way out of the lock.
const ESCAPE: CGKeyCode = 53;

/// Two presses of one button this close are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(300);

/// How close (points) an Accessibility window's frame must be to the
/// window server's to be the same window.
const FRAME_SLACK: f64 = 4.0;

/// AXValueType for the frame attributes.
const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;

type TapCallback = unsafe extern "C" fn(
    proxy: *mut c_void,
    kind: u32,
    event: core_graphics::sys::CGEventRef,
    user_info: *mut c_void,
) -> core_graphics::sys::CGEventRef;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    fn AXUIElementCreateApplication(pid: Pid) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: CFTypeRef, action: CFStringRef) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSetUserData(source: core_graphics::sys::CGEventSourceRef, data: i64);
    fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events: CGEventMask,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: core_graphics::sys::CGEventRef, field: u32) -> i64;
}

#[link(name = "ColorSync", kind = "framework")]
extern "C" {
    /// The display's UUID (a CFUUIDRef, create rule; null when unknown).
    fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> CFTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFUUIDCreateString(allocator: CFTypeRef, uuid: CFTypeRef) -> CFStringRef;
    fn CFRelease(value: CFTypeRef);
}

/// #FEED-161: the display's stable id, its UUID (the same across
/// reconnects and reorders); the CGDirectDisplayID when macOS names none.
fn display_uuid(id: CGDirectDisplayID) -> String {
    // SAFETY: plain queries; the UUID is released after its string is
    // taken (create rule both), the string wrapped under the create rule.
    unsafe {
        let uuid = CGDisplayCreateUUIDFromDisplayID(id);
        if uuid.is_null() {
            return format!("cg:{id}");
        }
        let text = CFUUIDCreateString(std::ptr::null(), uuid);
        CFRelease(uuid);
        if text.is_null() {
            return format!("cg:{id}");
        }
        CFString::wrap_under_create_rule(text).to_string()
    }
}

/// `pid_t`.
type Pid = i32;

pub(crate) fn unsupported() -> Option<String> {
    None
}

fn accessibility_granted() -> bool {
    // SAFETY: a plain query of this process's trust; no arguments.
    unsafe { AXIsProcessTrusted() != 0 }
}

pub(crate) fn permissions() -> Vec<(Permission, bool)> {
    vec![(Permission::ScreenRecording, ScreenCaptureAccess.preflight()), (Permission::Accessibility, accessibility_granted())]
}

pub(crate) fn request_permissions() -> Vec<(Permission, bool)> {
    // The OS shows each dialog once; afterwards these only read the state.
    ScreenCaptureAccess.request();
    // SAFETY: the option key is a constant CFString of the framework.
    let prompt = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(prompt.as_CFType(), CFBoolean::true_value().as_CFType())]);
    // SAFETY: `options` is a live CFDictionary for the length of the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
    permissions()
}

pub(crate) fn open() -> Result<Box<dyn Backend>, String> {
    Ok(Box::new(Mac::new()?))
}

pub(crate) struct Mac {
    /// The one source every event of this backend comes from, tagged.
    source: CGEventSource,
    /// The modifiers pressed through `key` and not released yet.
    held: Cell<CGEventFlags>,
    /// The left button is down: moves are drags.
    left_down: Cell<bool>,
    /// The last press, for the click count of the next.
    last_press: Cell<Option<(Button, Instant)>>,
    click_state: Cell<i64>,
}

// SAFETY: a CGEventSource is a CoreFoundation object (retain/release are
// thread-safe) that is never mutated after `new`; the backend moves to one
// thread at a time and is not `Sync`, so the cells never race.
unsafe impl Send for Mac {}

impl Mac {
    fn new() -> Result<Mac, String> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| "Could not create an event source.".to_string())?;
        // SAFETY: `source` is a live event source.
        unsafe { CGEventSourceSetUserData(source.as_ptr(), MAGIC) };
        Ok(Mac {
            source,
            held: Cell::new(CGEventFlags::CGEventFlagNull),
            left_down: Cell::new(false),
            last_press: Cell::new(None),
            click_state: Cell::new(1),
        })
    }

    /// Tag `event`, give it the held modifiers (plus `extra`) and post it
    /// at the HID tap.
    fn post(&self, event: CGEvent, extra: CGEventFlags) {
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, MAGIC);
        event.set_flags(self.held.get() | extra);
        event.post(CGEventTapLocation::HID);
    }

    fn pointer(&self) -> Result<CGPoint, String> {
        let event = CGEvent::new(self.source.clone()).map_err(|_| "Could not read the pointer.".to_string())?;
        Ok(event.location())
    }

    fn mouse(&self, kind: CGEventType, at: CGPoint, button: CGMouseButton) -> Result<CGEvent, String> {
        CGEvent::new_mouse_event(self.source.clone(), kind, at, button).map_err(|_| "Could not create a mouse event.".to_string())
    }

    fn keystroke(&self, keycode: CGKeyCode, down: bool) -> Result<CGEvent, String> {
        CGEvent::new_keyboard_event(self.source.clone(), keycode, down).map_err(|_| "Could not create a key event.".to_string())
    }

    /// A blank keystroke carrying `text`: whatever the layout, apps read
    /// the string.
    fn unicode(&self, text: &str, down: bool) -> Result<(), String> {
        let event = self.keystroke(0, down)?;
        event.set_string(text);
        self.post(event, CGEventFlags::CGEventFlagNull);
        Ok(())
    }
}

impl Backend for Mac {
    fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
        Ok(active_displays()?.into_iter().map(|(_, info)| info).collect())
    }

    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
        if !ScreenCaptureAccess.preflight() {
            return Err("Screen Recording is not granted (System Settings > Privacy & Security > Screen Recording).".to_string());
        }
        let id = active_displays()?
            .into_iter()
            .find(|(_, info)| same_rect(info, display))
            .map(|(id, _)| id)
            .ok_or("That display is gone.")?;
        let image = CGDisplay::new(id).image().ok_or("macOS returned no screen capture.")?;
        if image.bits_per_pixel() != 32 || image.bits_per_component() != 8 {
            return Err(format!("Unsupported screen format: {} bits per pixel.", image.bits_per_pixel()));
        }
        let (width, height) = (image.width(), image.height());
        let data = image.data();
        let rgba = bgra_to_rgba(data.bytes(), width, height, image.bytes_per_row()).ok_or("The screen capture came back short.")?;
        let width = u32::try_from(width).map_err(|_| "The screen capture is too wide.".to_string())?;
        let height = u32::try_from(height).map_err(|_| "The screen capture is too tall.".to_string())?;
        Ok(Image { width, height, rgba })
    }

    fn cursor_position(&self) -> Result<(i32, i32), String> {
        let at = self.pointer()?;
        Ok((at.x.round() as i32, at.y.round() as i32))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        let list = copy_window_info(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)
            .ok_or("macOS returned no window list.")?;
        let front = frontmost_pid();
        // NEW-59: a window whose bounds equal a display's is a fullscreen app.
        let displays: Vec<DisplayInfo> = active_displays().map(|all| all.into_iter().map(|(_, info)| info).collect()).unwrap_or_default();
        let mut active_seen = false;
        let mut windows = Vec::new();
        for item in list.iter() {
            // SAFETY: every entry of the window list is a CFDictionary with
            // CFString keys, alive as long as `list`.
            let entry: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
            if number(&entry, window_key(KeyName::Layer)) != Some(0) {
                continue;
            }
            let Some(app) = text(&entry, window_key(KeyName::OwnerName)) else { continue };
            let Some(id) = number(&entry, window_key(KeyName::Number)).and_then(|id| u64::try_from(id).ok()) else { continue };
            let Some(bounds) = entry
                .find(window_key(KeyName::Bounds))
                .and_then(|bounds| bounds.downcast::<CFDictionary>())
                .and_then(|bounds| CGRect::from_dict_representation(&bounds))
            else {
                continue;
            };
            let pid = number(&entry, window_key(KeyName::OwnerPid)).and_then(|pid| u32::try_from(pid).ok());
            let title = text(&entry, window_key(KeyName::Name)).filter(|title| !title.is_empty()).unwrap_or_else(|| app.clone());
            let active = !active_seen && front.is_some() && pid == front;
            active_seen |= active;
            let mut window = WindowInfo {
                id,
                title,
                app,
                pid,
                x: bounds.origin.x.round() as i32,
                y: bounds.origin.y.round() as i32,
                width: bounds.size.width.round().max(0.0) as u32,
                height: bounds.size.height.round().max(0.0) as u32,
                active,
                fullscreen: false,
            };
            window.fullscreen = fullscreen(&window, &displays);
            windows.push(window);
        }
        Ok(windows)
    }

    fn activate_window(&self, id: u64) -> Result<(), String> {
        let window = self.list_windows()?.into_iter().find(|window| window.id == id).ok_or_else(|| format!("No window {id}."))?;
        let pid = window.pid.and_then(|pid| Pid::try_from(pid).ok()).ok_or_else(|| format!("Window {id} has no process."))?;
        // The window first, then its app: activation brings the app's main
        // window forward, which the raise just made this one.
        raise(pid, &window);
        let activated = objc2::rc::autoreleasepool(|_| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid).is_some_and(|app| {
                #[allow(deprecated, reason = "macOS 13 and older need it to activate from the background; 14+ ignores it")]
                let options = NSApplicationActivationOptions::ActivateIgnoringOtherApps;
                app.activateWithOptions(options)
            })
        });
        if !activated {
            log::debug!("[computer] macOS did not activate the app of window {id}");
        }
        Ok(())
    }

    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
        let at = CGPoint::new(f64::from(x), f64::from(y));
        let kind = if self.left_down.get() { CGEventType::LeftMouseDragged } else { CGEventType::MouseMoved };
        self.post(self.mouse(kind, at, CGMouseButton::Left)?, CGEventFlags::CGEventFlagNull);
        Ok(())
    }

    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
        let (kind, cg_button) = match (button, down) {
            (Button::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (Button::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (Button::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (Button::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (Button::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (Button::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        };
        if down {
            let now = Instant::now();
            let repeat = matches!(self.last_press.get(), Some((last, at)) if last == button && now.duration_since(at) < DOUBLE_CLICK);
            self.click_state.set(if repeat { self.click_state.get() + 1 } else { 1 });
            self.last_press.set(Some((button, now)));
        }
        if button == Button::Left {
            self.left_down.set(down);
        }
        let event = self.mouse(kind, self.pointer()?, cg_button)?;
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, self.click_state.get());
        self.post(event, CGEventFlags::CGEventFlagNull);
        Ok(())
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        // macOS wheels count positive towards the top: content down = negative.
        let event = CGEvent::new_scroll_event(
            self.source.clone(),
            ScrollEventUnit::LINE,
            2,
            dy.saturating_mul(-3),
            dx.saturating_mul(-3),
            0,
        )
        .map_err(|_| "Could not create a scroll event.".to_string())?;
        self.post(event, CGEventFlags::CGEventFlagNull);
        Ok(())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        if let Key::Named(named) = key {
            if let Some(flag) = modifier_flag(named) {
                let held = self.held.get();
                self.held.set(if down { held | flag } else { held - flag });
            }
        }
        match stroke(key) {
            Stroke::Code(keycode, shift) => {
                let extra = if shift { CGEventFlags::CGEventFlagShift } else { CGEventFlags::CGEventFlagNull };
                self.post(self.keystroke(keycode, down)?, extra);
                Ok(())
            }
            Stroke::Unicode(ch) => self.unicode(&ch.to_string(), down),
            Stroke::Missing => Err(format!("{key:?} has no key on a Mac.")),
        }
    }

    fn type_char(&self, ch: char) -> Result<(), String> {
        let text = ch.to_string();
        self.unicode(&text, true)?;
        std::thread::sleep(Duration::from_millis(6));
        self.unicode(&text, false)
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(on_esc)?))
    }
}

/// Every active display with its id, in points of the global space.
pub(super) fn pixel_size(x: i32, y: i32, width: u32, height: u32) -> (u32, u32) {
    let Ok(displays) = active_displays() else { return (width, height) };
    displays
        .into_iter()
        .find(|(_, info)| info.x == x && info.y == y && info.width == width && info.height == height)
        .and_then(|(id, _)| {
            let mode = CGDisplay::new(id).display_mode()?;
            Some((mode.pixel_width() as u32, mode.pixel_height() as u32))
        })
        .unwrap_or((width, height))
}

fn active_displays() -> Result<Vec<(CGDirectDisplayID, DisplayInfo)>, String> {
    let ids = CGDisplay::active_displays().map_err(|err| format!("displays: CGError {err}"))?;
    let mut displays: Vec<(CGDirectDisplayID, DisplayInfo)> = ids
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let display = CGDisplay::new(id);
            let bounds = display.bounds();
            let info = DisplayInfo {
                id: display_uuid(id),
                x: bounds.origin.x.round() as i32,
                y: bounds.origin.y.round() as i32,
                width: bounds.size.width.round().max(0.0) as u32,
                height: bounds.size.height.round().max(0.0) as u32,
                primary: display.is_main(),
                name: format!("Display {}", index + 1),
            };
            (id, info)
        })
        .collect();
    if displays.is_empty() {
        return Err("macOS reports no active display.".to_string());
    }
    if !displays.iter().any(|(_, info)| info.primary) {
        displays[0].1.primary = true;
    }
    let mut infos: Vec<DisplayInfo> = displays.iter().map(|(_, info)| info.clone()).collect();
    crate::backend::finish_ids(&mut infos);
    for ((_, info), finished) in displays.iter_mut().zip(infos) {
        info.id = finished.id;
    }
    Ok(displays)
}

fn same_rect(a: &DisplayInfo, b: &DisplayInfo) -> bool {
    (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
}

/// The system's own windows, never a fullscreen app.
const SYSTEM_OWNERS: &[&str] = &["Dock", "Finder", "Window Server"];

/// The tallest band a notched display keeps above a fullscreen window (the
/// camera housing, points).
const NOTCH_BAND: i32 = 64;

/// NEW-59: a layer-0 window (the caller's filter) of an app whose bounds
/// equal one display's full bounds. On a notched display macOS lays a
/// fullscreen window out BELOW the camera housing, the same frame as a
/// window filling the screen under the menu bar with the Dock hidden:
/// there the window's own AXFullScreen decides (Accessibility, which the
/// lock needs anyway; without it, not fullscreen).
fn fullscreen(window: &WindowInfo, displays: &[DisplayInfo]) -> bool {
    if SYSTEM_OWNERS.contains(&window.app.as_str()) {
        return false;
    }
    let rect = (window.x, window.y, window.width, window.height);
    if displays.iter().any(|display| (display.x, display.y, display.width, display.height) == rect) {
        return true;
    }
    let below_notch = displays.iter().any(|display| {
        let band = window.y - display.y;
        window.x == display.x
            && window.width == display.width
            && band > 0
            && band <= NOTCH_BAND
            && window.y + window.height as i32 == display.y + display.height as i32
    });
    below_notch
        && window
            .pid
            .and_then(|pid| Pid::try_from(pid).ok())
            .and_then(|pid| ax_window(pid, window))
            .and_then(|element| ax_copy(&element, "AXFullScreen"))
            .and_then(|value| value.downcast::<CFBoolean>())
            .is_some_and(bool::from)
}

/// Rows of 32-bit little-endian BGRA (`stride` bytes apart) as packed
/// RGBA8, opaque; `None` when `data` is short.
fn bgra_to_rgba(data: &[u8], width: usize, height: usize, stride: usize) -> Option<Vec<u8>> {
    let row = width.checked_mul(4)?;
    let mut rgba = Vec::with_capacity(row.checked_mul(height)?);
    for y in 0..height {
        let start = y.checked_mul(stride)?;
        for pixel in data.get(start..start.checked_add(row)?)?.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    Some(rgba)
}

/// The window list's keys we read.
enum KeyName {
    Layer,
    OwnerName,
    OwnerPid,
    Number,
    Name,
    Bounds,
}

fn window_key(name: KeyName) -> CFString {
    // SAFETY: these are constant CFStrings exported by CoreGraphics.
    let key = unsafe {
        match name {
            KeyName::Layer => kCGWindowLayer,
            KeyName::OwnerName => kCGWindowOwnerName,
            KeyName::OwnerPid => kCGWindowOwnerPID,
            KeyName::Number => kCGWindowNumber,
            KeyName::Name => kCGWindowName,
            KeyName::Bounds => kCGWindowBounds,
        }
    };
    // SAFETY: a constant is valid for the process's life; get rule = no
    // ownership taken.
    unsafe { CFString::wrap_under_get_rule(key) }
}

fn number(entry: &CFDictionary<CFString, CFType>, key: CFString) -> Option<i64> {
    entry.find(&key)?.downcast::<CFNumber>()?.to_i64()
}

fn text(entry: &CFDictionary<CFString, CFType>, key: CFString) -> Option<String> {
    Some(entry.find(&key)?.downcast::<CFString>()?.to_string())
}

/// NEW-58: `pid`'s bundle id (`NSRunningApplication`), the last answer
/// kept: screen memory asks for the focused app 4 times a second.
pub(crate) fn app_id(pid: u32) -> Option<String> {
    static LAST: std::sync::Mutex<Option<(u32, Option<String>)>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached, id)) = last.as_ref() {
        if *cached == pid {
            return id.clone();
        }
    }
    let id = objc2::rc::autoreleasepool(|_| {
        let pid = Pid::try_from(pid).ok()?;
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        app.bundleIdentifier().map(|id| id.to_string())
    });
    *last = Some((pid, id.clone()));
    id
}

fn frontmost_pid() -> Option<u32> {
    objc2::rc::autoreleasepool(|_| {
        let pid = NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier();
        u32::try_from(pid).ok()
    })
}

/// The Accessibility window of `pid` whose frame matches the window
/// server's `window`; each miss logged.
fn ax_window(pid: Pid, window: &WindowInfo) -> Option<CFType> {
    // SAFETY: returns a new AXUIElement (create rule) for any pid.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        log::debug!("[computer] no Accessibility element for pid {pid}");
        return None;
    }
    // SAFETY: non-null and ours (create rule).
    let app = unsafe { CFType::wrap_under_create_rule(app) };
    let Some(list) = ax_copy(&app, "AXWindows").and_then(|list| list.downcast::<CFArray>()) else {
        log::debug!("[computer] pid {pid} lists no Accessibility windows");
        return None;
    };
    let found = list.iter().find_map(|item| {
        // SAFETY: AXWindows holds AXUIElements, alive as long as `list`.
        let element = unsafe { CFType::wrap_under_get_rule(*item) };
        let (origin, size) = (ax_point(&element)?, ax_size(&element)?);
        let close = |a: f64, b: i64| (a - b as f64).abs() <= FRAME_SLACK;
        let same = close(origin.x, i64::from(window.x))
            && close(origin.y, i64::from(window.y))
            && close(size.width, i64::from(window.width))
            && close(size.height, i64::from(window.height));
        same.then_some(element)
    });
    if found.is_none() {
        log::debug!("[computer] no Accessibility window matches window {}", window.id);
    }
    found
}

/// Raise `window` of `pid` over Accessibility: the AX window whose frame
/// matches the window server's. Best effort, each miss logged.
fn raise(pid: Pid, window: &WindowInfo) {
    let Some(element) = ax_window(pid, window) else { return };
    let main = CFString::from_static_string("AXMain");
    let raise = CFString::from_static_string("AXRaise");
    // SAFETY: live element, attribute and value for the length of the calls.
    let (main_err, raise_err) = unsafe {
        (
            AXUIElementSetAttributeValue(element.as_CFTypeRef(), main.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef()),
            AXUIElementPerformAction(element.as_CFTypeRef(), raise.as_concrete_TypeRef()),
        )
    };
    if main_err != 0 || raise_err != 0 {
        log::debug!("[computer] raising window {} over Accessibility: AXError {main_err}/{raise_err}", window.id);
    }
}

fn ax_copy(element: &CFType, attribute: &'static str) -> Option<CFType> {
    let name = CFString::from_static_string(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: live element and name; `value` receives a +1 reference.
    let err = unsafe { AXUIElementCopyAttributeValue(element.as_CFTypeRef(), name.as_concrete_TypeRef(), &mut value) };
    if err != 0 || value.is_null() {
        return None;
    }
    // SAFETY: a non-null copy we own (create rule).
    Some(unsafe { CFType::wrap_under_create_rule(value) })
}

fn ax_point(element: &CFType) -> Option<CGPoint> {
    let value = ax_copy(element, "AXPosition")?;
    let mut point = CGPoint::new(0.0, 0.0);
    // SAFETY: an AXValue of CGPoint type writes exactly one CGPoint.
    let ok = unsafe { AXValueGetValue(value.as_CFTypeRef(), AX_VALUE_CG_POINT, (&mut point as *mut CGPoint).cast()) };
    (ok != 0).then_some(point)
}

fn ax_size(element: &CFType) -> Option<CGSize> {
    let value = ax_copy(element, "AXSize")?;
    let mut size = CGSize::new(0.0, 0.0);
    // SAFETY: an AXValue of CGSize type writes exactly one CGSize.
    let ok = unsafe { AXValueGetValue(value.as_CFTypeRef(), AX_VALUE_CG_SIZE, (&mut size as *mut CGSize).cast()) };
    (ok != 0).then_some(size)
}

fn modifier_flag(named: Named) -> Option<CGEventFlags> {
    match named {
        Named::Control => Some(CGEventFlags::CGEventFlagControl),
        Named::Shift => Some(CGEventFlags::CGEventFlagShift),
        Named::Alt => Some(CGEventFlags::CGEventFlagAlternate),
        Named::Super => Some(CGEventFlags::CGEventFlagCommand),
        _ => None,
    }
}

/// How one key is pressed on a Mac.
#[derive(Debug, PartialEq, Eq)]
enum Stroke {
    /// A virtual keycode, with Shift when the character needs it.
    Code(CGKeyCode, bool),
    /// No key on the ANSI layout: the character as a unicode string.
    Unicode(char),
    /// A named key Macs do not have (F21 and up).
    Missing,
}

fn stroke(key: Key) -> Stroke {
    match key {
        Key::Named(named) => named_keycode(named).map_or(Stroke::Missing, |keycode| Stroke::Code(keycode, false)),
        Key::Char(ch) => match char_keycode(ch) {
            Some((keycode, shift)) => Stroke::Code(keycode, shift),
            None => Stroke::Unicode(ch),
        },
    }
}

/// kVK_* of a named key.
fn named_keycode(named: Named) -> Option<CGKeyCode> {
    const F_KEYS: [CGKeyCode; 20] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90];
    Some(match named {
        Named::Return => 36,
        Named::Tab => 48,
        Named::Escape => ESCAPE,
        Named::Backspace => 51,
        Named::Delete => 117,
        Named::Home => 115,
        Named::End => 119,
        Named::PageUp => 116,
        Named::PageDown => 121,
        Named::Left => 123,
        Named::Right => 124,
        Named::Up => 126,
        Named::Down => 125,
        Named::Insert => 114,
        Named::Space => 49,
        Named::F(n) => return F_KEYS.get(usize::from(n).checked_sub(1)?).copied(),
        Named::Control => 59,
        Named::Shift => 56,
        Named::Alt => 58,
        Named::Super => 55,
    })
}

/// kVK_ANSI_* of a character on the US layout, and whether it is the
/// key's Shift level.
fn char_keycode(ch: char) -> Option<(CGKeyCode, bool)> {
    if ch.is_ascii_uppercase() {
        return char_keycode(ch.to_ascii_lowercase()).map(|(keycode, _)| (keycode, true));
    }
    let plain = match ch {
        'a' => 0,
        's' => 1,
        'd' => 2,
        'f' => 3,
        'h' => 4,
        'g' => 5,
        'z' => 6,
        'x' => 7,
        'c' => 8,
        'v' => 9,
        'b' => 11,
        'q' => 12,
        'w' => 13,
        'e' => 14,
        'r' => 15,
        'y' => 16,
        't' => 17,
        '1' => 18,
        '2' => 19,
        '3' => 20,
        '4' => 21,
        '6' => 22,
        '5' => 23,
        '=' => 24,
        '9' => 25,
        '7' => 26,
        '-' => 27,
        '8' => 28,
        '0' => 29,
        ']' => 30,
        'o' => 31,
        'u' => 32,
        '[' => 33,
        'i' => 34,
        'p' => 35,
        'l' => 37,
        'j' => 38,
        '\'' => 39,
        'k' => 40,
        ';' => 41,
        '\\' => 42,
        ',' => 43,
        '/' => 44,
        'n' => 45,
        'm' => 46,
        '.' => 47,
        ' ' => 49,
        '`' => 50,
        _ => {
            let base = match ch {
                '!' => '1',
                '@' => '2',
                '#' => '3',
                '$' => '4',
                '%' => '5',
                '^' => '6',
                '&' => '7',
                '*' => '8',
                '(' => '9',
                ')' => '0',
                '_' => '-',
                '+' => '=',
                '{' => '[',
                '}' => ']',
                '|' => '\\',
                ':' => ';',
                '"' => '\'',
                '<' => ',',
                '>' => '.',
                '?' => '/',
                '~' => '`',
                _ => return None,
            };
            return char_keycode(base).map(|(keycode, _)| (keycode, true));
        }
    };
    Some((plain, false))
}

/// The event types the lock filters: every keyboard and pointer event.
const LOCKED_EVENTS: [CGEventType; 14] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::FlagsChanged,
    CGEventType::MouseMoved,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::LeftMouseDragged,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::RightMouseDragged,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
    CGEventType::OtherMouseDragged,
    CGEventType::ScrollWheel,
];

/// How long the tap thread sleeps between looks at the stop flag: only a
/// stop that lands before the run loop runs waits this long.
const STOP_POLL: Duration = Duration::from_millis(250);

/// The tap, alive on its own thread until dropped (or until Esc, after
/// which the thread ends on its own and the drop only collects it).
struct Lock {
    stop: Arc<AtomicBool>,
    run_loop: CFRunLoop,
    thread: Option<JoinHandle<()>>,
}

impl Lock {
    fn take(on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Lock, String> {
        if !accessibility_granted() {
            return Err("Accessibility is not granted.".to_string());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<CFRunLoop, String>>();
        let thread = std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn({
                let stop = stop.clone();
                move || hold(on_esc, stop, ready_tx)
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(run_loop)) => Ok(Lock { stop, run_loop, thread: Some(thread) }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => {
                stop.store(true, Ordering::Release);
                Err("The event tap did not start.".to_string())
            }
        }
    }
}

impl LockGuard for Lock {}

impl Drop for Lock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.run_loop.stop();
        if let Some(thread) = self.thread.take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

/// What the tap callback sees, owned by the tap thread for the tap's life.
struct TapState {
    on_esc: Box<dyn Fn() + Send + Sync>,
    fired: Cell<bool>,
    stop: Arc<AtomicBool>,
    port: Cell<CFMachPortRef>,
}

/// The tap thread: install the tap, report, run until `stop`, tear down.
fn hold(on_esc: Box<dyn Fn() + Send + Sync>, stop: Arc<AtomicBool>, ready: std::sync::mpsc::Sender<Result<CFRunLoop, String>>) {
    let state = Box::new(TapState { on_esc, fired: Cell::new(false), stop: stop.clone(), port: Cell::new(std::ptr::null_mut()) });
    let mask = LOCKED_EVENTS.iter().fold(0 as CGEventMask, |mask, &kind| mask | 1 << kind as CGEventMask);
    // SAFETY: `state` outlives the tap (invalidated below before `state`
    // drops); the callback matches CGEventTapCallBack.
    let port_ref = unsafe {
        CGEventTapCreate(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            tap_callback,
            (&*state as *const TapState).cast_mut().cast(),
        )
    };
    if port_ref.is_null() {
        let _ = ready.send(Err("macOS refused the input lock (the event tap); check Accessibility.".to_string()));
        return;
    }
    // SAFETY: non-null and ours (create rule).
    let port = unsafe { CFMachPort::wrap_under_create_rule(port_ref) };
    state.port.set(port_ref);
    let Ok(source) = port.create_runloop_source(0) else {
        // SAFETY: `port_ref` is the live tap.
        unsafe { CFMachPortInvalidate(port_ref) };
        let _ = ready.send(Err("Could not attach the input lock to a run loop.".to_string()));
        return;
    };
    let run_loop = CFRunLoop::get_current();
    // SAFETY: the common-modes constant of CoreFoundation.
    let common = unsafe { kCFRunLoopCommonModes };
    run_loop.add_source(&source, common);
    // SAFETY: `port_ref` is the live tap.
    unsafe { CGEventTapEnable(port_ref, true) };
    if ready.send(Ok(run_loop.clone())).is_err() {
        // `take` gave up waiting: nobody holds a guard for this tap.
        stop.store(true, Ordering::Release);
    }
    log::debug!("[computer] event tap installed");
    while !stop.load(Ordering::Acquire) {
        // SAFETY: the default-mode constant of CoreFoundation.
        CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, STOP_POLL, false);
    }
    // SAFETY: `port_ref` is the live tap; after this no callback runs.
    unsafe {
        CGEventTapEnable(port_ref, false);
        CFMachPortInvalidate(port_ref);
    }
    run_loop.remove_source(&source, common);
    log::debug!("[computer] event tap removed");
}

/// The tap: ours through, the person's Esc = `on_esc`, the rest swallowed.
unsafe extern "C" fn tap_callback(
    _proxy: *mut c_void,
    kind: u32,
    event: core_graphics::sys::CGEventRef,
    user_info: *mut c_void,
) -> core_graphics::sys::CGEventRef {
    // SAFETY: `user_info` is the tap thread's `TapState`, alive until the
    // tap is invalidated; callbacks run on that thread only.
    let state = unsafe { &*user_info.cast::<TapState>() };
    if kind == CGEventType::TapDisabledByTimeout as u32 || kind == CGEventType::TapDisabledByUserInput as u32 {
        // SAFETY: the port is set before the run loop first runs.
        unsafe { CGEventTapEnable(state.port.get(), true) };
        return event;
    }
    // SAFETY: `event` is the live event this callback was handed.
    let tag = unsafe { CGEventGetIntegerValueField(event, EventField::EVENT_SOURCE_USER_DATA) };
    if tag == MAGIC {
        return event;
    }
    if kind == CGEventType::KeyDown as u32 {
        // SAFETY: as above.
        let keycode = unsafe { CGEventGetIntegerValueField(event, EventField::KEYBOARD_EVENT_KEYCODE) };
        if keycode == i64::from(ESCAPE) && !state.fired.replace(true) {
            log::info!("[computer] Esc from the keyboard");
            // A panic must not unwind into the window server's callback.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (state.on_esc)())).is_err() {
                log::warn!("[computer] the Esc handler panicked");
            }
            state.stop.store(true, Ordering::Release);
            CFRunLoop::get_current().stop();
        }
    }
    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_mac_virtual_keycodes() {
        assert_eq!(stroke(Key::Named(Named::Return)), Stroke::Code(36, false));
        assert_eq!(stroke(Key::Named(Named::Escape)), Stroke::Code(53, false));
        assert_eq!(stroke(Key::Named(Named::Super)), Stroke::Code(55, false));
        assert_eq!(stroke(Key::Named(Named::F(1))), Stroke::Code(122, false));
        assert_eq!(stroke(Key::Named(Named::F(12))), Stroke::Code(111, false));
        assert_eq!(stroke(Key::Named(Named::F(20))), Stroke::Code(90, false));
        assert_eq!(stroke(Key::Named(Named::F(21))), Stroke::Missing);
        assert_eq!(stroke(Key::Char('a')), Stroke::Code(0, false));
        assert_eq!(stroke(Key::Char('A')), Stroke::Code(0, true));
        assert_eq!(stroke(Key::Char('0')), Stroke::Code(29, false));
        assert_eq!(stroke(Key::Char('?')), Stroke::Code(44, true));
        assert_eq!(stroke(Key::Char('€')), Stroke::Unicode('€'));
    }

    #[test]
    fn bgra_rows_become_opaque_rgba_and_skip_the_row_padding() {
        // 2x2, stride 12 (4 bytes of padding per row).
        let data = [1, 2, 3, 0, 4, 5, 6, 9, 0xee, 0xee, 0xee, 0xee, 7, 8, 9, 0, 10, 11, 12, 0, 0xee, 0xee, 0xee, 0xee];
        assert_eq!(
            bgra_to_rgba(&data, 2, 2, 12),
            Some(vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255])
        );
        // The last row needs no padding: 20 bytes hold both rows, 19 do not.
        assert!(bgra_to_rgba(&data[..20], 2, 2, 12).is_some());
        assert_eq!(bgra_to_rgba(&data[..19], 2, 2, 12), None);
    }
}
