//! What only Windows can answer for [`crate::desktop`]: the foreground
//! window, the last-input clock and the UI Automation tree. Input is
//! SendInput (through enigo), so the agent drives the FOREGROUND desktop:
//! the pointer and keyboard are the person's own.

use std::time::Duration;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use crate::backend::*;
use crate::guard::UI_NODES_MAX;

const UI_DEPTH_MAX: usize = 14;

/// `GetLastInputInfo` also moves on SendInput events.
pub const IDLE_COUNTS_INJECTED: bool = true;

pub fn readiness(_prompt: bool) -> Readiness {
    Readiness::Ready
}

pub fn idle() -> Option<Duration> {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        GetLastInputInfo(&mut info)
            .as_bool()
            .then(|| Duration::from_millis(u64::from(GetTickCount().wrapping_sub(info.dwTime))))
    }
}

fn hwnd(id: u32) -> HWND {
    HWND(id as usize as *mut core::ffi::c_void)
}

fn exe_name(pid: u32) -> String {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let named = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        if named.is_err() {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit(['\\', '/']).next().unwrap_or_default().to_string()
    }
}

pub fn windows() -> BackendResult<Vec<WindowInfo>> {
    let foreground = unsafe { GetForegroundWindow() }.0 as usize as u32;
    let list = xcap::Window::all().map_err(|err| format!("Could not list the windows: {err}"))?;
    // EnumWindows order is the z-order, front to back.
    Ok(list
        .into_iter()
        .filter(|window| !window.is_minimized().unwrap_or(false))
        .filter_map(|window| {
            let id = window.id().ok()?;
            let pid = window.pid().ok()?;
            let exe = exe_name(pid);
            let app = window.app_name().ok().filter(|name| !name.is_empty()).unwrap_or_else(|| exe.clone());
            Some(WindowInfo {
                id,
                pid,
                app,
                exe,
                title: window.title().unwrap_or_default(),
                rect: Rect {
                    x: f64::from(window.x().ok()?),
                    y: f64::from(window.y().ok()?),
                    width: f64::from(window.width().ok()?),
                    height: f64::from(window.height().ok()?),
                },
                focused: id == foreground,
                layer: 0,
            })
        })
        .collect())
}

pub fn focused() -> Option<WindowInfo> {
    windows().ok()?.into_iter().find(|window| window.focused)
}

pub fn focus_window(id: u32) -> BackendResult<()> {
    unsafe {
        if IsIconic(hwnd(id)).as_bool() {
            let _ = ShowWindow(hwnd(id), SW_RESTORE);
        }
        if !SetForegroundWindow(hwnd(id)).as_bool() {
            return Err("Windows refused to bring that window to the front.".to_string());
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    Ok(())
}

fn walk(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: usize,
    nodes: &mut Vec<UiNode>,
) {
    if nodes.len() > UI_NODES_MAX || depth > UI_DEPTH_MAX {
        return;
    }
    unsafe {
        let role = element.CurrentLocalizedControlType().map(|text| text.to_string()).unwrap_or_default();
        let label = element.CurrentName().map(|text| text.to_string()).unwrap_or_default();
        let rect = element
            .CurrentBoundingRectangle()
            .ok()
            .filter(|rect| rect.right > rect.left && rect.bottom > rect.top)
            .map(|rect| Rect {
                x: f64::from(rect.left),
                y: f64::from(rect.top),
                width: f64::from(rect.right - rect.left),
                height: f64::from(rect.bottom - rect.top),
            });
        nodes.push(UiNode { depth, role, label, rect });
        let mut child = walker.GetFirstChildElement(element).ok();
        while let Some(current) = child {
            walk(walker, &current, depth + 1, nodes);
            child = walker.GetNextSiblingElement(&current).ok();
        }
    }
}

pub fn read_ui(window: Option<u32>) -> BackendResult<Vec<UiNode>> {
    let ui = |err: windows::core::Error| format!("UI Automation failed: {err}");
    unsafe {
        // Already initialized on this thread is fine.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(ui)?;
        let target = match window {
            Some(id) => hwnd(id),
            None => GetForegroundWindow(),
        };
        if target.0.is_null() {
            return Err("No window has focus; pass a window id from list_windows.".to_string());
        }
        let root = automation.ElementFromHandle(target).map_err(ui)?;
        let walker = automation.ControlViewWalker().map_err(ui)?;
        let mut nodes = Vec::new();
        walk(&walker, &root, 0, &mut nodes);
        Ok(nodes)
    }
}
