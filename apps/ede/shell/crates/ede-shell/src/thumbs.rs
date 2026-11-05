//! Live thumbnails for the switcher (#EDE-12): the compositor's window
//! captures (ede-protocol, software-drawn) shrunk with a box filter, PNG
//! encoded and handed to `Image` as `data:` URLs at about 5 fps while the
//! switcher is open. gpui cannot show a foreign Wayland buffer, so v1 is
//! pictures; dmabuf comes later (the plan's live-thumbnail note).

use base64::Engine;
use ede_protocol::Capture;

/// The thumbnail box; the picture keeps its aspect ratio inside it.
pub const THUMB: (u32, u32) = (184, 110);

/// Box-filtered `argb8888` → RGBA at most `max` wide and tall.
pub fn shrink(capture: &Capture, max: (u32, u32)) -> (u32, u32, Vec<u8>) {
    let (w, h) = (capture.info.width.max(1) as u32, capture.info.height.max(1) as u32);
    let scale = (max.0 as f64 / w as f64).min(max.1 as f64 / h as f64).min(1.0);
    let (tw, th) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    let stride = capture.info.stride as usize;
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for ty in 0..th {
        let y0 = (ty * h / th) as usize;
        let y1 = (((ty + 1) * h / th) as usize).max(y0 + 1).min(h as usize);
        for tx in 0..tw {
            let x0 = (tx * w / tw) as usize;
            let x1 = (((tx + 1) * w / tw) as usize).max(x0 + 1).min(w as usize);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = y * stride + x * 4;
                    let px = &capture.pixels[i..i + 4];
                    b += px[0] as u64;
                    g += px[1] as u64;
                    r += px[2] as u64;
                    a += px[3] as u64;
                    n += 1;
                }
            }
            out.extend_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8, (a / n) as u8]);
        }
    }
    (tw, th, out)
}

/// A `data:image/png;base64,…` URL of the capture.
pub fn data_url(capture: &Capture) -> Option<String> {
    let (w, h, rgba) = shrink(capture, THUMB);
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// The capture at full size as a PNG file (the Print key).
pub fn write_png(path: &std::path::Path, capture: &Capture) -> std::io::Result<()> {
    let (w, h) = (capture.info.width as u32, capture.info.height as u32);
    let stride = capture.info.stride as usize;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        for px in capture.pixels[y * stride..y * stride + w as usize * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::CaptureInfo;

    #[test]
    fn shrinks_keeping_the_aspect_and_encodes_a_png() {
        let (w, h) = (400, 100);
        let mut pixels = Vec::new();
        for _ in 0..(w * h) {
            pixels.extend_from_slice(&0xff00_80ffu32.to_le_bytes());
        }
        let capture = Capture { info: CaptureInfo { width: w, height: h, stride: w * 4, format: "argb8888".into() }, pixels };
        let (tw, th, rgba) = shrink(&capture, THUMB);
        assert_eq!((tw, th), (184, 46));
        assert_eq!(&rgba[..4], &[0x00, 0x80, 0xff, 0xff]);
        let url = data_url(&capture).unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBOR"));
    }
}
