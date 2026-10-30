//! EXP-1324: the Text `shimmer` colours (catalog/style.json `textShimmer`),
//! a port of `src/text-shimmer.ts`; `fixtures/text-shimmer.json` locks it.
//! A painter fills a shimmering text's glyphs with the gradient the style's
//! `$comment` describes, from these colours; reduced motion = plain text.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::generated::catalog as g;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShimmerSpec {
    pub duration_ms: u32,
    pub angle_deg: f64,
    pub spread_px: f64,
    pub highlight_lightness_min: f64,
    pub highlight_lightness_boost: f64,
    pub highlight_alpha_boost: f64,
    pub light_alpha_factor: f64,
}

pub static SHIMMER: LazyLock<ShimmerSpec> = LazyLock::new(|| {
    let style: serde_json::Value = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    serde_json::from_value(style["textShimmer"].clone()).expect("style.json textShimmer")
});

/// The colours (`#rrggbb[aa]`) and numbers of one shimmering text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextShimmer {
    pub base: String,
    pub highlight: String,
    pub mid: String,
    pub duration_ms: u32,
    pub angle_deg: f64,
    pub spread_px: f64,
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` → (r, g, b 0–255, a 0–1).
fn parse_hex(color: &str) -> Option<(f64, f64, f64, f64)> {
    let h = color.strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = match h.len() {
        3 | 4 => h.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => h.to_string(),
        _ => return None,
    };
    let byte = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok().map(f64::from);
    let a = if full.len() == 8 { byte(6)? / 255.0 } else { 1.0 };
    Some((byte(0)?, byte(2)?, byte(4)?, a))
}

fn byte(n: f64) -> String {
    format!("{:02x}", n.round().clamp(0.0, 255.0) as u8)
}

fn to_hex(r: f64, g: f64, b: f64, a: f64) -> String {
    let base = format!("#{}{}{}", byte(r), byte(g), byte(b));
    if a >= 1.0 {
        base
    } else {
        format!("{base}{}", byte(a * 255.0))
    }
}

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_gamma(c: f64) -> f64 {
    let x = c.clamp(0.0, 1.0);
    if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_to_oklab(r: f64, g: f64, b: f64) -> [f64; 3] {
    let (lr, lg, lb) = (to_linear(r / 255.0), to_linear(g / 255.0), to_linear(b / 255.0));
    let l = (0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb).cbrt();
    let m = (0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb).cbrt();
    let s = (0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb).cbrt();
    [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ]
}

fn oklab_hex([lightness, a, b]: [f64; 3], alpha: f64) -> String {
    let l = (lightness + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (lightness - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (lightness - 0.0894841775 * a - 1.291485548 * b).powi(3);
    to_hex(
        to_gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s) * 255.0,
        to_gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s) * 255.0,
        to_gamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s) * 255.0,
        alpha,
    )
}

/// The shimmer colours for a text colour; `dark` = the surface's dark mode.
/// An unparsable colour shimmers from black.
pub fn text_shimmer(color: &str, dark: bool) -> TextShimmer {
    let t = *SHIMMER;
    let (r, g, b, a) = parse_hex(color).unwrap_or((0.0, 0.0, 0.0, 1.0));
    let base = rgb_to_oklab(r, g, b);
    let (high, high_alpha) = if dark {
        ([(base[0] + t.highlight_lightness_boost).max(t.highlight_lightness_min).min(1.0), base[1], base[2]], (a + t.highlight_alpha_boost).min(1.0))
    } else {
        (base, a * t.light_alpha_factor)
    };
    let sum = a + high_alpha;
    let mid = if sum > 0.0 { [0, 1, 2].map(|i| (base[i] * a + high[i] * high_alpha) / sum) } else { base };
    TextShimmer {
        base: to_hex(r, g, b, a),
        highlight: oklab_hex(high, high_alpha),
        mid: oklab_hex(mid, sum / 2.0),
        duration_ms: t.duration_ms,
        angle_deg: t.angle_deg,
        spread_px: t.spread_px,
    }
}
