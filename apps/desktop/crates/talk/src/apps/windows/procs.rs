//! The process table, exe paths and file descriptions, the OS build, and
//! the active audio sessions' pids (Win32 + the session enumerator).

use std::collections::HashMap;

use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::{
    eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::tree::{self, Proc};

/// The OS build number (RtlGetVersion: not subject to the manifest shim).
pub(super) fn os_build() -> u32 {
    let mut info = OSVERSIONINFOW { dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    // SAFETY: a correctly sized OSVERSIONINFOW.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() { info.dwBuildNumber } else { 0 }
}

/// pid → (parent pid, lowercased exe stem) of every running process.
pub(super) fn table() -> HashMap<u32, Proc> {
    let mut out = HashMap::new();
    // SAFETY: a process snapshot, walked and closed here.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            out.insert(entry.th32ProcessID, Proc { ppid: entry.th32ParentProcessID, stem: tree::stem_of(&exe) });
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    out
}

/// The full path of a process's exe (UTF-16, NUL-terminated).
pub(super) fn exe_path(pid: u32) -> Option<Vec<u16>> {
    // SAFETY: a query-only process handle, closed here.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size);
        let _ = CloseHandle(process);
        ok.ok()?;
        buffer.truncate(size as usize);
        buffer.push(0);
        Some(buffer)
    }
}

/// The exe's `FileDescription` (version resource), in its first
/// translation, else US English / Unicode.
pub(super) fn description(path: &[u16]) -> Option<String> {
    // SAFETY: the version block is sized by GetFileVersionInfoSizeW and the
    // pointers VerQueryValueW returns point into it.
    unsafe {
        let file = PCWSTR(path.as_ptr());
        let size = GetFileVersionInfoSizeW(file, None);
        if size == 0 {
            return None;
        }
        let mut block = vec![0u8; size as usize];
        GetFileVersionInfoW(file, None, size, block.as_mut_ptr().cast()).ok()?;
        let query = |key: &str| -> Option<(*const u16, u32)> {
            let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
            let mut value = std::ptr::null_mut();
            let mut len = 0u32;
            let found = VerQueryValueW(block.as_ptr().cast(), PCWSTR(key.as_ptr()), &mut value, &mut len).as_bool();
            (found && !value.is_null() && len > 0).then_some((value as *const u16, len))
        };
        let mut keys = Vec::new();
        if let Some((translation, len)) = query("\\VarFileInfo\\Translation") {
            if len >= 4 {
                let (lang, codepage) = (*translation, *translation.add(1));
                keys.push(format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"));
            }
        }
        keys.push("\\StringFileInfo\\040904b0\\FileDescription".to_string());
        for key in keys {
            if let Some((text, len)) = query(&key) {
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(text, len as usize));
                let text = text.trim_end_matches('\0').trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

/// The pids of every ACTIVE audio session on every active render device
/// (the system sounds session left out). Needs COM on this thread.
pub(super) fn active_session_pids() -> windows::core::Result<Vec<u32>> {
    let mut pids = Vec::new();
    // SAFETY: COM calls on interfaces this function owns.
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for index in 0..devices.GetCount()? {
            let Ok(device) = devices.Item(index) else { continue };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
            for session in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(control) = sessions.GetSession(session) else { continue };
                if control.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                let Ok(control) = control.cast::<IAudioSessionControl2>() else { continue };
                if control.IsSystemSoundsSession() == S_OK {
                    continue;
                }
                // AUDCLNT_S_NO_SINGLE_PROCESS still carries a pid.
                if let Ok(pid) = control.GetProcessId() {
                    if pid != 0 && !pids.contains(&pid) {
                        pids.push(pid);
                    }
                }
            }
        }
    }
    Ok(pids)
}
