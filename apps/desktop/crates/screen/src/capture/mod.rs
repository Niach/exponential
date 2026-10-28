//! One capturer per OS: every display's pixels now, in the computer crate's
//! display order (1-based numbers), full physical resolution. The ring
//! (`crate::ring`) decides when to call and what to keep.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::path::Path;

/// One display's pixels, RGBA8, physical resolution.
pub(crate) struct Frame {
    /// 1-based, the computer crate's order (`computer::displays`).
    pub display: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub(crate) trait Capturer: Send {
    /// Every display now. A display that has no new frame yet may be left
    /// out; an empty Vec is not an error.
    fn grab_all(&mut self) -> Result<Vec<Frame>, String>;
    /// One display now (the focus-change capture); `None` = nothing yet.
    fn grab(&mut self, display: u32) -> Result<Option<Frame>, String>;
}

/// Open this OS's capturer. `state_dir` = where it may keep state across
/// restarts (the Linux portal's restore token).
pub(crate) fn open(state_dir: &Path) -> Result<Box<dyn Capturer>, String> {
    #[cfg(target_os = "linux")]
    return linux::open(state_dir);
    #[cfg(target_os = "macos")]
    return macos::open(state_dir);
    #[cfg(target_os = "windows")]
    return windows::open(state_dir);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = state_dir;
        Err("Screen memory is not available on this OS.".to_string())
    }
}
