//! Resolved colours (`#rrggbb` / `#rrggbbaa`, what every `Visual` carries) →
//! gpui `Hsla`, and the visual's shadow layers → gpui `BoxShadow`s.

use exponential_ui::style::ShadowLayer;
use gpui::{point, px, BoxShadow, Hsla, Rgba};

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` → `Hsla`; anything else `None`.
pub fn parse_hex(spec: &str) -> Option<Hsla> {
    let hex = spec.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let (r, g, b, a) = match hex.len() {
        3 => (nib(0)?, nib(1)?, nib(2)?, 255),
        4 => (nib(0)?, nib(1)?, nib(2)?, nib(3)?),
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    Some(Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }.into())
}

/// An optional colour string → `Hsla` (unparseable = `None`).
pub fn color_of(spec: Option<&str>) -> Option<Hsla> {
    spec.and_then(parse_hex)
}

/// The visual's shadow layers as gpui shadows (unparseable colours skipped).
pub fn shadows(layers: &[ShadowLayer]) -> Vec<BoxShadow> {
    layers
        .iter()
        .filter_map(|l| {
            let color = parse_hex(&l.color)?;
            Some(BoxShadow { color, offset: point(px(l.x), px(l.y)), blur_radius: px(l.blur), spread_radius: px(l.spread), inset: false })
        })
        .collect()
}

/// `a` → `b` at `t` (0..1), blended in sRGB like a CSS colour transition.
pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (x, y) = (Rgba::from(a), Rgba::from(b));
    let l = |p: f32, q: f32| p + (q - p) * t.clamp(0.0, 1.0);
    Rgba { r: l(x.r, y.r), g: l(x.g, y.g), b: l(x.b, y.b), a: l(x.a, y.a) }.into()
}

/// `hsl(h s% l%)` with an alpha, for the avatar seed tint.
pub fn hsl(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h: (h / 360.0).rem_euclid(1.0), s, l, a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_of(c: Hsla) -> (u8, u8, u8, u8) {
        let r = Rgba::from(c);
        ((r.r * 255.0).round() as u8, (r.g * 255.0).round() as u8, (r.b * 255.0).round() as u8, (r.a * 255.0).round() as u8)
    }

    #[test]
    fn six_and_eight_digit_hex_parse() {
        assert_eq!(rgba_of(parse_hex("#ff0000").unwrap()), (255, 0, 0, 255));
        assert_eq!(rgba_of(parse_hex("#0a0a0a").unwrap()), (10, 10, 10, 255));
        assert_eq!(rgba_of(parse_hex("#00000080").unwrap()), (0, 0, 0, 128));
        assert_eq!(rgba_of(parse_hex("#3b82f6cc").unwrap()), (59, 130, 246, 204));
        assert_eq!(rgba_of(parse_hex("#fff").unwrap()), (255, 255, 255, 255));
        assert_eq!(rgba_of(parse_hex("#0008").unwrap()), (0, 0, 0, 136));
    }

    #[test]
    fn non_hex_is_rejected() {
        assert_eq!(parse_hex("red"), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#gg0000"), None);
        assert_eq!(parse_hex("$color.primary"), None);
        assert_eq!(color_of(None), None);
    }

    #[test]
    fn shadow_layers_convert() {
        let layers = vec![ShadowLayer { x: 0.0, y: 1.0, blur: 2.0, spread: 0.0, color: "#0000001a".into() }, ShadowLayer { x: 0.0, y: 0.0, blur: 0.0, spread: 0.0, color: "nope".into() }];
        let out = shadows(&layers);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].offset.y, px(1.0));
        assert_eq!(out[0].blur_radius, px(2.0));
    }
}
