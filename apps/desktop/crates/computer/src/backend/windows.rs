//! Windows = Win32, through windows-sys (raw bindings, nothing extra at
//! load time).
//!
//! Input is `SendInput`: the path the person's own mouse and keyboard take,
//! so every toolkit takes it. Every INPUT we send carries [`MAGIC`] in
//! `dwExtraInfo`, which is how the lock tells ours from theirs. Keys go
//! through the foreground window's keyboard layout (`VkKeyScanExW`); a
//! character the layout has no plain or Shift key for (AltGr, another
//! script) is a `KEYEVENTF_UNICODE` stroke instead. The process is per-monitor
//! DPI aware (v2), so every rect and point here is physical pixels of the
//! virtual screen. The screen is a GDI blit of the desktop; the windows are
//! `EnumWindows` cut down to what a person can see and switch to.
//!
//! The lock is a pair of low-level hooks (keyboard, mouse) on a thread of
//! their own: everything the person does is swallowed except Esc (the
//! abort) and the release of a key they held when the lock engaged (so no
//! modifier stays stuck under our typing). Windows silently drops a hook
//! that answers slower than `LowLevelHooksTimeout`, so the hook procs only
//! decide and post; `on_esc` runs off the message loop, hooks already gone.
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
use std::sync::{mpsc, Once};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, BOOL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors, GdiFlush, GetDC,
    EnumDisplayDevicesW, GetMonitorInfoW, MonitorFromWindow, DISPLAY_DEVICEW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
    DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONULL, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
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
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EDD_GET_DEVICE_INTERFACE_NAME, EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowLongW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, PeekMessageW, PostThreadMessageW, SetCursorPos, SetForegroundWindow, SetWindowsHookExW,
    ShowWindow, UnhookWindowsHookEx, GWL_EXSTYLE, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED,
    MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_RESTORE, WHEEL_DELTA, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOWS_HOOK_ID, WM_APP, WM_KEYDOWN, WM_QUIT,
    WM_SYSKEYDOWN, WS_EX_TOOLWINDOW,
};

use crate::backend::{Backend, Button, DisplayInfo, Image, LockGuard, WindowInfo};
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

fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for pixel in bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    rgba
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
        Ok(Image { width: display.width, height: display.height, rgba: bgra_to_rgba(bgra) })
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
        let until = Instant::now() + Duration::from_millis(500);
        loop {
            if foreground() == window {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(format!(
                    "Windows refused to activate window {id} (an elevated window cannot be driven without elevation)."
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
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
        let flags = match (button, down) {
            (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (Button::Left, false) => MOUSEEVENTF_LEFTUP,
            (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
            (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
        };
        send(&[mouse_input(0, 0, 0, flags)])
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
            Key::Char(ch) => match layout_stroke(ch) {
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
            },
        }
    }

    fn type_char(&self, ch: char) -> Result<(), String> {
        send(&unicode_inputs(ch, false))?;
        std::thread::sleep(Duration::from_millis(6));
        send(&unicode_inputs(ch, true))
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(on_esc)?))
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
/// engaged: each one's release passes, once.
fn judge_key(ours: bool, vk: u32, down: bool, held: &mut [bool; 256]) -> Verdict {
    if ours {
        return Verdict::Pass;
    }
    if down {
        return if vk == u32::from(VK_ESCAPE) { Verdict::Esc } else { Verdict::Swallow };
    }
    match held.get_mut(vk as usize) {
        Some(slot) if *slot => {
            *slot = false;
            Verdict::Pass
        }
        _ => Verdict::Swallow,
    }
}

// The hooks run on the thread that installed them (the lock thread, inside
// its GetMessageW), so their state is that thread's.
thread_local! {
    static HELD: Cell<[bool; 256]> = const { Cell::new([false; 256]) };
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
        if !is_ours(event.flags & LLMHF_INJECTED != 0, event.dwExtraInfo) {
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
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
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
                        let _ = ready_tx.send(Err(reason));
                        return;
                    }
                };
                // After the hooks: a key released between this snapshot
                // and the hooks cannot be missed. No hook runs before the
                // loop below pumps.
                HELD.with(|held| held.set(pressed_keys()));
                // SAFETY: no pointers.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                hold(hooks, &*on_esc, &mut msg);
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
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

/// Every key (and button) down right now.
fn pressed_keys() -> [bool; 256] {
    let mut held = [false; 256];
    for (vk, slot) in held.iter_mut().enumerate().skip(1) {
        // SAFETY: no pointers. The high bit = down.
        *slot = unsafe { GetAsyncKeyState(vk as i32) } < 0;
    }
    held
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
            }
        }
    }
}

impl LockGuard for Lock {}

impl Drop for Lock {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        // SAFETY: no pointers; the id names the lock thread, which lives
        // until it takes this WM_QUIT.
        if unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } == 0 {
            log::warn!("[computer] {}", last_error("Releasing the input lock"));
            return;
        }
        if thread.thread().id() != std::thread::current().id() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn wheel_signs_follow_the_trait() {
        let delta = WHEEL_DELTA as i32;
        assert_eq!(wheel_notches(0, 2), vec![(MOUSEEVENTF_WHEEL, -delta); 2]);
        assert_eq!(wheel_notches(0, -1), vec![(MOUSEEVENTF_WHEEL, delta)]);
        assert_eq!(wheel_notches(1, 0), vec![(MOUSEEVENTF_HWHEEL, delta)]);
        assert!(wheel_notches(0, 0).is_empty());
    }

    #[test]
    fn bgra_becomes_opaque_rgba() {
        assert_eq!(bgra_to_rgba(&[1, 2, 3, 0, 10, 20, 30, 7]), vec![3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn only_our_tagged_injected_input_passes() {
        assert!(is_ours(true, MAGIC));
        assert!(!is_ours(false, MAGIC));
        assert!(!is_ours(true, 0));
    }

    #[test]
    fn the_keyboard_hook_swallows_all_but_esc_and_held_releases() {
        let mut held = [false; 256];
        held[usize::from(VK_SHIFT)] = true;
        let esc = u32::from(VK_ESCAPE);
        let shift = u32::from(VK_SHIFT);
        assert_eq!(judge_key(false, esc, true, &mut held), Verdict::Esc);
        assert_eq!(judge_key(false, 0x41, true, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(false, 0x41, false, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(false, shift, false, &mut held), Verdict::Pass);
        assert_eq!(judge_key(false, shift, false, &mut held), Verdict::Swallow);
        assert_eq!(judge_key(true, 0x41, true, &mut held), Verdict::Pass);
    }
}
