//! EXP-1235 / EXP-1273: the second pointer cua drives X11 windows with, out
//! of sight.
//!
//! Background input on X11 rides a dedicated XInput master pointer (MPX, one
//! per session, named `CUA v1.…`) so the person keeps their own. The X server
//! creates a master in the centre of the root window, which on side-by-side
//! monitors is the seam between them, and renders its sprite like any cursor:
//! under a compositor as a software sprite that flickers on every repaint,
//! and with whatever cursor the window under it defines, so its look changes
//! as the application swaps I-beam, hand and arrow for the person's own
//! pointer. cua neither hides nor parks it.
//!
//! So after every tool call that is not a hover or a held button, every CUA
//! master pointer gets a blank per-device cursor and is warped to the root's
//! far corner. The server picks the cursor of the INNERMOST window that
//! defines one (per-device first, then the window's own), so a blank on the
//! root alone only hides the sprite over the bare desktop (EXP-1235) and it
//! showed again at every click target (EXP-1273): the blank now goes on every
//! window of the tree, which keeps the sprite invisible wherever cua warps it
//! next. The session's master keyboard loses its focus the same way
//! (`XISetFocus` None): cua focuses it on the target before every delivery,
//! and a second master keyboard parked on the person's window would change
//! which focus events that window sees. Idempotent, a few round trips on the
//! local socket, and skipped on Wayland (libei, no MPX).

use std::collections::VecDeque;
use std::ffi::CStr;
use std::ptr;
use std::sync::Mutex;

use x11::xinput2 as xi;
use x11::xlib;

/// Tools after which the virtual pointer must stay where the call left it:
/// a hover (`move_cursor`) is the point of the call, and a held button
/// (`mouse_button_down` … `mouse_drag` … `mouse_button_up`) drags from there.
const KEEPS_POINTER: &[&str] = &["move_cursor", "mouse_button_down", "mouse_drag", "parallel_mouse_drag"];

/// cua names its master pair after the owning process (`mpx_owner.rs`):
/// `CUA v1.<owner>` for the pointer, `… keyboard` for the keyboard.
const MASTER_PREFIX: &str = "CUA v1.";

/// How many windows one park blanks at most: a desktop has a few hundred;
/// a runaway tree must not turn a tool call into a long X conversation.
const WINDOW_CAP: usize = 4096;

/// Whether the virtual pointer parks once `tool` has returned.
pub(crate) fn parks_after(tool: &str) -> bool {
    !KEEPS_POINTER.contains(&tool)
}

/// Which half of a CUA master pair a device is, by its XI2 `use` and name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Master {
    Pointer,
    Keyboard,
}

/// `Some` for the master devices cua owns; slaves and the person's own
/// masters are `None`.
fn cua_master(name: &str, device_use: i32) -> Option<Master> {
    if !name.starts_with(MASTER_PREFIX) {
        return None;
    }
    match device_use {
        xi::XIMasterPointer => Some(Master::Pointer),
        xi::XIMasterKeyboard => Some(Master::Keyboard),
        _ => None,
    }
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
        Ok(Parked { pointers: 0, keyboards: 0, .. }) => {}
        Ok(parked) => log::debug!(
            "[computer] parked {} virtual pointer(s) over {} window(s), unfocused {} keyboard(s)",
            parked.pointers,
            parked.windows,
            parked.keyboards
        ),
        Err(err) => log::warn!("[computer] parking the virtual pointer: {err}"),
    }
}

/// What one park touched.
#[derive(Debug, Default, PartialEq, Eq)]
struct Parked {
    pointers: usize,
    keyboards: usize,
    windows: usize,
}

/// Ignore X errors while we hold the display: a device that vanished between
/// the query and the warp is a BadDevice, a window destroyed between the tree
/// walk and its cursor a BadWindow, not a reason to abort the worker (Xlib's
/// default handler exits the process).
unsafe extern "C" fn ignore_x_error(
    _display: *mut xlib::Display,
    _error: *mut xlib::XErrorEvent,
) -> std::os::raw::c_int {
    0
}

/// `XSetErrorHandler` is process-global and tool calls (so parks) run at
/// once: one park at a time, so a park never restores another park's
/// "previous" handler, i.e. our ignoring one, as the process default.
static PARK_LOCK: Mutex<()> = Mutex::new(());

fn park() -> Result<Parked, String> {
    let _one_at_a_time = PARK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // SAFETY: plain Xlib/XI2 calls on a display this function opens and
    // closes itself; every pointer handed back by the server is freed with
    // the matching X call before the display closes. The error handler swap
    // is serialised by `PARK_LOCK`.
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

/// Blank-cursor and warp every CUA master pointer, unfocus every CUA master
/// keyboard, on an open `display`.
unsafe fn park_on(display: *mut xlib::Display) -> Result<Parked, String> {
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
    let (mut pointers, mut keyboards) = (Vec::new(), Vec::new());
    for index in 0..count as usize {
        let device = *devices.add(index);
        if device.name.is_null() {
            continue;
        }
        let name = CStr::from_ptr(device.name).to_string_lossy();
        match cua_master(&name, device._use) {
            Some(Master::Pointer) => pointers.push(device.deviceid),
            Some(Master::Keyboard) => keyboards.push(device.deviceid),
            None => {}
        }
    }
    xi::XIFreeDeviceInfo(devices);
    if pointers.is_empty() && keyboards.is_empty() {
        return Ok(Parked::default());
    }

    let mut parked = Parked { pointers: pointers.len(), keyboards: keyboards.len(), windows: 0 };
    if !pointers.is_empty() {
        // One transparent 1×1 pixmap doubles as the cursor's shape and mask.
        let bit: [std::os::raw::c_char; 1] = [0];
        let pixmap = xlib::XCreateBitmapFromData(display, root, bit.as_ptr(), 1, 1);
        let mut colour: xlib::XColor = std::mem::zeroed();
        let blank = xlib::XCreatePixmapCursor(display, pixmap, pixmap, &mut colour, &mut colour, 0, 0);
        let windows = window_tree(display, root, WINDOW_CAP);
        parked.windows = windows.len();
        for &device in &pointers {
            for &window in &windows {
                xi::XIDefineCursor(display, device, window, blank);
            }
            xi::XIWarpPointer(display, device, 0, root, 0.0, 0.0, 0, 0, corner.0, corner.1);
        }
        // Every window keeps the cursor referenced; the pixmap and our
        // handle can go.
        xlib::XFreeCursor(display, blank);
        xlib::XFreePixmap(display, pixmap);
    }
    for &device in &keyboards {
        xi::XISetFocus(display, device, 0, xlib::CurrentTime);
    }
    Ok(parked)
}

/// The root and every window below it, breadth first, at most `cap` of them.
unsafe fn window_tree(display: *mut xlib::Display, root: xlib::Window, cap: usize) -> Vec<xlib::Window> {
    let mut windows = Vec::with_capacity(256);
    let mut queue = VecDeque::from([root]);
    while let Some(window) = queue.pop_front() {
        if windows.len() >= cap {
            break;
        }
        windows.push(window);
        let (mut tree_root, mut parent) = (0, 0);
        let mut children: *mut xlib::Window = ptr::null_mut();
        let mut child_count = 0;
        if xlib::XQueryTree(display, window, &mut tree_root, &mut parent, &mut children, &mut child_count) == 0 {
            // Destroyed meanwhile: its children are gone with it.
            continue;
        }
        for index in 0..child_count as usize {
            queue.push_back(*children.add(index));
        }
        if !children.is_null() {
            xlib::XFree(children as *mut _);
        }
    }
    windows
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

    #[test]
    fn only_cuas_masters_are_parked_pointers_and_keyboards_apart() {
        assert_eq!(cua_master("CUA v1.1234.5.6 pointer", xi::XIMasterPointer), Some(Master::Pointer));
        assert_eq!(cua_master("CUA v1.1234.5.6 keyboard", xi::XIMasterKeyboard), Some(Master::Keyboard));
        // The person's own pair and cua's slaves stay untouched.
        assert_eq!(cua_master("Virtual core pointer", xi::XIMasterPointer), None);
        assert_eq!(cua_master("Virtual core keyboard", xi::XIMasterKeyboard), None);
        assert_eq!(cua_master("CUA v1.1234.5.6 uinput pointer", xi::XISlavePointer), None);
        assert_eq!(cua_master("CUA v1.1234.5.6 uinput keyboard", xi::XISlaveKeyboard), None);
        // A pair of another naming generation is not ours to park.
        assert_eq!(cua_master("CUA mp-1-2 pointer", xi::XIMasterPointer), None);
    }

    /// The live X server, when there is one: the walk starts at the root,
    /// visits children before grandchildren and honours the cap.
    #[test]
    fn the_window_walk_starts_at_the_root_and_stops_at_the_cap() {
        if !on_x11() {
            return;
        }
        // SAFETY: a display opened and closed here; XQueryTree frees its list.
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                return;
            }
            let root = xlib::XDefaultRootWindow(display);
            let two = window_tree(display, root, 2);
            assert_eq!(two.len(), 2);
            assert_eq!(two[0], root);
            let all = window_tree(display, root, WINDOW_CAP);
            assert!(all.len() >= two.len());
            assert_eq!(all[0], root);
            xlib::XCloseDisplay(display);
        }
    }
}
