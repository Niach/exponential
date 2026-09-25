//! `ExpIcon` — the board-local Lucide icon set (masterplan-v3 §4.7).
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
use domain::rows::Board;
use domain::statuses::StatusTint;

// Generates `pub enum ExpIcon { CalendarDays, Circle, CircleCheck, … }` from
// the SVG files (path relative to this crate's CARGO_MANIFEST_DIR). `build.rs`
// declares a `rerun-if-changed` on that directory so a newly generated icon
// re-expands this macro instead of silently missing its variant.
icon_named!(ExpIcon, "../../assets/icons");

/// EXP-273: the shared icon registry (`packages/icons/icons.json`), projected
/// into Rust. `registry::icon_by_name` resolves a stored `boards.icon` /
/// `actions.icon` value, and the `registry::<CONCEPT>` consts name the nav /
/// status / editor glyphs every client now shares.
#[allow(dead_code)] // shared by four clients; not every concept is used here
pub mod registry {
    include!("icons.generated.rs");
}

impl gpui::RenderOnce for ExpIcon {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::from(self)
    }
}

/// `domain` glyph → the bundled [`ExpIcon`]. Exhaustive on purpose: adding a
/// glyph to the option tables without shipping its SVG + arm fails
/// compilation here.
fn glyph_expicon(glyph: IconGlyph) -> ExpIcon {
    match glyph {
        IconGlyph::CircleDashed => ExpIcon::CircleDashed,
        IconGlyph::Circle => ExpIcon::Circle,
        IconGlyph::Timer => ExpIcon::Timer,
        IconGlyph::GitPullRequest => ExpIcon::GitPullRequest,
        IconGlyph::Progress14 => ExpIcon::Progress14,
        IconGlyph::Progress24 => ExpIcon::Progress24,
        IconGlyph::Progress34 => ExpIcon::Progress34,
        IconGlyph::Progress15 => ExpIcon::Progress15,
        IconGlyph::Progress25 => ExpIcon::Progress25,
        IconGlyph::Progress35 => ExpIcon::Progress35,
        IconGlyph::Progress45 => ExpIcon::Progress45,
        IconGlyph::CircleCheck => ExpIcon::CircleCheck,
        IconGlyph::CircleX => ExpIcon::CircleX,
        IconGlyph::Copy => ExpIcon::Copy,
        IconGlyph::Minus => ExpIcon::Minus,
        IconGlyph::TriangleAlert => ExpIcon::TriangleAlert,
        IconGlyph::SignalHigh => ExpIcon::SignalHigh,
        IconGlyph::SignalMedium => ExpIcon::SignalMedium,
        IconGlyph::SignalLow => ExpIcon::SignalLow,
    }
}

/// `domain` glyph → the bundled SVG as an [`Icon`] element.
pub fn glyph_icon(glyph: IconGlyph) -> Icon {
    Icon::from(glyph_expicon(glyph))
}

/// EXP-423: `domain` glyph → the embedded SVG asset path (for paint-level
/// consumers like the editor's chip gutter, which draw via
/// `window.paint_svg` instead of mounting an element).
pub fn glyph_svg_path(glyph: IconGlyph) -> SharedString {
    glyph_expicon(glyph).path()
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

/// [`token_color`] WITHOUT a theme handle, for the typed pickers
/// (`picker::*`): their constructors take rows and a trigger, never a
/// `&App` — that signature is the cross-platform contract, and it is the
/// same on web, iOS and Android. Sound because the IDE forces ONE theme and
/// `theme::init` assigns `foreground` / `muted_foreground` from exactly
/// these two tokens; `picker::tests` draws a themed window and asserts the
/// two answers agree.
pub fn static_token_color(token: ColorToken) -> Hsla {
    match token {
        ColorToken::MutedForeground => theme::tokens::MUTED_FOREGROUND.to_hsla(),
        ColorToken::Foreground => theme::tokens::FOREGROUND.to_hsla(),
        ColorToken::Yellow => theme::tokens::YELLOW.to_hsla(),
        ColorToken::Green => theme::tokens::GREEN.to_hsla(),
        ColorToken::Red => theme::tokens::RED.to_hsla(),
        ColorToken::Orange => theme::tokens::ORANGE.to_hsla(),
        ColorToken::Blue => theme::tokens::BLUE.to_hsla(),
    }
}

/// [`status_tint_color`]'s theme-free twin — see [`static_token_color`].
pub fn static_status_tint_color(tint: &StatusTint) -> Hsla {
    match tint {
        StatusTint::Token(token) => static_token_color(*token),
        StatusTint::Hex(hex) => crate::settings::parse_hex_color(hex)
            .unwrap_or_else(|| theme::tokens::MUTED_FOREGROUND.to_hsla()),
    }
}

/// The colored icon of one option-table row (`web <Icon className={color}>`).
pub fn option_icon<V: 'static>(option: &IssueOption<V>, cx: &App) -> Icon {
    glyph_icon(option.icon).text_color(token_color(option.color, cx))
}

/// EXP-314: a resolved status' color. Builtin (and constructed-fallback) rows
/// resolve their THEME token — byte identical to pre-EXP-314 rendering and
/// theme-reactive; custom rows parse their stored hex through the same path
/// the label dots use, degrading to `muted_foreground` on a bad value.
pub fn status_tint_color(tint: &StatusTint, cx: &App) -> Hsla {
    match tint {
        StatusTint::Token(token) => token_color(*token, cx),
        StatusTint::Hex(hex) => {
            crate::settings::parse_hex_color(hex).unwrap_or(cx.theme().muted_foreground)
        }
    }
}

/// The colored icon of a resolved status — the ONE status-icon entrypoint the
/// per-team surfaces render through (`option_icon`'s dynamic sibling).
pub fn resolved_status_icon(status: &domain::statuses::ResolvedStatus, cx: &App) -> Icon {
    glyph_icon(status.glyph).text_color(status_tint_color(&status.tint, cx))
}

/// One curated icon name (`domain::contract::BOARD_ICON_VALUES`) → its glyph.
/// EXP-273: every curated name now ships its REAL Lucide SVG, generated into
/// `assets/icons` from the shared registry, so this is a straight lookup —
/// the old table substituted 11 of 16 names (and collided `terminal` with
/// `code`, `lightbulb` with `star`) because those glyphs were never bundled.
/// An unknown/uncurated name still yields `None` for the caller's fallback.
fn board_icon_glyph(name: &str) -> Option<ExpIcon> {
    registry::icon_by_name(name)
}

/// The glyph of a raw curated icon name, falling back to the code glyph for an
/// unknown name (used by the create-board icon picker, where there's no
/// `Board` yet — every curated name resolves).
pub fn board_icon_name_glyph(name: &str) -> Icon {
    board_icon_glyph(name)
        .map(Icon::from)
        .unwrap_or_else(|| Icon::from(ExpIcon::Code))
}

/// A board's fallback glyph when it carries no stored `icon`: a repo-backed
/// board gets the code brackets, a plain one a kanban. The drop migration
/// backfills `icon`, so this is a cosmetic safety net for rows synced before
/// the backfill.
fn board_fallback_glyph(board: &Board) -> ExpIcon {
    if board.repository_id.is_some() {
        ExpIcon::Code
    } else {
        ExpIcon::SquareKanban
    }
}

/// EXP-273: an action's rendered glyph — the stored curated `icon` when
/// present and known, else the registry's generic action glyph. Mirrors web's
/// `getActionIcon`; the builtins set their own icon explicitly, so the
/// fallback only covers actions authored before the column existed.
pub fn action_icon(icon: Option<&str>) -> Icon {
    icon.and_then(board_icon_glyph)
        .map(Icon::from)
        .unwrap_or_else(|| Icon::from(registry::ACTION_DEFAULT))
}

/// EXP-924 — THE device glyph resolver (web `getDeviceIconName`'s twin, ×4):
/// the row's stored `icon` when it names one of the six DEVICE icons, else the
/// kind default (`ui-server` for the headless daemon, `ui-device` for
/// everything else). A BOARD-set name (or one from a newer client) is not a
/// device icon and falls back like NULL — the device picker is the only writer
/// of this column, so anything else is noise, never a glyph to trust.
///
/// [`device_icon_name`] is the same answer as a NAME — what the device
/// settings picker shows selected, so a machine that never picked still reads
/// `monitor` (resolved), not "nothing picked".
pub fn device_icon_name(icon: Option<&str>, is_server: bool) -> &'static str {
    icon.and_then(|name| {
        registry::DEVICE_ICONS
            .iter()
            .copied()
            .find(|known| *known == name)
    })
    .unwrap_or(if is_server { "server" } else { "monitor" })
}

/// [`device_icon_name`]'s glyph — every site that draws a concrete device row
/// (the Devices list, the composer's device pin and its menu) goes through
/// here.
pub fn device_icon(icon: Option<&str>, is_server: bool) -> ExpIcon {
    registry::icon_by_name(device_icon_name(icon, is_server)).unwrap_or(if is_server {
        registry::UI_SERVER
    } else {
        registry::UI_DEVICE
    })
}

/// A board row's rendered glyph: the stored curated `icon` when present and
/// known, otherwise the attribute-derived fallback (the repo column drives
/// behavior; the glyph is cosmetic).
pub fn board_icon(board: &Board) -> Icon {
    Icon::from(board_glyph(board))
}

/// [`board_icon`]'s raw `ExpIcon` — for call sites that snapshot the glyph
/// (the search sheet denormalizes hits at search time, EXP-525).
pub fn board_glyph(board: &Board) -> ExpIcon {
    board
        .icon
        .as_deref()
        .and_then(board_icon_glyph)
        .unwrap_or_else(|| board_fallback_glyph(board))
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
        // EXP-314: every pie clock the started-category table can pick (the
        // 4-clock table subsumes the 2- and 3-clock ones for counts 1..=5).
        for count in 1..=5 {
            for index in 0..count {
                let glyph = domain::statuses::started_clock_glyph(index, count);
                let expected = format!("icons/{}.svg", glyph.file_name());
                assert_eq!(path_of(glyph), expected, "clock {index}/{count}");
            }
        }
        // And every non-started category glyph.
        for category in domain::statuses::IssueStatusCategory::DISPLAY_ORDER {
            let glyph = domain::statuses::category_glyph(category, 0, 2);
            assert_eq!(path_of(glyph), format!("icons/{}.svg", glyph.file_name()));
        }
    }

    /// EXP-924: the ONE device resolver — a pick wins, NULL takes the kind
    /// default, and a name outside the device set is not a device icon.
    #[test]
    fn device_icon_takes_the_pick_then_the_kind_default() {
        // `ExpIcon` carries no `PartialEq` (the generated enum derives only
        // `IntoElement + Clone`), so glyphs compare by their asset path.
        let path = |icon: ExpIcon| icon.path().to_string();
        // A picked device icon wins on either kind.
        assert_eq!(device_icon_name(Some("os-apple"), false), "os-apple");
        assert_eq!(
            path(device_icon(Some("os-apple"), false)),
            path(ExpIcon::OsApple)
        );
        assert_eq!(device_icon_name(Some("laptop"), true), "laptop");
        // NULL = the kind default.
        assert_eq!(device_icon_name(None, false), "monitor");
        assert_eq!(path(device_icon(None, false)), path(registry::UI_DEVICE));
        assert_eq!(device_icon_name(None, true), "server");
        assert_eq!(path(device_icon(None, true)), path(registry::UI_SERVER));
        // A BOARD-set name (or an unknown one) falls back like NULL.
        assert_eq!(device_icon_name(Some("rocket"), true), "server");
        assert_eq!(
            path(device_icon(Some("rocket"), false)),
            path(registry::UI_DEVICE)
        );
        assert_eq!(device_icon_name(Some(""), false), "monitor");
        // The contract's device list IS this picker's set.
        assert_eq!(
            registry::DEVICE_ICONS,
            domain::contract::DEVICE_ICON_VALUES,
            "the registry's device set has to be the contract's"
        );
        // And every one of them resolves to a real glyph.
        for name in registry::DEVICE_ICONS {
            assert_eq!(device_icon_name(Some(name), false), *name);
            assert!(registry::icon_by_name(name).is_some(), "{name} has no glyph");
        }
    }

    fn path_of(glyph: IconGlyph) -> String {
        let icon: ExpIcon = match glyph {
            IconGlyph::CircleDashed => ExpIcon::CircleDashed,
            IconGlyph::Circle => ExpIcon::Circle,
            IconGlyph::Timer => ExpIcon::Timer,
            IconGlyph::GitPullRequest => ExpIcon::GitPullRequest,
            IconGlyph::Progress14 => ExpIcon::Progress14,
            IconGlyph::Progress24 => ExpIcon::Progress24,
            IconGlyph::Progress34 => ExpIcon::Progress34,
            IconGlyph::Progress15 => ExpIcon::Progress15,
            IconGlyph::Progress25 => ExpIcon::Progress25,
            IconGlyph::Progress35 => ExpIcon::Progress35,
            IconGlyph::Progress45 => ExpIcon::Progress45,
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
