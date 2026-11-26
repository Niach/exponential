//! A screenshot from per-output captures: the Print keys (the shell) and
//! the Screenshot portal draw the same picture. The layout is logical and
//! each capture is its output's mode in pixels (#EDE-101), with real
//! positions and fractional scales (1.25 beside 1.0): the picture is drawn
//! at the highest scale among the outputs it covers, every pixel sampled
//! from the output under its centre, so no screen covers another and the
//! sharpest one stays 1:1.

use crate::{Capture, OutputInfo};

/// A rectangle in the global logical layout (as [`OutputInfo`] places outputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn of(o: &OutputInfo) -> Rect {
        Rect { x: o.x, y: o.y, width: o.width, height: o.height }
    }

    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.width && o.x < self.x + self.width && self.y < o.y + o.height && o.y < self.y + self.height
    }
}

/// Every output's rectangle together.
pub fn bounds(outputs: &[OutputInfo]) -> Option<Rect> {
    let x0 = outputs.iter().map(|o| o.x).min()?;
    let y0 = outputs.iter().map(|o| o.y).min()?;
    let x1 = outputs.iter().map(|o| o.x + o.width).max()?;
    let y1 = outputs.iter().map(|o| o.y + o.height).max()?;
    Some(Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
}

/// `region` (default: every output) as `(width, height, RGBA)`, from
/// `shots[i]` = the capture of `outputs[i]`. Where no output is, black.
pub fn compose(outputs: &[OutputInfo], shots: &[Capture], region: Option<Rect>) -> Option<(usize, usize, Vec<u8>)> {
    let region = region.or_else(|| bounds(outputs))?;
    if region.width <= 0 || region.height <= 0 {
        return None;
    }
    // Pixels per logical unit of each output, as captured.
    let ratio = |o: &OutputInfo, s: &Capture| s.info.width as f64 / o.width.max(1) as f64;
    let covered: Vec<(&OutputInfo, &Capture)> = outputs.iter().zip(shots).filter(|(o, _)| Rect::of(o).overlaps(&region)).collect();
    let k = covered.iter().map(|(o, s)| ratio(o, s)).fold(0.0_f64, f64::max);
    if k <= 0.0 {
        return None;
    }
    let width = (region.width as f64 * k).round() as usize;
    let height = (region.height as f64 * k).round() as usize;
    let mut rgba = [0u8, 0, 0, 0xff].repeat(width * height);
    for (o, shot) in covered {
        let r = ratio(o, shot);
        let (sw, sh) = (shot.info.width, shot.info.height);
        // The canvas pixels whose centres fall on this output.
        let span = |lo: i32, len: i32, from: i32| -> (usize, usize) {
            let a = (((lo - from) as f64) * k - 0.5).ceil().max(0.0) as usize;
            let b = (((lo + len - from) as f64) * k - 0.5).ceil().max(0.0) as usize;
            (a, b)
        };
        let (cx0, cx1) = span(o.x, o.width, region.x);
        let (cy0, cy1) = span(o.y, o.height, region.y);
        for cy in cy0..cy1.min(height) {
            let ly = region.y as f64 + (cy as f64 + 0.5) / k - o.y as f64;
            let sy = ((ly * r) as i32).clamp(0, sh - 1);
            for cx in cx0..cx1.min(width) {
                let lx = region.x as f64 + (cx as f64 + 0.5) / k - o.x as f64;
                let p = shot.at(((lx * r) as i32).clamp(0, sw - 1), sy);
                let i = (cy * width + cx) * 4;
                rgba[i..i + 4].copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 0xff]);
            }
        }
    }
    Some((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaptureInfo;

    fn output(name: &str, x: i32, width: i32, height: i32, scale: f64) -> OutputInfo {
        OutputInfo { name: name.into(), x, y: 0, width, height, scale, modes: vec![], make: String::new(), model: String::new(), physical_mm: [0, 0], space: None }
    }

    /// A capture whose every pixel names its place: R = y, G = x (mod 256), B = `tag`.
    fn shot(w: i32, h: i32, tag: u8) -> Capture {
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&[tag, (x % 256) as u8, (y % 256) as u8, 0xff]);
            }
        }
        Capture { info: CaptureInfo { width: w, height: h, stride: w * 4, format: "argb8888".into(), serial: 1, unchanged: false, notice: None }, pixels }
    }

    fn px(img: &(usize, usize, Vec<u8>), x: usize, y: usize) -> [u8; 3] {
        let i = (y * img.0 + x) * 4;
        [img.2[i], img.2[i + 1], img.2[i + 2]]
    }

    /// Danny's mint: a 3840×2160 screen at 1.0 and a 2560×1440 one at 1.25
    /// to its right (logical 2048×1152 at x 3840).
    #[test]
    fn a_fractional_screen_beside_a_plain_one_is_drawn_at_the_higher_scale() {
        let outputs = [output("HDMI-A-3", 0, 3840, 2160, 1.0), output("DP-2", 3840, 2048, 1152, 1.25)];
        let shots = [shot(3840, 2160, 1), shot(2560, 1440, 2)];
        let img = compose(&outputs, &shots, None).unwrap();
        assert_eq!((img.0, img.1), (7360, 2700));
        // The 1.25 screen's pixels land 1:1 at x 4800.
        assert_eq!(px(&img, 4800 + 77, 33), [33, 77, 2]);
        assert_eq!(px(&img, 4800 + 2559, 1439), [(1439 % 256) as u8, (2559 % 256) as u8, 2]);
        // Under the 1.25 screen's bottom: nothing.
        assert_eq!(px(&img, 5000, 2000), [0, 0, 0]);
    }

    /// An area wholly on the 1.25 screen: its own pixels, 1:1.
    #[test]
    fn an_area_on_one_screen_keeps_that_screens_pixels() {
        let outputs = [output("A", 0, 3840, 2160, 1.0), output("B", 3840, 2048, 1152, 1.25)];
        let shots = [shot(3840, 2160, 1), shot(2560, 1440, 2)];
        let img = compose(&outputs, &shots, Some(Rect { x: 3840 + 100, y: 40, width: 200, height: 80 })).unwrap();
        assert_eq!((img.0, img.1), (250, 100));
        assert_eq!(px(&img, 0, 0), [50, 125, 2]);
        // On the plain screen alone: 1:1 there.
        let img = compose(&outputs, &shots, Some(Rect { x: 10, y: 20, width: 30, height: 40 })).unwrap();
        assert_eq!((img.0, img.1), (30, 40));
        assert_eq!(px(&img, 5, 6), [26, 15, 1]);
    }

    #[test]
    fn an_empty_area_is_nothing() {
        let outputs = [output("A", 0, 100, 100, 1.0)];
        assert!(compose(&outputs, &[shot(100, 100, 1)], Some(Rect { x: 0, y: 0, width: 0, height: 5 })).is_none());
        assert!(compose(&[], &[], None).is_none());
    }
}
