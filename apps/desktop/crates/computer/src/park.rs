//! EXP-1235: the second pointer cua drives X11 windows with, out of sight.
//!
//! Background input on X11 rides a dedicated XInput master pointer (MPX, one
//! per session, named `CUA v1.…`) so the person keeps their own. The X server
//! creates a master in the centre of the root window, which on side-by-side
//! monitors is the seam between them, and renders its sprite like any cursor:
//! a second arrow flickers there before the first gesture and sits on the
//! last click after it. cua neither hides nor parks it.
//!
//! So after every tool call that is not a hover or a held button, every CUA
//! master pointer gets a blank per-device cursor on the root window
//! (`XIDefineCursor`: nothing renders wherever no window defines its own) and
//! is warped to the root's far corner, where an arrow's hotspot leaves its
//! image off-screen even over a window that does. Idempotent, two round
//! trips on the local socket, and skipped on Wayland (libei, no MPX).

use std::ffi::CStr;
use std::ptr;

use x11::xinput2 as xi;
use x11::xlib;

/// Tools after which the virtual pointer must stay where the call left it:
/// a hover (`move_cursor`) is the point of the call, and a held button
/// (`mouse_button_down` … `mouse_drag` … `mouse_button_up`) drags from there.
const KEEPS_POINTER: &[&str] = &["move_cursor", "mouse_button_down", "mouse_drag", "parallel_mouse_drag"];

/// cua names its master pair after the owning process (`mpx_owner.rs`):
/// `CUA v1.<owner>` for the pointer, `… keyboard` for the keyboard.
const MASTER_PREFIX: &str = "CUA v1.";

/// Whether the virtual pointer parks once `tool` has returned.
pub(crate) fn parks_after(tool: &str) -> bool {
    !KEEPS_POINTER.contains(&tool)
}

/// Whether this process talks to an X server directly (the MPX path); under
/// a Wayland compositor cua injects through libei and owns no master pointer.
fn on_x11() -> bool {
    std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

/// Park every CUA master pointer; errors only log, nothing here may fail a
/// tool call that already succeeded.
pub(crate) fn park_virtual_pointers() {
    if !on_x11() {
        return;
    }
    match park() {
        Ok(0) => {}
        Ok(parked) => log::debug!("[computer] parked {parked} virtual pointer(s)"),
        Err(err) => log::warn!("[computer] parking the virtual pointer: {err}"),
    }
}

/// Ignore X errors while we hold the display: a device that vanished between
/// the query and the warp is a BadDevice, not a reason to abort the worker
/// (Xlib's default handler exits the process).
unsafe extern "C" fn ignore_x_error(
    _display: *mut xlib::Display,
    _error: *mut xlib::XErrorEvent,
) -> std::os::raw::c_int {
    0
}

fn park() -> Result<usize, String> {
    // SAFETY: plain Xlib/XI2 calls on a display this function opens and
    // closes itself; every pointer handed back by the server is freed with
    // the matching X call before the display closes.
    unsafe {
        let display = xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("XOpenDisplay failed".to_owned());
        }
        let previous_handler = xlib::XSetErrorHandler(Some(ignore_x_error));
        let result = park_on(display);
        xlib::XSync(display, 0);
        xlib::XSetErrorHandler(previous_handler);
        xlib::XCloseDisplay(display);
        result
    }
}

/// Blank-cursor and warp every CUA master pointer on an open `display`.
unsafe fn park_on(display: *mut xlib::Display) -> Result<usize, String> {
    let (mut major, mut minor) = (2, 0);
    if xi::XIQueryVersion(display, &mut major, &mut minor) != 0 {
        return Err("XInput 2 unavailable".to_owned());
    }
    let root = xlib::XDefaultRootWindow(display);
    let screen = xlib::XDefaultScreen(display);
    let corner = (
        f64::from(xlib::XDisplayWidth(display, screen) - 1),
        f64::from(xlib::XDisplayHeight(display, screen) - 1),
    );

    let mut count = 0;
    let devices = xi::XIQueryDevice(display, xi::XIAllMasterDevices, &mut count);
    if devices.is_null() {
        return Err("XIQueryDevice failed".to_owned());
    }
    let masters: Vec<i32> = (0..count as usize)
        .map(|index| *devices.add(index))
        .filter(|device| device._use == xi::XIMasterPointer && !device.name.is_null())
        .filter(|device| CStr::from_ptr(device.name).to_string_lossy().starts_with(MASTER_PREFIX))
        .map(|device| device.deviceid)
        .collect();
    xi::XIFreeDeviceInfo(devices);
    if masters.is_empty() {
        return Ok(0);
    }

    // One transparent 1×1 pixmap doubles as the cursor's shape and mask.
    let bit: [std::os::raw::c_char; 1] = [0];
    let pixmap = xlib::XCreateBitmapFromData(display, root, bit.as_ptr(), 1, 1);
    let mut colour: xlib::XColor = std::mem::zeroed();
    let blank = xlib::XCreatePixmapCursor(display, pixmap, pixmap, &mut colour, &mut colour, 0, 0);
    for &device in &masters {
        xi::XIDefineCursor(display, device, root, blank);
        xi::XIWarpPointer(display, device, 0, root, 0.0, 0.0, 0, 0, corner.0, corner.1);
    }
    // The root window keeps the cursor referenced; the pixmap and our handle
    // can go.
    xlib::XFreeCursor(display, blank);
    xlib::XFreePixmap(display, pixmap);
    Ok(masters.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_moving_tools_park_and_hovers_do_not() {
        for tool in ["click", "double_click", "right_click", "drag", "scroll", "type_text", "hotkey", "set_value"] {
            assert!(parks_after(tool), "{tool} should park");
        }
        for tool in KEEPS_POINTER {
            assert!(!parks_after(tool), "{tool} should keep the pointer");
        }
    }

    #[test]
    fn read_only_tools_park_too_because_parking_is_idempotent() {
        assert!(parks_after("get_window_state"));
        assert!(parks_after("verify_state"));
    }
}
