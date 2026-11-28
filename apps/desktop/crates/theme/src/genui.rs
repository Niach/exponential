//! #EDE-278 — the genUI theme as the IDE reads it.
//!
//! The desktop paints two layers: genUI surfaces (eui-gpui, which resolves
//! `eui_core::themes::default_theme()` itself) and the gpui-component chrome
//! around them. This module hands the chrome the SAME resolved `exponential`
//! theme (dark), so a colour, radius or type rung means one value in both:
//! [`crate::exponential_dark`] and [`crate::apply_exponential_dark`] read
//! their genUI-named fields from here instead of a local copy. The theme is
//! embedded already resolved (eui-core's generated `themes.rs`) and parsed
//! once per process.

use std::sync::{Arc, LazyLock};

use eui_core::theme::{Mode, ResolvedTheme};
use gpui::{Hsla, Rgba};

use crate::Srgb8;

static THEME: LazyLock<Arc<ResolvedTheme>> = LazyLock::new(eui_core::themes::default_theme);

/// The resolved built-in theme genUI paints with (`exponential`).
pub fn theme() -> &'static ResolvedTheme {
    &THEME
}

/// `#rrggbb` / `#rrggbbaa` (the resolved theme's only colour spelling).
pub fn parse_hex(hex: &str) -> Option<Srgb8> {
    let h = hex.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Srgb8 { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 }),
        8 => Some(Srgb8 { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
        _ => None,
    }
}

/// A dark-mode `$color.<name>` as bytes. Panics on a name the theme lacks:
/// the names used are fixed, and `tests::every_name_resolves` pins them.
pub fn srgb(name: &str) -> Srgb8 {
    let value = theme().modes.get(Mode::Dark).color.get(name).unwrap_or_else(|| panic!("genUI theme has no $color.{name}"));
    parse_hex(value).unwrap_or_else(|| panic!("genUI $color.{name} = {value:?} is not hex"))
}

/// A dark-mode `$color.<name>` as gpui [`Hsla`].
pub fn color(name: &str) -> Hsla {
    Hsla::from(Rgba::from(srgb(name)))
}

/// A numeric token (`$radius.md`, `$spacing.sm`, `$type.size.sm`, …).
pub fn number(group: &str, key: &str) -> f32 {
    theme().tokens.numbers(group).and_then(|g| g.get(key)).copied().unwrap_or_else(|| panic!("genUI theme has no ${group}.{key}")) as f32
}

/// A `$type.size.<key>` in px.
pub fn number_type_size(key: &str) -> f32 {
    theme().tokens.r#type.size.get(key).copied().unwrap_or_else(|| panic!("genUI theme has no $type.size.{key}")) as f32
}

/// A `$type.family.<key>` name (`sans` = Inter, `mono` = JetBrains Mono).
pub fn family(key: &str) -> &'static str {
    theme().tokens.r#type.family.get(key).map(String::as_str).unwrap_or_else(|| panic!("genUI theme has no $type.family.{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens as t;

    #[test]
    fn hex_parses_both_spellings() {
        assert_eq!(parse_hex("#0a0a0a"), Some(Srgb8 { r: 10, g: 10, b: 10, a: 255 }));
        assert_eq!(parse_hex("#ffffff1c"), Some(Srgb8 { r: 255, g: 255, b: 255, a: 28 }));
        assert_eq!(parse_hex("0a0a0a"), None);
        assert_eq!(parse_hex("#0a0"), None);
    }

    /// The IDE's generated token scales (`@exp/design-tokens`) and the genUI
    /// theme are ONE ladder: a drift on either side fails here, not in a
    /// side-by-side screenshot.
    #[test]
    fn the_design_token_scales_are_the_genui_scales() {
        for (key, v) in [("xxs", t::spacing::XXS), ("xs", t::spacing::XS), ("sm", t::spacing::SM), ("md", t::spacing::MD), ("lg", t::spacing::LG), ("xl", t::spacing::XL), ("xl2", t::spacing::XL2)] {
            assert_eq!(number("spacing", key), v, "$spacing.{key}");
        }
        for (key, v) in [("sm", t::radius::SM), ("md", t::radius::MD), ("lg", t::radius::LG), ("xl", t::radius::XL), ("xl2", t::radius::XL2), ("xl3", t::radius::XL3)] {
            assert_eq!(number("radius", key), v, "$radius.{key}");
        }
        use t::typography::{line_height as lh, size, weight};
        for (key, s, l) in [("xs", size::XS, lh::XS), ("sm", size::SM, lh::SM), ("base", size::BASE, lh::BASE), ("lg", size::LG, lh::LG), ("xl", size::XL, lh::XL), ("xl2", size::XL2, lh::XL2)] {
            assert_eq!(theme().tokens.r#type.size[key] as f32, s, "$type.size.{key}");
            assert_eq!(theme().tokens.r#type.line_height[key] as f32, l, "$type.lineHeight.{key}");
        }
        for (key, v) in [("regular", weight::REGULAR), ("medium", weight::MEDIUM), ("semibold", weight::SEMIBOLD), ("bold", weight::BOLD)] {
            assert_eq!(theme().tokens.r#type.weight[key] as f32, v, "$type.weight.{key}");
        }
        assert_eq!(number_type_size("base"), crate::REM_PX);
        assert_eq!(number_type_size("sm"), crate::FONT_SIZE_PX);
        assert_eq!(family("sans"), t::typography::FONT_FAMILY);
        assert_eq!(family("mono"), crate::terminal::FONT_FAMILY);
        use t::motion::{duration, ease, scale};
        assert_eq!(number("motion", "fast") as u64, duration::FAST_MS);
        assert_eq!(number("motion", "standard") as u64, duration::STANDARD_MS);
        assert_eq!(number("motion", "slow") as u64, duration::SLOW_MS);
        for (key, c) in [("standard", ease::STANDARD), ("decelerate", ease::DECELERATE), ("accelerate", ease::ACCELERATE)] {
            let curve: Vec<f32> = theme().tokens.ease[key].iter().map(|v| *v as f32).collect();
            assert_eq!(curve, c.to_vec(), "$ease.{key}");
        }
        assert_eq!(number("scale", "press"), scale::PRESS);
        assert_eq!(number("scale", "popoverFrom"), scale::POPOVER_FROM);
        use t::typography::emphasis;
        for (key, v) in [("primary", emphasis::PRIMARY), ("secondary", emphasis::SECONDARY), ("tertiary", emphasis::TERTIARY), ("quaternary", emphasis::QUATERNARY)] {
            assert_eq!(number("opacity", key), v, "$opacity.{key}");
        }
    }

    /// The genUI colours that have a design-token twin are byte-equal to it
    /// (the glass ones included: genUI's card/accent ARE the glass fills).
    #[test]
    fn the_design_token_colours_are_the_genui_colours() {
        for (name, token) in [
            ("background", t::BACKGROUND),
            ("foreground", t::FOREGROUND),
            ("primary", t::PRIMARY),
            ("primaryForeground", t::PRIMARY_FOREGROUND),
            ("mutedForeground", t::MUTED_FOREGROUND),
            ("destructive", t::DESTRUCTIVE),
            ("border", t::BORDER),
            ("input", t::INPUT),
            ("ring", t::RING),
            ("card", t::glass::FILL_CARD),
            ("secondary", t::glass::FILL_CARD),
            ("muted", t::glass::FILL_SECTION),
            ("accent", t::glass::FILL_ACTIVE),
            ("borderStrong", t::glass::STROKE_STRONG),
            ("strokeActive", t::glass::STROKE_ACTIVE),
            ("bubbleOwn", t::glass::FILL_OWN),
            ("success", t::GREEN),
            ("warning", t::YELLOW),
            ("info", t::BLUE),
        ] {
            assert_eq!(srgb(name), token, "$color.{name}");
        }
    }
}
