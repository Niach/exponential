//! HASH (`hashCases`): a 64-bit average hash of the frame. Integer luma,
//! a box downscale to 64 x 64 that reads every kx-th pixel of every ky-th
//! row of each box (kx = max(1, W / 256), ky likewise: about 4 x 4 samples
//! a box, so a 5K frame costs ~65 k lumas, not 15 M), 8 x 8 blocks of
//! 8 x 8, bit k (the first block the most significant) set when the block is
//! brighter than the mean. MASK: a masked pixel reads black (luma 0), as if
//! the frame had been filled there.

use crate::store::Rect;

const SIDE: usize = 64;
const BLOCKS: usize = 8;

/// The source span of target index `index` along an axis of `len` pixels:
/// [i*len/64, max((i+1)*len/64, i*len/64 + 1)).
fn span(index: usize, len: usize) -> (usize, usize) {
    let start = index * len / SIDE;
    (start, ((index + 1) * len / SIDE).max(start + 1))
}

/// The sampling step along an axis of `len` pixels.
fn step(len: usize) -> usize {
    (len / (SIDE * 4)).max(1)
}

/// Integer luma of one pixel whose red is byte `R` and blue byte `2 - R`.
#[inline]
fn luma<const R: usize>(pixel: &[u8]) -> u32 {
    (299 * u32::from(pixel[R]) + 587 * u32::from(pixel[1]) + 114 * u32::from(pixel[2 - R])) / 1000
}

/// The hash of a tightly packed RGBA8 image; 0 for an empty one.
#[cfg(test)]
pub(crate) fn hash(width: u32, height: u32, rgba: &[u8]) -> u64 {
    if rgba.len() < width as usize * height as usize * 4 {
        return 0;
    }
    hash_rows(width, height, width as usize * 4, false, rgba, &[])
}

/// The hash of a frame as the OS lends it: rows `stride` bytes apart, RGBA
/// or (`bgra`) BGRA, `masks` read as black; 0 for an empty one.
pub(crate) fn hash_rows(width: u32, height: u32, stride: usize, bgra: bool, bytes: &[u8], masks: &[Rect]) -> u64 {
    if bgra {
        hash_with::<2>(width, height, stride, bytes, masks)
    } else {
        hash_with::<0>(width, height, stride, bytes, masks)
    }
}

fn hash_with<const R: usize>(width: u32, height: u32, stride: usize, bytes: &[u8], masks: &[Rect]) -> u64 {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || stride < width * 4 || bytes.len() < stride * (height - 1) + width * 4 {
        return 0;
    }
    let (kx, ky) = (step(width), step(height));
    let columns: Vec<(usize, usize)> = (0..SIDE).map(|i| span(i, width)).collect();
    let mut targets = [0u64; SIDE * SIDE];
    let mut covered: Vec<(usize, usize)> = Vec::new();
    for j in 0..SIDE {
        let (top, bottom) = span(j, height);
        let mut sums = [0u64; SIDE];
        let mut rows = 0u64;
        for y in (top..bottom).step_by(ky) {
            rows += 1;
            let row = &bytes[y * stride..y * stride + width * 4];
            covered.clear();
            covered.extend(masks.iter().filter(|mask| (mask.y.max(0) as usize..(mask.y.max(0) as usize + mask.height as usize)).contains(&y)).map(|mask| {
                let left = mask.x.max(0) as usize;
                (left, left + mask.width as usize)
            }));
            for (sum, &(left, right)) in sums.iter_mut().zip(&columns) {
                let part: u32 = if covered.is_empty() {
                    (left..right).step_by(kx).map(|x| luma::<R>(&row[x * 4..x * 4 + 4])).sum()
                } else {
                    (left..right)
                        .step_by(kx)
                        .filter(|x| !covered.iter().any(|(from, to)| (*from..*to).contains(x)))
                        .map(|x| luma::<R>(&row[x * 4..x * 4 + 4]))
                        .sum()
                };
                *sum += u64::from(part);
            }
        }
        for (i, sum) in sums.iter().enumerate() {
            let (left, right) = columns[i];
            targets[j * SIDE + i] = sum / (rows * (right - left).div_ceil(kx) as u64);
        }
    }
    let total: u64 = targets.iter().sum();
    let cell = SIDE / BLOCKS;
    let mut hash = 0u64;
    for k in 0..BLOCKS * BLOCKS {
        let (block_row, block_column) = (k / BLOCKS, k % BLOCKS);
        let mut block = 0u64;
        for j in block_row * cell..(block_row + 1) * cell {
            for i in block_column * cell..(block_column + 1) * cell {
                block += targets[j * SIDE + i];
            }
        }
        if 64 * block > total {
            hash |= 1 << (63 - k);
        }
    }
    hash
}

/// The Hamming distance of two hashes.
pub(crate) fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// The 16 lowercase hex digits the index stores.
pub(crate) fn hex(hash: u64) -> String {
    format!("{hash:016x}")
}

#[cfg(test)]
pub(crate) fn parse(hex: &str) -> Option<u64> {
    u64::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fixture::fixture;
    use serde_json::Value;

    /// The fixture's `pattern` images, RGBA8.
    pub(crate) fn pattern(image: &Value) -> (u32, u32, Vec<u8>) {
        let width = image["width"].as_u64().unwrap() as u32;
        let height = image["height"].as_u64().unwrap() as u32;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let rgb: [u32; 3] = match image["pattern"].as_str().unwrap() {
                    "solid" => {
                        let rgb = image["rgb"].as_array().unwrap();
                        [0, 1, 2].map(|c| rgb[c].as_u64().unwrap() as u32)
                    }
                    "split" => [if x < width / 2 { 0 } else { 255 }; 3],
                    "checker" => {
                        let cell = image["cell"].as_u64().unwrap() as u32;
                        [if (x / cell + y / cell) % 2 == 0 { 0 } else { 255 }; 3]
                    }
                    "gradientX" => [255 * x / (width - 1); 3],
                    "gradientY" => {
                        let v = 255 * y / (height - 1);
                        [v, 0, 255 - v]
                    }
                    other => panic!("unknown pattern {other}"),
                };
                rgba.extend(rgb.map(|c| c as u8));
                rgba.push(255);
            }
        }
        (width, height, rgba)
    }

    #[test]
    fn the_hash_replays_every_hash_case() {
        for case in fixture()["hashCases"].as_array().unwrap() {
            let (width, height, rgba) = pattern(&case["image"]);
            assert_eq!(hex(hash(width, height, &rgba)), case["hash"].as_str().unwrap(), "{}", case["image"]);
        }
    }

    #[test]
    fn distance_counts_differing_bits_and_hex_round_trips() {
        assert_eq!(distance(parse("0f0f0f0f0f0f0f0f").unwrap(), parse("0f0f0f0f0f0f0f1f").unwrap()), 1);
        assert_eq!(distance(0, u64::MAX), 64);
        assert_eq!(hex(parse("00000000ffffffff").unwrap()), "00000000ffffffff");
        assert_eq!(hash(0, 0, &[]), 0);
    }

    #[test]
    fn a_padded_bgra_frame_hashes_like_its_rgba() {
        for case in fixture()["hashCases"].as_array().unwrap() {
            let (width, height, rgba) = pattern(&case["image"]);
            let stride = width as usize * 4 + 12;
            let mut bgra = vec![7u8; stride * height as usize];
            for (y, row) in rgba.chunks_exact(width as usize * 4).enumerate() {
                for (x, px) in row.chunks_exact(4).enumerate() {
                    bgra[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&[px[2], px[1], px[0], 0]);
                }
            }
            assert_eq!(hex(hash_rows(width, height, stride, true, &bgra, &[])), case["hash"].as_str().unwrap(), "{}", case["image"]);
        }
    }

    /// MASK: a masked rect hashes exactly like the frame filled black there
    /// (a rect past the edge clips).
    #[test]
    fn a_masked_rect_hashes_like_the_frame_filled_black() {
        let image = serde_json::json!({ "width": 1030, "height": 600, "pattern": "gradientX" });
        let (width, height, mut rgba) = pattern(&image);
        let masks = [Rect { x: 100, y: 40, width: 300, height: 200 }, Rect { x: 900, y: 500, width: 400, height: 400 }];
        let masked = hash_rows(width, height, width as usize * 4, false, &rgba, &masks);
        for mask in &masks {
            for y in mask.y as u32..(mask.y as u32 + mask.height).min(height) {
                for x in mask.x as u32..(mask.x as u32 + mask.width).min(width) {
                    let at = ((y * width + x) * 4) as usize;
                    rgba[at..at + 3].fill(0);
                }
            }
        }
        assert_eq!(masked, hash(width, height, &rgba));
        assert_ne!(masked, hash_rows(width, height, width as usize * 4, false, &pattern(&image).2, &[]));
    }
}
