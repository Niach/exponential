//! NEW-58: the Linux capturer (screen-memory.json CAPTURE).
//!
//! - The xdg ScreenCast portal + PipeWire (`portal`): ONE portal session for
//!   every monitor, `persist_mode` 2, the restore token in a file under
//!   `state_dir` so the portal asks once.
//! - X11 without a portal (missing or declined): the root window read per
//!   RandR monitor, like the computer crate's screenshots.
//! - Wayland without a portal: `Err`, nothing else can read the screen.
//!
//! Display numbers are the computer crate's (`computer::displays`, the RandR
//! monitors' order). On pure Wayland the computer crate has no displays, so
//! the portal's streams are numbered by position (left to right).

#[cfg(feature = "portal")]
mod portal;

use std::path::Path;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use super::{Capturer, Frame};

/// How long a read of the display set is reused (it opens an X connection).
const DISPLAYS_TTL: Duration = Duration::from_secs(5);

pub(super) fn open(state_dir: &Path) -> Result<Box<dyn Capturer>, String> {
    #[cfg(feature = "portal")]
    let portal_reason = match portal::Portal::open(state_dir) {
        Ok(portal) => return Ok(Box::new(portal)),
        // The person said no: that is an answer, never a reason to read the
        // X root behind their back.
        Err(reason) if reason == portal::DECLINED => return Err("Screen sharing was declined.".to_string()),
        Err(reason) => reason,
    };
    #[cfg(not(feature = "portal"))]
    let portal_reason = {
        let _ = state_dir;
        "this build has no ScreenCast portal".to_string()
    };
    if wayland() {
        return Err(format!("Screen memory needs the ScreenCast portal on Wayland: {portal_reason}"));
    }
    if std::env::var_os("DISPLAY").is_none() {
        return Err(format!("No screen to capture: {portal_reason}, and DISPLAY is not set."));
    }
    log::info!("[screen] no ScreenCast portal ({portal_reason}); reading the X root window");
    Ok(Box::new(RootReader::connect()?))
}

fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
}

/// `computer::displays()`, re-read at most every [`DISPLAYS_TTL`].
#[derive(Default)]
struct DisplayCache {
    read_at: Option<Instant>,
    displays: Vec<computer::Display>,
}

impl DisplayCache {
    fn get(&mut self) -> &[computer::Display] {
        if self.read_at.is_none_or(|at| at.elapsed() >= DISPLAYS_TTL) {
            self.displays = computer::displays();
            self.read_at = Some(Instant::now());
        }
        &self.displays
    }
}

/// A display's number and rect: (number, x, y, width, height).
type Rect = (u32, i32, i32, u32, u32);

/// X11 without a portal: `GetImage` on the root, one rect per display.
struct RootReader {
    conn: RustConnection,
    screen: usize,
    root: xproto::Window,
    displays: DisplayCache,
}

impl RootReader {
    fn connect() -> Result<RootReader, String> {
        let (conn, screen) = x11rb::connect(None).map_err(|error| format!("X connection: {error}"))?;
        let root = conn.setup().roots[screen].root;
        Ok(RootReader { conn, screen, root, displays: DisplayCache::default() })
    }

    /// The display rects (number, x, y, width, height); the whole root as
    /// display 1 when the computer crate reports none.
    fn rects(&mut self) -> Result<Vec<Rect>, String> {
        let rects: Vec<_> = self
            .displays
            .get()
            .iter()
            .map(|display| (display.number, display.x, display.y, display.width, display.height))
            .collect();
        if !rects.is_empty() {
            return Ok(rects);
        }
        let geometry = self
            .conn
            .get_geometry(self.root)
            .map_err(|error| format!("root geometry: {error}"))?
            .reply()
            .map_err(|error| format!("root geometry: {error}"))?;
        Ok(vec![(1, 0, 0, u32::from(geometry.width), u32::from(geometry.height))])
    }

    fn read(&self, (number, x, y, width, height): Rect) -> Result<Frame, String> {
        let shot = self
            .conn
            .get_image(xproto::ImageFormat::Z_PIXMAP, self.root, x as i16, y as i16, width as u16, height as u16, !0)
            .map_err(|error| format!("screen capture: {error}"))?
            .reply()
            .map_err(|error| format!("screen capture: {error}"))?;
        let setup = self.conn.setup();
        let bits = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == shot.depth)
            .map(|format| format.bits_per_pixel)
            .ok_or_else(|| format!("No pixmap format for depth {}.", shot.depth))?;
        let visual = setup.roots[self.screen]
            .allowed_depths
            .iter()
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.visual_id == shot.visual)
            .ok_or("The root visual is unknown.")?;
        let little = setup.image_byte_order == xproto::ImageOrder::LSB_FIRST;
        let rgba = x_pixels_to_rgba(
            &shot.data,
            usize::from(bits / 8),
            little,
            (visual.red_mask, visual.green_mask, visual.blue_mask),
            (width as usize) * (height as usize),
        )?;
        Ok(Frame { display: number, width, height, rgba })
    }
}

impl Capturer for RootReader {
    fn grab_all(&mut self) -> Result<Vec<Frame>, String> {
        let rects = self.rects()?;
        let mut frames = Vec::with_capacity(rects.len());
        let mut failure = None;
        for rect in rects {
            match self.read(rect) {
                Ok(frame) => frames.push(frame),
                Err(reason) => {
                    log::debug!("[screen] display {} not read: {reason}", rect.0);
                    failure = Some(reason);
                }
            }
        }
        match failure {
            Some(reason) if frames.is_empty() => Err(reason),
            _ => Ok(frames),
        }
    }

    fn grab(&mut self, display: u32) -> Result<Option<Frame>, String> {
        match self.rects()?.into_iter().find(|rect| rect.0 == display) {
            Some(rect) => self.read(rect).map(Some),
            None => Ok(None),
        }
    }
}

/// A Z-pixmap of `count` pixels → RGBA8 (the computer crate's conversion:
/// a straight copy for the common 32-bit BGRX little-endian screen, the
/// visual's masks otherwise).
fn x_pixels_to_rgba(data: &[u8], bytes: usize, little: bool, masks: (u32, u32, u32), count: usize) -> Result<Vec<u8>, String> {
    if bytes != 4 && bytes != 3 {
        return Err(format!("Unsupported screen format: {} bits per pixel.", bytes * 8));
    }
    if data.len() < count * bytes {
        return Err("The screen capture came back short.".to_string());
    }
    let mut rgba = vec![255u8; count * 4];
    if bytes == 4 && little && masks == (0xff0000, 0xff00, 0xff) {
        for (out, pixel) in rgba.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
            out[0] = pixel[2];
            out[1] = pixel[1];
            out[2] = pixel[0];
        }
        return Ok(rgba);
    }
    let channel = |pixel: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let width = (mask >> shift).count_ones();
        let value = (pixel & mask) >> shift;
        (if width >= 8 { value >> (width - 8) } else { value << (8 - width) }) as u8
    };
    for (out, pixel) in rgba.chunks_exact_mut(4).zip(data.chunks_exact(bytes)) {
        let value = match (bytes, little) {
            (4, true) => u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]),
            (4, false) => u32::from_be_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]),
            (_, true) => u32::from_le_bytes([pixel[0], pixel[1], pixel[2], 0]),
            (_, false) => u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]),
        };
        out[0] = channel(value, masks.0);
        out[1] = channel(value, masks.1);
        out[2] = channel(value, masks.2);
    }
    Ok(rgba)
}

/// A portal stream as the portal describes it (compositor coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StreamRect {
    position: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
}

/// The display number of each portal stream. With the computer crate's
/// displays (X11): the display the stream's rect overlaps most (an exact
/// rect wins), each display once; a stream left over takes the next free
/// display in position order, then numbers past the last display. Without
/// them (pure Wayland): 1.. by position, left to right then top to bottom,
/// position-less streams last in the portal's order.
fn number_streams(streams: &[StreamRect], displays: &[computer::Display]) -> Vec<u32> {
    let mut numbers: Vec<Option<u32>> = vec![None; streams.len()];
    let mut taken = vec![false; displays.len()];
    let mut pairs = Vec::new();
    for (stream_index, stream) in streams.iter().enumerate() {
        let (Some((x, y)), Some((width, height))) = (stream.position, stream.size) else {
            continue;
        };
        for (display_index, display) in displays.iter().enumerate() {
            let left = x.max(display.x) as i64;
            let top = y.max(display.y) as i64;
            let right = (x as i64 + width as i64).min(display.x as i64 + display.width as i64);
            let bottom = (y as i64 + height as i64).min(display.y as i64 + display.height as i64);
            let overlap = (right - left).max(0) * (bottom - top).max(0);
            if overlap == 0 {
                continue;
            }
            let exact = (x, y, width as i64, height as i64) == (display.x, display.y, display.width as i64, display.height as i64);
            pairs.push((exact, overlap, stream_index, display_index));
        }
    }
    // Best first: exact rects, then the largest overlap; ties by order.
    pairs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
    for (_, _, stream_index, display_index) in pairs {
        if numbers[stream_index].is_none() && !taken[display_index] {
            numbers[stream_index] = Some(displays[display_index].number);
            taken[display_index] = true;
        }
    }
    let mut left_over: Vec<usize> = (0..streams.len()).filter(|&index| numbers[index].is_none()).collect();
    left_over.sort_by_key(|&index| match streams[index].position {
        Some((x, y)) => (0, x, y),
        None => (1, 0, 0),
    });
    let mut free = displays.iter().zip(&taken).filter(|(_, taken)| !**taken).map(|(display, _)| display.number);
    let mut next = displays.iter().map(|display| display.number).max().unwrap_or(0);
    for index in left_over {
        numbers[index] = Some(free.next().unwrap_or_else(|| {
            next += 1;
            next
        }));
    }
    numbers.into_iter().map(|number| number.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(number: u32, x: i32, y: i32, width: u32, height: u32) -> computer::Display {
        computer::Display { number, x, y, width, height, primary: number == 1 }
    }

    fn stream(x: i32, y: i32, width: i32, height: i32) -> StreamRect {
        StreamRect { position: Some((x, y)), size: Some((width, height)) }
    }

    #[test]
    fn streams_take_the_number_of_the_randr_monitor_they_cover() {
        // RandR lists the right monitor first; the portal the left one.
        let displays = [display(1, 1920, 0, 2560, 1440), display(2, 0, 0, 1920, 1080)];
        let streams = [stream(0, 0, 1920, 1080), stream(1920, 0, 2560, 1440)];
        assert_eq!(number_streams(&streams, &displays), vec![2, 1]);
    }

    #[test]
    fn a_scaled_stream_maps_by_overlap_and_a_stranger_takes_the_free_number() {
        let displays = [display(1, 0, 0, 3840, 2160), display(2, 3840, 0, 1920, 1080)];
        // A logical (scaled) rect for display 1, none for display 2.
        let streams = [StreamRect { position: None, size: None }, stream(0, 0, 1920, 1080)];
        assert_eq!(number_streams(&streams, &displays), vec![2, 1]);
        // More streams than displays: numbers past the last one.
        let streams = [stream(0, 0, 3840, 2160), stream(9000, 0, 10, 10), stream(3840, 0, 1920, 1080)];
        let three = [display(1, 0, 0, 3840, 2160)];
        assert_eq!(number_streams(&streams, &three), vec![1, 3, 2]);
    }

    #[test]
    fn without_displays_streams_number_by_position() {
        let streams = [stream(1920, 0, 1920, 1080), StreamRect { position: None, size: None }, stream(0, 0, 1920, 1080)];
        assert_eq!(number_streams(&streams, &[]), vec![2, 3, 1]);
    }

    #[test]
    fn x_pixels_convert_bgrx_and_565() {
        let bgrx = [10, 20, 30, 0, 1, 2, 3, 0];
        assert_eq!(x_pixels_to_rgba(&bgrx, 4, true, (0xff0000, 0xff00, 0xff), 2).unwrap(), vec![30, 20, 10, 255, 3, 2, 1, 255]);
        // 565 is not 3 or 4 bytes: refused.
        assert!(x_pixels_to_rgba(&[0, 0], 2, true, (0xf800, 0x7e0, 0x1f), 1).is_err());
        assert!(x_pixels_to_rgba(&bgrx, 4, true, (0xff0000, 0xff00, 0xff), 3).is_err());
    }
}
