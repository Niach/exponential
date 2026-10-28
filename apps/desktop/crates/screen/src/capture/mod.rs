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

/// One display's pixels, 8 bits per channel, physical resolution, as the
/// OS hands them out: rows `stride` bytes apart, RGBA or BGRA. Only a frame
/// the ring keeps is ever copied (cut to its window on the encode worker),
/// so a changing screen costs one hash per grab, not a conversion.
pub(crate) struct Frame {
    /// 1-based, the computer crate's order (`computer::displays`).
    pub display: u32,
    pub width: u32,
    pub height: u32,
    /// Bytes from one row to the next (>= width * 4).
    pub stride: usize,
    /// Blue first (BGRA, macOS) instead of RGBA; alpha is ignored.
    pub bgra: bool,
    pixels: Box<dyn Pixels>,
}

/// A frame's bytes: an owned buffer, or one the OS lends while it lives.
pub(crate) trait Pixels: Send {
    fn bytes(&self) -> &[u8];
}

impl Pixels for Vec<u8> {
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl Frame {
    /// A tightly packed RGBA8 frame.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn rgba(display: u32, width: u32, height: u32, rgba: Vec<u8>) -> Frame {
        Frame { display, width, height, stride: width as usize * 4, bgra: false, pixels: Box::new(rgba) }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn lent(display: u32, width: u32, height: u32, stride: usize, bgra: bool, pixels: Box<dyn Pixels>) -> Frame {
        Frame { display, width, height, stride, bgra, pixels }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.pixels.bytes()
    }

    /// Non-empty, and every row's pixels lie within the bytes.
    pub(crate) fn valid(&self) -> bool {
        let (width, height) = (self.width as usize, self.height as usize);
        width > 0 && height > 0 && self.stride >= width * 4 && self.bytes().len() >= self.stride * (height - 1) + width * 4
    }
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
