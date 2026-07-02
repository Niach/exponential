//! `theme` — the Exponential Dark theme (masterplan-v3 §3.1 / §04).
//!
//! `src/tokens.generated.rs` (emitted from `@exp/design-tokens`, COMMITTED)
//! holds the palette as [`Srgb8`] byte structs. This hand-written `lib.rs`
//! owns the `Srgb8` struct and, from Phase 1, the `Srgb8 → gpui::Rgba`/`Hsla`
//! bridge plus the programmatic gpui-component `ThemeColor` builder. Only this
//! file will touch gpui; the generated tokens stay gpui-free.

/// An sRGB color as explicit named bytes (masterplan-v3 §2.4). Emitted-into by
/// the design-tokens generator; sidesteps gpui's `rgb(0xRRGGBB)` vs
/// `rgba(0xRRGGBBAA)` byte-order hazard entirely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Srgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

// Phase 1 adds (gpui dep lands then — §2.4/§3.7):
// impl Srgb8 {
//     pub const fn to_rgba(self) -> gpui::Rgba { … }
//     pub fn to_hsla(self) -> gpui::Hsla { self.to_rgba().into() }
// }

pub mod tokens {
    include!("tokens.generated.rs");
}
