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
use domain::rows::Project;

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
        IconGlyph::GitPullRequest => ExpIcon::GitPullRequest,
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

/// The type glyph for a raw `project_type` string (mirrors web `Code2` /
/// `SquareKanban` / `Megaphone`, masterplan v7): dev boards get code brackets,
/// task boards a kanban, feedback boards a megaphone. Unknown resolves to dev.
pub fn project_type_glyph(project_type: &str) -> Icon {
    let glyph = if project_type == domain::contract::PROJECT_TYPE_FEEDBACK {
        ExpIcon::Megaphone
    } else if project_type == domain::contract::PROJECT_TYPE_TASKS {
        ExpIcon::SquareKanban
    } else {
        ExpIcon::Code
    };
    Icon::from(glyph)
}

/// A project row's type glyph. Absent/unknown `project_type` resolves to dev.
pub fn project_type_icon(project: &Project) -> Icon {
    project_type_glyph(
        project
            .project_type
            .as_deref()
            .unwrap_or(domain::contract::PROJECT_TYPE_DEV),
    )
}

/// One curated icon name (`domain::contract::PROJECT_ICON_VALUES`) → its glyph.
/// The bundled Lucide set doesn't ship every curated name, so several map to the
/// closest available glyph (collisions are fine — the stored name is the source
/// of truth). An unknown/uncurated name yields `None`.
fn project_icon_glyph(name: &str) -> Option<ExpIcon> {
    let glyph = match name {
        "code" => ExpIcon::Code,
        "square-kanban" => ExpIcon::SquareKanban,
        "megaphone" => ExpIcon::Megaphone,
        "bug" => ExpIcon::CircleDot,
        "rocket" => ExpIcon::Rocket,
        "book-open" => ExpIcon::List,
        "globe" => ExpIcon::Globe,
        "heart" => ExpIcon::Circle,
        "star" => ExpIcon::Sparkles,
        "zap" => ExpIcon::SignalHigh,
        "wrench" => ExpIcon::Pencil,
        "shield" => ExpIcon::CircleCheck,
        "package" => ExpIcon::Square,
        "terminal" => ExpIcon::Code,
        "lightbulb" => ExpIcon::Sparkles,
        "message-circle" => ExpIcon::MessageSquare,
        _ => return None,
    };
    Some(glyph)
}

/// The glyph of a raw curated icon name, falling back to the type-derived glyph
/// (used by the create-project template cards, where there's no `Project` yet).
pub fn project_icon_name_glyph(name: &str, project_type: &str) -> Icon {
    match project_icon_glyph(name) {
        Some(glyph) => Icon::from(glyph),
        None => project_type_glyph(project_type),
    }
}

/// A project row's rendered glyph: the stored curated `icon` when present and
/// known, otherwise the legacy type-derived fallback (`is_public`/repo columns
/// drive behavior; the glyph is cosmetic).
pub fn project_icon(project: &Project) -> Icon {
    project
        .icon
        .as_deref()
        .and_then(project_icon_glyph)
        .map(Icon::from)
        .unwrap_or_else(|| project_type_icon(project))
}

/// The public-board marker glyph (a globe) — appended next to feedback-board
/// projects, which are readable by anyone with the link.
pub fn public_board_icon() -> Icon {
    Icon::from(ExpIcon::Globe)
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
            IconGlyph::GitPullRequest => ExpIcon::GitPullRequest,
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
