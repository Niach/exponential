//! HASH (`hashCases`): a 64-bit average hash of the frame. Integer luma,
//! a box downscale to 64 x 64, 8 x 8 blocks of 8 x 8, bit k (the first block
//! the most significant) set when the block is brighter than the mean.
//! One pass over the source rows, each summed in its 64 column spans, so
//! the cost is a luma per pixel and nothing else.

const SIDE: usize = 64;
const BLOCKS: usize = 8;

/// The source span of target index `index` along an axis of `len` pixels:
/// [i*len/64, max((i+1)*len/64, i*len/64 + 1)).
fn span(index: usize, len: usize) -> (usize, usize) {
    let start = index * len / SIDE;
    (start, ((index + 1) * len / SIDE).max(start + 1))
}

#[inline]
fn luma(pixel: &[u8]) -> u32 {
    (299 * u32::from(pixel[0]) + 587 * u32::from(pixel[1]) + 114 * u32::from(pixel[2])) / 1000
}

/// The hash of a tightly packed RGBA8 image; 0 for an empty one.
pub(crate) fn hash(width: u32, height: u32, rgba: &[u8]) -> u64 {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return 0;
    }
    let columns: Vec<(usize, usize)> = (0..SIDE).map(|i| span(i, width)).collect();
    let mut targets = [0u64; SIDE * SIDE];
    for j in 0..SIDE {
        let (top, bottom) = span(j, height);
        let mut sums = [0u64; SIDE];
        for y in top..bottom {
            let row = &rgba[y * width * 4..(y + 1) * width * 4];
            for (sum, &(left, right)) in sums.iter_mut().zip(&columns) {
                let part: u32 = row[left * 4..right * 4].chunks_exact(4).map(luma).sum();
                *sum += u64::from(part);
            }
        }
        let rows = (bottom - top) as u64;
        for (i, sum) in sums.iter().enumerate() {
            let (left, right) = columns[i];
            targets[j * SIDE + i] = sum / (rows * (right - left) as u64);
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
}
