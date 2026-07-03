//! `ExpIcon` — the project-local Lucide icon set (masterplan-v3 §4.7).
//!
//! gpui-component's bundled `IconName` misses several glyphs the option
//! tables need (`circle`, `circle-dashed`, `timer`, `signal-*`, …), so the
//! needed Lucide SVGs ship in `apps/desktop/assets/icons/` and this enum is
//! generated from them with the same `icon_named!` macro. `Icon::from(ExpIcon)`
//! is drop-in via the `IconNamed → Icon` blanket impl. The app's embedded
//! `AssetSource` (`app/src/assets.rs`) already includes `icons/**/*.svg`, so
//! the generated `icons/{name}.svg` paths resolve at render time.
//!
//! This module also maps the gpui-free `domain::options` presentation data
//! ([`IconGlyph`] glyph names, [`ColorToken`] color roles) onto gpui types —
//! the one seam between the verbatim-ported tables and the theme (§4.3:
//! status/priority accents come from the generated tokens, never loose hex).

use gpui::{App, Hsla, IntoElement, SharedString, Styled as _, Window};
use gpui_component::{ActiveTheme as _, Icon, IconNamed};
use gpui_component_macros::icon_named;

use domain::options::{ColorToken, IconGlyph, IssueOption};

// Generates `pub enum ExpIcon { CalendarDays, Circle, CircleCheck, … }` from
// the SVG files (path relative to this crate's CARGO_MANIFEST_DIR).
icon_named!(ExpIcon, "../../assets/icons");

impl gpui::RenderOnce for ExpIcon {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::from(self)
    }
}

/// `domain` glyph → the bundled SVG. Exhaustive on purpose: adding a glyph to
/// the option tables without shipping its SVG + arm fails compilation here.
pub fn glyph_icon(glyph: IconGlyph) -> Icon {
    let icon = match glyph {
        IconGlyph::CircleDashed => ExpIcon::CircleDashed,
        IconGlyph::Circle => ExpIcon::Circle,
        IconGlyph::Timer => ExpIcon::Timer,
        IconGlyph::CircleCheck => ExpIcon::CircleCheck,
        IconGlyph::CircleX => ExpIcon::CircleX,
        IconGlyph::Copy => ExpIcon::Copy,
        IconGlyph::Minus => ExpIcon::Minus,
        IconGlyph::TriangleAlert => ExpIcon::TriangleAlert,
        IconGlyph::SignalHigh => ExpIcon::SignalHigh,
        IconGlyph::SignalMedium => ExpIcon::SignalMedium,
        IconGlyph::SignalLow => ExpIcon::SignalLow,
    };
    Icon::from(icon)
}

/// `domain` color role → the live theme color (§4.3: `muted_foreground` /
/// `foreground` follow the theme; the five accents are the generated
/// design-token values — token-locked, not Tailwind hex).
pub fn token_color(token: ColorToken, cx: &App) -> Hsla {
    match token {
        ColorToken::MutedForeground => cx.theme().muted_foreground,
        ColorToken::Foreground => cx.theme().foreground,
        ColorToken::Yellow => theme::tokens::YELLOW.to_hsla(),
        ColorToken::Green => theme::tokens::GREEN.to_hsla(),
        ColorToken::Red => theme::tokens::RED.to_hsla(),
        ColorToken::Orange => theme::tokens::ORANGE.to_hsla(),
        ColorToken::Blue => theme::tokens::BLUE.to_hsla(),
    }
}

/// The colored icon of one option-table row (`web <Icon className={color}>`).
pub fn option_icon<V: 'static>(option: &IssueOption<V>, cx: &App) -> Icon {
    glyph_icon(option.icon).text_color(token_color(option.color, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_glyph_resolves_to_an_embedded_svg_path() {
        // §4.7: SVG file names line up with the glyph names one-to-one.
        for option in &domain::options::ISSUE_STATUS_OPTIONS {
            let glyph = option.icon;
            let expected = format!("icons/{}.svg", glyph.file_name());
            assert_eq!(path_of(glyph), expected);
        }
        for option in &domain::options::ISSUE_PRIORITY_OPTIONS {
            let glyph = option.icon;
            let expected = format!("icons/{}.svg", glyph.file_name());
            assert_eq!(path_of(glyph), expected);
        }
    }

    fn path_of(glyph: IconGlyph) -> String {
        let icon: ExpIcon = match glyph {
            IconGlyph::CircleDashed => ExpIcon::CircleDashed,
            IconGlyph::Circle => ExpIcon::Circle,
            IconGlyph::Timer => ExpIcon::Timer,
            IconGlyph::CircleCheck => ExpIcon::CircleCheck,
            IconGlyph::CircleX => ExpIcon::CircleX,
            IconGlyph::Copy => ExpIcon::Copy,
            IconGlyph::Minus => ExpIcon::Minus,
            IconGlyph::TriangleAlert => ExpIcon::TriangleAlert,
            IconGlyph::SignalHigh => ExpIcon::SignalHigh,
            IconGlyph::SignalMedium => ExpIcon::SignalMedium,
            IconGlyph::SignalLow => ExpIcon::SignalLow,
        };
        icon.path().to_string()
    }
}
