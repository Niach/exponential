//! One capturer per OS: the asked displays' pixels, in the computer crate's
//! display order (1-based numbers), full physical resolution. The ring
//! (`crate::ring`) decides when to call and what to keep.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::path::Path;
use std::time::Instant;

use crate::store::Rect;

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
    /// STAMP: when the OS captured or delivered it (Unix ms); `None` = a
    /// live read (the grab's time).
    pub at: Option<i64>,
    /// MASK: rects (within the frame) read as black by the hash and filled
    /// black in the stored picture: denylisted windows that are not on top.
    pub masks: Vec<Rect>,
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

/// A buffer the capturer keeps and shares (Windows' latest frame): handed
/// out without a copy.
impl Pixels for std::sync::Arc<Vec<u8>> {
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl Frame {
    /// A tightly packed RGBA8 frame.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn rgba(display: u32, width: u32, height: u32, rgba: Vec<u8>) -> Frame {
        Frame { display, width, height, stride: width as usize * 4, bgra: false, at: None, masks: Vec::new(), pixels: Box::new(rgba) }
    }

    pub(crate) fn lent(display: u32, width: u32, height: u32, stride: usize, bgra: bool, pixels: Box<dyn Pixels>) -> Frame {
        Frame { display, width, height, stride, bgra, at: None, masks: Vec::new(), pixels }
    }

    /// The same frame stamped `at` (Unix ms).
    pub(crate) fn stamped(mut self, at: i64) -> Frame {
        self.at = Some(at);
        self
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

/// One display a grab asks for. STALE: a frame the OS delivered before
/// `since` is not taken (a capturer that reads live ignores it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Want {
    pub display: u32,
    pub since: Option<Instant>,
}

impl Want {
    /// Whether a frame delivered at `delivered` may be taken.
    #[allow(dead_code, reason = "the X11 root reader reads live")]
    pub(crate) fn takes(&self, delivered: Instant) -> bool {
        self.since.is_none_or(|since| delivered >= since)
    }
}

pub(crate) trait Capturer: Send {
    /// The wanted displays now, each only when it has a frame new since the
    /// last `grab_all` and delivered no earlier than its `since` (a display
    /// without one is left out; an empty Vec is not an error).
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String>;
    /// One display's latest frame delivered no earlier than its `since`
    /// (the focus-change capture, the retry after a skipped batch); `None` =
    /// nothing (new enough) yet.
    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String>;
    /// FRESH: one capture of `display` taken NOW, not the stream's last
    /// delivery (a still screen after a resume, which the OS stream never
    /// delivers again); `None` = this capturer cannot (the next delivery
    /// counts).
    fn fresh(&mut self, display: u32) -> Result<Option<Frame>, String> {
        let _ = display;
        Ok(None)
    }
}

/// Unix ms now.
pub(crate) fn wall_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_millis() as i64).unwrap_or_default()
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
