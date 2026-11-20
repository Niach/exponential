//! Windows = Win32, through windows-sys (raw bindings, nothing extra at
//! load time).
//!
//! Input is `SendInput`: the path the person's own mouse and keyboard take,
//! so every toolkit takes it. Every INPUT we send carries [`MAGIC`] in
//! `dwExtraInfo`, which is how the lock tells ours from theirs. Keys go
//! through the foreground window's keyboard layout (`VkKeyScanExW`); a
//! character the layout has no plain or Shift key for (AltGr, another
//! script) is a `KEYEVENTF_UNICODE` stroke instead, except a letter or digit
//! of a shortcut, which is its own VK (#EXP-1922), a digit without the Shift
//! a layout types it with (#EXP-1980). The process is per-monitor
//! DPI aware (v2), so every rect and point here is physical pixels of the
//! virtual screen. The screen is a GDI blit of the desktop; the windows are
//! `EnumWindows` cut down to what a person can see and switch to.
//!
//! The lock is a pair of low-level hooks (keyboard, mouse) on a thread of
//! their own: everything the person does is swallowed except Esc (the
//! abort) and the release of a key or button they held when the lock
//! engaged (so no modifier stays stuck under our typing, no button under
//! our moves: #EXP-1921), unless our own input pressed or released it first
//! (#EXP-1979); while such a button is still down the agent's pointer
//! waits for its release ([`PERSON_BUTTONS`]). Buttons are the logical ones
//! everywhere: on a left-handed mouse the primary is the physical right
//! button. Windows silently drops a hook that answers slower than
//! `LowLevelHooksTimeout`, so the hook procs only decide and post; `on_esc`
//! runs off the message loop, hooks already gone.
//!
//! The UIPI boundary: without elevation we can neither drive nor lock an
//! elevated window or the secure desktop (UAC prompts, the sign-in screen).
//! `SendInput` to them is dropped without an error, the person's input
//! there escapes the hooks, and Ctrl+Alt+Del always wins. Nothing here
//! needs admin.

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Once};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, GlobalFree, BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors, GdiFlush, GetDC,
    EnumDisplayDevicesW, GetMonitorInfoW, MonitorFromWindow, DISPLAY_DEVICEW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
    DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONULL, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::{
    CF_BITMAP, CF_DIB, CF_DIBV5, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_ENHMETAFILE, CF_METAFILEPICT,
    CF_OEMTEXT, CF_OWNERDISPLAY, CF_PALETTE, CF_TEXT, CF_UNICODETEXT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, MapVirtualKeyW, SendInput, VkKeyScanExW, INPUT, INPUT_0, INPUT_KEYBOARD,
    INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_BACK, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR,
    VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MBUTTON, VK_RBUTTON, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT,
    VK_SPACE, VK_TAB, VK_UP, VK_XBUTTON1, VK_XBUTTON2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EDD_GET_DEVICE_INTERFACE_NAME, EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowLongW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, PeekMessageW, PostThreadMessageW, SetCursorPos, SetForegroundWindow, SetWindowsHookExW,
    ShowWindow, UnhookWindowsHookEx, GWL_EXSTYLE, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
    MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_SWAPBUTTON, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_RESTORE, WHEEL_DELTA, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOWS_HOOK_ID, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
    WS_EX_TOOLWINDOW, XBUTTON1, XBUTTON2,
};

use crate::backend::{settle_paste, Backend, Button, Clipboard, DisplayInfo, HeldAtEngage, Image, LockGuard, Pasted, SavedClipboard, WindowInfo, CLIPBOARD_SAVE_MAX, PERSON_BUTTONS};
use crate::keys::{Key, Named};

/// Our tag on every INPUT ("EXP1"): the lock's hooks let these through.
const MAGIC: usize = 0x4558_5031;

/// Posted to the lock thread by its keyboard hook on the person's Esc.
const WM_ESC: u32 = WM_APP + 1;

pub(crate) fn unsupported() -> Option<String> {
    None
}

pub(crate) fn open() -> Result<Box<dyn Backend>, String> {
    static DPI: Once = Once::new();
    DPI.call_once(|| {
        // SAFETY: no pointers. Fails when the awareness is already set (the
        // IDE's manifest), which is the awareness we want anyway.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    });
    Ok(Box::new(Win32))
}

pub(crate) struct Win32;

/// `what` failed, with this thread's last Windows error.
fn last_error(what: &str) -> String {
    // SAFETY: reads this thread's last-error value, nothing else.
    let code = unsafe { GetLastError() };
    format!("{what} failed (Windows error {code}).")
}

/// Runs its closure on drop: the cleanup of a GDI or process handle on
/// every path out.
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

fn keyboard_input(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: MAGIC } },
    }
}

fn mouse_input(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            // mouseData is a DWORD holding a signed wheel delta.
            mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: MAGIC },
        },
    }
}

/// One virtual-key stroke, its scan code filled in for the apps that read
/// scan codes (terminals, games).
fn vk_input(vk: VIRTUAL_KEY, extended: bool, up: bool) -> INPUT {
    // SAFETY: no pointers; a VK without a scan code maps to 0.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = 0;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard_input(vk, scan, flags)
}

/// `ch` as UTF-16 units, layout-free (one unit, or a surrogate pair).
fn unicode_inputs(ch: char, up: bool) -> Vec<INPUT> {
    let flags = if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE };
    ch.encode_utf16(&mut [0; 2]).iter().map(|&unit| keyboard_input(0, unit, flags)).collect()
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let count = u32::try_from(inputs.len()).map_err(|_| "Too much input at once.".to_string())?;
    // SAFETY: `inputs` is a live slice of `count` initialised INPUTs, the
    // element size passed alongside.
    let sent = unsafe { SendInput(count, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    if sent != count {
        return Err(last_error("SendInput"));
    }
    Ok(())
}

/// The VK for a named key, and whether it is on the extended (navigation)
/// cluster, which Windows otherwise reads as the numpad's.
fn named_vk(named: Named) -> (VIRTUAL_KEY, bool) {
    match named {
        Named::Return => (VK_RETURN, false),
        Named::Tab => (VK_TAB, false),
        Named::Escape => (VK_ESCAPE, false),
        Named::Backspace => (VK_BACK, false),
        Named::Delete => (VK_DELETE, true),
        Named::Home => (VK_HOME, true),
        Named::End => (VK_END, true),
        Named::PageUp => (VK_PRIOR, true),
        Named::PageDown => (VK_NEXT, true),
        Named::Left => (VK_LEFT, true),
        Named::Right => (VK_RIGHT, true),
        Named::Up => (VK_UP, true),
        Named::Down => (VK_DOWN, true),
        Named::Insert => (VK_INSERT, true),
        Named::Space => (VK_SPACE, false),
        Named::F(n) => (VK_F1 + u16::from(n.clamp(1, 24) - 1), false),
        Named::Control => (VK_CONTROL, false),
        Named::Shift => (VK_SHIFT, false),
        Named::Alt => (VK_MENU, false),
        Named::Super => (VK_LWIN, false),
    }
}

/// A `VkKeyScanExW` answer as a stroke: the VK and whether Shift is needed.
/// `None` = no key on the layout, or one behind Ctrl/Alt (AltGr), which a
/// Unicode stroke types more reliably than a synthesized AltGr.
fn char_stroke(scan: i16) -> Option<(VIRTUAL_KEY, bool)> {
    let [vk, state] = scan.to_le_bytes();
    if vk == 0xff && state == 0xff {
        return None;
    }
    if state & !1 != 0 {
        return None;
    }
    Some((VIRTUAL_KEY::from(vk), state & 1 != 0))
}

/// #EXP-1922: the key for `ch` when the layout has none. Under Ctrl, Alt
/// or Win (`modified`) a letter or digit is a shortcut, which apps bind to
/// its layout-independent VK ('A'..'Z' and '0'..'9' are their own VK codes),
/// never to a Unicode packet; anything else is typed as Unicode (`None`).
fn shortcut_vk(ch: char, modified: bool) -> Option<VIRTUAL_KEY> {
    (modified && ch.is_ascii_alphanumeric()).then(|| VIRTUAL_KEY::from(ch.to_ascii_uppercase() as u8))
}

/// #EXP-1980: how `key` sends `ch`: a VK and whether Shift goes with it,
/// `None` = a Unicode packet. `on_layout` = its key on the foreground layout
/// ([`layout_stroke`]); under a shortcut (`modified`) apps bind a letter or
/// digit to its own VK, so a digit goes without the Shift French AZERTY
/// types it with (Ctrl+1, never Ctrl+Shift+1) and a capital the layout
/// lacks (Russian) keeps its Shift (Ctrl+Shift+T).
fn char_key(ch: char, on_layout: Option<(VIRTUAL_KEY, bool)>, modified: bool) -> Option<(VIRTUAL_KEY, bool)> {
    match shortcut_vk(ch, modified) {
        Some(vk) if ch.is_ascii_digit() || on_layout.is_none() => Some((vk, ch.is_ascii_uppercase())),
        _ => on_layout,
    }
}

/// Whether `vk` is down now.
fn pressed(vk: VIRTUAL_KEY) -> bool {
    // SAFETY: no pointers. The high bit = down.
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// `ch` on the keyboard layout of the window that gets the keys.
fn layout_stroke(ch: char) -> Option<(VIRTUAL_KEY, bool)> {
    let unit = u16::try_from(u32::from(ch)).ok()?;
    // SAFETY: no pointers; a null window / thread id 0 fall back to this
    // thread's layout.
    let scan = unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        VkKeyScanExW(unit, GetKeyboardLayout(thread))
    };
    char_stroke(scan)
}

/// Whether the person swapped the mouse buttons (a left-handed mouse: the
/// physical right button is the primary).
fn buttons_swapped() -> bool {
    // SAFETY: no pointers.
    unsafe { GetSystemMetrics(SM_SWAPBUTTON) != 0 }
}

/// #EXP-1979: `SendInput`'s flags for the logical `button` (the left one =
/// the primary). SendInput presses physical buttons, which Windows swaps
/// on a left-handed mouse: there the primary is the physical right one.
fn button_flags(button: Button, down: bool, swapped: bool) -> MOUSE_EVENT_FLAGS {
    let physical = match (button, swapped) {
        (Button::Left, true) => Button::Right,
        (Button::Right, true) => Button::Left,
        (button, _) => button,
    };
    match (physical, down) {
        (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (Button::Left, false) => MOUSEEVENTF_LEFTUP,
        (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
    }
}

/// #EXP-1979: a snapshot of `GetAsyncKeyState` (physical buttons) in the
/// hook's terms (logical: WM_LBUTTON* is the primary): on a left-handed
/// mouse the physical right button is VK_LBUTTON's slot.
fn logical_buttons(mut held: HeldAtEngage<256>, swapped: bool) -> HeldAtEngage<256> {
    if swapped {
        held.0.swap(usize::from(VK_LBUTTON), usize::from(VK_RBUTTON));
    }
    held
}

/// One coordinate of the virtual screen as `SendInput`'s absolute 0..=65535.
fn normalise(value: i32, origin: i32, extent: i32) -> i32 {
    let span = i64::from(extent.max(2) - 1);
    let offset = (i64::from(value) - i64::from(origin)).clamp(0, span);
    ((offset * 65535 + span / 2) / span) as i32
}

/// One INPUT's flags + wheel data per notch: `dy > 0` (content down) is a
/// negative wheel delta, `dx > 0` (right) a positive horizontal one.
fn wheel_notches(dx: i32, dy: i32) -> Vec<(MOUSE_EVENT_FLAGS, i32)> {
    let notch = WHEEL_DELTA as i32;
    let vertical = std::iter::repeat_n((MOUSEEVENTF_WHEEL, -dy.signum() * notch), dy.unsigned_abs() as usize);
    let horizontal = std::iter::repeat_n((MOUSEEVENTF_HWHEEL, dx.signum() * notch), dx.unsigned_abs() as usize);
    vertical.chain(horizontal).collect()
}

fn rect_size(rect: &RECT) -> (u32, u32) {
    ((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32)
}

fn hwnd_of(id: u64) -> Result<HWND, String> {
    let raw = usize::try_from(id).map_err(|_| "Not a window id.".to_string())?;
    Ok(raw as HWND)
}

/// # Safety
/// `data` is a `*mut Vec<HMONITOR>` alive for the whole enumeration.
unsafe extern "system" fn collect_monitor(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: the caller's contract: `data` is `displays`' vector.
    let monitors = unsafe { &mut *(data as *mut Vec<HMONITOR>) };
    monitors.push(monitor);
    1
}

/// # Safety
/// `data` is a `*mut Vec<HWND>` alive for the whole enumeration.
unsafe extern "system" fn collect_window(window: HWND, data: LPARAM) -> BOOL {
    // SAFETY: the caller's contract: `data` is `list_windows`' vector.
    let windows = unsafe { &mut *(data as *mut Vec<HWND>) };
    windows.push(window);
    1
}

fn window_title(window: HWND) -> String {
    // SAFETY: no pointers.
    let len = unsafe { GetWindowTextLengthW(window) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    // SAFETY: `buf` holds `buf.len()` units; the call writes at most that
    // many, the NUL included, and returns the count without it.
    let copied = unsafe { GetWindowTextW(window, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..copied.clamp(0, len) as usize])
}

/// Hidden by DWM though "visible" (another virtual desktop, a suspended
/// store app).
fn cloaked(window: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: `value` is a live u32, its size passed alongside.
    let result = unsafe {
        DwmGetWindowAttribute(window, DWMWA_CLOAKED as u32, (&mut value as *mut u32).cast::<c_void>(), size_of::<u32>() as u32)
    };
    result == 0 && value != 0
}

/// The visible frame (no invisible resize borders), else the window rect.
fn frame(window: HWND) -> Option<RECT> {
    // SAFETY: RECT is plain data.
    let mut rect: RECT = unsafe { zeroed() };
    // SAFETY: `rect` is a live RECT, its size passed alongside.
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut rect as *mut RECT).cast::<c_void>(),
            size_of::<RECT>() as u32,
        )
    };
    if result == 0 {
        return Some(rect);
    }
    // SAFETY: `rect` is a live RECT.
    (unsafe { GetWindowRect(window, &mut rect) } != 0).then_some(rect)
}

/// The exe stem of `pid`; empty when Windows will not say (an elevated
/// process, a protected one).
fn exe_stem(pid: u32) -> String {
    // SAFETY: no pointers; null = denied.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return String::new();
    }
    // SAFETY: closes the handle opened above, once.
    let _close = Defer(|| unsafe {
        CloseHandle(process);
    });
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` units; the call writes at most that many and
    // stores the count written in `len`.
    let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) } != 0;
    if !ok {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]);
    Path::new(&path).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The shell's own top-level windows (the desktop, the taskbars): they
/// cover a monitor without being a fullscreen app.
const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

fn class_name(window: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: `buf` holds `buf.len()` units; the call writes at most that
    // many, the NUL included, and returns the count without it.
    let len = unsafe { GetClassNameW(window, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.clamp(0, buf.len() as i32) as usize])
}

/// NEW-59: a (visible, non-tool: the caller's filter) window whose `rect`
/// covers its monitor's full rect, the shell's own windows aside.
fn fullscreen(window: HWND, rect: &RECT) -> bool {
    // SAFETY: no pointers; null = off every monitor.
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONULL) };
    if monitor.is_null() {
        return false;
    }
    // SAFETY: MONITORINFO is plain data.
    let mut info: MONITORINFO = unsafe { zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    // SAFETY: `info` is a live MONITORINFO whose cbSize says so.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return false;
    }
    let screen = info.rcMonitor;
    let covers = rect.left <= screen.left && rect.top <= screen.top && rect.right >= screen.right && rect.bottom >= screen.bottom;
    covers && !SHELL_CLASSES.contains(&class_name(window).as_str())
}

/// #FEED-161: the monitor's device interface path (`\\?\DISPLAY#…#{…}`),
/// stable across reconnects and reorders, of the monitor on adapter
/// `device` (`MONITORINFOEXW::szDevice`, NUL-terminated); `None` when
/// Windows names none.
fn monitor_interface(device: &[u16]) -> Option<String> {
    // SAFETY: DISPLAY_DEVICEW is plain data.
    let mut found: DISPLAY_DEVICEW = unsafe { zeroed() };
    found.cb = size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: `device` is NUL-terminated (szDevice), `found` a live
    // DISPLAY_DEVICEW whose cb says so.
    if unsafe { EnumDisplayDevicesW(device.as_ptr(), 0, &mut found, EDD_GET_DEVICE_INTERFACE_NAME) } == 0 {
        return None;
    }
    let end = found.DeviceID.iter().position(|&unit| unit == 0).unwrap_or(found.DeviceID.len());
    Some(String::from_utf16_lossy(&found.DeviceID[..end])).filter(|id| !id.is_empty())
}

fn foreground() -> HWND {
    // SAFETY: no pointers.
    unsafe { GetForegroundWindow() }
}

impl Backend for Win32 {
    fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
        let mut monitors: Vec<HMONITOR> = Vec::new();
        // SAFETY: the callback only pushes into `monitors`, which outlives
        // the (synchronous) enumeration.
        let ok = unsafe {
            EnumDisplayMonitors(null_mut(), null(), Some(collect_monitor), &mut monitors as *mut Vec<HMONITOR> as LPARAM)
        };
        if ok == 0 {
            return Err(last_error("EnumDisplayMonitors"));
        }
        let mut displays = Vec::with_capacity(monitors.len());
        // #FEED-164: twins are told apart by their GDI device name.
        let mut connectors = Vec::with_capacity(monitors.len());
        for monitor in monitors {
            // SAFETY: MONITORINFOEXW is plain data.
            let mut info: MONITORINFOEXW = unsafe { zeroed() };
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: `info` is a live MONITORINFOEXW whose cbSize says so,
            // which GetMonitorInfoW accepts in place of a MONITORINFO.
            if unsafe { GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>()) } == 0 {
                continue;
            }
            let rect = info.monitorInfo.rcMonitor;
            let (width, height) = rect_size(&rect);
            if width == 0 || height == 0 {
                continue;
            }
            let end = info.szDevice.iter().position(|&unit| unit == 0).unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..end]);
            connectors.push(Some(name.trim_start_matches(r"\\.\").to_string()).filter(|connector| !connector.is_empty()));
            displays.push(DisplayInfo {
                id: monitor_interface(&info.szDevice).unwrap_or_else(|| name.clone()),
                x: rect.left,
                y: rect.top,
                width,
                height,
                primary: info.monitorInfo.dwFlags & 1 != 0,
                name,
                connector: None,
            });
        }
        if displays.is_empty() {
            return Err("Windows reports no display.".to_string());
        }
        if !displays.iter().any(|display| display.primary) {
            displays[0].primary = true;
        }
        crate::backend::finish_ids(&mut displays, &connectors);
        Ok(displays)
    }

    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
        let width = i32::try_from(display.width).map_err(|_| "The display is too wide.".to_string())?;
        let height = i32::try_from(display.height).map_err(|_| "The display is too tall.".to_string())?;
        if width == 0 || height == 0 {
            return Err("The display has no pixels.".to_string());
        }
        // SAFETY: a null window = the DC of the whole virtual screen.
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return Err(last_error("GetDC"));
        }
        // SAFETY: releases the DC taken above, once.
        let _screen = Defer(|| unsafe {
            ReleaseDC(null_mut(), screen);
        });
        // SAFETY: `screen` is a live DC.
        let memory = unsafe { CreateCompatibleDC(screen) };
        if memory.is_null() {
            return Err(last_error("CreateCompatibleDC"));
        }
        // SAFETY: deletes the DC created above, once (after the bitmap is
        // selected out and deleted: drops run in reverse).
        let _memory = Defer(|| unsafe {
            DeleteDC(memory);
        });
        // SAFETY: BITMAPINFO is plain data.
        let mut info: BITMAPINFO = unsafe { zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative = top-down rows, the order `Image` wants.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..info.bmiHeader
        };
        let mut bits: *mut c_void = null_mut();
        // SAFETY: `info` describes a 32-bit top-down DIB; `bits` receives
        // its pixels, owned by the bitmap.
        let bitmap = unsafe { CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
        if bitmap.is_null() || bits.is_null() {
            return Err(last_error("CreateDIBSection"));
        }
        // SAFETY: deletes the bitmap created above, once, after it is
        // selected out below.
        let _bitmap = Defer(|| unsafe {
            DeleteObject(bitmap);
        });
        // SAFETY: both handles are live; the previous object goes back on drop.
        let previous = unsafe { SelectObject(memory, bitmap) };
        // SAFETY: puts back the object `memory` came with, freeing the bitmap.
        let _selected = Defer(|| unsafe {
            SelectObject(memory, previous);
        });
        // SAFETY: both DCs are live; the blit stays inside the bitmap.
        let blitted = unsafe { BitBlt(memory, 0, 0, width, height, screen, display.x, display.y, SRCCOPY | CAPTUREBLT) };
        if blitted == 0 {
            return Err(last_error("BitBlt"));
        }
        // SAFETY: no pointers; GDI's batch must land before the bits are read.
        unsafe { GdiFlush() };
        let len = (display.width as usize) * (display.height as usize) * 4;
        // SAFETY: the DIB holds width × height 32-bit pixels at `bits`, alive
        // until `_bitmap` drops, after this borrow ends.
        let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) };
        // A 32-bit DIB's rows are packed.
        Image::from_bgra(bgra, display.width, display.height, display.width as usize * 4).ok_or_else(|| "The screen capture came back short.".to_string())
    }

    fn cursor_position(&self) -> Result<(i32, i32), String> {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: `point` is a live POINT.
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return Err(last_error("GetCursorPos"));
        }
        Ok((point.x, point.y))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: the callback only pushes into `handles`, which outlives
        // the (synchronous) enumeration.
        if unsafe { EnumWindows(Some(collect_window), &mut handles as *mut Vec<HWND> as LPARAM) } == 0 {
            return Err(last_error("EnumWindows"));
        }
        let active = foreground();
        let mut windows = Vec::new();
        for window in handles {
            // SAFETY: no pointers; a window gone meanwhile answers 0.
            let (visible, ex_style) = unsafe { (IsWindowVisible(window) != 0, GetWindowLongW(window, GWL_EXSTYLE) as u32) };
            if !visible || ex_style & WS_EX_TOOLWINDOW != 0 || cloaked(window) {
                continue;
            }
            let Some(rect) = frame(window) else { continue };
            let fullscreen = fullscreen(window, &rect);
            let mut title = window_title(window);
            // An untitled window is listed only when it is a fullscreen app
            // (a game): the work screen reads it.
            if title.is_empty() && !fullscreen {
                continue;
            }
            let (width, height) = rect_size(&rect);
            let mut pid = 0u32;
            // SAFETY: `pid` is a live u32.
            unsafe { GetWindowThreadProcessId(window, &mut pid) };
            let pid = (pid != 0).then_some(pid);
            let app = pid.map(exe_stem).unwrap_or_default();
            if title.is_empty() {
                title = app.clone();
            }
            windows.push(WindowInfo {
                id: window as usize as u64,
                title,
                app,
                pid,
                x: rect.left,
                y: rect.top,
                width,
                height,
                active: window == active,
                fullscreen,
            });
        }
        Ok(windows)
    }

    fn activate_window(&self, id: u64) -> Result<(), String> {
        let window = hwnd_of(id)?;
        // SAFETY: IsWindow takes any value.
        if unsafe { IsWindow(window) } == 0 {
            return Err(format!("There is no window {id}."));
        }
        // SAFETY: no pointers; `window` is a window.
        unsafe {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
        }
        // SAFETY: no pointers.
        if unsafe { SetForegroundWindow(window) } == 0 {
            // Windows hands the foreground only to the process with the last
            // input event: make that ours with an Alt tap, then ask again.
            send(&[vk_input(VK_MENU, false, false), vk_input(VK_MENU, false, true)])?;
            // SAFETY: no pointers.
            unsafe { SetForegroundWindow(window) };
        }
        if crate::backend::wait_active(crate::backend::ACTIVATE_WAIT_WINDOWS, || foreground() == window) {
            return Ok(());
        }
        Err(format!("Windows refused to activate window {id} (an elevated window cannot be driven without elevation)."))
    }

    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
        // SAFETY: no pointers.
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        send(&[mouse_input(normalise(x, left, width), normalise(y, top, height), 0, flags)])?;
        // The 0..=65535 grid rounds on wide desktops; land on the pixel.
        // SAFETY: no pointers.
        if unsafe { SetCursorPos(x, y) } == 0 {
            return Err(last_error("SetCursorPos"));
        }
        Ok(())
    }

    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
        send(&[mouse_input(0, 0, 0, button_flags(button, down, buttons_swapped()))])
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        for (flags, data) in wheel_notches(dx, dy) {
            send(&[mouse_input(0, 0, data, flags)])?;
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        match key {
            Key::Named(named) => {
                let (vk, extended) = named_vk(named);
                send(&[vk_input(vk, extended, !down)])
            }
            Key::Char(ch) => {
                let modified = [VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].into_iter().any(pressed);
                // A release follows its press: the VK, if that is down.
                let held = |vk: VIRTUAL_KEY| !down && pressed(vk);
                match char_key(ch, layout_stroke(ch), modified).or_else(|| shortcut_vk(ch, true).filter(|&vk| held(vk)).map(|vk| (vk, ch.is_ascii_uppercase()))) {
                    Some((vk, shift)) => {
                        let mut inputs = Vec::with_capacity(2);
                        if shift && down {
                            inputs.push(vk_input(VK_SHIFT, false, false));
                        }
                        inputs.push(vk_input(vk, false, !down));
                        if shift && !down {
                            inputs.push(vk_input(VK_SHIFT, false, true));
                        }
                        send(&inputs)
                    }
                    None => send(&unicode_inputs(ch, !down)),
                }
            }
        }
    }

    fn char_stroke(&self, ch: char, down: bool) -> Result<(), String> {
        send(&unicode_inputs(ch, !down))
    }

    fn person_holds_button(&self) -> bool {
        person_holds_button()
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(on_esc)?))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
        Ok(Box::new(WinClipboard { set_sequence: Cell::new(None) }))
    }
}

/// Whether a hooked event is our own `SendInput` (the person's never
/// carries the injected flag, let alone our tag).
fn is_ours(injected: bool, extra_info: usize) -> bool {
    injected && extra_info == MAGIC
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Swallow,
    Esc,
}

/// The keyboard hook's decision. `held` = the keys down when the lock
/// engaged: each one's release passes, once, unless ours came first.
fn judge_key(ours: bool, vk: u32, down: bool, held: &mut HeldAtEngage<256>) -> Verdict {
    if ours {
        held.ours(vk as usize);
        // Our Shift, Ctrl and Alt go out neutral, on the left keys' scan
        // codes; the person's arrive sided.
        if let Some(left) = left_of(vk) {
            held.ours(usize::from(left));
        }
        return Verdict::Pass;
    }
    if down && vk == u32::from(VK_ESCAPE) {
        return Verdict::Esc;
    }
    if held.passes(vk as usize, down) {
        Verdict::Pass
    } else {
        Verdict::Swallow
    }
}

/// The left key a neutral modifier VK stands for when we send it.
fn left_of(vk: u32) -> Option<VIRTUAL_KEY> {
    match u16::try_from(vk).ok()? {
        VK_SHIFT => Some(VK_LSHIFT),
        VK_CONTROL => Some(VK_LCONTROL),
        VK_MENU => Some(VK_LMENU),
        _ => None,
    }
}

/// The button a mouse `message` presses or releases (`mouse_data` = the
/// MSLLHOOKSTRUCT's), as its VK, and whether it is the press.
fn hooked_button(message: u32, mouse_data: u32) -> Option<(VIRTUAL_KEY, bool)> {
    let side = || match (mouse_data >> 16) as u16 {
        XBUTTON1 => Some(VK_XBUTTON1),
        XBUTTON2 => Some(VK_XBUTTON2),
        _ => None,
    };
    match message {
        WM_LBUTTONDOWN => Some((VK_LBUTTON, true)),
        WM_LBUTTONUP => Some((VK_LBUTTON, false)),
        WM_RBUTTONDOWN => Some((VK_RBUTTON, true)),
        WM_RBUTTONUP => Some((VK_RBUTTON, false)),
        WM_MBUTTONDOWN => Some((VK_MBUTTON, true)),
        WM_MBUTTONUP => Some((VK_MBUTTON, false)),
        WM_XBUTTONDOWN => side().map(|vk| (vk, true)),
        WM_XBUTTONUP => side().map(|vk| (vk, false)),
        _ => None,
    }
}

/// The mouse hook's decision on `message` (`mouse_data` = the
/// MSLLHOOKSTRUCT's): the release of a button held when the lock engaged
/// passes, once, like a key's (#EXP-1921), unless ours came first
/// (#EXP-1979); the rest of the person's is swallowed.
fn judge_mouse(ours: bool, message: u32, mouse_data: u32, held: &mut HeldAtEngage<256>) -> Verdict {
    let button = hooked_button(message, mouse_data);
    if ours {
        if let Some((vk, _)) = button {
            held.ours(usize::from(vk));
        }
        return Verdict::Pass;
    }
    match button {
        Some((vk, down)) if held.passes(usize::from(vk), down) => Verdict::Pass,
        _ => Verdict::Swallow,
    }
}

// The hooks run on the thread that installed them (the lock thread, inside
// its GetMessageW), so their state is that thread's.
thread_local! {
    static HELD: Cell<HeldAtEngage<256>> = const { Cell::new(HeldAtEngage::NONE) };
    static ESC_POSTED: Cell<bool> = const { Cell::new(false) };
}

/// # Safety
/// Called by Windows as a WH_KEYBOARD_LL hook proc.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for code >= 0, `lparam` points at the event's
        // KBDLLHOOKSTRUCT for the duration of the call.
        let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let ours = is_ours(event.flags & LLKHF_INJECTED != 0, event.dwExtraInfo);
        let verdict = HELD.with(|held| {
            let mut keys = held.get();
            let verdict = judge_key(ours, event.vkCode, down, &mut keys);
            held.set(keys);
            verdict
        });
        match verdict {
            Verdict::Pass => {}
            Verdict::Swallow => return 1,
            Verdict::Esc => {
                if !ESC_POSTED.replace(true) {
                    // SAFETY: no pointers; this thread's own queue exists
                    // (made before the hooks went in).
                    unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_ESC, 0, 0) };
                }
                return 1;
            }
        }
    }
    // SAFETY: hands the event on unchanged.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// # Safety
/// Called by Windows as a WH_MOUSE_LL hook proc.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for code >= 0, `lparam` points at the event's
        // MSLLHOOKSTRUCT for the duration of the call.
        let event = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let ours = is_ours(event.flags & LLMHF_INJECTED != 0, event.dwExtraInfo);
        let verdict = HELD.with(|held| {
            let mut buttons = held.get();
            let verdict = judge_mouse(ours, wparam as u32, event.mouseData, &mut buttons);
            held.set(buttons);
            publish_buttons(&buttons);
            verdict
        });
        if verdict != Verdict::Pass {
            return 1;
        }
    }
    // SAFETY: hands the event on unchanged.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// An installed hook, removed on drop.
struct Hook(HHOOK);

impl Hook {
    fn install(kind: WINDOWS_HOOK_ID, proc: HOOKPROC) -> Result<Hook, String> {
        // SAFETY: `proc` is a 'static fn; a low-level hook takes this
        // module and thread 0 (every thread on the desktop).
        let handle = unsafe { SetWindowsHookExW(kind, proc, GetModuleHandleW(null()), 0) };
        if handle.is_null() {
            return Err(last_error("The input lock (SetWindowsHookExW)"));
        }
        Ok(Hook(handle))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        // SAFETY: the handle came from SetWindowsHookExW and is unhooked once.
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

/// The hooks, alive on their own thread's message loop until dropped (or
/// until Esc, after which the hooks go and the thread waits for the drop).
struct Lock {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Lock {
    fn take(on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Lock, String> {
        let (ready, start) = crate::backend::lock_start::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn(move || {
                // SAFETY: MSG is plain data.
                let mut msg: MSG = unsafe { zeroed() };
                // SAFETY: `msg` is a live MSG. Peeking makes this thread's
                // queue, so the guard's WM_QUIT can never miss it.
                unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE) };
                let hooks = match Hook::install(WH_KEYBOARD_LL, Some(keyboard_proc))
                    .and_then(|keyboard| Ok((keyboard, Hook::install(WH_MOUSE_LL, Some(mouse_proc))?)))
                {
                    Ok(hooks) => hooks,
                    Err(reason) => {
                        ready.send(Err(reason));
                        return;
                    }
                };
                // After the hooks: a key released between this snapshot
                // and the hooks cannot be missed. No hook runs before the
                // loop below pumps.
                let held = pressed_keys();
                HELD.with(|slot| slot.set(held));
                publish_buttons(&held);
                // SAFETY: no pointers.
                if !ready.send(Ok(unsafe { GetCurrentThreadId() })) {
                    // #EXP-1921: `take` gave up waiting; nobody would ever
                    // release these hooks.
                    log::warn!("[computer] the input hooks started after the lock gave up: removed");
                    PERSON_BUTTONS.store(0, Ordering::Release);
                    return;
                }
                hold(hooks, &*on_esc, &mut msg);
                PERSON_BUTTONS.store(0, Ordering::Release);
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        // A start that answered just as the wait gave up is ended here.
        let late = |answer: Result<u32, String>| {
            if let Ok(thread_id) = answer {
                // SAFETY: no pointers; the thread's queue exists (peeked).
                unsafe { PostThreadMessageW(thread_id, WM_QUIT, 0, 0) };
            }
        };
        match start.wait(Duration::from_secs(5), late) {
            Ok(Ok(thread_id)) => Ok(Lock { thread_id, thread: Some(thread) }),
            Ok(Err(reason)) => {
                let _ = thread.join();
                Err(reason)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("The input lock's thread died.".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) => Err("The input hooks did not start.".to_string()),
        }
    }
}

/// The mouse buttons' VKs, in [`PERSON_BUTTONS`]' bit order.
const BUTTON_VKS: [VIRTUAL_KEY; 5] = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2];

/// #EXP-1979: the mouse buttons of `held` the person still holds, for the
/// pointer tools ([`PERSON_BUTTONS`]).
fn publish_buttons(held: &HeldAtEngage<256>) {
    PERSON_BUTTONS.store(held.armed(BUTTON_VKS.map(usize::from)), Ordering::Release);
}

fn person_holds_button() -> bool {
    PERSON_BUTTONS.load(Ordering::Acquire) != 0
}

/// Every key (and button: VK 1..6, the hook's logical ones) down right now.
fn pressed_keys() -> HeldAtEngage<256> {
    let mut held = HeldAtEngage::NONE;
    for (vk, slot) in held.0.iter_mut().enumerate().skip(1) {
        // SAFETY: no pointers. The high bit = down.
        *slot = unsafe { GetAsyncKeyState(vk as i32) } < 0;
    }
    logical_buttons(held, buttons_swapped())
}

/// Pump the lock thread's messages (the hooks run inside GetMessageW) until
/// WM_QUIT; the person's Esc unhooks, then runs `on_esc` here.
fn hold(hooks: (Hook, Hook), on_esc: &(dyn Fn() + Send + Sync), msg: &mut MSG) {
    let mut hooks = Some(hooks);
    loop {
        // SAFETY: `msg` is a live MSG; a null window = this thread's
        // messages, thread messages included.
        let got = unsafe { GetMessageW(msg, null_mut(), 0, 0) };
        if got == 0 || got == -1 {
            break;
        }
        if msg.message == WM_ESC {
            if let Some(hooks) = hooks.take() {
                drop(hooks);
                log::info!("[computer] Esc: the input lock is released");
                on_esc();
                // The hooks are gone: the person's release is theirs again.
                PERSON_BUTTONS.store(0, Ordering::Release);
            }
        }
    }
}

impl LockGuard for Lock {}

impl Drop for Lock {
    fn drop(&mut self) {
        PERSON_BUTTONS.store(0, Ordering::Release);
        let Some(thread) = self.thread.take() else { return };
        // SAFETY: no pointers; the id names the lock thread, which lives
        // until it takes this WM_QUIT.
        if unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } == 0 {
            // #ERR-29: the hooks stay and keep the person's input until Esc.
            super::lock_lost("[computer] the input lock was not released", &last_error("Releasing the input lock"));
            return;
        }
        if thread.thread().id() != std::thread::current().id() {
            let _ = thread.join();
        }
    }
}

/// The clipboard, opened per call with no owner window (`OpenClipboard(NULL)`
/// then `EmptyClipboard` leaves the owner NULL; `SetClipboardData` still
/// takes plain HGLOBAL data, which is all we ever put there, never delayed
/// rendering). Contents are never logged.
struct WinClipboard {
    /// The sequence number right after our own `set_text`.
    set_sequence: Cell<Option<u32>>,
}

/// Formats whose data is a GDI or other OS handle, not HGLOBAL memory.
const HANDLE_FORMATS: [u16; 8] = [
    CF_BITMAP,
    CF_ENHMETAFILE,
    CF_METAFILEPICT,
    CF_PALETTE,
    CF_DSPBITMAP,
    CF_DSPENHMETAFILE,
    CF_DSPMETAFILEPICT,
    CF_OWNERDISPLAY,
];

/// The clipboard, open; closed on drop.
struct OpenClipboardGuard;

impl OpenClipboardGuard {
    /// Another program may hold the clipboard for a moment: retry a while.
    fn open() -> Result<Self, String> {
        let mut error = String::new();
        for attempt in 0..10 {
            // SAFETY: no owner window; paired with CloseClipboard on drop.
            if unsafe { OpenClipboard(null_mut()) } != 0 {
                return Ok(Self);
            }
            error = last_error("Opening the clipboard");
            if attempt < 9 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Err(error)
    }
}

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard in `open`.
        unsafe { CloseClipboard() };
    }
}

/// The bytes of an HGLOBAL, `None` when it is not readable memory or is
/// larger than `budget`.
///
/// SAFETY: `handle` must be clipboard data of an HGLOBAL format, with the
/// clipboard open.
unsafe fn global_bytes(handle: HANDLE, budget: usize) -> Option<Vec<u8>> {
    if handle.is_null() {
        return None;
    }
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > budget {
        return None;
    }
    let ptr = unsafe { GlobalLock(handle) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: a locked HGLOBAL is `size` readable bytes until unlocked.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) }.to_vec();
    unsafe { GlobalUnlock(handle) };
    Some(bytes)
}

/// Put `bytes` on the (open, emptied) clipboard as `format`, in a fresh
/// movable HGLOBAL the system owns once it is set.
fn set_global(format: u32, bytes: &[u8]) -> Result<(), String> {
    // SAFETY: a fresh allocation, filled while locked, freed on any failure
    // before the system took it.
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if handle.is_null() {
            return Err(last_error("Allocating clipboard memory"));
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            let error = last_error("Locking clipboard memory");
            GlobalFree(handle);
            return Err(error);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        GlobalUnlock(handle);
        if SetClipboardData(format, handle).is_null() {
            let error = last_error("Setting clipboard data");
            GlobalFree(handle);
            return Err(error);
        }
    }
    Ok(())
}

fn register_format(name: &str) -> Result<u32, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    match unsafe { RegisterClipboardFormatW(wide.as_ptr()) } {
        0 => Err(last_error("Registering a clipboard format")),
        format => Ok(format),
    }
}

/// `CF:<n>` for a predefined format, the registered name otherwise.
fn format_name(format: u32) -> Option<String> {
    if format < 0xC000 {
        return Some(format!("CF:{format}"));
    }
    let mut buf = [0u16; 256];
    // SAFETY: writes at most `buf.len()` units into `buf`.
    let len = unsafe { GetClipboardFormatNameW(format, buf.as_mut_ptr(), buf.len() as i32) };
    (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// The format a saved name stands for on this session.
fn format_id(name: &str) -> Result<u32, String> {
    match name.strip_prefix("CF:") {
        Some(number) => number.parse().map_err(|_| "A saved clipboard format has a bad name.".to_string()),
        None => register_format(name),
    }
}

/// Whether Windows synthesizes `format` again from a format `save` keeps:
/// the text and DIB groups from their first member, a bitmap and its
/// palette from the DIB. Any other format `keep_format` leaves out (a
/// metafile, a display or private handle) is gone after a restore.
fn remade(format: u32) -> bool {
    [CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT, CF_DIB, CF_DIBV5, CF_BITMAP, CF_PALETTE].iter().any(|&known| u32::from(known) == format)
}

/// Whether `format` is worth saving given the formats before it (Windows
/// lists what the program set first, then what it synthesizes from those).
fn keep_format(format: u32, earlier: &[u32]) -> bool {
    const TEXT: [u16; 3] = [CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT];
    const DIB: [u16; 2] = [CF_DIB, CF_DIBV5];
    if let Ok(short) = u16::try_from(format) {
        if HANDLE_FORMATS.contains(&short) {
            return false;
        }
        // Private (0x200..0x2FF) and GDI-object (0x300..0x3FF) formats may
        // be any handle.
        if (0x200..=0x3FF).contains(&short) {
            return false;
        }
        for group in [&TEXT[..], &DIB[..]] {
            if group.contains(&short) && earlier.iter().any(|&e| group.iter().any(|&g| u32::from(g) == e)) {
                return false;
            }
        }
    }
    true
}

impl Clipboard for WinClipboard {
    fn change_count(&self) -> Result<u64, String> {
        // SAFETY: no arguments, reads a counter.
        Ok(u64::from(unsafe { GetClipboardSequenceNumber() }))
    }

    fn save(&self) -> Result<SavedClipboard, String> {
        let _open = OpenClipboardGuard::open()?;
        let mut seen: Vec<u32> = Vec::new();
        let mut saved = SavedClipboard::default();
        let mut used = 0usize;
        let mut format = 0u32;
        loop {
            // SAFETY: the clipboard is open.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            let keep = keep_format(format, &seen);
            seen.push(format);
            if !keep {
                // Left out (#EXP-1900): a loss unless Windows makes it again.
                saved.partial |= !remade(format);
                continue;
            }
            let Some(name) = format_name(format) else {
                saved.partial = true;
                continue;
            };
            // SAFETY: the clipboard is open and `format` is an HGLOBAL
            // format (`keep_format`).
            let bytes = unsafe { global_bytes(GetClipboardData(format), CLIPBOARD_SAVE_MAX - used) };
            match bytes {
                Some(bytes) => {
                    used += bytes.len();
                    saved.formats.push((name, bytes));
                }
                // Too large, or not readable memory.
                None => saved.partial = true,
            }
        }
        Ok(saved)
    }

    fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String> {
        let bytes: Vec<u8> =
            text.encode_utf16().chain(std::iter::once(0)).flat_map(|unit| unit.to_ne_bytes()).collect();
        {
            let _open = OpenClipboardGuard::open()?;
            // SAFETY: the clipboard is open.
            if unsafe { EmptyClipboard() } == 0 {
                return Err(last_error("Clearing the clipboard"));
            }
            set_global(u32::from(CF_UNICODETEXT), &bytes)?;
            if sensitive {
                // Microsoft's conventions: monitors, clipboard history and
                // the cloud clipboard skip content carrying these.
                let marked = (|| {
                    set_global(register_format("ExcludeClipboardContentFromMonitorProcessing")?, &[0])?;
                    set_global(register_format("CanIncludeInClipboardHistory")?, &0u32.to_ne_bytes())?;
                    set_global(register_format("CanUploadToCloudClipboard")?, &0u32.to_ne_bytes())
                })();
                if let Err(error) = marked {
                    // Never leave a secret on the clipboard unmarked.
                    // SAFETY: the clipboard is open.
                    unsafe { EmptyClipboard() };
                    return Err(error);
                }
            }
        }
        // SAFETY: no arguments, reads a counter.
        self.set_sequence.set(Some(unsafe { GetClipboardSequenceNumber() }));
        Ok(())
    }

    fn wait_pasted(&self, timeout: Duration) -> Pasted {
        // Windows does not tell when a program reads the clipboard: settle.
        // SAFETY: no arguments, reads a counter.
        settle_paste(timeout, self.set_sequence.get().map(u64::from), || u64::from(unsafe { GetClipboardSequenceNumber() }))
    }

    fn restore(&self, saved: SavedClipboard) -> Result<(), String> {
        let _open = OpenClipboardGuard::open()?;
        // SAFETY: the clipboard is open.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(last_error("Clearing the clipboard"));
        }
        let mut first_error = None;
        for (name, bytes) in &saved.formats {
            if let Err(error) = format_id(name).and_then(|format| set_global(format, bytes)) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn text(&self) -> Result<Option<String>, String> {
        let _open = OpenClipboardGuard::open()?;
        let format = u32::from(CF_UNICODETEXT);
        // SAFETY: the clipboard is open.
        if unsafe { IsClipboardFormatAvailable(format) } == 0 {
            return Ok(None);
        }
        // SAFETY: the clipboard is open; CF_UNICODETEXT is HGLOBAL memory.
        let Some(bytes) = (unsafe { global_bytes(GetClipboardData(format), usize::MAX) }) else {
            return Ok(None);
        };
        let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_ne_bytes([pair[0], pair[1]])).collect();
        let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
        Ok(Some(String::from_utf16_lossy(&units[..end])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RCONTROL;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE;

    #[test]
    fn coordinates_normalise_over_the_virtual_screen() {
        assert_eq!(normalise(0, 0, 1920), 0);
        assert_eq!(normalise(1919, 0, 1920), 65535);
        assert_eq!(normalise(960, 0, 1921), 32768);
        // A monitor left of the primary: the virtual screen starts negative.
        assert_eq!(normalise(-1920, -1920, 3840), 0);
        assert_eq!(normalise(1919, -1920, 3840), 65535);
        assert_eq!(normalise(5000, 0, 1920), 65535);
        assert_eq!(normalise(-5, 0, 1920), 0);
    }

    #[test]
    fn named_keys_map_to_vks() {
        assert_eq!(named_vk(Named::Return), (VK_RETURN, false));
        assert_eq!(named_vk(Named::Left), (VK_LEFT, true));
        assert_eq!(named_vk(Named::PageDown), (VK_NEXT, true));
        assert_eq!(named_vk(Named::F(1)), (VK_F1, false));
        assert_eq!(named_vk(Named::F(12)), (0x7b, false));
        assert_eq!(named_vk(Named::Alt), (VK_MENU, false));
    }

    #[test]
    fn layout_answers_become_strokes() {
        assert_eq!(char_stroke(0x0041), Some((0x41, false)));
        assert_eq!(char_stroke(0x0141), Some((0x41, true)));
        assert_eq!(char_stroke(-1), None);
        // Ctrl+Alt (AltGr) goes the Unicode way.
        assert_eq!(char_stroke(0x0651), None);
    }

    /// #EXP-1922: a layout without the letter (Russian) still gets Ctrl+V
    /// as the V key; text stays Unicode.
    #[test]
    fn a_shortcut_letter_off_the_layout_is_its_own_vk() {
        assert_eq!(shortcut_vk('v', true), Some(0x56));
        assert_eq!(shortcut_vk('V', true), Some(0x56));
        assert_eq!(shortcut_vk('7', true), Some(0x37));
        assert_eq!(shortcut_vk('v', false), None);
        assert_eq!(shortcut_vk('ж', true), None);
        assert_eq!(shortcut_vk('-', true), None);
    }

    /// #EXP-1979: once the agent itself pressed or released a button or key
    /// the person held at engage, the person's later release is swallowed.
    #[test]
    fn the_agents_own_press_of_a_held_button_disarms_its_release() {
        let mut held = HeldAtEngage::<256>::NONE;
        held.0[usize::from(VK_LBUTTON)] = true;
        held.0[usize::from(VK_SHIFT)] = true;
        assert_eq!(judge_mouse(true, WM_LBUTTONDOWN, 0, &mut held), Verdict::Pass);
        assert_eq!(judge_mouse(true, WM_LBUTTONUP, 0, &mut held), Verdict::Pass);
        assert_eq!(judge_mouse(false, WM_LBUTTONUP, 0, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(true, u32::from(VK_SHIFT), true, &mut held), Verdict::Pass);
        assert_eq!(judge_key(false, u32::from(VK_SHIFT), false, &mut held), Verdict::Swallow);
        // Ours is neutral and on the left keys, the person's sided.
        held.0[usize::from(VK_LCONTROL)] = true;
        held.0[usize::from(VK_RCONTROL)] = true;
        assert_eq!(judge_key(true, u32::from(VK_CONTROL), true, &mut held), Verdict::Pass);
        assert_eq!(judge_key(false, u32::from(VK_LCONTROL), false, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(false, u32::from(VK_RCONTROL), false, &mut held), Verdict::Pass, "never touched by us");
    }

    /// #EXP-1979: a button the person held when the hooks went in holds the
    /// pointer tools back until its release passes the hook, at most the
    /// wait; a held key holds nothing back.
    #[test]
    fn the_pointer_waits_for_the_release_of_a_button_held_at_engage() {
        let wait = |millis| crate::backend::wait_for_person_buttons(person_holds_button, || false, Duration::from_millis(millis));
        let mut held = HeldAtEngage::<256>::NONE;
        held.0[usize::from(VK_SHIFT)] = true;
        publish_buttons(&held);
        assert_eq!(wait(60), crate::backend::ButtonWait::Free, "a key never holds the pointer back");
        held.0[usize::from(VK_LBUTTON)] = true;
        publish_buttons(&held);
        assert_eq!(wait(60), crate::backend::ButtonWait::Held, "the wait ran out");
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(judge_mouse(false, WM_LBUTTONUP, 0, &mut held), Verdict::Pass);
            publish_buttons(&held);
        });
        assert_eq!(wait(5000), crate::backend::ButtonWait::Free, "the person let go");
        release.join().unwrap();
        PERSON_BUTTONS.store(0, Ordering::Release);
    }

    /// #EXP-1979: on a left-handed mouse the primary is the physical right
    /// button: GetAsyncKeyState names it VK_RBUTTON, the hook WM_LBUTTON*.
    /// The snapshot and the agent's clicks speak the hook's buttons.
    #[test]
    fn a_left_handed_mouse_speaks_logical_buttons() {
        let mut physical = HeldAtEngage::<256>::NONE;
        physical.0[usize::from(VK_RBUTTON)] = true;
        let mut held = logical_buttons(physical, true);
        assert_eq!(judge_mouse(false, WM_LBUTTONUP, 0, &mut held), Verdict::Pass, "the held primary's release");
        assert_eq!(judge_mouse(false, WM_RBUTTONUP, 0, &mut held), Verdict::Swallow);
        assert!(logical_buttons(physical, false).0[usize::from(VK_RBUTTON)]);
        assert_eq!(button_flags(Button::Left, true, true), MOUSEEVENTF_RIGHTDOWN);
        assert_eq!(button_flags(Button::Left, false, true), MOUSEEVENTF_RIGHTUP);
        assert_eq!(button_flags(Button::Right, true, true), MOUSEEVENTF_LEFTDOWN);
        assert_eq!(button_flags(Button::Middle, true, true), MOUSEEVENTF_MIDDLEDOWN);
        assert_eq!(button_flags(Button::Left, true, false), MOUSEEVENTF_LEFTDOWN);
    }

    /// #EXP-1980, on what VkKeyScanExW answers there: under a shortcut a
    /// digit is its own VK without the Shift French AZERTY types it with, a
    /// capital the layout lacks keeps its Shift; typing keeps the layout's
    /// key, or goes as Unicode.
    #[test]
    fn a_shortcut_letter_or_digit_is_its_own_vk_on_any_layout() {
        // French AZERTY: '1' = Shift + VK_1, '&' = VK_1, 'a' = VK_A.
        assert_eq!(char_key('1', char_stroke(0x0131), true), Some((0x31, false)), "ctrl+1, never ctrl+shift+1");
        assert_eq!(char_key('1', char_stroke(0x0131), false), Some((0x31, true)));
        assert_eq!(char_key('&', char_stroke(0x0031), true), Some((0x31, false)));
        assert_eq!(char_key('a', char_stroke(0x0041), true), Some((0x41, false)));
        // German QWERTZ: 'z' = VK_Z, 'Z' = Shift + VK_Z.
        assert_eq!(char_key('z', char_stroke(0x005a), true), Some((0x5a, false)));
        assert_eq!(char_key('Z', char_stroke(0x015a), true), Some((0x5a, true)));
        // Russian: no Latin letters.
        assert_eq!(char_key('v', char_stroke(-1), true), Some((0x56, false)));
        assert_eq!(char_key('T', char_stroke(-1), true), Some((0x54, true)), "ctrl+T keeps Shift");
        assert_eq!(char_key('v', char_stroke(-1), false), None);
        assert_eq!(char_key('-', char_stroke(-1), true), None);
    }

    #[test]
    fn wheel_signs_follow_the_trait() {
        let delta = WHEEL_DELTA as i32;
        assert_eq!(wheel_notches(0, 2), vec![(MOUSEEVENTF_WHEEL, -delta); 2]);
        assert_eq!(wheel_notches(0, -1), vec![(MOUSEEVENTF_WHEEL, delta)]);
        assert_eq!(wheel_notches(1, 0), vec![(MOUSEEVENTF_HWHEEL, delta)]);
        assert!(wheel_notches(0, 0).is_empty());
    }

    /// #EDE-140 (#EXP-1900): what `save` keeps of the person's clipboard.
    /// Windows lists what the program set first, then what it makes from
    /// those, so of the text formats and of the DIBs only the first is the
    /// program's own; a GDI, display or private handle is no memory to copy;
    /// everything else (a registered format: HTML, a file list) is kept.
    #[test]
    fn save_keeps_the_first_of_a_group_and_never_a_handle() {
        let [unicode, text, oem, dib, dibv5] = [CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT, CF_DIB, CF_DIBV5].map(u32::from);
        const HDROP: u32 = 15;
        const LOCALE: u32 = 16;
        // Unicode text was set: the two after it are Windows' own.
        assert!(keep_format(unicode, &[]));
        assert!(!keep_format(text, &[unicode, LOCALE]));
        assert!(!keep_format(oem, &[unicode, LOCALE, text]));
        // An older program set CF_TEXT: that is the one to keep.
        assert!(keep_format(text, &[]));
        assert!(!keep_format(unicode, &[text]));
        // A picture: the first DIB of either kind, whatever text came before.
        assert!(keep_format(dibv5, &[unicode]));
        assert!(!keep_format(dib, &[unicode, dibv5]));
        assert!(keep_format(dib, &[text]));
        // Handles: every GDI and display format, the private and GDI-object ranges.
        for handle in HANDLE_FORMATS {
            assert!(!keep_format(u32::from(handle), &[]), "{handle}");
        }
        for handle in [0x200, 0x2FF, 0x300, 0x3FF] {
            assert!(!keep_format(handle, &[]), "{handle:#x}");
        }
        // Everything else stays, registered formats (0xC000 and up) included.
        for kept in [HDROP, LOCALE, 0x1FF, 0x400, 0xC000, 0xC1A5, 0x1_0000] {
            assert!(keep_format(kept, &[unicode, dib]), "{kept:#x}");
        }

        // What a restore cannot bring back (`partial`): a format left out that Windows does not make again.
        let lost = |formats: &[u32]| formats.iter().enumerate().any(|(at, &format)| !keep_format(format, &formats[..at]) && !remade(format));
        assert!(!lost(&[unicode, LOCALE, text, oem]), "copied text");
        assert!(!lost(&[u32::from(CF_BITMAP), dib, dibv5, u32::from(CF_PALETTE)]), "a copied picture: Windows makes the bitmap again from the DIB");
        assert!(lost(&[u32::from(CF_ENHMETAFILE), unicode]), "a metafile is gone");
        assert!(lost(&[unicode, 0x250]), "a private handle is gone");
    }

    #[test]
    fn only_our_tagged_injected_input_passes() {
        assert!(is_ours(true, MAGIC));
        assert!(!is_ours(false, MAGIC));
        assert!(!is_ours(true, 0));
    }

    #[test]
    fn the_keyboard_hook_swallows_all_but_esc_and_held_releases() {
        let mut held = HeldAtEngage::<256>::NONE;
        held.0[usize::from(VK_SHIFT)] = true;
        let esc = u32::from(VK_ESCAPE);
        let shift = u32::from(VK_SHIFT);
        assert_eq!(judge_key(false, esc, true, &mut held), Verdict::Esc);
        assert_eq!(judge_key(false, 0x41, true, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(false, 0x41, false, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(false, shift, false, &mut held), Verdict::Pass);
        assert_eq!(judge_key(false, shift, false, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(true, 0x41, true, &mut held), Verdict::Pass);
    }

    /// #EXP-1921: a button the person held when the lock engaged gets its
    /// release through, once.
    #[test]
    fn the_mouse_hook_passes_the_release_of_a_button_held_at_engage_once() {
        let mut held = HeldAtEngage::<256>::NONE;
        held.0[usize::from(VK_LBUTTON)] = true;
        held.0[usize::from(VK_XBUTTON2)] = true;
        let xbutton = |which: u16| u32::from(which) << 16;
        assert_eq!(judge_mouse(false, WM_LBUTTONUP, 0, &mut held), Verdict::Pass);
        assert_eq!(judge_mouse(false, WM_LBUTTONUP, 0, &mut held), Verdict::Swallow);
        assert_eq!(judge_mouse(false, WM_RBUTTONUP, 0, &mut held), Verdict::Swallow);
        assert_eq!(judge_mouse(false, WM_XBUTTONUP, xbutton(XBUTTON1), &mut held), Verdict::Swallow);
        assert_eq!(judge_mouse(false, WM_XBUTTONUP, xbutton(XBUTTON2), &mut held), Verdict::Pass);
        assert_eq!(judge_mouse(false, WM_MOUSEMOVE, 0, &mut held), Verdict::Swallow);
        assert_eq!(judge_mouse(true, WM_MOUSEMOVE, 0, &mut held), Verdict::Pass);
    }
}
