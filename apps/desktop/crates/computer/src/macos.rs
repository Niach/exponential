//! What only macOS can answer for [`crate::desktop`]: the window list with
//! its layers, the focused app, the hardware idle clock, the two privacy
//! permissions, the keyboard layout and the Accessibility tree.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Duration;

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::display::CGRect;
use core_graphics::geometry::{CGPoint, CGSize};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly,
};

use crate::backend::*;
use crate::guard::UI_NODES_MAX;

type AXUIElementRef = *const c_void;
const AX_OK: i32 = 0;
const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;
const HID_SYSTEM_STATE: i32 = 1;
const ANY_INPUT_EVENT: u32 = u32::MAX;
const UI_DEPTH_MAX: usize = 14;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    /// Private but stable since 10.x: an AX window's CGWindowID.
    fn _AXUIElementGetWindow(element: AXUIElementRef, window: *mut u32) -> i32;
}

/// The HID-state clock only moves on hardware events: our own CGEvents never
/// read as the person.
pub const IDLE_COUNTS_INJECTED: bool = false;

pub fn idle() -> Option<Duration> {
    let seconds =
        unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) };
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// EXP-1196: `(screen_recording, accessibility)` granted, read without
/// prompting (the device doctor's permission rows).
pub fn permissions() -> (bool, bool) {
    unsafe { (CGPreflightScreenCaptureAccess(), AXIsProcessTrusted()) }
}

pub fn readiness(prompt: bool) -> Readiness {
    let screen = unsafe { CGPreflightScreenCaptureAccess() };
    let accessibility = unsafe { AXIsProcessTrusted() };
    if screen && accessibility {
        return Readiness::Ready;
    }
    if prompt {
        unsafe {
            if !screen {
                CGRequestScreenCaptureAccess();
            }
            if !accessibility {
                let options = CFDictionary::from_CFType_pairs(&[(
                    CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt),
                    CFBoolean::true_value(),
                )]);
                AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
            }
        }
    }
    let missing = match (screen, accessibility) {
        (false, false) => "Screen Recording and Accessibility",
        (false, true) => "Screen Recording",
        _ => "Accessibility",
    };
    Readiness::MissingPermission(format!(
        "Computer use needs the {missing} permission for Exponential on this Mac. The person \
         grants it in System Settings > Privacy & Security, then restarts Exponential. Tell \
         them; you cannot grant it yourself."
    ))
}

// ---------------------------------------------------------------------------
// keyboard layout
// ---------------------------------------------------------------------------

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    fn LMGetKbdType() -> u8;
    #[allow(clippy::too_many_arguments)]
    fn UCKeyTranslate(
        layout: *const u8,
        key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        actual_length: *mut usize,
        string: *mut u16,
    ) -> i32;
}

/// Where a character sits on the keyboard: its virtual keycode and whether
/// it needs Shift.
pub type KeySpot = (u16, bool);

/// The current layout's character → key map, as last read on the main
/// thread ([`refresh_key_layout`]). `None` until then.
static KEY_LAYOUT: Mutex<Option<HashMap<char, KeySpot>>> = Mutex::new(None);

/// Read the current keyboard layout into [`KEY_LAYOUT`]. MUST run on the
/// main thread, and does nothing anywhere else: the Text Input Source calls
/// assert the main queue inside an app and take the whole process down from
/// a worker thread (which is why a chord's character key is never resolved
/// through enigo, whose lookup makes exactly those calls at press time).
pub fn refresh_key_layout() {
    if unsafe { libc::pthread_main_np() } == 0 {
        return;
    }
    let mut map: HashMap<char, KeySpot> = HashMap::new();
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return;
        }
        let source = CFType::wrap_under_create_rule(source);
        let data =
            TISGetInputSourceProperty(source.as_CFTypeRef(), kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return;
        }
        let data = CFData::wrap_under_get_rule(data.cast());
        let layout = data.bytes().as_ptr();
        let keyboard = u32::from(LMGetKbdType());
        // Unshifted first, so a character on both levels maps to its plain key.
        for (shift, modifiers) in [(false, 0u32), (true, 2u32)] {
            for keycode in 0..128u16 {
                let mut dead_keys = 0u32;
                let mut length = 0usize;
                let mut chars = [0u16; 4];
                // Action 0 = key down; option 1 = no dead-key processing.
                let status = UCKeyTranslate(
                    layout,
                    keycode,
                    0,
                    modifiers,
                    keyboard,
                    1,
                    &mut dead_keys,
                    chars.len(),
                    &mut length,
                    chars.as_mut_ptr(),
                );
                if status != 0 || length != 1 {
                    continue;
                }
                let Some(c) = char::from_u32(u32::from(chars[0])).filter(|c| !c.is_control())
                else {
                    continue;
                };
                map.entry(c).or_insert((keycode, shift));
            }
        }
    }
    if !map.is_empty() {
        *KEY_LAYOUT.lock().unwrap() = Some(map);
    }
}

/// The US ANSI positions, for a host that never read its layout.
const ANSI_KEYS: &[(char, u16)] = &[
    ('a', 0), ('s', 1), ('d', 2), ('f', 3), ('h', 4), ('g', 5), ('z', 6), ('x', 7), ('c', 8),
    ('v', 9), ('b', 11), ('q', 12), ('w', 13), ('e', 14), ('r', 15), ('y', 16), ('t', 17),
    ('1', 18), ('2', 19), ('3', 20), ('4', 21), ('6', 22), ('5', 23), ('=', 24), ('9', 25),
    ('7', 26), ('-', 27), ('8', 28), ('0', 29), (']', 30), ('o', 31), ('u', 32), ('[', 33),
    ('i', 34), ('p', 35), ('l', 37), ('j', 38), ('\'', 39), ('k', 40), (';', 41), ('\\', 42),
    (',', 43), ('/', 44), ('n', 45), ('m', 46), ('.', 47), ('`', 50),
];

/// The key that produces `c` in the layout last read, else its US position.
pub fn key_spot(c: char) -> Option<KeySpot> {
    match KEY_LAYOUT.lock().unwrap().as_ref() {
        Some(layout) => layout.get(&c).copied(),
        None => ANSI_KEYS.iter().find(|(key, _)| *key == c).map(|(_, code)| (*code, false)),
    }
}

// ---------------------------------------------------------------------------
// windows
// ---------------------------------------------------------------------------

fn exe_name(pid: u32) -> String {
    let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32)
    };
    if length <= 0 {
        return String::new();
    }
    let path = String::from_utf8_lossy(&buffer[..length as usize]).into_owned();
    path.rsplit('/').next().unwrap_or_default().to_string()
}

/// The pid of the app that has keyboard focus (needs Accessibility).
pub fn focused_pid() -> Option<u32> {
    unsafe {
        let system = Element::owned(AXUIElementCreateSystemWide())?;
        let app = system.attribute("AXFocusedApplication")?;
        let mut pid = 0;
        (AXUIElementGetPid(app.raw(), &mut pid) == AX_OK && pid > 0).then_some(pid as u32)
    }
}

/// The CGWindowID of `pid`'s key window (its `AXFocusedWindow`).
pub fn key_window_of(pid: u32) -> Option<u32> {
    let window = app_element(pid).ok()?.attribute("AXFocusedWindow")?;
    let mut id = 0u32;
    (unsafe { _AXUIElementGetWindow(window.raw(), &mut id) } == AX_OK && id != 0).then_some(id)
}

pub fn windows() -> BackendResult<Vec<WindowInfo>> {
    let list = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )
    .ok_or("Could not list the windows.")?;
    let focused_pid = focused_pid();
    let mut windows = Vec::new();
    let mut focus_given = false;
    for item in list.iter() {
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let number = |key: &'static str| {
            dict.find(CFString::from_static_string(key))
                .and_then(|value| value.downcast::<CFNumber>())
                .and_then(|number| number.to_i64())
        };
        let string = |key: &'static str| {
            dict.find(CFString::from_static_string(key))
                .and_then(|value| value.downcast::<CFString>())
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
        let (Some(id), Some(pid)) = (number("kCGWindowNumber"), number("kCGWindowOwnerPID")) else {
            continue;
        };
        let Some(bounds) = dict
            .find(CFString::from_static_string("kCGWindowBounds"))
            .and_then(|value| value.downcast::<CFDictionary>())
            .and_then(|bounds| CGRect::from_dict_representation(&bounds))
        else {
            continue;
        };
        // Slivers (status items, shadows) are nothing to click into.
        if bounds.size.width < 24.0 || bounds.size.height < 24.0 {
            continue;
        }
        let layer = number("kCGWindowLayer").unwrap_or(0) as i32;
        let pid = pid as u32;
        // Front to back: the focused app's first ordinary window has focus.
        let focused = !focus_given
            && layer == 0
            && focused_pid.map_or(windows.iter().all(|w: &WindowInfo| w.layer != 0), |f| f == pid);
        focus_given |= focused;
        windows.push(WindowInfo {
            id: id as u32,
            pid,
            app: string("kCGWindowOwnerName"),
            exe: exe_name(pid),
            title: string("kCGWindowName"),
            rect: Rect {
                x: bounds.origin.x,
                y: bounds.origin.y,
                width: bounds.size.width,
                height: bounds.size.height,
            },
            focused,
            layer,
        });
    }
    Ok(windows)
}

/// The app the keyboard goes to, even when it shows no ordinary window (a
/// system prompt): named by pid, so the answer can name it.
pub fn focused() -> Option<WindowInfo> {
    let all = windows().ok()?;
    let Some(pid) = focused_pid() else {
        return all.into_iter().find(|window| window.focused);
    };
    all.iter()
        .find(|window| window.pid == pid && window.layer == 0)
        .or_else(|| all.iter().find(|window| window.pid == pid))
        .cloned()
        .or_else(|| {
            let exe = exe_name(pid);
            Some(WindowInfo {
                id: 0,
                pid,
                app: exe.clone(),
                exe,
                title: String::new(),
                rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
                focused: true,
                layer: 0,
            })
        })
}

// ---------------------------------------------------------------------------
// accessibility
// ---------------------------------------------------------------------------

/// An owned AX element or attribute value.
struct Element(CFType);

impl Element {
    /// Take ownership of a +1 reference.
    unsafe fn owned(reference: AXUIElementRef) -> Option<Self> {
        (!reference.is_null()).then(|| Self(CFType::wrap_under_create_rule(reference)))
    }

    fn raw(&self) -> CFTypeRef {
        self.0.as_CFTypeRef()
    }

    fn attribute(&self, name: &str) -> Option<Element> {
        let key = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        unsafe {
            let status = AXUIElementCopyAttributeValue(
                self.raw(),
                key.as_concrete_TypeRef(),
                &mut value,
            );
            if status != AX_OK {
                return None;
            }
            Element::owned(value)
        }
    }

    fn text(&self, name: &str) -> String {
        self.attribute(name)
            .and_then(|value| value.0.downcast::<CFString>())
            .map(|text| text.to_string())
            .unwrap_or_default()
    }

    fn children(&self, name: &str) -> Vec<Element> {
        let Some(array) = self.attribute(name).and_then(|value| value.0.downcast::<CFArray>())
        else {
            return Vec::new();
        };
        array
            .iter()
            .filter(|item| !item.is_null())
            .map(|item| Element(unsafe { CFType::wrap_under_get_rule(*item) }))
            .collect()
    }

    fn rect(&self) -> Option<Rect> {
        let position = self.attribute("AXPosition")?;
        let size = self.attribute("AXSize")?;
        let mut point = CGPoint::new(0.0, 0.0);
        let mut extent = CGSize::new(0.0, 0.0);
        unsafe {
            let ok = AXValueGetValue(
                position.raw(),
                AX_VALUE_CGPOINT,
                (&mut point as *mut CGPoint).cast(),
            ) && AXValueGetValue(
                size.raw(),
                AX_VALUE_CGSIZE,
                (&mut extent as *mut CGSize).cast(),
            );
            ok.then_some(Rect { x: point.x, y: point.y, width: extent.width, height: extent.height })
        }
    }
}

fn app_element(pid: u32) -> BackendResult<Element> {
    let app = unsafe { Element::owned(AXUIElementCreateApplication(pid as i32)) }
        .ok_or("Could not reach the app's accessibility interface.")?;
    // A hung app must not hang the tool.
    unsafe { AXUIElementSetMessagingTimeout(app.raw(), 1.5) };
    Ok(app)
}

/// The AX window of `info`'s app that carries its title (its first one when
/// titles do not tell them apart).
fn ax_window(app: &Element, info: &WindowInfo) -> Option<Element> {
    let mut windows = app.children("AXWindows");
    let by_title = windows.iter().position(|window| window.text("AXTitle") == info.title);
    match by_title {
        Some(index) => Some(windows.swap_remove(index)),
        None if windows.is_empty() => None,
        None => Some(windows.swap_remove(0)),
    }
}

pub fn focus_window(id: u32) -> BackendResult<()> {
    let info = windows()?
        .into_iter()
        .find(|window| window.id == id)
        .ok_or_else(|| format!("No window has id {id}; call list_windows."))?;
    let app = app_element(info.pid)?;
    unsafe {
        if let Some(window) = ax_window(&app, &info) {
            AXUIElementPerformAction(
                window.raw(),
                CFString::from_static_string("AXRaise").as_concrete_TypeRef(),
            );
            AXUIElementSetAttributeValue(
                window.raw(),
                CFString::from_static_string("AXMain").as_concrete_TypeRef(),
                CFBoolean::true_value().as_CFTypeRef(),
            );
        }
        let status = AXUIElementSetAttributeValue(
            app.raw(),
            CFString::from_static_string("AXFrontmost").as_concrete_TypeRef(),
            CFBoolean::true_value().as_CFTypeRef(),
        );
        if status != AX_OK {
            return Err(format!("{} did not come to the front (AX error {status}).", info.app));
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    Ok(())
}

fn walk(element: &Element, depth: usize, nodes: &mut Vec<UiNode>) {
    if nodes.len() > UI_NODES_MAX || depth > UI_DEPTH_MAX {
        return;
    }
    let role = element.text("AXRole");
    let label = ["AXTitle", "AXValue", "AXDescription", "AXPlaceholderValue"]
        .iter()
        .map(|name| element.text(name))
        .find(|text| !text.trim().is_empty())
        .unwrap_or_default();
    let children = element.children("AXChildren");
    // A bare group is structure, not content: skip the line, keep the depth.
    let structural = label.is_empty() && matches!(role.as_str(), "AXGroup" | "AXUnknown" | "");
    let child_depth = if structural { depth } else { depth + 1 };
    if !structural {
        nodes.push(UiNode {
            depth,
            role: role.strip_prefix("AX").unwrap_or(&role).to_lowercase(),
            label,
            rect: element.rect(),
        });
    }
    for child in &children {
        walk(child, child_depth, nodes);
    }
}

pub fn read_ui(window: Option<u32>) -> BackendResult<Vec<UiNode>> {
    let info = match window {
        Some(id) => windows()?
            .into_iter()
            .find(|window| window.id == id)
            .ok_or_else(|| format!("No window has id {id}; call list_windows."))?,
        None => focused().ok_or("No window has focus; pass a window id from list_windows.")?,
    };
    let app = app_element(info.pid)?;
    let root = match window {
        Some(_) => ax_window(&app, &info),
        None => app.attribute("AXFocusedWindow").or_else(|| ax_window(&app, &info)),
    }
    .ok_or_else(|| format!("{} exposes no accessibility window.", info.app))?;
    let mut nodes = Vec::new();
    walk(&root, 0, &mut nodes);
    Ok(nodes)
}
