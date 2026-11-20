//! macOS = Quartz events, through core-graphics.
//!
//! Input is CGEvents posted at the HID tap: exactly where the person's own
//! mouse and keyboard enter, so every app takes it. A character goes to its
//! key on the active keyboard layout (#EXP-1922), a ⌘ shortcut to its key
//! on the layout's Command level (#EXP-1980), the ANSI (US) keycodes only
//! when the layout has no key data; a character with no key (or only a
//! dead one) is typed as a unicode string on a blank keystroke, and a
//! shortcut no key holds is an error. Held modifiers ride on every event
//! as its flags, the way Cocoa and Chrome read shortcuts. Windows come
//! from the window server's list (CGWindowListCopyWindowInfo) and are
//! activated through AppKit, the exact window raised over Accessibility.
//!
//! The lock is an active event tap at the HID level on its own thread:
//! every keyboard and pointer event passes it, ours (tagged with
//! [`MAGIC`] in the event source's user data) go through, the person's are
//! swallowed except the release of a key or button they held when the lock
//! engaged (#EXP-1921), and while such a button is still down the agent's
//! pointer waits for its release (#EXP-1979); their Esc fires `on_esc`. Needs Accessibility; the screenshot
//! needs Screen Recording. Coordinates are points of the global display
//! space (origin top-left of the main display, y down).
//!
//! Nothing here may depend on the host's main run loop (#EXP-1921): the
//! daemon has none, and AppKit state that only moves while it turns
//! (NSWorkspace's frontmost app) goes stale there.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSRunningApplication,
};
use objc2_foundation::{NSArray, NSData, NSString};

use crate::backend::{settle_paste, Backend, Button, Clipboard, DisplayInfo, HeldAtEngage, Image, LockGuard, Pasted, SavedClipboard, WindowInfo, CLIPBOARD_SAVE_MAX, PERSON_BUTTONS};
use crate::keys::{Key, Named};
use crate::Permission;
use crate::backend::{LockReady, LockStartError, LockThread, Verdict};

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
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementGetPid(element: CFTypeRef, pid: *mut Pid) -> i32;
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
    fn CGEventSourceKeyState(state: CGEventSourceStateID, key: CGKeyCode) -> bool;
    fn CGEventSourceButtonState(state: CGEventSourceStateID, button: u32) -> bool;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const u8,
        keycode: u16,
        action: u16,
        modifiers: u32,
        keyboard: u32,
        options: u32,
        dead_keys: *mut u32,
        max: usize,
        len: *mut usize,
        text: *mut u16,
    ) -> i32;
}

/// libdispatch's main queue (`dispatch_get_main_queue()`).
#[repr(C)]
struct DispatchQueue {
    _private: [u8; 0],
}

extern "C" {
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_async_f(queue: *const DispatchQueue, context: *mut c_void, work: extern "C" fn(*mut c_void));
    fn pthread_main_np() -> i32;
    fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
}

/// dlsym's "every loaded image" handle (`<dlfcn.h>`).
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

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
    /// The active keyboard layout, read at the first character key.
    layout: std::cell::OnceCell<Option<Layout>>,
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
            layout: std::cell::OnceCell::new(),
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
        let width = u32::try_from(image.width()).map_err(|_| "The screen capture is too wide.".to_string())?;
        let height = u32::try_from(image.height()).map_err(|_| "The screen capture is too tall.".to_string())?;
        let data = image.data();
        Image::from_bgra(data.bytes(), width, height, image.bytes_per_row()).ok_or_else(|| "The screen capture came back short.".to_string())
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
        // #EXP-1979: the window itself, not just its app: a raise that
        // missed leaves another of the app's windows in front.
        let front = u32::try_from(pid).ok();
        settle_activation(id, activated, crate::backend::ACTIVATE_WAIT, || {
            let named = ax_front_windows(pid);
            window_in_front(id, frontmost_pid() == front, named.as_deref(), || self.list_windows().ok().and_then(|windows| front_window(&windows)))
        })
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
        if let (Key::Char(_), None) = (key, self.layout.get()) {
            let _ = self.layout.set(current_layout()?);
        }
        let layout = match key {
            Key::Char(_) => self.layout.get().and_then(Option::as_ref),
            Key::Named(_) => None,
        };
        let held = self.held.get();
        let under = if held.contains(CGEventFlags::CGEventFlagCommand) {
            Under::Command
        } else if held.intersects(CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagAlternate) {
            Under::Shortcut
        } else {
            Under::Nothing
        };
        match stroke(key, layout, under) {
            Stroke::Code(keycode, shift) => {
                let extra = if shift { CGEventFlags::CGEventFlagShift } else { CGEventFlags::CGEventFlagNull };
                self.post(self.keystroke(keycode, down)?, extra);
                Ok(())
            }
            Stroke::Unicode(ch) => self.unicode(&ch.to_string(), down),
            Stroke::Missing => Err(format!("{key:?} has no key on a Mac.")),
            Stroke::Unreachable(ch) => Err(format!("{ch:?} has no key of its own on this keyboard layout, so no shortcut can hold it.")),
        }
    }

    fn char_stroke(&self, ch: char, down: bool) -> Result<(), String> {
        self.unicode(&ch.to_string(), down)
    }

    fn person_holds_button(&self) -> bool {
        person_holds_button()
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(on_esc)?))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
        Ok(Box::new(MacClipboard::new()))
    }
}

/// nspasteboard.org: clipboard managers hide (concealed) and skip
/// (transient) an entry carrying these types.
const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

/// The general pasteboard. Never logs what it holds. `Retained` keeps it
/// off other threads (not `Send`); NSPasteboard itself works off the main
/// thread.
struct MacClipboard {
    board: Retained<NSPasteboard>,
    /// The change count right after our own `set_text`.
    ours: Cell<Option<isize>>,
}

impl MacClipboard {
    fn new() -> MacClipboard {
        MacClipboard { board: NSPasteboard::generalPasteboard(), ours: Cell::new(None) }
    }
}

/// `public.utf8-plain-text`.
fn string_type() -> &'static NSString {
    // SAFETY: a constant NSString AppKit exports, alive for the process.
    unsafe { NSPasteboardTypeString }
}

/// Write `items` as the pasteboard's new contents (after `clearContents`).
fn write_items(board: &NSPasteboard, items: Vec<Retained<NSPasteboardItem>>) -> Result<(), String> {
    let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items.into_iter().map(ProtocolObject::from_retained).collect();
    let array = NSArray::from_retained_slice(&objects);
    if board.writeObjects(&array) {
        Ok(())
    } else {
        Err("macOS refused to write the clipboard.".to_string())
    }
}

impl Clipboard for MacClipboard {
    fn change_count(&self) -> Result<u64, String> {
        Ok(self.board.changeCount() as u64)
    }

    fn save(&self) -> Result<SavedClipboard, String> {
        objc2::rc::autoreleasepool(|_| {
            let mut formats = Vec::new();
            // A type without data or past the cap is left out: partial (#EXP-1900).
            let mut partial = false;
            let Some(items) = self.board.pasteboardItems() else { return Ok(SavedClipboard::default()) };
            let mut total = 0usize;
            for (index, item) in items.iter().enumerate() {
                for kind in item.types().iter() {
                    let Some(data) = item.dataForType(&kind) else {
                        partial = true;
                        continue;
                    };
                    let len = data.len();
                    if total.saturating_add(len) > CLIPBOARD_SAVE_MAX {
                        partial = true;
                        continue;
                    }
                    total += len;
                    formats.push((format!("{index}\t{kind}"), data.to_vec()));
                }
            }
            Ok(SavedClipboard { formats, partial })
        })
    }

    fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String> {
        objc2::rc::autoreleasepool(|_| {
            let item = NSPasteboardItem::new();
            if !item.setString_forType(&NSString::from_str(text), string_type()) {
                return Err("macOS refused the text for the clipboard.".to_string());
            }
            if sensitive {
                let marker = NSData::with_bytes(&[]);
                for kind in [CONCEALED_TYPE, TRANSIENT_TYPE] {
                    item.setData_forType(&marker, &NSString::from_str(kind));
                }
            }
            self.board.clearContents();
            let written = write_items(&self.board, vec![item]);
            self.ours.set(Some(self.board.changeCount()));
            written
        })
    }

    fn wait_pasted(&self, timeout: Duration) -> Pasted {
        // macOS cannot tell when the focused app read the pasteboard.
        settle_paste(timeout, self.ours.get().map(|ours| ours as u64), || self.board.changeCount() as u64)
    }

    fn restore(&self, saved: SavedClipboard) -> Result<(), String> {
        objc2::rc::autoreleasepool(|_| {
            self.board.clearContents();
            if saved.formats.is_empty() {
                return Ok(());
            }
            let mut items: BTreeMap<usize, Retained<NSPasteboardItem>> = BTreeMap::new();
            for (name, bytes) in &saved.formats {
                let Some((index, kind)) = name.split_once('\t') else { continue };
                let Ok(index) = index.parse::<usize>() else { continue };
                let item = items.entry(index).or_insert_with(NSPasteboardItem::new);
                item.setData_forType(&NSData::with_bytes(bytes), &NSString::from_str(kind));
            }
            if items.is_empty() {
                return Ok(());
            }
            write_items(&self.board, items.into_values().collect())
        })
    }

    fn text(&self) -> Result<Option<String>, String> {
        Ok(objc2::rc::autoreleasepool(|_| self.board.stringForType(string_type()).map(|text| text.to_string())))
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
    let infos: Vec<DisplayInfo> = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| {
            let display = CGDisplay::new(id);
            let bounds = display.bounds();
            DisplayInfo {
                id: display_uuid(id),
                x: bounds.origin.x.round() as i32,
                y: bounds.origin.y.round() as i32,
                width: bounds.size.width.round().max(0.0) as u32,
                height: bounds.size.height.round().max(0.0) as u32,
                primary: display.is_main(),
                name: format!("Display {}", index + 1),
                connector: None,
            }
        })
        .collect();
    // #FEED-164: twins are told apart by their unit number (the framebuffer
    // they hang off), never by the OS's order.
    let connectors: Vec<Option<String>> = ids.iter().map(|&id| Some(CGDisplay::new(id).unit_number().to_string())).collect();
    finish_reported(ids, infos, &connectors).ok_or_else(|| "macOS reports no active display.".to_string())
}

/// The displays macOS reported (`ids[i]` with `infos[i]` and its unit
/// number `connectors[i]`) finished by the one tail
/// ([`crate::backend::finish_displays`]), each id kept with its display:
/// #EXP-1919, each one's connector too, so a pick stays on its monitor.
fn finish_reported(ids: Vec<CGDirectDisplayID>, infos: Vec<DisplayInfo>, connectors: &[Option<String>]) -> Option<Vec<(CGDirectDisplayID, DisplayInfo)>> {
    Some(ids.into_iter().zip(crate::backend::finish_displays(infos, connectors)?).collect())
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
    below_notch(window, displays)
        && window
            .pid
            .and_then(|pid| Pid::try_from(pid).ok())
            .and_then(|pid| ax_window(pid, window))
            .and_then(|element| ax_copy(&element, "AXFullScreen"))
            .and_then(|value| value.downcast::<CFBoolean>())
            .is_some_and(bool::from)
}

/// The frame macOS gives a fullscreen window on a notched display: the
/// display's width, from at most [`NOTCH_BAND`] points below its top down
/// to its bottom.
fn below_notch(window: &WindowInfo, displays: &[DisplayInfo]) -> bool {
    displays.iter().any(|display| {
        let band = window.y - display.y;
        window.x == display.x
            && window.width == display.width
            && band > 0
            && band <= NOTCH_BAND
            && window.y + window.height as i32 == display.y + display.height as i32
    })
}

/// #EXP-1921: `activate_window`'s answer: an activation macOS refused, or
/// one that did not put the window in front within `wait` (`in_front`;
/// #EXP-1979: the window, not just its app), is an error (the agent's keys
/// would go to the window in front).
fn settle_activation(id: u64, activated: bool, wait: Duration, in_front: impl FnMut() -> bool) -> Result<(), String> {
    if activated && crate::backend::wait_active(wait, in_front) {
        return Ok(());
    }
    Err(format!("macOS did not activate window {id}."))
}

/// #EXP-1979: the window in front of a [`Backend::list_windows`] answer:
/// the focused app's first window (the one marked active).
fn front_window(windows: &[WindowInfo]) -> Option<u64> {
    windows.iter().find(|window| window.active).map(|window| window.id)
}

/// #EXP-1979: whether window `id` is in front: its app has the focus, and
/// it is the app's main or focused window as Accessibility names them
/// (`named`), so a sheet or a bubble listed above it is still its. Where
/// Accessibility names none (our own process), the window list's front
/// window (`listed`).
fn window_in_front(id: u64, app_in_front: bool, named: Option<&[u64]>, listed: impl FnOnce() -> Option<u64>) -> bool {
    app_in_front && named.map_or_else(|| listed() == Some(id), |named| named.contains(&id))
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

/// The app in front, asked fresh (#EXP-1921): Accessibility's focused
/// application, else the owner of the frontmost window. Never NSWorkspace,
/// whose answer only moves while the host's main run loop turns.
fn frontmost_pid() -> Option<u32> {
    focused_by_accessibility().or_else(front_window_owner)
}

fn focused_by_accessibility() -> Option<u32> {
    // SAFETY: returns a new AXUIElement (create rule), never null.
    let system = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateSystemWide()) };
    let app = ax_copy(&system, "AXFocusedApplication")?;
    let mut pid: Pid = 0;
    // SAFETY: `app` is a live AXUIElement; `pid` receives its process id.
    let err = unsafe { AXUIElementGetPid(app.as_CFTypeRef(), &mut pid) };
    (err == 0).then(|| u32::try_from(pid).ok()).flatten().filter(|&pid| pid != 0)
}

/// The owner of the topmost app window (layer 0) on screen.
fn front_window_owner() -> Option<u32> {
    let list = copy_window_info(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)?;
    list.iter().find_map(|item| {
        // SAFETY: every entry of the window list is a CFDictionary with
        // CFString keys, alive as long as `list`.
        let entry: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        if number(&entry, window_key(KeyName::Layer)) != Some(0) {
            return None;
        }
        if text(&entry, window_key(KeyName::OwnerName)).is_some_and(|owner| owner == "Window Server" || owner == "Dock") {
            return None;
        }
        number(&entry, window_key(KeyName::OwnerPid)).and_then(|pid| u32::try_from(pid).ok())
    })
}

/// Whether Accessibility may address `pid` from this (non-main) thread.
/// Never our own process (EXP-1436): an AX call into ourselves skips the
/// IPC and runs AppKit right here, so `AXMain` on our own window ordered it
/// off the main thread and AppKit trapped ("Must only be used from the main
/// thread"), killing the app while an agent activated the Exponential window.
fn ax_reachable(pid: Pid) -> bool {
    u32::try_from(pid).ok() != Some(std::process::id())
}

/// One of an app's Accessibility windows, as [`pick_ax_window`] weighs it.
#[derive(Clone, Debug)]
struct AxCandidate {
    /// Its window number ([`ax_window_number`]), when macOS tells.
    number: Option<u64>,
    /// Its frame is the window server's, within [`FRAME_SLACK`].
    frame: bool,
    title: Option<String>,
}

/// #EXP-1979: which of an app's Accessibility windows is `window`: the one
/// with its window number; else, of those whose frame matches (and whose
/// number is unknown), the only one, or the one with its title (two
/// zoomed windows share one frame), or the first.
fn pick_ax_window(candidates: &[AxCandidate], window: &WindowInfo) -> Option<usize> {
    if let Some(index) = candidates.iter().position(|candidate| candidate.number == Some(window.id)) {
        return Some(index);
    }
    let framed: Vec<usize> = (0..candidates.len()).filter(|&index| candidates[index].frame && candidates[index].number.is_none()).collect();
    match framed.as_slice() {
        [] => None,
        [only] => Some(*only),
        several => several.iter().copied().find(|&index| candidates[index].title.as_deref() == Some(window.title.as_str())).or(Some(several[0])),
    }
}

/// An Accessibility window's window number, through HIServices' private
/// `_AXUIElementGetWindow` (the same for a decade; yabai, Hammerspoon and
/// AltTab use it), looked up at run time: `None` when macOS has none.
fn ax_window_number(element: &CFType) -> Option<u64> {
    type GetWindow = unsafe extern "C" fn(CFTypeRef, *mut u32) -> i32;
    static GET: std::sync::OnceLock<Option<GetWindow>> = std::sync::OnceLock::new();
    let get = (*GET.get_or_init(|| {
        // SAFETY: a symbol lookup in the loaded images (ApplicationServices
        // is linked); a hit is `AXError _AXUIElementGetWindow(AXUIElementRef,
        // CGWindowID *)`.
        let symbol = unsafe { dlsym(RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr()) };
        (!symbol.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, GetWindow>(symbol) })
    }))?;
    let mut number = 0u32;
    // SAFETY: `element` is a live AXUIElement; `number` receives its id.
    let err = unsafe { get(element.as_CFTypeRef(), &mut number) };
    (err == 0 && number != 0).then_some(u64::from(number))
}

/// The Accessibility element of app `pid`; `None` for our own process.
fn ax_app(pid: Pid) -> Option<CFType> {
    if !ax_reachable(pid) {
        return None;
    }
    // SAFETY: returns a new AXUIElement (create rule) for any pid.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        log::debug!("[computer] no Accessibility element for pid {pid}");
        return None;
    }
    // SAFETY: non-null and ours (create rule).
    Some(unsafe { CFType::wrap_under_create_rule(app) })
}

/// #EXP-1979: the window numbers of `pid`'s main and focused windows over
/// Accessibility; `None` when it names none (our own process).
fn ax_front_windows(pid: Pid) -> Option<Vec<u64>> {
    let app = ax_app(pid)?;
    let named: Vec<u64> = ["AXMainWindow", "AXFocusedWindow"].into_iter().filter_map(|attribute| ax_copy(&app, attribute)).filter_map(|window| ax_window_number(&window)).collect();
    (!named.is_empty()).then_some(named)
}

/// The Accessibility window of `pid` that is the window server's `window`
/// ([`pick_ax_window`]); each miss logged. `None` for our own process.
fn ax_window(pid: Pid, window: &WindowInfo) -> Option<CFType> {
    let Some(app) = ax_app(pid) else {
        log::debug!("[computer] window {} is ours or has no Accessibility element", window.id);
        return None;
    };
    let Some(list) = ax_copy(&app, "AXWindows").and_then(|list| list.downcast::<CFArray>()) else {
        log::debug!("[computer] pid {pid} lists no Accessibility windows");
        return None;
    };
    let close = |a: f64, b: i32| (a - f64::from(b)).abs() <= FRAME_SLACK;
    let same_frame = |element: &CFType| match (ax_point(element), ax_size(element)) {
        (Some(origin), Some(size)) => {
            close(origin.x, window.x) && close(origin.y, window.y) && close(size.width, window.width as i32) && close(size.height, window.height as i32)
        }
        _ => false,
    };
    // SAFETY: AXWindows holds AXUIElements, alive as long as `list`.
    let mut elements: Vec<CFType> = list.iter().map(|item| unsafe { CFType::wrap_under_get_rule(*item) }).collect();
    let candidates: Vec<AxCandidate> = elements
        .iter()
        .map(|element| {
            // The number decides; the frame and title only stand in for it.
            let number = ax_window_number(element);
            let unknown = number.is_none();
            let title = || ax_copy(element, "AXTitle").and_then(|title| title.downcast::<CFString>()).map(|title| title.to_string());
            AxCandidate { number, frame: unknown && same_frame(element), title: unknown.then(title).flatten() }
        })
        .collect();
    let found = pick_ax_window(&candidates, window).map(|index| elements.swap_remove(index));
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
    /// No key for it: the character as a unicode string.
    Unicode(char),
    /// A named key Macs do not have (F21 and up).
    Missing,
    /// #EXP-1980: a character for a shortcut that no key of the layout
    /// types (German `]` is Option+6): an error, never another key.
    Unreachable(char),
}

/// The modifiers held under a key, as far as they decide its keystroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Under {
    /// None, or Shift alone: typing.
    Nothing,
    /// Control or Option, without Command.
    Shortcut,
    /// Command (with any other).
    Command,
}

/// #EXP-1922: a character goes to its key on the active `layout`; one the
/// layout has no plain or Shift key for is typed as itself. Without a
/// layout to read, the ANSI keys. #EXP-1980: under Command the layout's
/// Command level decides first (the key a person presses for that ⌘
/// shortcut); a dead key under nothing is typed as itself, since alone it
/// types nothing; a shortcut character off the plain and Shift levels is
/// its Command level's key (a non-Latin layout's Latin letters), else an
/// error: the ANSI key there is another character's (German `]` = `+`).
fn stroke(key: Key, layout: Option<&Layout>, under: Under) -> Stroke {
    let ch = match key {
        Key::Named(named) => return named_keycode(named).map_or(Stroke::Missing, |keycode| Stroke::Code(keycode, false)),
        Key::Char(ch) => ch,
    };
    let Some(layout) = layout else {
        return char_keycode(ch).map_or(Stroke::Unicode(ch), |(keycode, shift)| Stroke::Code(keycode, shift));
    };
    let command = || layout.command_key(ch).map(|(keycode, shift)| Stroke::Code(keycode, shift));
    if under == Under::Command {
        if let Some(stroke) = command() {
            return stroke;
        }
    }
    match (layout.typed.get(&ch), under) {
        (Some(place), _) if !place.dead || under != Under::Nothing => Stroke::Code(place.keycode, place.shift),
        (_, Under::Nothing) => Stroke::Unicode(ch),
        (_, Under::Shortcut) => command().unwrap_or(Stroke::Unreachable(ch)),
        (_, Under::Command) => Stroke::Unreachable(ch),
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

/// #EXP-1922: what each plain and Shift keystroke types on one keyboard
/// layout, by character (a plain key before a Shift one, the lower keycode
/// first, a live key before a dead one; the keypad aside). #EXP-1980: and
/// what each key types under Command, the level macOS reads ⌘ shortcuts
/// on: the plain level on most layouts, the QWERTY keys on "Dvorak –
/// QWERTY ⌘", Latin letters and the US punctuation on Russian; on such a
/// layout also under Command+Shift (with Shift).
#[derive(Debug, Default)]
struct Layout {
    typed: std::collections::HashMap<char, Place>,
    command: std::collections::HashMap<char, (CGKeyCode, bool)>,
}

/// The key that types a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    keycode: CGKeyCode,
    shift: bool,
    /// A dead key: alone it types nothing and accents the next key.
    dead: bool,
}

/// The keypad's keycodes: its digits and operators are the main keys'.
const KEYPAD: [CGKeyCode; 18] = [65, 67, 69, 71, 75, 76, 78, 81, 82, 83, 84, 85, 86, 87, 88, 89, 91, 92];

impl Layout {
    /// `translate(keycode, shift, command)` = the one character that
    /// keystroke types, and whether its key is dead.
    fn from(translate: impl Fn(CGKeyCode, bool, bool) -> Option<(char, bool)>) -> Layout {
        let keys = || (0..128).filter(|keycode| !KEYPAD.contains(keycode));
        let typed = |keycode, shift, command| translate(keycode, shift, command).filter(|(ch, _)| !ch.is_control());
        let mut layout = Layout::default();
        for shift in [false, true] {
            for keycode in keys() {
                let Some((ch, dead)) = typed(keycode, shift, false) else { continue };
                let place = Place { keycode, shift, dead };
                let slot = layout.typed.entry(ch).or_insert(place);
                if slot.dead && !dead {
                    *slot = place;
                }
            }
        }
        for keycode in keys() {
            if let Some((ch, _)) = typed(keycode, false, true) {
                layout.command.entry(ch).or_insert((keycode, false));
            }
        }
        // Where Command moves the keys, a shifted shortcut is read on the
        // Command+Shift level; elsewhere that level carries the US base
        // characters (German Cmd+Shift+7 = 7), and the Shift keys stand.
        let char_of = |keycode, command| typed(keycode, false, command).map(|(ch, _)| ch);
        if keys().any(|keycode| char_of(keycode, true) != char_of(keycode, false)) {
            for keycode in keys() {
                if let Some((ch, _)) = typed(keycode, true, true) {
                    layout.command.entry(ch).or_insert((keycode, true));
                }
            }
        }
        layout
    }

    /// The key of the ⌘ shortcut `ch`, and whether it takes Shift: a capital
    /// the Command levels lack is its letter's key with Shift.
    fn command_key(&self, ch: char) -> Option<(CGKeyCode, bool)> {
        match self.command.get(&ch) {
            Some(&key) => Some(key),
            None if ch.is_ascii_uppercase() => self.command.get(&ch.to_ascii_lowercase()).map(|&(keycode, _)| (keycode, true)),
            None => None,
        }
    }
}

/// The active keyboard layout (`None` = macOS names none with key data, a
/// layout of an input method), one read at a time ([`TIS`]). `Err` = the
/// main thread did not answer: never guess the US keys then (German cmd+z
/// would be Cmd+Y).
fn current_layout() -> Result<Option<Layout>, String> {
    // SAFETY: no arguments.
    if read_in_place(unsafe { pthread_main_np() } != 0, app_kit_runs()) {
        return Ok(read_layout());
    }
    extern "C" fn work(context: *mut c_void) {
        // SAFETY: the box `current_layout` handed over, taken back once.
        let reply = unsafe { Box::from_raw(context.cast::<std::sync::mpsc::Sender<Option<Layout>>>()) };
        let _ = reply.send(read_layout());
    }
    let (reply, answer) = std::sync::mpsc::channel::<Option<Layout>>();
    // SAFETY: the main queue is a static of libdispatch; `work` owns the box.
    unsafe { dispatch_async_f(&_dispatch_main_q, Box::into_raw(Box::new(reply)).cast(), work) };
    answer.recv_timeout(Duration::from_secs(2)).map_err(|_| "The keyboard layout could not be read: the app is busy.".to_string())
}

/// #EXP-1980: where TIS is read. Where AppKit runs (the IDE) it is the main
/// thread's, whose TSM a read on another thread would cut into (HIToolbox
/// aborts the process): there it goes to the main queue, whatever the main
/// run loop is doing this instant (between two of its turns, while AppKit
/// dispatches an event, it has no mode). Where nothing runs AppKit (the
/// daemon, tests), on this thread.
fn read_in_place(on_main_thread: bool, app_kit_runs: bool) -> bool {
    on_main_thread || !app_kit_runs
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    /// The application object, nil until one is made.
    static NSApp: *mut objc2::runtime::AnyObject;
}

/// Whether an NSApplication runs its event loop in this process.
fn app_kit_runs() -> bool {
    // SAFETY: AppKit's global, a pointer read; `isRunning` is a property read.
    unsafe { NSApp.as_ref() }.is_some_and(|app| unsafe { objc2::msg_send![app, isRunning] })
}

/// #EXP-1980: HIToolbox aborts the process when two threads call TIS at
/// once in a host without an event loop (the daemon): every TIS call here
/// holds this.
static TIS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// [`current_layout`]'s read.
fn read_layout() -> Option<Layout> {
    let _tis = TIS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // SAFETY: no arguments; a source is returned under the create rule.
    let source = unsafe { TISCopyCurrentKeyboardLayoutInputSource() };
    if source.is_null() {
        return None;
    }
    // SAFETY: non-null and ours (create rule); released before `_tis`.
    let source = unsafe { CFType::wrap_under_create_rule(source) };
    layout_of(&source)
}

/// The keyboard layout of the input source `source`, through UCKeyTranslate
/// (`None` = it has no key data: an input method). The caller holds [`TIS`].
fn layout_of(source: &CFType) -> Option<Layout> {
    // SAFETY: `source` is a live input source; its layout data is the
    // source's (get rule), alive while it is.
    unsafe {
        let data = TISGetInputSourceProperty(source.as_CFTypeRef(), kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return None;
        }
        let data = core_foundation::data::CFData::wrap_under_get_rule(data as core_foundation::data::CFDataRef);
        let keyboard = u32::from(LMGetKbdType());
        Some(Layout::from(|keycode, shift, command| {
            const DOWN: u16 = 0; // kUCKeyActionDown
            const DISPLAY: u16 = 3; // kUCKeyActionDisplay
            const NO_DEAD_KEYS: u32 = 1; // kUCKeyTranslateNoDeadKeysMask
            // (cmdKey >> 8) & 0xff, (shiftKey >> 8) & 0xff.
            let modifiers = u32::from(command) | if shift { 2 } else { 0 };
            let translate = |action, options, dead: &mut u32| {
                let (mut len, mut text) = (0usize, [0u16; 4]);
                let err = UCKeyTranslate(data.bytes().as_ptr(), keycode, action, modifiers, keyboard, options, dead, text.len(), &mut len, text.as_mut_ptr());
                let mut chars = char::decode_utf16(text.get(..len)?.iter().copied());
                match (err, chars.next(), chars.next()) {
                    (0, Some(Ok(ch)), None) => Some(ch),
                    _ => None,
                }
            };
            let ch = translate(DISPLAY, NO_DEAD_KEYS, &mut 0)?;
            // A dead key's press leaves a dead-key state and no text.
            let mut state = 0;
            let _ = translate(DOWN, 0, &mut state);
            Some((ch, state != 0))
        }))
    }
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
    thread: LockThread,
}

impl Lock {
    fn take(on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Lock, String> {
        if !accessibility_granted() {
            return Err("Accessibility is not granted.".to_string());
        }
        let stop = Arc::new(AtomicBool::new(false));
        // A tap that went in just as the wait gave up stops here.
        let late = {
            let stop = stop.clone();
            move |answer: Result<CFRunLoop, String>| {
                stop.store(true, Ordering::Release);
                if let Ok(run_loop) = answer {
                    run_loop.stop();
                }
            }
        };
        let body = {
            let stop = stop.clone();
            move |ready| hold(on_esc, stop, ready)
        };
        match LockThread::start(body, late) {
            Ok((Ok(run_loop), thread)) => Ok(Lock { stop, run_loop, thread }),
            Ok((Err(reason), _)) => Err(reason),
            Err(LockStartError::Spawn(reason)) => Err(reason),
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
        self.thread.join();
        PERSON_BUTTONS.store(0, Ordering::Release);
    }
}

/// What the tap callback sees, owned by the tap thread for the tap's life.
struct TapState {
    on_esc: Box<dyn Fn() + Send + Sync>,
    fired: Cell<bool>,
    stop: Arc<AtomicBool>,
    port: Cell<CFMachPortRef>,
    /// What the person held when the tap went in ([`TAP_HELD`]).
    held: Cell<HeldAtEngage<TAP_HELD>>,
}

/// [`TapState::held`]: keycodes 0..128, then mouse buttons 0..32 from
/// [`TAP_BUTTONS`].
const TAP_BUTTONS: usize = 128;
const TAP_HELD: usize = TAP_BUTTONS + 32;

/// The tap thread: install the tap, report, run until `stop`, tear down.
fn hold(on_esc: Box<dyn Fn() + Send + Sync>, stop: Arc<AtomicBool>, ready: LockReady<Result<CFRunLoop, String>>) {
    let state = Box::new(TapState {
        on_esc,
        fired: Cell::new(false),
        stop: stop.clone(),
        port: Cell::new(std::ptr::null_mut()),
        held: Cell::new(HeldAtEngage::NONE),
    });
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
        ready.send(Err("macOS refused the input lock (the event tap); check Accessibility.".to_string()));
        return;
    }
    // SAFETY: non-null and ours (create rule).
    let port = unsafe { CFMachPort::wrap_under_create_rule(port_ref) };
    state.port.set(port_ref);
    let Ok(source) = port.create_runloop_source(0) else {
        // SAFETY: `port_ref` is the live tap.
        unsafe { CFMachPortInvalidate(port_ref) };
        ready.send(Err("Could not attach the input lock to a run loop.".to_string()));
        return;
    };
    let run_loop = CFRunLoop::get_current();
    // SAFETY: the common-modes constant of CoreFoundation.
    let common = unsafe { kCFRunLoopCommonModes };
    run_loop.add_source(&source, common);
    // SAFETY: `port_ref` is the live tap.
    unsafe { CGEventTapEnable(port_ref, true) };
    // After the tap: a release between this snapshot and the tap cannot be
    // missed. No callback runs before the run loop below.
    let held = held_now();
    state.held.set(held);
    publish_buttons(&held);
    if !ready.send(Ok(run_loop.clone())) {
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
    PERSON_BUTTONS.store(0, Ordering::Release);
    log::debug!("[computer] event tap removed");
}

/// #EXP-1979: the mouse buttons of `held` the person still holds, for the
/// pointer tools ([`PERSON_BUTTONS`]).
fn publish_buttons(held: &HeldAtEngage<TAP_HELD>) {
    PERSON_BUTTONS.store(held.armed(TAP_BUTTONS..TAP_HELD), Ordering::Release);
}

fn person_holds_button() -> bool {
    PERSON_BUTTONS.load(Ordering::Acquire) != 0
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
    let field = |field: u32| unsafe { CGEventGetIntegerValueField(event, field) };
    let ours = field(EventField::EVENT_SOURCE_USER_DATA) == MAGIC;
    let mut held = state.held.get();
    let verdict = tap_verdict(ours, kind, field(EventField::KEYBOARD_EVENT_KEYCODE), field(EventField::MOUSE_EVENT_BUTTON_NUMBER), &mut held);
    state.held.set(held);
    publish_buttons(&held);
    match verdict {
        Verdict::Pass => return event,
        Verdict::Swallow => {}
        Verdict::Esc => {
            if !state.fired.replace(true) {
                log::info!("[computer] Esc from the keyboard");
                // A panic must not unwind into the window server's callback.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (state.on_esc)())).is_err() {
                    log::warn!("[computer] the Esc handler panicked");
                }
                state.stop.store(true, Ordering::Release);
                CFRunLoop::get_current().stop();
            }
        }
    }
    std::ptr::null_mut()
}

/// The tap's decision on one event of `kind` (its keycode and mouse
/// button number read whatever the kind).
/// `held` = what the person held when the tap went in: each one's release
/// passes, once (a modifier's next FlagsChanged is its release), unless
/// ours pressed or released it first (#EXP-1979).
fn tap_verdict(ours: bool, kind: u32, keycode: i64, button: i64, held: &mut HeldAtEngage<TAP_HELD>) -> Verdict {
    let pressed = tap_index(kind, keycode, button);
    if ours {
        if let Some((index, _)) = pressed {
            held.ours(index);
        }
        return Verdict::Pass;
    }
    if kind == CGEventType::KeyDown as u32 && keycode == i64::from(ESCAPE) {
        return Verdict::Esc;
    }
    match pressed {
        Some((index, down)) if held.passes(index, down) => Verdict::Pass,
        _ => Verdict::Swallow,
    }
}

/// The [`TAP_HELD`] index an event of `kind` presses or releases, and
/// whether it is the press (a FlagsChanged counts as a release).
fn tap_index(kind: u32, keycode: i64, button: i64) -> Option<(usize, bool)> {
    let index = |value: i64, base: usize, room: usize| usize::try_from(value).ok().filter(|&value| value < room).map(|value| base + value);
    let key = || index(keycode, 0, TAP_BUTTONS);
    let pointer = |number: i64| index(number, TAP_BUTTONS, TAP_HELD - TAP_BUTTONS);
    match kind {
        k if k == CGEventType::KeyDown as u32 => key().map(|index| (index, true)),
        k if k == CGEventType::KeyUp as u32 || k == CGEventType::FlagsChanged as u32 => key().map(|index| (index, false)),
        k if k == CGEventType::LeftMouseDown as u32 => pointer(0).map(|index| (index, true)),
        k if k == CGEventType::LeftMouseUp as u32 => pointer(0).map(|index| (index, false)),
        k if k == CGEventType::RightMouseDown as u32 => pointer(1).map(|index| (index, true)),
        k if k == CGEventType::RightMouseUp as u32 => pointer(1).map(|index| (index, false)),
        k if k == CGEventType::OtherMouseDown as u32 => pointer(button).map(|index| (index, true)),
        k if k == CGEventType::OtherMouseUp as u32 => pointer(button).map(|index| (index, false)),
        _ => None,
    }
}

/// Every key and mouse button down right now ([`TAP_HELD`]'s indexes), as
/// the HID system sees them.
fn held_now() -> HeldAtEngage<TAP_HELD> {
    let mut held = HeldAtEngage::NONE;
    for (index, slot) in held.0.iter_mut().enumerate() {
        // SAFETY: plain queries of the HID system's state.
        *slot = unsafe {
            match index.checked_sub(TAP_BUTTONS) {
                None => CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, index as CGKeyCode),
                Some(button) => CGEventSourceButtonState(CGEventSourceStateID::HIDSystemState, button as u32),
            }
        };
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1919 SAME MONITOR on macOS (`work-screen.json` `pickCases`):
    /// what macOS reports (each display's UUID and unit number) finishes
    /// into the displays every case reads, connectors included, and the pick
    /// lands where the case says. #EXP-1994: macOS kept no connector before.
    #[test]
    fn a_pick_stays_on_its_monitor_when_a_twin_goes_or_arrives_on_macos() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/work-screen.json")).unwrap();
        for case in fixture["pickCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let rows = case["displays"].as_array().unwrap();
            let connectors: Vec<Option<String>> = rows.iter().map(|row| row["connector"].as_str().map(str::to_string)).collect();
            // What macOS reports: the bare UUID, before any twin is told apart.
            let reported: Vec<DisplayInfo> = rows
                .iter()
                .zip(&connectors)
                .map(|(row, connector)| {
                    let id = row["id"].as_str().unwrap();
                    let base = connector.as_deref().and_then(|connector| id.strip_suffix(&format!("@{connector}"))).unwrap_or(id);
                    DisplayInfo { id: base.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None }
                })
                .collect();
            let ids: Vec<CGDirectDisplayID> = (1..=rows.len() as CGDirectDisplayID).collect();
            let finished = finish_reported(ids.clone(), reported, &connectors).unwrap();
            assert_eq!(finished.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ids, "{name}: each id stays with its display");
            let displays: Vec<DisplayInfo> = finished.into_iter().map(|(_, info)| info).collect();
            let carried: Vec<(&str, Option<&str>)> = displays.iter().map(|display| (display.id.as_str(), display.connector.as_deref())).collect();
            let expected: Vec<(&str, Option<&str>)> = rows.iter().map(|row| (row["id"].as_str().unwrap(), row["connector"].as_str())).collect();
            assert_eq!(carried, expected, "{name}: the case's displays, connectors included");
            let stored = case["stored"].as_str().unwrap();
            let id_of = |index: Option<usize>| index.map(|index| displays[index].id.as_str());
            assert_eq!(id_of(crate::work_screen::pick_index(&displays, stored, case["seen"].as_str())), case["expect"].as_str(), "{name}");
            assert_eq!(id_of(crate::work_screen::carried_index(&displays, stored)), case["hub"].as_str(), "{name}: hub");
        }
    }

    #[test]
    fn accessibility_never_addresses_our_own_process() {
        let ours = Pid::try_from(std::process::id()).unwrap();
        assert!(!ax_reachable(ours));
        let window = WindowInfo {
            id: 1,
            title: String::new(),
            app: String::new(),
            pid: Some(std::process::id()),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            active: true,
            fullscreen: false,
        };
        assert!(ax_window(ours, &window).is_none());
        assert!(ax_reachable(1));
    }

    /// #EXP-1921: a button or key the person held when the lock engaged
    /// gets its release through, once; everything else of theirs stays out.
    #[test]
    fn the_tap_passes_the_release_of_what_was_held_at_engage_once() {
        let mut held = HeldAtEngage::<TAP_HELD>::NONE;
        held.0[56] = true; // kVK_Shift
        held.0[TAP_BUTTONS] = true; // the left button
        held.0[TAP_BUTTONS + 3] = true; // a side button
        let mut verdict = |kind: CGEventType, keycode: i64, button: i64| tap_verdict(false, kind as u32, keycode, button, &mut held);
        assert_eq!(verdict(CGEventType::LeftMouseDragged, 0, 0), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::LeftMouseUp, 0, 0), Verdict::Pass);
        assert_eq!(verdict(CGEventType::LeftMouseUp, 0, 0), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::RightMouseUp, 0, 1), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::OtherMouseUp, 0, 2), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::OtherMouseUp, 0, 3), Verdict::Pass);
        assert_eq!(verdict(CGEventType::FlagsChanged, 56, 0), Verdict::Pass);
        assert_eq!(verdict(CGEventType::FlagsChanged, 56, 0), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::KeyUp, 0, 0), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::LeftMouseDown, 0, 0), Verdict::Swallow);
        assert_eq!(verdict(CGEventType::KeyDown, i64::from(ESCAPE), 0), Verdict::Esc);
        assert_eq!(tap_verdict(true, CGEventType::LeftMouseDown as u32, 0, 0, &mut held), Verdict::Pass);
        // The snapshot's FFI answers (what is down is the person's business).
        let _ = held_now();
    }

    /// #EXP-1979: once the agent itself pressed or released a button or key
    /// the person held at engage, the person's later release is swallowed:
    /// it would cut into the agent's own drag or combo.
    #[test]
    fn the_agents_own_press_of_a_held_button_disarms_its_release() {
        let mut held = HeldAtEngage::<TAP_HELD>::NONE;
        held.0[TAP_BUTTONS] = true; // the left button
        held.0[56] = true; // kVK_Shift
        assert_eq!(tap_verdict(true, CGEventType::LeftMouseDown as u32, 0, 0, &mut held), Verdict::Pass);
        assert_eq!(tap_verdict(true, CGEventType::LeftMouseUp as u32, 0, 0, &mut held), Verdict::Pass);
        assert_eq!(tap_verdict(false, CGEventType::LeftMouseUp as u32, 0, 0, &mut held), Verdict::Swallow);
        assert_eq!(tap_verdict(true, CGEventType::FlagsChanged as u32, 56, 0, &mut held), Verdict::Pass);
        assert_eq!(tap_verdict(false, CGEventType::FlagsChanged as u32, 56, 0, &mut held), Verdict::Swallow);
    }

    /// #EXP-1979: a button the person held when the tap went in holds the
    /// pointer tools back until its release passes the tap, at most the
    /// wait; a held key holds nothing back.
    #[test]
    fn the_pointer_waits_for_the_release_of_a_button_held_at_engage() {
        let wait = |millis| crate::backend::wait_for_person_buttons(person_holds_button, || false, Duration::from_millis(millis));
        let mut held = HeldAtEngage::<TAP_HELD>::NONE;
        held.0[56] = true; // kVK_Shift
        publish_buttons(&held);
        assert_eq!(wait(60), crate::backend::ButtonWait::Free, "a key never holds the pointer back");
        held.0[TAP_BUTTONS] = true; // the left button
        publish_buttons(&held);
        assert_eq!(wait(60), crate::backend::ButtonWait::Held, "the wait ran out");
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(tap_verdict(false, CGEventType::LeftMouseUp as u32, 0, 0, &mut held), Verdict::Pass);
            publish_buttons(&held);
        });
        assert_eq!(wait(5000), crate::backend::ButtonWait::Free, "the person let go");
        release.join().unwrap();
        PERSON_BUTTONS.store(0, Ordering::Release);
    }

    /// The pid `lsappinfo` (Launch Services, asked fresh) says is in front.
    fn front_by_launch_services() -> Option<u32> {
        let run = |args: &[&str]| String::from_utf8(std::process::Command::new("lsappinfo").args(args).output().ok()?.stdout).ok();
        let asn = run(&["front"])?;
        let info = run(&["info", asn.trim()])?;
        let after = info.split("pid").nth(1)?;
        after.trim_start_matches(|c: char| !c.is_ascii_digit()).split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    }

    /// A window of its own in front for two seconds (JXA).
    const FRONT_WINDOW: &str = "ObjC.import('Cocoa'); var app = $.NSApplication.sharedApplication; app.setActivationPolicy(0); \
        var w = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(200, 200, 320, 120), 1, 2, false); \
        w.title = 'epilot test'; w.makeKeyAndOrderFront(null); app.activateIgnoringOtherApps(true); \
        $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(2));";

    /// #EXP-1921, LIVE (`--ignored`): shows a window of its own for two
    /// seconds, types and clicks nothing. A host without a main run loop
    /// (the daemon, this test) still reads the app in front now.
    #[test]
    #[ignore = "drives the real screen"]
    fn the_focused_app_is_read_fresh_without_a_main_run_loop() {
        let before = frontmost_pid();
        let mut dialog = std::process::Command::new("osascript").args(["-l", "JavaScript", "-e", FRONT_WINDOW]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        let (now, truth) = (frontmost_pid(), front_by_launch_services());
        dialog.wait().unwrap();
        assert_eq!(truth, Some(dialog.id()), "the window came to the front");
        assert_ne!(truth, before);
        assert_eq!(now, truth);
    }

    /// #EDE-140: fullscreen by geometry (NEW-59). A window on a display's
    /// full bounds is fullscreen, the system's own never. Below a notch (the
    /// display's width, a band of at most [`NOTCH_BAND`] points above it,
    /// down to the display's bottom) the frame alone does not say so: a
    /// window filling the screen under the menu bar has it too, so
    /// Accessibility decides, and a window with no process to ask is not
    /// fullscreen.
    #[test]
    fn fullscreen_is_a_displays_bounds_and_below_a_notch_only_by_accessibility() {
        let display = |x: i32, y: i32, width: u32, height: u32| DisplayInfo { id: format!("{x},{y}"), x, y, width, height, primary: x == 0, name: String::new(), connector: None };
        // A notched built-in display and an external one to its left.
        let displays = [display(0, 0, 1512, 982), display(-1920, 0, 1920, 1080)];
        let at = |app: &str, x: i32, y: i32, width: u32, height: u32| WindowInfo { id: 1, title: String::new(), app: app.into(), pid: None, x, y, width, height, active: true, fullscreen: false };

        assert!(fullscreen(&at("Keynote", 0, 0, 1512, 982), &displays));
        assert!(fullscreen(&at("Keynote", -1920, 0, 1920, 1080), &displays), "the external display's bounds");
        assert!(!fullscreen(&at("Dock", 0, 0, 1512, 982), &displays), "the system's own windows never");
        assert!(!fullscreen(&at("Keynote", 0, 0, 1512, 981), &displays), "a point short of the display");
        assert!(!fullscreen(&at("Keynote", -1920, 0, 1512, 982), &displays), "one display's size at the other's origin");

        // The frame below the camera housing (32 points on this display), down to the bottom.
        let notched = |band: i32| at("Keynote", 0, band, 1512, (982 - band) as u32);
        assert!(below_notch(&notched(32), &displays));
        assert!(below_notch(&notched(1), &displays) && below_notch(&notched(NOTCH_BAND), &displays));
        assert!(!below_notch(&notched(0), &displays), "no band is the display's bounds, decided before");
        assert!(!below_notch(&notched(NOTCH_BAND + 1), &displays), "a taller band is no notch");
        assert!(!below_notch(&at("Keynote", 0, 32, 1511, 950), &displays), "not the display's width");
        assert!(!below_notch(&at("Keynote", 0, 32, 1512, 949), &displays), "not down to the display's bottom");
        assert!(!below_notch(&at("Keynote", 1, 32, 1512, 950), &displays), "not at the display's left edge");
        assert!(below_notch(&at("Keynote", -1920, 25, 1920, 1055), &displays), "a window under the menu bar of any display has the frame");
        // The frame alone is not fullscreen, and here is no process for Accessibility to ask.
        assert!(!fullscreen(&notched(32), &displays));
    }

    fn window(id: u64, title: &str, pid: u32, active: bool) -> WindowInfo {
        WindowInfo { id, title: title.into(), app: "Chrome".into(), pid: Some(pid), x: 0, y: 25, width: 1440, height: 875, active, fullscreen: false }
    }

    /// #EXP-1979: the window in front is the first window of the focused
    /// app (what list_windows marks active), not just any window of it.
    #[test]
    fn the_window_in_front_is_the_focused_apps_first() {
        let after = [window(1, "Docs", 42, true), window(2, "Mail", 42, false)];
        assert_eq!(front_window(&after), Some(1));
        assert!(settle_activation(2, true, Duration::from_millis(60), || front_window(&after) == Some(2)).is_err(), "Chrome is in front, Mail is not");
        assert_eq!(front_window(&[window(1, "Docs", 42, false)]), None);
    }

    /// #EXP-1979: the window is in front when its app is and Accessibility
    /// names it the main or focused window, though a sheet of it is first
    /// in the window list; the list decides only where Accessibility names
    /// none (our own process).
    #[test]
    fn a_window_with_a_sheet_in_front_of_it_is_in_front() {
        let sheet_first = || Some(9);
        assert!(window_in_front(7, true, Some(&[7]), sheet_first), "its sheet is listed above it");
        assert!(!window_in_front(2, true, Some(&[1, 1]), || Some(2)), "Docs is the main window, not Mail");
        assert!(!window_in_front(7, false, Some(&[7]), sheet_first), "its app is not in front");
        assert!(window_in_front(2, true, None, || Some(2)));
        assert!(!window_in_front(2, true, None, || Some(1)));
    }

    /// #EXP-1979: of an app's Accessibility windows, `window` is the one
    /// with its window number; without numbers, of those with its frame
    /// the one with its title (two zoomed windows share one frame).
    #[test]
    fn same_frame_windows_are_told_apart() {
        let mail = window(2, "Mail", 42, false);
        let docs = AxCandidate { number: None, frame: true, title: Some("Docs".into()) };
        let mail_ax = AxCandidate { number: None, frame: true, title: Some("Mail".into()) };
        assert_eq!(pick_ax_window(&[docs.clone(), mail_ax.clone()], &mail), Some(1));
        let numbered = [AxCandidate { number: Some(1), ..docs.clone() }, AxCandidate { number: Some(2), frame: false, ..mail_ax.clone() }];
        assert_eq!(pick_ax_window(&numbered, &mail), Some(1), "the number wins over the frame");
        let off = AxCandidate { frame: false, ..mail_ax };
        assert_eq!(pick_ax_window(&[docs.clone(), off], &mail), Some(0), "the only frame match");
        assert_eq!(pick_ax_window(&[AxCandidate { frame: false, ..docs }], &mail), None);
    }

    /// Two windows of its own on one frame, in front for five seconds (JXA;
    /// finishLaunching = it answers Accessibility).
    const TWO_WINDOWS: &str = "ObjC.import('Cocoa'); var app = $.NSApplication.sharedApplication; app.setActivationPolicy(0); app.finishLaunching; \
        function make(title) { var w = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(240, 240, 360, 140), 1, 2, false); \
        w.title = title; w.makeKeyAndOrderFront(null); return w; } \
        var a = make('epilot A'); var b = make('epilot B'); app.activateIgnoringOtherApps(true); \
        $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(5));";

    /// #EXP-1979, LIVE (`--ignored`): shows two windows of its own on one
    /// frame for five seconds, types and clicks nothing. activate_window
    /// brings the one behind to the front, and answers so only then.
    #[test]
    #[ignore = "drives the real screen"]
    fn activate_window_brings_that_window_forward_not_just_its_app() {
        let mut script = std::process::Command::new("osascript").args(["-l", "JavaScript", "-e", TWO_WINDOWS]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let mac = Mac::new().unwrap();
        let ours = |title: &str| {
            let windows = mac.list_windows().unwrap();
            windows.into_iter().find(|window| window.pid == Some(script.id()) && window.title == title).map(|window| window.id)
        };
        let (a, b) = (ours("epilot A").expect("window A"), ours("epilot B").expect("window B"));
        assert_eq!(front_window(&mac.list_windows().unwrap()), Some(b), "B opened last");
        let to_a = mac.activate_window(a);
        let front_after_a = front_window(&mac.list_windows().unwrap());
        let to_b = mac.activate_window(b);
        let front_after_b = front_window(&mac.list_windows().unwrap());
        script.wait().unwrap();
        assert_eq!((to_a, front_after_a), (Ok(()), Some(a)));
        assert_eq!((to_b, front_after_b), (Ok(()), Some(b)));
    }

    /// Two windows of its own, the one behind with a sheet, for five
    /// seconds (JXA).
    const WINDOW_WITH_A_SHEET: &str = "ObjC.import('Cocoa'); var app = $.NSApplication.sharedApplication; app.setActivationPolicy(0); app.finishLaunching; \
        function make(title) { var w = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(240, 240, 360, 140), 1, 2, false); \
        w.title = title; w.makeKeyAndOrderFront(null); return w; } \
        var a = make('epilot A'); var sheet = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer($.NSMakeRect(0, 0, 200, 60), 1, 2, false); \
        a.beginSheetCompletionHandler(sheet, function (code) {}); var b = make('epilot B'); app.activateIgnoringOtherApps(true); \
        $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(5));";

    /// #EXP-1979, LIVE (`--ignored`): shows two windows of its own, one with
    /// a sheet, for five seconds, types and clicks nothing. The window under
    /// the sheet is activated (the sheet comes first in the window list).
    #[test]
    #[ignore = "drives the real screen"]
    fn activate_window_counts_a_window_whose_sheet_is_in_front() {
        let mut script = std::process::Command::new("osascript").args(["-l", "JavaScript", "-e", WINDOW_WITH_A_SHEET]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let mac = Mac::new().unwrap();
        let windows = mac.list_windows().unwrap();
        let id = |title: &str| windows.iter().find(|window| window.pid == Some(script.id()) && window.title == title).map(|window| window.id);
        let (a, b) = (id("epilot A").expect("window A"), id("epilot B").expect("window B"));
        let answer = mac.activate_window(a);
        let front = front_window(&mac.list_windows().unwrap());
        script.wait().unwrap();
        assert_eq!(answer, Ok(()));
        assert_ne!(front, Some(b), "B is behind now");
    }

    /// #EXP-1921: activate_window answers success only once the window's
    /// app is in front.
    #[test]
    fn an_activation_counts_once_the_app_is_in_front() {
        let wait = Duration::from_millis(100);
        assert!(settle_activation(4, false, wait, || true).is_err(), "macOS refused");
        assert!(settle_activation(4, true, wait, || false).is_err(), "never in front");
        let mut polls = 0;
        // Room for a slow runner's sleeps: the answer comes at the third poll.
        assert_eq!(
            settle_activation(4, true, Duration::from_secs(10), || {
                polls += 1;
                polls == 3
            }),
            Ok(())
        );
    }

    /// #EXP-1922: on the layout this Mac has active, the key pressed for a
    /// character types that character (German QWERTZ: `z` is not keycode 6).
    #[test]
    fn every_key_types_its_character_on_the_active_layout() {
        let Some(active) = current_layout().unwrap() else { return };
        for ch in ('a'..='z').chain('0'..='9').chain(['-', '.', ',']) {
            let Stroke::Code(keycode, shift) = stroke(Key::Char(ch), Some(&active), Under::Nothing) else { continue };
            let typed = active.typed.iter().find(|(_, place)| (place.keycode, place.shift) == (keycode, shift)).map(|(&typed, _)| typed);
            assert_eq!(typed, Some(ch), "{ch:?} pressed as keycode {keycode}");
        }
    }

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        static kTISPropertyInputSourceID: CFStringRef;
        fn TISCreateInputSourceList(properties: CFDictionaryRef, include_all_installed: u8) -> CFTypeRef;
    }

    /// Apple's own data of the installed keyboard layout `id`, read the way
    /// the active one is; the layout is never selected (the person's input
    /// source stays as it is).
    fn installed(id: &str) -> Layout {
        // SAFETY: a constant CFString of the framework.
        let key = unsafe { CFString::wrap_under_get_rule(kTISPropertyInputSourceID) };
        let filter = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFString::new(id).as_CFType())]);
        let _tis = TIS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // SAFETY: `filter` is live for the call; the list is ours (create rule).
        let list = unsafe { TISCreateInputSourceList(filter.as_concrete_TypeRef(), 1) };
        assert!(!list.is_null(), "macOS lists no {id}");
        // SAFETY: non-null, a CFArray of input sources, ours.
        let list = unsafe { CFArray::<CFType>::wrap_under_create_rule(list as core_foundation::array::CFArrayRef) };
        let source = list.get(0).unwrap_or_else(|| panic!("macOS lists no {id}"));
        layout_of(&source).unwrap_or_else(|| panic!("{id} has no key data"))
    }

    /// #EXP-1980, on Apple's QWERTZ and AZERTY data: cmd+z stays Cmd+Z on
    /// German, cmd+a never becomes Cmd+Q on French, a bare key types its
    /// character.
    #[test]
    fn qwertz_and_azerty_press_the_key_of_the_character() {
        let german = installed("com.apple.keylayout.German");
        assert_eq!(stroke(Key::Char('z'), Some(&german), Under::Command), Stroke::Code(16, false));
        assert_eq!(stroke(Key::Char('y'), Some(&german), Under::Command), Stroke::Code(6, false));
        assert_eq!(stroke(Key::Char('Z'), Some(&german), Under::Command), Stroke::Code(16, true));
        assert_eq!(stroke(Key::Char('-'), Some(&german), Under::Nothing), Stroke::Code(44, false));
        assert_eq!(stroke(Key::Char('/'), Some(&german), Under::Nothing), Stroke::Code(26, true));
        let french = installed("com.apple.keylayout.French");
        assert_eq!(stroke(Key::Char('a'), Some(&french), Under::Command), Stroke::Code(12, false));
        assert_eq!(stroke(Key::Char('q'), Some(&french), Under::Command), Stroke::Code(0, false));
        assert_eq!(stroke(Key::Char('w'), Some(&french), Under::Command), Stroke::Code(6, false));
        assert_eq!(stroke(Key::Char('z'), Some(&french), Under::Command), Stroke::Code(13, false));
        assert_eq!(stroke(Key::Char('1'), Some(&french), Under::Nothing), Stroke::Code(18, true));
    }

    /// #EXP-1980: macOS reads a ⌘ shortcut on the layout's Command level:
    /// "Dvorak – QWERTY ⌘" puts every one on its QWERTY key, Russian its
    /// punctuation on the US keys, a shifted one on its Command+Shift key;
    /// typing keeps the layout's own keys, and a layout whose Command level
    /// is its typing level its Shift keys (German cmd+/ = Shift+7).
    #[test]
    fn a_command_shortcut_goes_to_its_key_on_the_command_level() {
        let dvorak = installed("com.apple.keylayout.DVORAK-QWERTYCMD");
        assert_eq!(stroke(Key::Char('z'), Some(&dvorak), Under::Command), Stroke::Code(6, false), "cmd+z, not Cmd+/");
        assert_eq!(stroke(Key::Char('c'), Some(&dvorak), Under::Command), Stroke::Code(8, false), "cmd+c, not Cmd+I");
        assert_eq!(stroke(Key::Char('q'), Some(&dvorak), Under::Command), Stroke::Code(12, false), "cmd+q, not Cmd+X");
        assert_eq!(stroke(Key::Char('Z'), Some(&dvorak), Under::Command), Stroke::Code(6, true));
        assert_eq!(stroke(Key::Char('+'), Some(&dvorak), Under::Command), Stroke::Code(24, true), "cmd+plus, not Cmd+}}");
        assert_eq!(stroke(Key::Char('?'), Some(&dvorak), Under::Command), Stroke::Code(44, true), "cmd+?, not Cmd+{{");
        assert_eq!(stroke(Key::Char('{'), Some(&dvorak), Under::Command), Stroke::Code(33, true));
        assert_eq!(stroke(Key::Char('}'), Some(&dvorak), Under::Command), Stroke::Code(30, true));
        assert_eq!(stroke(Key::Char('z'), Some(&dvorak), Under::Nothing), Stroke::Code(44, false));
        let russian = installed("com.apple.keylayout.Russian");
        assert_eq!(stroke(Key::Char('v'), Some(&russian), Under::Command), Stroke::Code(9, false));
        assert_eq!(stroke(Key::Char('V'), Some(&russian), Under::Command), Stroke::Code(9, true));
        assert_eq!(stroke(Key::Char('.'), Some(&russian), Under::Command), Stroke::Code(47, false), "cmd+., not Cmd+&");
        assert_eq!(stroke(Key::Char('['), Some(&russian), Under::Command), Stroke::Code(33, false), "cmd+[, not Cmd+~");
        assert_eq!(stroke(Key::Char(':'), Some(&russian), Under::Command), Stroke::Code(41, true), "cmd+:, not Cmd+Shift+5");
        assert_eq!(stroke(Key::Char('>'), Some(&russian), Under::Command), Stroke::Code(47, true));
        assert_eq!(stroke(Key::Char('{'), Some(&russian), Under::Command), Stroke::Code(33, true));
        assert_eq!(stroke(Key::Char('~'), Some(&russian), Under::Command), Stroke::Code(50, true));
        let german = installed("com.apple.keylayout.German");
        assert_eq!(stroke(Key::Char('/'), Some(&german), Under::Command), Stroke::Code(26, true), "Shift+7, not Shift+ß");
    }

    /// #EXP-1980: a shortcut character no key of a Latin layout types alone
    /// (German `]` is Option+6) is an error, never the ANSI key (there `+`,
    /// Cmd++ zooms in); a non-Latin layout's goes to its Command level's key;
    /// typed, it is the character.
    #[test]
    fn a_shortcut_character_off_the_layout_is_never_another_key() {
        let german = installed("com.apple.keylayout.German");
        assert_eq!(stroke(Key::Char(']'), Some(&german), Under::Command), Stroke::Unreachable(']'), "not Cmd++");
        assert_eq!(stroke(Key::Char('['), Some(&german), Under::Shortcut), Stroke::Unreachable('['));
        assert_eq!(stroke(Key::Char(']'), Some(&german), Under::Nothing), Stroke::Unicode(']'));
        let russian = installed("com.apple.keylayout.Russian");
        assert_eq!(stroke(Key::Char('c'), Some(&russian), Under::Shortcut), Stroke::Code(8, false), "ctrl+c");
        assert_eq!(stroke(Key::Char(']'), Some(&russian), Under::Shortcut), Stroke::Code(50, false), "Russian types ] on its own key");
        assert_eq!(stroke(Key::Char(']'), None, Under::Command), Stroke::Code(30, false), "no layout: the US keys");
    }

    /// #EXP-1980: in the IDE (AppKit running) TIS is read on the main queue
    /// from every other thread; in the daemon and tests where it is called.
    #[test]
    fn the_layout_is_read_on_the_main_thread_wherever_app_kit_runs() {
        assert!(!read_in_place(false, true));
        assert!(read_in_place(true, true));
        assert!(read_in_place(false, false));
        assert!(!app_kit_runs(), "no NSApplication in a test");
    }

    /// #EXP-1980: a dead key types nothing alone (German ^ waits for the
    /// next key and puts its accent on it): a bare one goes as its
    /// character; the character of a live key stays a keystroke.
    #[test]
    fn a_bare_dead_key_is_typed_as_its_character() {
        let german = installed("com.apple.keylayout.German");
        assert_eq!(stroke(Key::Char('^'), Some(&german), Under::Nothing), Stroke::Unicode('^'));
        assert_eq!(stroke(Key::Char('´'), Some(&german), Under::Nothing), Stroke::Unicode('´'));
        assert_eq!(stroke(Key::Char('°'), Some(&german), Under::Nothing), Stroke::Code(10, true));
        let french = installed("com.apple.keylayout.French");
        assert_eq!(stroke(Key::Char('^'), Some(&french), Under::Nothing), Stroke::Unicode('^'));
        assert_eq!(stroke(Key::Char('@'), Some(&french), Under::Nothing), Stroke::Code(10, false));
    }

    /// The US keys moved the way a layout moves them: `swaps` exchanges
    /// two keys' characters (both levels), `cyrillic` = no Latin letters.
    fn layout(swaps: &[(char, char)], cyrillic: bool) -> Layout {
        Layout::from(|keycode, shift, command| {
            let ch = (0..=127u8).map(char::from).find(|&ch| char_keycode(ch) == Some((keycode, shift)))?;
            let lower = ch.to_ascii_lowercase();
            let moved = swaps.iter().find_map(|&(a, b)| (lower == a).then_some(b).or((lower == b).then_some(a))).unwrap_or(lower);
            let moved = if ch.is_ascii_uppercase() { moved.to_ascii_uppercase() } else { moved };
            // Under Command a Cyrillic layout types the US letters.
            match cyrillic && !command && moved.is_ascii_alphabetic() {
                true => char::from_u32(u32::from(moved.to_ascii_lowercase()) - u32::from('a') + 0x430),
                false => Some(moved),
            }
            .map(|ch| (ch, false))
        })
    }

    /// #EXP-1922: QWERTZ and AZERTY press the key the layout has the
    /// character on; a non-Latin layout keeps the US key for a shortcut and
    /// types a bare letter as itself.
    #[test]
    fn characters_go_to_their_key_on_the_layout() {
        let qwertz = layout(&[('y', 'z')], false);
        assert_eq!(stroke(Key::Char('z'), Some(&qwertz), Under::Command), Stroke::Code(16, false));
        assert_eq!(stroke(Key::Char('y'), Some(&qwertz), Under::Command), Stroke::Code(6, false));
        assert_eq!(stroke(Key::Char('Z'), Some(&qwertz), Under::Nothing), Stroke::Code(16, true));
        let azerty = layout(&[('a', 'q'), ('z', 'w')], false);
        assert_eq!(stroke(Key::Char('a'), Some(&azerty), Under::Command), Stroke::Code(12, false));
        assert_eq!(stroke(Key::Char('q'), Some(&azerty), Under::Command), Stroke::Code(0, false));
        assert_eq!(stroke(Key::Char('w'), Some(&azerty), Under::Command), Stroke::Code(6, false));
        let cyrillic = layout(&[], true);
        assert_eq!(stroke(Key::Char('v'), Some(&cyrillic), Under::Command), Stroke::Code(9, false));
        assert_eq!(stroke(Key::Char('v'), Some(&cyrillic), Under::Shortcut), Stroke::Code(9, false));
        assert_eq!(stroke(Key::Char('v'), Some(&cyrillic), Under::Nothing), Stroke::Unicode('v'));
        assert_eq!(stroke(Key::Char('\u{0430}'), Some(&cyrillic), Under::Nothing), Stroke::Code(0, false));
        assert_eq!(stroke(Key::Char('€'), Some(&qwertz), Under::Command), Stroke::Unreachable('€'));
    }

    #[test]
    fn keys_map_to_mac_virtual_keycodes() {
        assert_eq!(stroke(Key::Named(Named::Return), None, Under::Nothing), Stroke::Code(36, false));
        assert_eq!(stroke(Key::Named(Named::Escape), None, Under::Nothing), Stroke::Code(53, false));
        assert_eq!(stroke(Key::Named(Named::Super), None, Under::Nothing), Stroke::Code(55, false));
        assert_eq!(stroke(Key::Named(Named::F(1)), None, Under::Nothing), Stroke::Code(122, false));
        assert_eq!(stroke(Key::Named(Named::F(12)), None, Under::Nothing), Stroke::Code(111, false));
        assert_eq!(stroke(Key::Named(Named::F(20)), None, Under::Nothing), Stroke::Code(90, false));
        assert_eq!(stroke(Key::Named(Named::F(21)), None, Under::Nothing), Stroke::Missing);
        assert_eq!(stroke(Key::Char('a'), None, Under::Nothing), Stroke::Code(0, false));
        assert_eq!(stroke(Key::Char('A'), None, Under::Nothing), Stroke::Code(0, true));
        assert_eq!(stroke(Key::Char('0'), None, Under::Nothing), Stroke::Code(29, false));
        assert_eq!(stroke(Key::Char('?'), None, Under::Nothing), Stroke::Code(44, true));
        assert_eq!(stroke(Key::Char('€'), None, Under::Nothing), Stroke::Unicode('€'));
    }
}
