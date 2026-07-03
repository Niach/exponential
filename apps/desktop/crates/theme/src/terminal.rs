//! Terminal ANSI palette + type metrics (masterplan-v3 §6.8 / §6.9).
//!
//! §6.8: `Color::Named(NamedColor)` maps through "the theme's `terminal_ansi_*`
//! token (black/red/green/yellow/blue/magenta/cyan/white + their bright
//! variants, plus foreground/background/cursor). These 16+ tokens live in the
//! `theme` crate, derived from the design-tokens (§04), so the terminal
//! matches the app chrome."
//!
//! Where a semantic design token exists (RED/GREEN/YELLOW/BLUE + the surface
//! tokens) the palette is token-locked. magenta/cyan have **no** design token
//! and no product surface (same situation as `ThemeColor`'s magenta/cyan —
//! see `exponential_dark()`); they are pinned here to the Tailwind shades the
//! token accents already come from (RED = red-500, GREEN = green-500, …), so
//! the six ANSI hues sit in one family: fuchsia-500 / cyan-500, bright at 400.
//!
//! The bright/dim variants are derived in HSL from the base tokens (bright =
//! lighter, dim = the xterm-style 2/3 attenuation), keeping every derived
//! color anchored to the token values.

use crate::tokens as t;
use gpui::Hsla;

/// Terminal cell line-height multiplier (§6.9's `theme.terminal_line_height`,
/// "e.g. 1.3").
pub const LINE_HEIGHT: f32 = 1.3;

/// Terminal font size in px — matches the compact-density UI base (§4.4).
pub const FONT_SIZE: f32 = 13.0;

/// The bundled monospace family (§6.9 cell metrics / §3.2 "no runtime font
/// path"). The JetBrainsMono-*.ttf faces live in `apps/desktop/assets/fonts/`
/// (OFL-1.1, license alongside) and are registered by the app shell's
/// `load_embedded_fonts` with the Inter faces, so the family always resolves
/// inside a `.app`/AppImage.
pub const FONT_FAMILY: &str = "JetBrains Mono";

/// The resolved 16-color ANSI table + specials the terminal element paints
/// with (§6.8's color table). All `Hsla`, ready for gpui.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalPalette {
    /// Default foreground (`NamedColor::Foreground`).
    pub foreground: Hsla,
    /// Default background (`NamedColor::Background`) — also the element's
    /// clear color.
    pub background: Hsla,
    /// Cursor fill (`NamedColor::Cursor`); web caret parity = foreground.
    pub cursor: Hsla,
    /// Glyph color painted *inside* a block cursor (inverted).
    pub cursor_text: Hsla,
    /// Local selection band (mirrors the app-wide selection surface).
    pub selection: Hsla,
    /// ANSI 0–7 then bright 8–15.
    pub ansi: [Hsla; 16],
    /// `NamedColor::Dim*` (dim black..dim white).
    pub dim: [Hsla; 8],
    /// `NamedColor::BrightForeground`.
    pub bright_foreground: Hsla,
    /// `NamedColor::DimForeground`.
    pub dim_foreground: Hsla,
}

impl TerminalPalette {
    /// ANSI index 0..=15 → color (callers pass `NamedColor as usize` or the
    /// low half of `Color::Indexed`).
    pub fn ansi(&self, index: usize) -> Hsla {
        self.ansi[index.min(15)]
    }
}

/// Lighten toward white in HSL space (bright-variant derivation).
fn lighter(color: Hsla, amount: f32) -> Hsla {
    Hsla {
        l: (color.l + amount).min(1.0),
        ..color
    }
}

/// The xterm-style dim attenuation (2/3 lightness), matching the emulator's
/// own `default_color` dim math so on-screen and OSC-reported dims agree.
fn dimmed(color: Hsla) -> Hsla {
    Hsla {
        l: color.l * (2.0 / 3.0),
        ..color
    }
}

// No design token exists for magenta/cyan (documented intentional — see
// module docs). Tailwind fuchsia-500 / cyan-500 + their 400 brights, the same
// scale RED/GREEN/YELLOW/BLUE are drawn from.
const MAGENTA: crate::Srgb8 = crate::Srgb8 { r: 0xd9, g: 0x46, b: 0xef, a: 0xff }; // fuchsia-500
const BRIGHT_MAGENTA: crate::Srgb8 = crate::Srgb8 { r: 0xe8, g: 0x79, b: 0xf9, a: 0xff }; // fuchsia-400
const CYAN: crate::Srgb8 = crate::Srgb8 { r: 0x06, g: 0xb6, b: 0xd4, a: 0xff }; // cyan-500
const BRIGHT_CYAN: crate::Srgb8 = crate::Srgb8 { r: 0x22, g: 0xd3, b: 0xee, a: 0xff }; // cyan-400

/// Build the Exponential terminal palette from the generated design tokens.
/// Pure — no gpui `App` required (unit-testable, §6.2).
pub fn terminal_palette() -> TerminalPalette {
    let foreground = t::FOREGROUND.to_hsla();
    let background = t::BACKGROUND.to_hsla();

    // ANSI black must stay visible on the near-black background → the web
    // secondary surface; bright black is the ring gray (both token-locked).
    let black = t::SECONDARY.to_hsla();
    let bright_black = t::RING.to_hsla();
    let red = t::RED.to_hsla();
    let green = t::GREEN.to_hsla();
    let yellow = t::YELLOW.to_hsla();
    let blue = t::BLUE.to_hsla();
    let magenta = MAGENTA.to_hsla();
    let cyan = CYAN.to_hsla();
    // White < bright-white, mirroring the primary/foreground token pair.
    let white = t::PRIMARY.to_hsla();
    let bright_white = foreground;

    let ansi = [
        black,
        red,
        green,
        yellow,
        blue,
        magenta,
        cyan,
        white,
        bright_black,
        lighter(red, 0.08),
        lighter(green, 0.08),
        lighter(yellow, 0.08),
        lighter(blue, 0.08),
        BRIGHT_MAGENTA.to_hsla(),
        BRIGHT_CYAN.to_hsla(),
        bright_white,
    ];

    TerminalPalette {
        foreground,
        background,
        cursor: foreground,
        cursor_text: background,
        // Token-locked semantic BLUE at the same 0.3 alpha the app-wide
        // `ThemeColor::selection` uses (see exponential_dark()).
        selection: Hsla { a: 0.3, ..blue },
        ansi,
        dim: [
            dimmed(black),
            dimmed(red),
            dimmed(green),
            dimmed(yellow),
            dimmed(blue),
            dimmed(magenta),
            dimmed(cyan),
            dimmed(white),
        ],
        bright_foreground: bright_white,
        dim_foreground: dimmed(foreground),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_is_token_locked() {
        let p = terminal_palette();
        assert_eq!(p.foreground, t::FOREGROUND.to_hsla());
        assert_eq!(p.background, t::BACKGROUND.to_hsla());
        assert_eq!(p.ansi(1), t::RED.to_hsla());
        assert_eq!(p.ansi(2), t::GREEN.to_hsla());
        assert_eq!(p.ansi(3), t::YELLOW.to_hsla());
        assert_eq!(p.ansi(4), t::BLUE.to_hsla());
        assert_eq!(p.ansi(15), t::FOREGROUND.to_hsla());
    }

    #[test]
    fn bright_variants_are_lighter_dims_darker() {
        let p = terminal_palette();
        for base in 1..=4 {
            assert!(
                p.ansi(base + 8).l > p.ansi(base).l,
                "bright {base} must be lighter"
            );
            assert!(p.dim[base].l < p.ansi(base).l, "dim {base} must be darker");
        }
    }

    #[test]
    fn ansi_black_visible_on_background() {
        let p = terminal_palette();
        assert!(p.ansi(0).l > p.background.l);
        assert!(p.ansi(8).l > p.ansi(0).l);
    }

    #[test]
    fn out_of_range_index_clamps() {
        let p = terminal_palette();
        assert_eq!(p.ansi(99), p.ansi(15));
    }
}
