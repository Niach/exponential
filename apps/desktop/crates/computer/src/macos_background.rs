//! BACKGROUND input on macOS (EXP-1196 spike): events posted to ONE process
//! instead of the global HID queue, so the person's cursor stays where it is
//! and the app they are using stays in front.
//!
//! The recipe is cua-driver's (trycua/cua, `libs/cua-driver/rust/crates/
//! platform-macos/src/input/{skylight,keyboard,mouse}.rs` at rev b0968e1),
//! cut down to what our tools need and called synchronously. MIT, Copyright
//! (c) 2025 Cua AI, Inc.; the focus-without-raise record sequence is
//! yabai's (MIT, Copyright (c) 2019 Åsmund Vikane). Private SkyLight symbols
//! resolve once through `dlsym`; when one is missing the public
//! `CGEventPostToPid` carries the event instead.
//!
//! What we do NOT take from cua (its SDK does it, we do not): the check that
//! the target window is on the current Space, the window-change detector that
//! notices a click opened a sheet or another app came forward, focus-steal
//! suppression while the event lands, and the foreground fallback when a
//! background post cannot work. The answer says "posted", not "landed".
//!
//! Where we differ from cua, measured on TextEdit (macOS 27): the stamped
//! window location is WINDOW-LOCAL (cua's screen point lands the caret at the
//! end of the text), and a command chord goes through [`shortcut`] (key
//! without raise + unauthenticated post) because a plain pid post never
//! reaches the menu: `cmd+s` did nothing, and AXPress on File > Save failed
//! too (the item is disabled while the app has no key window).

use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::OnceLock;
use std::time::Duration;

use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use foreign_types::ForeignType;

use crate::backend::{BackendResult, Button, Chord, Key, Modifier};

// ---------------------------------------------------------------------------
// SkyLight symbols
// ---------------------------------------------------------------------------

type PostToPidFn = unsafe extern "C" fn(libc::pid_t, *mut c_void);
type SetAuthMessageFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
type SetWindowLocationFn = unsafe extern "C" fn(*mut c_void, f64, f64);
type SetIntegerFieldFn = unsafe extern "C" fn(*mut c_void, u32, i64);
type ConnectionIdFn = unsafe extern "C" fn() -> u32;
type GetWindowOwnerFn = unsafe extern "C" fn(u32, u32, *mut u32) -> i32;
type GetConnectionPsnFn = unsafe extern "C" fn(u32, *mut c_void) -> i32;
type PostEventRecordFn = unsafe extern "C" fn(*const c_void, *const u8) -> i32;
type GetFrontProcessFn = unsafe extern "C" fn(*mut c_void) -> i32;
type GetProcessForPidFn = unsafe extern "C" fn(libc::pid_t, *mut c_void) -> i32;
type ObjcLookupFn = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type RespondsToFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
/// `+[SLSEventAuthenticationMessage messageWithEventRecord:pid:version:]`.
type AuthFactoryFn =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, c_int, c_uint) -> *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// Public (10.11+); core-graphics hides it behind a feature.
    fn CGEventPostToPid(pid: libc::pid_t, event: *mut c_void);
}

fn symbol(name: &CStr) -> Option<*mut c_void> {
    static LOADED: OnceLock<()> = OnceLock::new();
    LOADED.get_or_init(|| unsafe {
        libc::dlopen(
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_GLOBAL,
        );
    });
    let pointer = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    (!pointer.is_null()).then_some(pointer)
}

/// One lazily resolved symbol as a typed function pointer.
macro_rules! spi {
    ($fn_name:ident, $ty:ty, $symbol:literal) => {
        fn $fn_name() -> Option<$ty> {
            static SYMBOL: OnceLock<Option<usize>> = OnceLock::new();
            SYMBOL
                .get_or_init(|| symbol($symbol).map(|pointer| pointer as usize))
                // SAFETY: the symbol has this C signature on every macOS that
                // exports it (cua-driver's and yabai's reading).
                .map(|address| unsafe { std::mem::transmute::<usize, $ty>(address) })
        }
    };
}

spi!(post_to_pid_fn, PostToPidFn, c"SLEventPostToPid");
spi!(set_auth_message_fn, SetAuthMessageFn, c"SLEventSetAuthenticationMessage");
spi!(set_window_location_fn, SetWindowLocationFn, c"CGEventSetWindowLocation");
spi!(set_integer_field_fn, SetIntegerFieldFn, c"SLEventSetIntegerValueField");
spi!(connection_id_fn, ConnectionIdFn, c"CGSMainConnectionID");
spi!(get_window_owner_fn, GetWindowOwnerFn, c"SLSGetWindowOwner");
spi!(get_connection_psn_fn, GetConnectionPsnFn, c"SLSGetConnectionPSN");
spi!(post_event_record_fn, PostEventRecordFn, c"SLPSPostEventRecordTo");
spi!(get_front_process_fn, GetFrontProcessFn, c"_SLPSGetFrontProcess");
spi!(get_process_for_pid_fn, GetProcessForPidFn, c"GetProcessForPID");
spi!(objc_get_class_fn, ObjcLookupFn, c"objc_getClass");
spi!(sel_register_fn, ObjcLookupFn, c"sel_registerName");
spi!(responds_to_fn, RespondsToFn, c"class_respondsToSelector");
spi!(auth_factory_fn, AuthFactoryFn, c"objc_msgSend");

/// The `SLSEventRecord *` inside a `CGEvent` (`{CFRuntimeBase, u32,
/// record *}`: offset 24 on 64-bit; 32 and 16 tried as cua does).
unsafe fn event_record(event: *mut c_void) -> *mut c_void {
    for offset in [24usize, 32, 16] {
        let slot = (event as *const u8).add(offset).cast::<*mut c_void>();
        let record = std::ptr::read_unaligned(slot);
        if !record.is_null() {
            return record;
        }
    }
    std::ptr::null_mut()
}

/// Post `event` to `pid`. `authenticate` attaches the
/// `SLSEventAuthenticationMessage` WindowServer wants before Chromium-class
/// apps accept a synthetic KEY event (macOS 15+; skipped on 14, where the
/// factory selector does not exist). Without it the post takes the
/// IOHIDPostEvent path, the one NSMenu key equivalents are dispatched from.
fn post(pid: u32, event: &CGEvent, authenticate: bool) {
    let pointer = event.as_ptr().cast::<c_void>();
    let Some(post_fn) = post_to_pid_fn() else {
        unsafe { CGEventPostToPid(pid as libc::pid_t, pointer) };
        return;
    };
    if authenticate {
        attach_auth_message(pid, pointer);
    }
    unsafe { post_fn(pid as libc::pid_t, pointer) };
}

fn attach_auth_message(pid: u32, event: *mut c_void) {
    let (Some(get_class), Some(register), Some(responds), Some(factory), Some(set_auth)) = (
        objc_get_class_fn(),
        sel_register_fn(),
        responds_to_fn(),
        auth_factory_fn(),
        set_auth_message_fn(),
    ) else {
        return;
    };
    unsafe {
        let class = get_class(c"SLSEventAuthenticationMessage".as_ptr());
        let selector = register(c"messageWithEventRecord:pid:version:".as_ptr());
        if class.is_null() || selector.is_null() || !responds(class, selector) {
            return;
        }
        let record = event_record(event);
        if record.is_null() {
            return;
        }
        let message = factory(class, selector, record, pid as c_int, 0);
        if !message.is_null() {
            set_auth(event, message);
        }
    }
}

fn set_field(event: &CGEvent, field: u32, value: i64) {
    match set_integer_field_fn() {
        Some(set) => unsafe { set(event.as_ptr().cast(), field, value) },
        None => event.set_integer_value_field(field, value),
    }
}

fn set_window_location(event: &CGEvent, x: f64, y: f64) {
    if let Some(set) = set_window_location_fn() {
        unsafe { set(event.as_ptr().cast(), x, y) };
    }
}

fn source() -> BackendResult<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Could not create an input event source.".to_string())
}

fn gesture_id() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as i64
}

// ---------------------------------------------------------------------------
// focus without raise (yabai's record)
// ---------------------------------------------------------------------------

type Psn = [u8; 8];

fn psn_of_window(window: u32, pid: u32) -> Option<Psn> {
    let mut psn = [0u8; 8];
    if let (Some(owner), Some(connection_psn), Some(connection)) =
        (get_window_owner_fn(), get_connection_psn_fn(), connection_id_fn())
    {
        let mut owner_connection = 0u32;
        unsafe {
            if owner(connection(), window, &mut owner_connection) == 0
                && owner_connection != 0
                && connection_psn(owner_connection, psn.as_mut_ptr().cast()) == 0
            {
                return Some(psn);
            }
        }
    }
    let by_pid = get_process_for_pid_fn()?;
    (unsafe { by_pid(pid as libc::pid_t, psn.as_mut_ptr().cast()) } == 0).then_some(psn)
}

/// The 248-byte focus (`0x01`) / defocus (`0x02`) record for `window`.
fn focus_record(window: u32, focus: bool) -> [u8; 0xF8] {
    let mut record = [0u8; 0xF8];
    record[0x04] = 0xF8;
    record[0x08] = 0x0D;
    record[0x3C..0x40].copy_from_slice(&window.to_le_bytes());
    record[0x8A] = if focus { 0x01 } else { 0x02 };
    record
}

/// Make `window` AppKit-key WITHOUT raising it or switching Spaces: defocus
/// the front process, focus the target. Clicks into a background window
/// need it (AppKit drops a mouseDown on an inactive window's controls).
fn activate_without_raise(pid: u32, window: u32) -> bool {
    let (Some(post_record), Some(front_process)) = (post_event_record_fn(), get_front_process_fn())
    else {
        return false;
    };
    let mut front = [0u8; 8];
    if unsafe { front_process(front.as_mut_ptr().cast()) } != 0 {
        return false;
    }
    let Some(target) = psn_of_window(window, pid) else { return false };
    unsafe {
        let defocused = post_record(front.as_ptr().cast(), focus_record(window, false).as_ptr()) == 0;
        let focused = post_record(target.as_ptr().cast(), focus_record(window, true).as_ptr()) == 0;
        defocused && focused
    }
}

/// Undo [`activate_without_raise`]: the person's app stayed in front, but its
/// key window got a defocus record and would swallow no more typing until
/// clicked. Hand key focus back to it.
fn restore_focus(previous_pid: u32, previous_window: u32, pid: u32, window: u32) -> bool {
    let Some(post_record) = post_event_record_fn() else { return false };
    let (Some(previous), Some(target)) =
        (psn_of_window(previous_window, previous_pid), psn_of_window(window, pid))
    else {
        return false;
    };
    unsafe {
        let defocused = post_record(target.as_ptr().cast(), focus_record(window, false).as_ptr()) == 0;
        let focused =
            post_record(previous.as_ptr().cast(), focus_record(previous_window, true).as_ptr()) == 0;
        defocused && focused
    }
}

// ---------------------------------------------------------------------------
// mouse
// ---------------------------------------------------------------------------

/// Stamp the window-routing fields on a mouse event.
fn stamp_mouse(event: &CGEvent, pid: u32, window: u32, phase: i64, click_state: i64, button: i64, gesture: i64) {
    set_field(event, 0, phase); // gesture phase
    set_field(event, 1, click_state); // kCGMouseEventClickState
    set_field(event, 3, button); // kCGMouseEventButtonNumber
    set_field(event, 7, 3); // kCGMouseEventSubtype = NSEventSubtypeTouch
    set_field(event, 40, i64::from(pid)); // the target pid (Chromium's filter)
    set_field(event, 51, i64::from(window)); // windowNumber
    set_field(event, 58, gesture); // click group: one gesture
    set_field(event, 91, i64::from(window)); // window under the pointer
    set_field(event, 92, i64::from(window)); // ... that can handle the event
}

/// A click at screen point (x, y) delivered to `window` of `pid` (its
/// top-left at screen point `origin`), with the hardware pointer untouched. `previous` = the app the person is in (its
/// pid and key window), handed its keyboard focus back afterwards.
#[allow(clippy::too_many_arguments)]
pub fn click(
    pid: u32,
    window: u32,
    x: f64,
    y: f64,
    button: Button,
    count: u8,
    previous: Option<(u32, u32)>,
    origin: (f64, f64),
) -> BackendResult<()> {
    let activated = activate_without_raise(pid, window);
    // Let AppKit route key/active state before the events arrive.
    std::thread::sleep(Duration::from_millis(50));
    let source = source()?;
    let gesture = gesture_id();
    let target = CGPoint::new(x, y);
    let event = |kind: CGEventType, at: CGPoint, mouse_button: CGMouseButton| {
        CGEvent::new_mouse_event(source.clone(), kind, at, mouse_button)
            .map_err(|()| "Could not create a mouse event.".to_string())
    };
    let send = |event: &CGEvent, at: CGPoint| {
        // WINDOW-LOCAL, measured on TextEdit (macOS 27): the screen point
        // cua-driver stamps put the caret at the end of the text, not under
        // the click.
        set_window_location(event, at.x - origin.0, at.y - origin.1);
        post(pid, event, false);
    };
    // A move first: a backgrounded window's hit-testing state is stale.
    let moved = event(CGEventType::MouseMoved, target, CGMouseButton::Left)?;
    stamp_mouse(&moved, pid, window, 2, 0, 0, gesture);
    send(&moved, target);
    std::thread::sleep(Duration::from_millis(15));
    let (down, up, mouse_button, number) = match button {
        Button::Left => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp, CGMouseButton::Left, 0),
        Button::Right => (CGEventType::RightMouseDown, CGEventType::RightMouseUp, CGMouseButton::Right, 1),
        Button::Middle => (CGEventType::OtherMouseDown, CGEventType::OtherMouseUp, CGMouseButton::Center, 2),
    };
    if button == Button::Left {
        // An off-screen primer pair opens Chromium's user-activation gate
        // without hitting anything.
        let off = CGPoint::new(-1.0, -1.0);
        let primer_down = event(down, off, mouse_button)?;
        stamp_mouse(&primer_down, pid, window, 1, 1, number, gesture);
        send(&primer_down, off);
        std::thread::sleep(Duration::from_millis(1));
        let primer_up = event(up, off, mouse_button)?;
        stamp_mouse(&primer_up, pid, window, 2, 1, number, gesture);
        send(&primer_up, off);
        std::thread::sleep(Duration::from_millis(100));
    }
    for pair in 1..=i64::from(count.max(1)) {
        let pressed = event(down, target, mouse_button)?;
        stamp_mouse(&pressed, pid, window, 3, pair, number, gesture);
        send(&pressed, target);
        // An NSButton's tracking loop polls for the up; too tight a gap races it.
        std::thread::sleep(Duration::from_millis(28));
        let released = event(up, target, mouse_button)?;
        stamp_mouse(&released, pid, window, 3, pair, number, gesture);
        send(&released, target);
        if pair < i64::from(count) {
            std::thread::sleep(Duration::from_millis(80));
        }
    }
    if activated {
        if let Some((previous_pid, previous_window)) = previous.filter(|(p, _)| *p != pid) {
            std::thread::sleep(Duration::from_millis(50));
            restore_focus(previous_pid, previous_window, pid, window);
        }
    }
    Ok(())
}

/// Wheel notches at screen point (x, y) delivered to `window` of `pid`
/// (positive `dy` = down, `dx` = right, like the foreground tool).
pub fn scroll(
    pid: u32,
    window: u32,
    x: f64,
    y: f64,
    dx: i32,
    dy: i32,
    origin: (f64, f64),
) -> BackendResult<()> {
    let (local_x, local_y) = (x - origin.0, y - origin.1);
    let source = source()?;
    let gesture = gesture_id();
    let at = CGPoint::new(x, y);
    let moved = CGEvent::new_mouse_event(source.clone(), CGEventType::MouseMoved, at, CGMouseButton::Left)
        .map_err(|()| "Could not create a mouse event.".to_string())?;
    stamp_mouse(&moved, pid, window, 2, 0, 0, gesture);
    set_window_location(&moved, local_x, local_y);
    post(pid, &moved, false);
    std::thread::sleep(Duration::from_millis(12));
    let notches = dy.unsigned_abs().max(dx.unsigned_abs());
    for step in 0..notches {
        // Line units: a negative wheel1 reveals content BELOW.
        let wheel_y = if (step as i32) < dy.abs() { -dy.signum() } else { 0 };
        let wheel_x = if (step as i32) < dx.abs() { -dx.signum() } else { 0 };
        let wheel = CGEvent::new_scroll_event(source.clone(), ScrollEventUnit::LINE, 2, wheel_y, wheel_x, 0)
            .map_err(|()| "Could not create a scroll event.".to_string())?;
        wheel.set_location(at);
        set_window_location(&wheel, local_x, local_y);
        set_field(&wheel, 40, i64::from(pid));
        set_field(&wheel, 51, i64::from(window));
        set_field(&wheel, 91, i64::from(window));
        set_field(&wheel, 92, i64::from(window));
        post(pid, &wheel, false);
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// keyboard
// ---------------------------------------------------------------------------

const KEY_GAP: Duration = Duration::from_millis(8);

/// Type `text` into `pid`'s focused element: one Unicode key event pair per
/// character, so the keyboard layout does not matter.
pub fn type_text(pid: u32, text: &str) -> BackendResult<()> {
    let source = source()?;
    let mut buffer = [0u16; 2];
    for c in text.chars() {
        let units = c.encode_utf16(&mut buffer);
        for down in [true, false] {
            let event = CGEvent::new_keyboard_event(source.clone(), 0, down)
                .map_err(|()| "Could not create a key event.".to_string())?;
            event.set_string_from_utf16_unchecked(units);
            // Empty flags: a held Shift must not leak into the character.
            event.set_flags(CGEventFlags::CGEventFlagNull);
            post(pid, &event, true);
            std::thread::sleep(KEY_GAP);
        }
    }
    Ok(())
}

fn modifier_key(modifier: Modifier) -> (u16, CGEventFlags) {
    match modifier {
        Modifier::Meta => (55, CGEventFlags::CGEventFlagCommand),
        Modifier::Shift => (56, CGEventFlags::CGEventFlagShift),
        Modifier::Alt => (58, CGEventFlags::CGEventFlagAlternate),
        Modifier::Control => (59, CGEventFlags::CGEventFlagControl),
    }
}

/// The virtual keycode of a named key (a character key is resolved by the
/// caller against the current layout).
pub fn named_keycode(key: Key) -> Option<u16> {
    Some(match key {
        Key::Char(_) => return None,
        Key::Enter => 36,
        Key::Tab => 48,
        Key::Space => 49,
        Key::Backspace => 51,
        Key::Escape => 53,
        Key::Delete => 117,
        Key::Home => 115,
        Key::PageUp => 116,
        Key::End => 119,
        Key::PageDown => 121,
        Key::Left => 123,
        Key::Right => 124,
        Key::Down => 125,
        Key::Up => 126,
        Key::F(n) => match n {
            1 => 122,
            2 => 120,
            3 => 99,
            4 => 118,
            5 => 96,
            6 => 97,
            7 => 98,
            8 => 100,
            9 => 101,
            10 => 109,
            11 => 103,
            _ => 111,
        },
    })
}

/// Press `keycode` with `chord`'s modifiers, all addressed to `pid`: the
/// modifiers go down and up as their own key events around it, as from a
/// real keyboard. A command chord posts WITHOUT the auth envelope so AppKit
/// dispatches it as a menu key equivalent (`cmd+s`); plain keys carry it.
/// Press `keycode` with `chord`'s modifiers, addressed to `pid`: the
/// modifiers go down and up as their own key events around it, as from a
/// real keyboard. Text navigation and editing keys; a COMMAND chord takes
/// [`shortcut`].
pub fn key(pid: u32, chord: &Chord, keycode: u16) -> BackendResult<()> {
    press(pid, chord, keycode, true)
}

/// A command chord (`cmd+s`): AppKit dispatches key equivalents to the menu
/// only in a key window, and only from the IOHIDPostEvent path (a post
/// WITHOUT the auth envelope). So: make the window key without raising it,
/// post the chord unauthenticated, let it land, hand key focus back to the
/// person's window. Their app never leaves the front.
pub fn shortcut(
    pid: u32,
    window: u32,
    chord: &Chord,
    keycode: u16,
    previous: Option<(u32, u32)>,
) -> BackendResult<()> {
    let activated = activate_without_raise(pid, window);
    std::thread::sleep(Duration::from_millis(50));
    let result = press(pid, chord, keycode, false);
    // The unauthenticated path is slower than the authenticated one: a key
    // posted right after could overtake the chord. Let it land first.
    std::thread::sleep(Duration::from_millis(300));
    if activated {
        if let Some((previous_pid, previous_window)) = previous.filter(|(p, _)| *p != pid) {
            restore_focus(previous_pid, previous_window, pid, window);
        }
    }
    result
}

fn press(pid: u32, chord: &Chord, keycode: u16, authenticate: bool) -> BackendResult<()> {
    let source = source()?;
    let send = |code: u16, down: bool, flags: CGEventFlags| -> BackendResult<()> {
        let event = CGEvent::new_keyboard_event(source.clone(), code, down)
            .map_err(|()| "Could not create a key event.".to_string())?;
        // Always set, even empty: a physically held modifier must not leak in.
        event.set_flags(flags);
        post(pid, &event, authenticate);
        std::thread::sleep(KEY_GAP);
        Ok(())
    };
    let mut flags = CGEventFlags::CGEventFlagNull;
    let mut pressed = Vec::new();
    let mut result = Ok(());
    for modifier in &chord.modifiers {
        let (code, flag) = modifier_key(*modifier);
        flags |= flag;
        result = send(code, true, flags);
        if result.is_err() {
            break;
        }
        pressed.push((code, flag));
    }
    if result.is_ok() {
        result = send(keycode, true, flags).and_then(|()| send(keycode, false, flags));
    }
    // Release what went down even when the key itself failed.
    for (code, flag) in pressed.into_iter().rev() {
        flags.remove(flag);
        let _ = send(code, false, flags);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focus_record_carries_the_window_and_the_direction() {
        let record = focus_record(0x0102_0304, true);
        assert_eq!(record.len(), 248);
        assert_eq!((record[0x04], record[0x08]), (0xF8, 0x0D));
        assert_eq!(&record[0x3C..0x40], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(record[0x8A], 0x01);
        assert_eq!(focus_record(7, false)[0x8A], 0x02);
    }

    #[test]
    fn named_keys_have_macos_keycodes() {
        assert_eq!(named_keycode(Key::Enter), Some(36));
        assert_eq!(named_keycode(Key::F(5)), Some(96));
        assert_eq!(named_keycode(Key::Char('s')), None);
    }
}
