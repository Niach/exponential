//! # Chrome (VAPP-90) — the token values the generic controls paint with
//!
//! The glass controls in [`crate::controls`] used to read the Exponential
//! app's generated design tokens (`theme::tokens::glass::*`, `radius::*`)
//! directly. This crate links NOTHING from the app, so those values arrive as
//! one host-installed [`Chrome`]: the glass fills and strokes, the corner
//! ladder, the handful of glyphs a control draws on its own (the search
//! field's magnifier, a fold's chevrons, the checkbox tick) and the standard
//! easing curve.
//!
//! - [`Chrome::default`] = the Exponential dark values, so a test or a third
//!   party host needs no init at all.
//! - [`install`] makes a host's chrome the one every `cx`-taking builder
//!   reads ([`Chrome::global`]); builders without a `cx` (the pure `Div`
//!   recipes) take the `&Chrome` explicitly.
//! - [`Chrome::from_theme`] derives a chrome from an Exponential UI theme.
//!
//! Colours (gpui-component's `cx.theme()`: foreground, muted, primary,
//! danger, popover, list hover) are NOT here — the controls keep reading
//! those from gpui-component's `ActiveTheme`, which the host already themes.

use std::sync::LazyLock;

use gpui::{App, Global, Hsla, Rgba, SharedString};
use gpui_component::{IconName, IconNamed as _};

/// The glyphs a control draws by itself, as embedded asset paths (what
/// gpui-component's `IconNamed::path` returns; render with
/// `Icon::empty().path(…)`). The default is gpui-component's own bundled
/// set; the IDE installs its registry's concept glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct ChromeIcons {
    /// The search field's leading glyph (`nav-search`).
    pub search: SharedString,
    /// The search field's clear button (`ui-clear`, a circled cross).
    pub clear: SharedString,
    /// A folded disclosure / group band (`ui-chevron-right`).
    pub chevron_right: SharedString,
    /// An open disclosure / group band (`ui-chevron-down`).
    pub chevron_down: SharedString,
    /// A checked checkbox (`ui-check`).
    pub check: SharedString,
    /// An indeterminate checkbox (`ui-minus`).
    pub minus: SharedString,
}

impl Default for ChromeIcons {
    fn default() -> Self {
        Self {
            search: IconName::Search.path(),
            clear: IconName::CircleX.path(),
            chevron_right: IconName::ChevronRight.path(),
            chevron_down: IconName::ChevronDown.path(),
            check: IconName::Check.path(),
            minus: IconName::Minus.path(),
        }
    }
}

/// Every token value the generic controls read. See the module doc.
#[derive(Clone, Debug, PartialEq)]
pub struct Chrome {
    /// A group band / segmented capsule / code block (`glass.fillSection`).
    pub fill_section: Hsla,
    /// A row, a group, a checkbox square (`glass.fillRow`).
    pub fill_row: Hsla,
    /// A card, a pill, a glass icon button (`glass.fillCard`).
    pub fill_card: Hsla,
    /// The cutout panel wash (`glass.fillPanel`).
    pub fill_panel: Hsla,
    /// Hover / selected (`glass.fillActive`).
    pub fill_active: Hsla,
    /// Row hairlines and dividers (`glass.strokeRow`).
    pub stroke_row: Hsla,
    /// The segmented capsule's stroke (`glass.strokeSection`).
    pub stroke_section: Hsla,
    /// Card / pill / alert hairline (`glass.strokeCard`).
    pub stroke_card: Hsla,
    /// The bulk bar, a meter's track, an unchecked checkbox
    /// (`glass.strokeStrong`).
    pub stroke_strong: Hsla,
    /// A focused field, a selected segment / pill (`glass.strokeActive`).
    pub stroke_active: Hsla,
    /// `radius.sm` (8): checkbox, typeahead row.
    pub radius_sm: f32,
    /// `radius.md` (10): rows, bands, alerts, ghost icon buttons.
    pub radius_md: f32,
    /// `radius.lg` (12): fields and groups.
    pub radius_lg: f32,
    /// `radius.xl` (16): cards.
    pub radius_xl: f32,
    /// `radius.xl2` (20).
    pub radius_xl2: f32,
    /// `radius.xl3` (24): the bulk bar capsule.
    pub radius_xl3: f32,
    /// The glyphs a control draws by itself.
    pub icons: ChromeIcons,
    /// The STANDARD motion curve as CSS cubic-bezier control points
    /// (`motion.ease.standard`), evaluated through [`ease`].
    pub ease_standard: [f32; 4],
}

impl Global for Chrome {}

/// An sRGB byte colour as gpui's `Hsla`, through the SAME float bridge the
/// app's `theme::Srgb8::to_hsla` takes (`byte / 255` per channel, then
/// `Rgba → Hsla`), so the default paints byte-identically to the IDE.
fn srgb8(r: u8, g: u8, b: u8, a: u8) -> Hsla {
    Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: a as f32 / 255.,
    }
    .into()
}

impl Default for Chrome {
    /// The Exponential dark chrome. Values copied from the IDE's generated
    /// design tokens, `apps/desktop/crates/theme/src/tokens.generated.rs`
    /// (`glass::*`, `radius::*`, `motion::ease::STANDARD`; source of truth
    /// `packages/design-tokens/tokens.json`). The IDE's own chrome is built
    /// from those consts and a test there pins it equal to this one.
    fn default() -> Self {
        Self {
            fill_section: srgb8(255, 255, 255, 10),
            fill_row: srgb8(255, 255, 255, 15),
            fill_card: srgb8(255, 255, 255, 15),
            fill_panel: srgb8(255, 255, 255, 10),
            fill_active: srgb8(255, 255, 255, 28),
            stroke_row: srgb8(255, 255, 255, 15),
            stroke_section: srgb8(255, 255, 255, 20),
            stroke_card: srgb8(255, 255, 255, 26),
            stroke_strong: srgb8(255, 255, 255, 31),
            stroke_active: srgb8(255, 255, 255, 41),
            radius_sm: 8.,
            radius_md: 10.,
            radius_lg: 12.,
            radius_xl: 16.,
            radius_xl2: 20.,
            radius_xl3: 24.,
            icons: ChromeIcons::default(),
            ease_standard: [0.2, 0.0, 0.0, 1.0],
        }
    }
}

static DEFAULT_CHROME: LazyLock<Chrome> = LazyLock::new(Chrome::default);

/// The glass alphas a theme-derived chrome lays over its foreground: the
/// Exponential ladder (`glass.fill*` 10/15/15/10/28 and `glass.stroke*`
/// 15/20/26/31/41 of 255) rounded to whole percents.
const FILL_ALPHAS: [f32; 5] = [0.04, 0.06, 0.06, 0.04, 0.11];
const STROKE_ALPHAS: [f32; 5] = [0.06, 0.08, 0.10, 0.12, 0.16];

impl Chrome {
    /// The chrome installed on this app ([`install`]), or the Exponential
    /// default when none is — tests and third-party hosts need no init.
    pub fn global(cx: &App) -> &Chrome {
        cx.try_global::<Chrome>().unwrap_or(&DEFAULT_CHROME)
    }

    /// Derives a chrome from an Exponential UI theme for one `mode`: the
    /// glass fills and strokes are the mode's `foreground` colour at the
    /// glass alphas (fills 4/6/6/4/11 %, strokes 6/8/10/12/16 %), the corner
    /// ladder is the theme's `tokens.radius` (`sm`…`xl3`). Anything the theme
    /// does not carry (an unparsable foreground, a missing radius name) keeps
    /// the [`Chrome::default`] value; icons and easing stay the default's.
    pub fn from_theme(
        theme: &exponential_ui::theme::ResolvedTheme,
        mode: exponential_ui::theme::Mode,
    ) -> Chrome {
        let mut chrome = Chrome::default();
        let foreground = theme
            .modes
            .get(mode)
            .color
            .get("foreground")
            .and_then(|hex| parse_hex(hex));
        if let Some(fg) = foreground {
            let at = |alpha: f32| Hsla { a: fg.a * alpha, ..fg };
            chrome.fill_section = at(FILL_ALPHAS[0]);
            chrome.fill_row = at(FILL_ALPHAS[1]);
            chrome.fill_card = at(FILL_ALPHAS[2]);
            chrome.fill_panel = at(FILL_ALPHAS[3]);
            chrome.fill_active = at(FILL_ALPHAS[4]);
            chrome.stroke_row = at(STROKE_ALPHAS[0]);
            chrome.stroke_section = at(STROKE_ALPHAS[1]);
            chrome.stroke_card = at(STROKE_ALPHAS[2]);
            chrome.stroke_strong = at(STROKE_ALPHAS[3]);
            chrome.stroke_active = at(STROKE_ALPHAS[4]);
        }
        let radius = |name: &str, fallback: f32| {
            theme
                .tokens
                .radius
                .get(name)
                .map(|value| *value as f32)
                .unwrap_or(fallback)
        };
        chrome.radius_sm = radius("sm", chrome.radius_sm);
        chrome.radius_md = radius("md", chrome.radius_md);
        chrome.radius_lg = radius("lg", chrome.radius_lg);
        chrome.radius_xl = radius("xl", chrome.radius_xl);
        chrome.radius_xl2 = radius("xl2", chrome.radius_xl2);
        chrome.radius_xl3 = radius("xl3", chrome.radius_xl3);
        chrome
    }
}

/// Makes `chrome` the one [`Chrome::global`] answers on this app.
pub fn install(cx: &mut App, chrome: Chrome) {
    cx.set_global(chrome);
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` → `Hsla` (the theme file's colour forms).
fn parse_hex(hex: &str) -> Option<Hsla> {
    let digits = hex.strip_prefix('#')?;
    let byte = |ix: usize| u8::from_str_radix(digits.get(ix..ix + 2)?, 16).ok();
    let (r, g, b, a) = match digits.len() {
        3 => {
            let nibble = |ix: usize| {
                u8::from_str_radix(digits.get(ix..ix + 1)?, 16)
                    .ok()
                    .map(|n| n * 17)
            };
            (nibble(0)?, nibble(1)?, nibble(2)?, 255)
        }
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    Some(srgb8(r, g, b, a))
}

/// A CSS-accurate cubic-bezier easing over control points `[x1, y1, x2, y2]`
/// (P0 = (0,0) and P3 = (1,1) implicit): Newton–Raphson solves `x(t) =
/// progress`, then the result is evaluated through `y(t)` — the same solver
/// as the IDE's `theme::motion::ease` (EXP-523), which is what keeps a curve
/// identical to CSS `cubic-bezier()`, SwiftUI and Compose. Never hand these
/// points to `gpui_component::animation::cubic_bezier`: that helper evaluates
/// `y` over the RAW progress.
pub fn ease(c: [f32; 4]) -> impl Fn(f32) -> f32 {
    move |progress: f32| {
        let x = progress.clamp(0.0, 1.0);
        let bez = |p1: f32, p2: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
        };
        let dbez = |p1: f32, p2: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
        };
        // 8 iterations seeded at t = x — WebKit's UnitBezier budget.
        let mut t = x;
        for _ in 0..8 {
            let dx = dbez(c[0], c[2], t);
            if dx.abs() < 1e-6 {
                break;
            }
            t = (t - (bez(c[0], c[2], t) - x) / dx).clamp(0.0, 1.0);
        }
        bez(c[1], c[3], t)
    }
}

/// The DECELERATE curve (`motion.ease.decelerate`, CSS `cubic-bezier(0, 0,
/// 0.2, 1)`) — also the CSS `animate-ping` curve the live dot ripples on.
pub const EASE_DECELERATE: [f32; 4] = [0.0, 0.0, 0.2, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms_parse() {
        assert_eq!(parse_hex("#ffffff0f"), Some(srgb8(255, 255, 255, 15)));
        assert_eq!(parse_hex("#fafafa"), Some(srgb8(250, 250, 250, 255)));
        assert_eq!(parse_hex("#fff"), Some(srgb8(255, 255, 255, 255)));
        assert_eq!(parse_hex("fafafa"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn a_theme_chrome_lays_the_glass_alphas_over_its_foreground() {
        let mut theme = exponential_ui::theme::ResolvedTheme::default();
        theme
            .modes
            .dark
            .color
            .insert("foreground".into(), "#ffffff".into());
        theme.tokens.radius.insert("md".into(), 6.);
        let chrome = Chrome::from_theme(&theme, exponential_ui::theme::Mode::Dark);
        assert!((chrome.fill_active.a - 0.11).abs() < 1e-6);
        assert!((chrome.stroke_card.a - 0.10).abs() < 1e-6);
        assert_eq!(chrome.fill_row.l, 1.0);
        assert_eq!(chrome.radius_md, 6.);
        // Missing names keep the default ladder.
        assert_eq!(chrome.radius_lg, Chrome::default().radius_lg);
        // A mode with no foreground keeps the default paint.
        let light = Chrome::from_theme(&theme, exponential_ui::theme::Mode::Light);
        assert_eq!(light.fill_card, Chrome::default().fill_card);
    }

    #[test]
    fn the_standard_curve_starts_and_lands() {
        let curve = ease(Chrome::default().ease_standard);
        assert!(curve(0.).abs() < 1e-4);
        assert!((curve(1.) - 1.).abs() < 1e-4);
        assert!(curve(0.5) > 0.5, "standard eases out in its second half");
    }
}
