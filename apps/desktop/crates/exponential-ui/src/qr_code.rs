//! VAPP-162: the QrCode native's MATRIX (catalog/style.json `qrCode`). A
//! faithful Rust port of Project Nayuki's QR Code generator library
//! (https://www.nayuki.io/page/qr-code-generator-library, MIT licence,
//! Copyright (c) Project Nayuki; the notice is in docs/third-party-licences.md
//! and packages/exponential-ui/vendor/nayuki-qr/LICENSE.txt), mirroring the
//! TS reference `src/qr-code.ts`: ONE byte-mode segment over the value's
//! UTF-8 bytes, the smallest version 1–40 that fits, the given error
//! correction level kept exact (no `boostEcl`), the mask with the lowest
//! penalty. `fixtures/qr-code.json` locks every matrix bit.
//!
//! Copyright (c) Project Nayuki. MIT License: the full notice ships in
//! packages/exponential-ui/vendor/nayuki-qr/LICENSE.txt and every client's NOTICES.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::generated::catalog as g;

/// style.json `qrCode`: the painted geometry's constants.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeSpec {
    pub quiet_zone: f64,
    pub label_gap: f64,
    pub max_value_bytes: usize,
}

pub static QR_CODE: LazyLock<QrCodeSpec> = LazyLock::new(|| {
    let style: serde_json::Value = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    serde_json::from_value(style["qrCode"].clone()).expect("style.json qrCode")
});

/// The error correction level (catalog enum): anything else is `M`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String")]
pub enum QrLevel {
    L,
    #[default]
    M,
    Q,
    H,
}

impl From<String> for QrLevel {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl QrLevel {
    pub fn parse(s: &str) -> Self {
        match s {
            "L" => Self::L,
            "Q" => Self::Q,
            "H" => Self::H,
            _ => Self::M,
        }
    }

    fn ordinal(self) -> usize {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }

    /// The two format bits the spec gives each level (L 1, M 0, Q 3, H 2).
    fn format_bits(self) -> u32 {
        match self {
            Self::L => 1,
            Self::M => 0,
            Self::Q => 3,
            Self::H => 2,
        }
    }
}

/// `size` rows of `size` characters: '1' = dark, '0' = light; no quiet zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrMatrix {
    /// Modules per side (21 + 4 × (version − 1)).
    pub size: usize,
    pub version: usize,
    /// The mask pattern chosen (0..7).
    pub mask: u8,
    pub rows: Vec<String>,
}

impl QrMatrix {
    /// Whether the module at column `x`, row `y` is dark.
    pub fn dark(&self, x: usize, y: usize) -> bool {
        self.rows.get(y).and_then(|r| r.as_bytes().get(x)).is_some_and(|b| *b == b'1')
    }
}

/// The painted geometry of a QrCode of edge `size` px: (module edge, quiet
/// zone inset), both px, fractional (painters snap module EDGES).
pub fn qr_layout(size: f64, modules: usize) -> (f64, f64) {
    let q = QR_CODE.quiet_zone;
    let module = size / (modules as f64 + 2.0 * q);
    (module, module * q)
}

// Nayuki: ECC_CODEWORDS_PER_BLOCK / NUM_ERROR_CORRECTION_BLOCKS, indexed
// [level ordinal L M Q H][version]; index 0 is padding.
const ECC_CODEWORDS_PER_BLOCK: [[i16; 41]; 4] = [
    [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
    [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
];
const NUM_ERROR_CORRECTION_BLOCKS: [[i16; 41]; 4] = [
    [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
    [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
    [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
    [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
];

const PENALTY_N1: i32 = 3;
const PENALTY_N2: i32 = 3;
const PENALTY_N3: i32 = 40;
const PENALTY_N4: i32 = 10;

/// Raw data modules of a version (all modules minus the function patterns).
fn num_raw_data_modules(ver: usize) -> usize {
    let mut result = (16 * ver + 128) * ver + 64;
    if ver >= 2 {
        let num_align = ver / 7 + 2;
        result -= (25 * num_align - 10) * num_align - 55;
        if ver >= 7 {
            result -= 36;
        }
    }
    result
}

fn ecc_per_block(ver: usize, level: QrLevel) -> usize {
    ECC_CODEWORDS_PER_BLOCK[level.ordinal()][ver] as usize
}

fn num_blocks(ver: usize, level: QrLevel) -> usize {
    NUM_ERROR_CORRECTION_BLOCKS[level.ordinal()][ver] as usize
}

fn num_data_codewords(ver: usize, level: QrLevel) -> usize {
    num_raw_data_modules(ver) / 8 - ecc_per_block(ver, level) * num_blocks(ver, level)
}

/// Byte mode's character-count field width per version band.
fn byte_count_bits(ver: usize) -> usize {
    if ver < 10 { 8 } else { 16 }
}

// --- Reed–Solomon over GF(2^8 / 0x11D) --------------------------------------

fn rs_multiply(x: u8, y: u8) -> u8 {
    let mut z: u32 = 0;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x11d);
        z ^= ((y as u32 >> i) & 1) * x as u32;
    }
    (z & 0xff) as u8
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree - 1];
    result.push(1);
    let mut root: u8 = 1;
    for _ in 0..degree {
        for j in 0..result.len() {
            result[j] = rs_multiply(result[j], root);
            if j + 1 < result.len() {
                result[j] ^= result[j + 1];
            }
        }
        root = rs_multiply(root, 0x02);
    }
    result
}

fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &b in data {
        let factor = b ^ result.remove(0);
        result.push(0);
        for (r, &coef) in result.iter_mut().zip(divisor) {
            *r ^= rs_multiply(coef, factor);
        }
    }
    result
}

// --- The symbol ---------------------------------------------------------------

struct Symbol {
    version: usize,
    level: QrLevel,
    size: usize,
    modules: Vec<Vec<bool>>,
    is_function: Vec<Vec<bool>>,
}

impl Symbol {
    fn new(version: usize, level: QrLevel) -> Self {
        let size = version * 4 + 17;
        Self { version, level, size, modules: vec![vec![false; size]; size], is_function: vec![vec![false; size]; size] }
    }

    fn set_function(&mut self, x: usize, y: usize, dark: bool) {
        self.modules[y][x] = dark;
        self.is_function[y][x] = true;
    }

    fn draw_function_patterns(&mut self) {
        for i in 0..self.size {
            self.set_function(6, i, i % 2 == 0);
            self.set_function(i, 6, i % 2 == 0);
        }
        let s = self.size as i32;
        self.draw_finder(3, 3);
        self.draw_finder(s - 4, 3);
        self.draw_finder(3, s - 4);
        let align = self.alignment_positions();
        let n = align.len();
        for i in 0..n {
            for j in 0..n {
                if !((i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0)) {
                    self.draw_alignment(align[i], align[j]);
                }
            }
        }
        self.draw_format_bits(0); // dummy, overwritten after the mask is chosen
        self.draw_version();
    }

    fn draw_format_bits(&mut self, mask: u8) {
        let data = (self.level.format_bits() << 3) | mask as u32;
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = ((data << 10) | rem) ^ 0x5412;
        let bit = |i: u32| (bits >> i) & 1 != 0;
        for i in 0..=5 {
            self.set_function(8, i as usize, bit(i));
        }
        self.set_function(8, 7, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(7, 8, bit(8));
        for i in 9..15 {
            self.set_function(14 - i as usize, 8, bit(i));
        }
        let size = self.size;
        for i in 0..8 {
            self.set_function(size - 1 - i as usize, 8, bit(i));
        }
        for i in 8..15 {
            self.set_function(8, size - 15 + i as usize, bit(i));
        }
        self.set_function(8, size - 8, true); // always dark
    }

    fn draw_version(&mut self) {
        if self.version < 7 {
            return;
        }
        let mut rem = self.version as u32;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1f25);
        }
        let bits = ((self.version as u32) << 12) | rem;
        for i in 0..18usize {
            let dark = (bits >> i) & 1 != 0;
            let a = self.size - 11 + i % 3;
            let b = i / 3;
            self.set_function(a, b, dark);
            self.set_function(b, a, dark);
        }
    }

    fn draw_finder(&mut self, x: i32, y: i32) {
        let s = self.size as i32;
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let dist = dx.abs().max(dy.abs());
                let (xx, yy) = (x + dx, y + dy);
                if (0..s).contains(&xx) && (0..s).contains(&yy) {
                    self.set_function(xx as usize, yy as usize, dist != 2 && dist != 4);
                }
            }
        }
    }

    fn draw_alignment(&mut self, x: usize, y: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                self.set_function((x as i32 + dx) as usize, (y as i32 + dy) as usize, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    fn alignment_positions(&self) -> Vec<usize> {
        if self.version == 1 {
            return Vec::new();
        }
        let num_align = self.version / 7 + 2;
        let step = (self.version * 8 + num_align * 3 + 5) / (num_align * 4 - 4) * 2;
        let mut result = vec![6usize];
        let mut pos = self.size - 7;
        while result.len() < num_align {
            result.insert(1, pos);
            pos -= step;
        }
        result
    }

    fn add_ecc_and_interleave(&self, data: &[u8]) -> Vec<u8> {
        let n_blocks = num_blocks(self.version, self.level);
        let block_ecc_len = ecc_per_block(self.version, self.level);
        let raw_codewords = num_raw_data_modules(self.version) / 8;
        let num_short_blocks = n_blocks - raw_codewords % n_blocks;
        let short_block_len = raw_codewords / n_blocks;
        let divisor = rs_divisor(block_ecc_len);
        let mut blocks: Vec<Vec<u8>> = Vec::with_capacity(n_blocks);
        let mut k = 0;
        for i in 0..n_blocks {
            let len = short_block_len - block_ecc_len + usize::from(i >= num_short_blocks);
            let mut dat = data[k..(k + len).min(data.len())].to_vec();
            k += dat.len();
            let ecc = rs_remainder(&dat, &divisor);
            if i < num_short_blocks {
                dat.push(0);
            }
            dat.extend(ecc);
            blocks.push(dat);
        }
        let mut result = Vec::with_capacity(raw_codewords);
        for i in 0..blocks[0].len() {
            for (j, block) in blocks.iter().enumerate() {
                if i != short_block_len - block_ecc_len || j >= num_short_blocks {
                    result.push(block[i]);
                }
            }
        }
        result
    }

    fn draw_codewords(&mut self, data: &[u8]) {
        let mut i = 0usize;
        let mut right = self.size as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..self.size {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { self.size - 1 - vert } else { vert };
                    if !self.is_function[y][x] && i < data.len() * 8 {
                        self.modules[y][x] = (data[i >> 3] >> (7 - (i & 7))) & 1 != 0;
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let invert = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => (x * y) % 2 + (x * y) % 3 == 0,
                    6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
                    _ => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
                };
                if !self.is_function[y][x] && invert {
                    self.modules[y][x] = !self.modules[y][x];
                }
            }
        }
    }

    fn penalty_score(&self) -> i32 {
        let mut result = 0i32;
        let size = self.size;
        for horizontal in [true, false] {
            for a in 0..size {
                let mut run_color = false;
                let mut run = 0i32;
                let mut history = [0i32; 7];
                for b in 0..size {
                    let c = if horizontal { self.modules[a][b] } else { self.modules[b][a] };
                    if c == run_color {
                        run += 1;
                        if run == 5 {
                            result += PENALTY_N1;
                        } else if run > 5 {
                            result += 1;
                        }
                    } else {
                        self.finder_penalty_add_history(run, &mut history);
                        if !run_color {
                            result += self.finder_penalty_count_patterns(&history) * PENALTY_N3;
                        }
                        run_color = c;
                        run = 1;
                    }
                }
                result += self.finder_penalty_terminate_and_count(run_color, run, &mut history) * PENALTY_N3;
            }
        }
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let color = self.modules[y][x];
                if color == self.modules[y][x + 1] && color == self.modules[y + 1][x] && color == self.modules[y + 1][x + 1] {
                    result += PENALTY_N2;
                }
            }
        }
        let dark: i32 = self.modules.iter().map(|row| row.iter().filter(|c| **c).count() as i32).sum();
        let total = (size * size) as i32;
        // ceil(|dark·20 − total·10| / total) − 1, in integers
        let num = (dark * 20 - total * 10).abs();
        let k = (num + total - 1) / total - 1;
        result += k * PENALTY_N4;
        result
    }

    fn finder_penalty_count_patterns(&self, h: &[i32; 7]) -> i32 {
        let n = h[1];
        let core = n > 0 && h[2] == n && h[3] == n * 3 && h[4] == n && h[5] == n;
        i32::from(core && h[0] >= n * 4 && h[6] >= n) + i32::from(core && h[6] >= n * 4 && h[0] >= n)
    }

    fn finder_penalty_terminate_and_count(&self, current_color: bool, mut current_len: i32, h: &mut [i32; 7]) -> i32 {
        if current_color {
            self.finder_penalty_add_history(current_len, h);
            current_len = 0;
        }
        current_len += self.size as i32;
        self.finder_penalty_add_history(current_len, h);
        self.finder_penalty_count_patterns(h)
    }

    fn finder_penalty_add_history(&self, mut current_len: i32, h: &mut [i32; 7]) {
        if h[0] == 0 {
            current_len += self.size as i32;
        }
        h.copy_within(0..6, 1);
        h[0] = current_len;
    }
}

/// The data codewords of one byte segment at `version` (header, data,
/// terminator, byte padding, the 0xEC/0x11 pad bytes).
fn data_codewords(bytes: &[u8], version: usize, level: QrLevel) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::new();
    let append = |bits: &mut Vec<u8>, val: u32, len: usize| {
        for i in (0..len).rev() {
            bits.push(((val >> i) & 1) as u8);
        }
    };
    append(&mut bits, 0x4, 4);
    append(&mut bits, bytes.len() as u32, byte_count_bits(version));
    for &b in bytes {
        append(&mut bits, b as u32, 8);
    }
    let capacity_bits = num_data_codewords(version, level) * 8;
    let term = 4.min(capacity_bits - bits.len());
    append(&mut bits, 0, term);
    let pad = (8 - bits.len() % 8) % 8;
    append(&mut bits, 0, pad);
    let mut pad_byte = 0xecu32;
    while bits.len() < capacity_bits {
        append(&mut bits, pad_byte, 8);
        pad_byte ^= 0xec ^ 0x11;
    }
    let mut out = vec![0u8; bits.len() / 8];
    for (i, b) in bits.iter().enumerate() {
        out[i >> 3] |= b << (7 - (i & 7));
    }
    out
}

/// The smallest version whose data capacity holds `byte_len` bytes at `level`.
pub fn qr_version(byte_len: usize, level: QrLevel) -> Option<usize> {
    (1..=40).find(|&version| {
        let used = 4 + byte_count_bits(version) + byte_len * 8;
        byte_len < (1usize << byte_count_bits(version)) && used <= num_data_codewords(version, level) * 8
    })
}

/// The QR matrix of `value` (UTF-8, byte mode) at `level`; `None` for an
/// empty value or one too long for version 40 at that level.
pub fn qr_matrix(value: &str, level: QrLevel) -> Option<QrMatrix> {
    if value.is_empty() {
        return None;
    }
    let bytes = value.as_bytes();
    let version = qr_version(bytes.len(), level)?;
    let mut sym = Symbol::new(version, level);
    sym.draw_function_patterns();
    let codewords = sym.add_ecc_and_interleave(&data_codewords(bytes, version, level));
    sym.draw_codewords(&codewords);
    let mut mask = 0u8;
    let mut min_penalty = i32::MAX;
    for i in 0..8u8 {
        sym.apply_mask(i);
        sym.draw_format_bits(i);
        let penalty = sym.penalty_score();
        if penalty < min_penalty {
            mask = i;
            min_penalty = penalty;
        }
        sym.apply_mask(i); // XOR undoes it
    }
    sym.apply_mask(mask);
    sym.draw_format_bits(mask);
    let rows = sym.modules.iter().map(|row| row.iter().map(|d| if *d { '1' } else { '0' }).collect()).collect();
    Some(QrMatrix { size: sym.size, version, mask, rows })
}
