//! STORE's picture: what `get_screenshots` returns is what is stored, so a
//! kept frame is cropped to its focused window (when that is at least
//! `tool.minCrop` each way), fitted to `tool.maxEdge` and only then encoded
//! (lossy WebP, `store.webpQuality`). A 5K frame encodes at 1568 px, not
//! 5120: the encode shrinks with the pixel count.
//!
//! The fit is an integer 2 x 2 box halving while the image is at least
//! twice the target each way, then one separable Triangle (linear) pass in
//! 12-bit fixed point for the rest: vertical first (it vectorizes), then
//! horizontal over the fewer rows (unrolled per tap count). A 2765 x 2234
//! window to 1568 px: ~8 ms, against ~25 ms for `image`'s resize.

use crate::capture::Frame;
use crate::store::Rect;
use crate::{fixture, ring};

/// An RGBA8 picture, tightly packed.
pub(crate) struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The rect the stored picture is cut to: the focused window's when it lies
/// within the frame and is at least `tool.minCrop` each way (a tab-hover
/// strip or a tooltip that holds the focus keeps the whole frame).
pub(crate) fn crop_of(focus: Option<Rect>, width: u32, height: u32) -> Option<Rect> {
    focus
        .filter(|rect| rect.x >= 0 && rect.y >= 0 && rect.x as u32 + rect.width <= width && rect.y as u32 + rect.height <= height)
        .filter(|rect| rect.width >= fixture::min_crop() && rect.height >= fixture::min_crop())
}

/// The frame cut to `crop_of(focus)` and fitted to `tool.maxEdge`.
pub(crate) fn prepare(frame: &Frame, focus: Option<Rect>) -> Picture {
    let rect = crop_of(focus, frame.width, frame.height).unwrap_or(Rect { x: 0, y: 0, width: frame.width, height: frame.height });
    let (target_width, target_height) = ring::fit(rect.width, rect.height, None);
    let mut picture = cut(frame, rect);
    mask(&mut picture, rect, &frame.masks);
    while picture.width >= 2 * target_width && picture.height >= 2 * target_height {
        picture = halve(&picture);
    }
    let mut picture = horizontal(vertical(picture, target_height), target_width);
    // The fit treats channels alike, so BGRA turns RGBA on the fewest
    // pixels, last.
    if frame.bgra {
        for pixel in picture.rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
    }
    picture
}

/// `picture` scaled down to `width` x `height` (each at most its own).
pub(crate) fn resize(picture: Picture, width: u32, height: u32) -> Picture {
    horizontal(vertical(picture, height.max(1)), width.max(1))
}

const ONE: u32 = 1 << 12;

/// Triangle taps for `dst` samples over `src` (dst <= src): per output, the
/// first source index and its weights (summing to `ONE`).
fn taps(src: usize, dst: usize) -> Vec<(usize, Vec<u32>)> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|out| {
            let center = (out as f64 + 0.5) * scale;
            let first = (center - scale).floor().max(0.) as usize;
            let last = ((center + scale).ceil() as usize).min(src);
            let raw: Vec<f64> = (first..last).map(|i| (1. - (i as f64 + 0.5 - center).abs() / scale).max(0.)).collect();
            let total: f64 = raw.iter().sum();
            let mut weights: Vec<u32> = raw.iter().map(|w| (w / total * f64::from(ONE)).round() as u32).collect();
            // The rounding rest goes to the heaviest tap.
            let heaviest = (0..weights.len()).max_by_key(|&i| weights[i]).unwrap_or(0);
            let sum: u32 = weights.iter().sum();
            weights[heaviest] = (weights[heaviest] + ONE).saturating_sub(sum);
            let skip = weights.iter().take_while(|w| **w == 0).count();
            weights.drain(..skip);
            while weights.last() == Some(&0) {
                weights.pop();
            }
            (first + skip, weights)
        })
        .collect()
}

fn vertical(picture: Picture, height: u32) -> Picture {
    if height == picture.height {
        return picture;
    }
    let stride = picture.width as usize * 4;
    let mut rgba = Vec::with_capacity(stride * height as usize);
    let mut sums = vec![0u32; stride];
    for (first, weights) in taps(picture.height as usize, height as usize) {
        sums.fill(ONE / 2);
        for (k, &weight) in weights.iter().enumerate() {
            let row = &picture.rgba[(first + k) * stride..(first + k + 1) * stride];
            for (sum, &v) in sums.iter_mut().zip(row) {
                *sum += weight * u32::from(v);
            }
        }
        rgba.extend(sums.iter().map(|sum| (sum >> 12).min(255) as u8));
    }
    Picture { width: picture.width, height, rgba }
}

fn horizontal(picture: Picture, width: u32) -> Picture {
    if width == picture.width {
        return picture;
    }
    let (src, stride) = (picture.width as usize, picture.width as usize * 4);
    // Every output reads the same number of taps (zero-padded, shifted
    // inside the row at the edges): a fixed inner loop.
    let taps = taps(src, width as usize);
    let n = taps.iter().map(|(_, weights)| weights.len()).max().unwrap_or(1).min(src);
    let mut firsts = Vec::with_capacity(taps.len());
    let mut flat = Vec::with_capacity(taps.len() * n);
    for (first, weights) in taps {
        let start = first.min(src - n);
        firsts.push(start);
        flat.extend((start..start + n).map(|i| if i >= first { weights.get(i - first).copied().unwrap_or(0) } else { 0 }));
    }
    let mut rgba = vec![0u8; width as usize * picture.height as usize * 4];
    let out_stride = width as usize * 4;
    match n {
        2 => rows::<2>(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat),
        3 => rows::<3>(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat),
        4 => rows::<4>(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat),
        5 => rows::<5>(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat),
        6 => rows::<6>(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat),
        _ => rows_any(&picture.rgba, stride, &mut rgba, out_stride, &firsts, &flat, n),
    }
    Picture { width, height: picture.height, rgba }
}

/// The horizontal pass with `N` taps per output (unrolled).
fn rows<const N: usize>(source: &[u8], stride: usize, out: &mut [u8], out_stride: usize, firsts: &[usize], flat: &[u32]) {
    for (row, out) in source.chunks_exact(stride).zip(out.chunks_exact_mut(out_stride)) {
        for ((pixel, &first), weights) in out.chunks_exact_mut(4).zip(firsts).zip(flat.chunks_exact(N)) {
            let taps = &row[first * 4..(first + N) * 4];
            let weights: &[u32; N] = weights.try_into().expect("N weights");
            let mut sum = [ONE / 2; 4];
            for k in 0..N {
                let tap: &[u8; 4] = taps[k * 4..k * 4 + 4].try_into().expect("a pixel");
                for c in 0..4 {
                    sum[c] += weights[k] * u32::from(tap[c]);
                }
            }
            for c in 0..4 {
                pixel[c] = (sum[c] >> 12).min(255) as u8;
            }
        }
    }
}

fn rows_any(source: &[u8], stride: usize, out: &mut [u8], out_stride: usize, firsts: &[usize], flat: &[u32], n: usize) {
    for (row, out) in source.chunks_exact(stride).zip(out.chunks_exact_mut(out_stride)) {
        for ((pixel, &first), weights) in out.chunks_exact_mut(4).zip(firsts).zip(flat.chunks_exact(n)) {
            let mut sum = [ONE / 2; 4];
            for (source, &weight) in row[first * 4..(first + n) * 4].chunks_exact(4).zip(weights) {
                for c in 0..4 {
                    sum[c] += weight * u32::from(source[c]);
                }
            }
            for c in 0..4 {
                pixel[c] = (sum[c] >> 12).min(255) as u8;
            }
        }
    }
}

/// The rect's pixels, packed, in the frame's channel order.
fn cut(frame: &Frame, rect: Rect) -> Picture {
    let (x, width) = (rect.x as usize, rect.width as usize);
    let bytes = frame.bytes();
    let mut rgba = Vec::with_capacity(width * rect.height as usize * 4);
    for y in rect.y as usize..rect.y as usize + rect.height as usize {
        let row = &bytes[y * frame.stride + x * 4..y * frame.stride + (x + width) * 4];
        rgba.extend_from_slice(row);
    }
    Picture { width: rect.width, height: rect.height, rgba }
}

/// MASK: each of the frame's `masks` that falls in the cut `rect` filled
/// black (opaque) in the cut picture, before any scaling.
fn mask(picture: &mut Picture, rect: Rect, masks: &[Rect]) {
    let stride = picture.width as usize * 4;
    for mask in masks {
        let left = i64::from(mask.x).max(i64::from(rect.x));
        let top = i64::from(mask.y).max(i64::from(rect.y));
        let right = (i64::from(mask.x) + i64::from(mask.width)).min(i64::from(rect.x) + i64::from(rect.width));
        let bottom = (i64::from(mask.y) + i64::from(mask.height)).min(i64::from(rect.y) + i64::from(rect.height));
        if right <= left || bottom <= top {
            continue;
        }
        let (x0, x1) = ((left - i64::from(rect.x)) as usize, (right - i64::from(rect.x)) as usize);
        for y in (top - i64::from(rect.y)) as usize..(bottom - i64::from(rect.y)) as usize {
            for pixel in picture.rgba[y * stride + x0 * 4..y * stride + x1 * 4].chunks_exact_mut(4) {
                pixel.copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
}

/// A 2 x 2 box mean (rounded); an odd last row or column is dropped.
fn halve(picture: &Picture) -> Picture {
    let (width, height) = (picture.width as usize / 2, picture.height as usize / 2);
    let stride = picture.width as usize * 4;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let top = &picture.rgba[2 * y * stride..(2 * y + 1) * stride];
        let bottom = &picture.rgba[(2 * y + 1) * stride..(2 * y + 2) * stride];
        for x in 0..width {
            let at = x * 8;
            for c in 0..4 {
                let sum = u16::from(top[at + c]) + u16::from(top[at + 4 + c]) + u16::from(bottom[at + c]) + u16::from(bottom[at + 4 + c]);
                rgba.push(((sum + 2) / 4) as u8);
            }
        }
    }
    Picture { width: width as u32, height: height as u32, rgba }
}

/// Lossy WebP at `store.webpQuality`, `method` 0 of 0-6 (the fastest): at
/// 1568 px ~15 ms for a real screen, ~30 ms for a page of dense text.
pub(crate) fn encode(picture: &Picture) -> Vec<u8> {
    encode_at(picture, fixture::webp_quality())
}

/// Lossy WebP at `quality` (0-100), method 0.
pub(crate) fn encode_at(picture: &Picture, quality: f32) -> Vec<u8> {
    let encoder = webp::Encoder::from_rgba(&picture.rgba, picture.width, picture.height);
    let mut config = webp::WebPConfig::new().expect("libwebp config");
    config.lossless = 0;
    config.quality = quality;
    config.method = 0;
    match encoder.encode_advanced(&config) {
        Ok(memory) => memory.to_vec(),
        Err(err) => {
            log::warn!("[screen] WebP encoding failed: {err:?}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::fixture;

    fn frame(width: u32, height: u32) -> Frame {
        let rgba = (0..width * height).flat_map(|i| [(i % 251) as u8, (i % 13) as u8, 200, 255]).collect();
        Frame::rgba(1, width, height, rgba)
    }

    #[test]
    fn the_stored_picture_replays_every_fit_case() {
        for case in fixture()["fitCases"].as_array().unwrap() {
            let width = case["width"].as_u64().unwrap() as u32;
            let height = case["height"].as_u64().unwrap() as u32;
            let crop = case["crop"].as_object().map(|crop| Rect {
                x: crop["x"].as_i64().unwrap() as i32,
                y: crop["y"].as_i64().unwrap() as i32,
                width: crop["width"].as_u64().unwrap() as u32,
                height: crop["height"].as_u64().unwrap() as u32,
            });
            let picture = prepare(&frame(width, height), crop);
            let expect = (case["expect"]["width"].as_u64().unwrap() as u32, case["expect"]["height"].as_u64().unwrap() as u32);
            assert_eq!((picture.width, picture.height), expect, "{}", case["name"]);
            assert_eq!(picture.rgba.len(), (expect.0 * expect.1 * 4) as usize);
        }
    }

    /// MASK: a masked rect is black in the stored picture, also where it
    /// only partly lies in the window cut; the rest is untouched.
    #[test]
    fn a_masked_rect_is_filled_black_in_the_cut() {
        let mut frame = frame(320, 240);
        frame.masks = vec![Rect { x: 0, y: 0, width: 50, height: 60 }, Rect { x: 300, y: 200, width: 20, height: 40 }];
        let window = Rect { x: 10, y: 20, width: 240, height: 210 };
        let picture = prepare(&frame, Some(window));
        let at = |x: usize, y: usize| &picture.rgba[(y * 240 + x) * 4..(y * 240 + x) * 4 + 4];
        // (0..50, 0..60) of the frame = (0..40, 0..40) of the cut.
        assert_eq!(at(0, 0), [0, 0, 0, 255]);
        assert_eq!(at(39, 39), [0, 0, 0, 255]);
        assert_ne!(at(40, 39), [0, 0, 0, 255]);
        assert_ne!(at(39, 40), [0, 0, 0, 255]);
        let whole = prepare(&frame, None);
        assert_eq!(&whole.rgba[(239 * 320 + 319) * 4..], [0, 0, 0, 255]);
        assert_ne!(&whole.rgba[(199 * 320 + 319) * 4..(199 * 320 + 320) * 4], [0, 0, 0, 255]);
    }

    #[test]
    fn a_crop_under_min_crop_or_off_the_frame_keeps_the_whole_frame() {
        let strip = Rect { x: 0, y: 0, width: 320, height: 34 };
        let off = Rect { x: 200, y: 0, width: 300, height: 300 };
        for focus in [Some(strip), Some(off), None] {
            let picture = prepare(&frame(320, 240), focus);
            assert_eq!((picture.width, picture.height), (320, 240));
        }
        let window = Rect { x: 10, y: 20, width: 240, height: 210 };
        let picture = prepare(&frame(320, 240), Some(window));
        assert_eq!((picture.width, picture.height), (240, 210));
        // The cut starts at the window's corner.
        assert_eq!(picture.rgba[..4], frame(320, 240).bytes()[(20 * 320 + 10) * 4..(20 * 320 + 10) * 4 + 4]);
        // A padded BGRA frame cuts to the same RGBA.
        let rgba = frame(320, 240);
        let stride = 320 * 4 + 64;
        let mut bgra = vec![0u8; stride * 240];
        for (y, row) in rgba.bytes().chunks_exact(320 * 4).enumerate() {
            for (x, px) in row.chunks_exact(4).enumerate() {
                bgra[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&[px[2], px[1], px[0], 9]);
            }
        }
        let lent = Frame::lent(1, 320, 240, stride, true, Box::new(bgra));
        assert_eq!(prepare(&lent, Some(window)).rgba, picture.rgba);
    }

    #[test]
    fn the_linear_fit_keeps_flat_colour_and_weights_sum_to_one() {
        for (src, dst) in [(2765, 1568), (1787, 1014), (2560, 1568), (5, 3), (7, 7)] {
            for (first, weights) in taps(src, dst) {
                assert_eq!(weights.iter().sum::<u32>(), ONE, "{src} -> {dst}");
                assert!(first + weights.len() <= src);
            }
        }
        let flat = Picture { width: 7, height: 5, rgba: [12, 200, 255, 255].repeat(35) };
        let fitted = horizontal(vertical(flat, 3), 4);
        assert_eq!((fitted.width, fitted.height), (4, 3));
        assert_eq!(fitted.rgba, [12, 200, 255, 255].repeat(12));
    }

    #[test]
    fn halving_is_a_rounded_box_mean() {
        let picture = Picture { width: 3, height: 2, rgba: vec![0, 10, 255, 255, 1, 10, 255, 255, 9, 9, 9, 9, 2, 10, 0, 255, 2, 11, 0, 255, 9, 9, 9, 9] };
        let half = halve(&picture);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.rgba, [1, 10, 128, 255]);
    }
}

