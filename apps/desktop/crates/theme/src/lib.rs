//! `theme` — the Exponential Dark theme (masterplan-v3 §3.1 / §4.3 / §4.4).
//!
//! `src/tokens.generated.rs` (emitted from `@exp/design-tokens`, COMMITTED,
//! drift-guarded in CI) holds the palette as [`Srgb8`] byte structs. This
//! hand-written `lib.rs` owns the `Srgb8 → gpui::Rgba → Hsla` bridge and the
//! builder that assembles the gpui-component [`ThemeColor`] programmatically
//! from those consts. The design tokens are the single source of truth — there
//! is no hand-authored theme JSON (§4.3: the web tokens are OKLCH, which the
//! JSON theme parser cannot read).
//!
//! Bootstrap contract (§3.6 — ORDER IS LOAD-BEARING):
//!
//! ```ignore
//! gpui_component::init(cx);          // installs the ThemeRegistry + Theme globals
//! theme::init(cx);                   // = Theme::change(Dark) THEN apply_exponential_dark
//! ```
//!
//! `Theme::change` → `apply_config` reassigns BOTH `theme.colors` and
//! `theme.tokens` from the stock dark `ThemeConfig`, so it clobbers any palette
//! set before it. [`init`] therefore forces dark FIRST, then overwrites colors
//! and rebuilds tokens. The app is dark-only, exactly like web (`html.dark`) —
//! never call `sync_system_appearance`, never observe system appearance.

use gpui::{px, App, Hsla, Rgba};
use gpui_component::theme::{Colorize as _, Theme, ThemeColor, ThemeMode, ThemeTokens};

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

impl Srgb8 {
    /// Bridge to gpui's float-channel [`Rgba`] (0.0..=1.0 per channel).
    pub const fn to_rgba(self) -> Rgba {
        Rgba {
            r: self.r as f32 / 255.,
            g: self.g as f32 / 255.,
            b: self.b as f32 / 255.,
            a: self.a as f32 / 255.,
        }
    }

    /// Bridge to gpui's [`Hsla`] (what every `ThemeColor` field holds).
    pub fn to_hsla(self) -> Hsla {
        self.to_rgba().into()
    }
}

impl From<Srgb8> for Rgba {
    fn from(c: Srgb8) -> Self {
        c.to_rgba()
    }
}

impl From<Srgb8> for Hsla {
    fn from(c: Srgb8) -> Self {
        c.to_hsla()
    }
}

pub mod tokens {
    include!("tokens.generated.rs");
}

use tokens as t;

/// Compact-density base font size (§4.4 knob 1): web is 14px (`text-sm`);
/// desktop goes one notch tighter. Hand-set per the plan — the generated token
/// file carries no type-scale consts (tokens.json `type.baseSize` is the web's
/// 16px root, not the component base).
pub const FONT_SIZE_PX: f32 = 13.0;

/// Build the full Exponential Dark palette as a gpui-component [`ThemeColor`].
///
/// Starts from the component's stock dark palette, then overwrites EVERY field
/// (§4.3: "leave none defaulted so the look is fully ours") from the generated
/// tokens. Two kinds of mapping are in play:
///
/// 1. **Direct token fields** — the 22 `palette` consts map 1:1 onto their
///    shadcn-named `ThemeColor` fields.
/// 2. **Derived fields** — `ThemeColor` carries ~90 more fields than the web
///    theme (hover/active states, buttons, lists, tables, tabs, charts, …).
///    Where the web has a concrete surface, we mirror it (documented inline
///    against the web component/class). Where it does not, we apply
///    gpui-component's own dark-mode derivation formulas from
///    `theme/schema.rs::apply_config` — the sanctioned fallbacks for themes
///    that only define base colors — so widgets look native to the component
///    set while staying token-locked.
///
/// §4.3 note: there is NO `card`/`card_foreground` field on `ThemeColor`; the
/// generated `CARD` token (web's `#171717` card surface) maps onto the desktop
/// chrome surfaces that play that role (`tab_bar`, `title_bar`, `status_bar`,
/// `tiles`). The `list`/`list_head` fields that tokens.json lacks map from the
/// real web issue-list surfaces (see inline notes / EXP-1 #4).
pub fn exponential_dark() -> ThemeColor {
    // ThemeColor::dark() returns Arc<Self>; ThemeColor is Copy, so deref-copy it.
    let mut c = *ThemeColor::dark();

    let transparent = Hsla::transparent_black();
    // gpui-component's dark-mode derivation constants (schema.rs::apply_config).
    let active_darken = 0.2;
    let hover_opacity = 0.9;

    let bg = t::BACKGROUND.to_hsla();
    let fg = t::FOREGROUND.to_hsla();
    let border = t::BORDER.to_hsla(); // white @ 10% — matches web `--border`
    let input = t::INPUT.to_hsla(); // white @ 15% — matches web `--input`
    let primary = t::PRIMARY.to_hsla();
    let primary_foreground = t::PRIMARY_FOREGROUND.to_hsla();
    let secondary = t::SECONDARY.to_hsla();
    let accent = t::ACCENT.to_hsla();
    let muted_foreground = t::MUTED_FOREGROUND.to_hsla();
    let danger = t::DESTRUCTIVE.to_hsla();
    let card = t::CARD.to_hsla(); // web card surface — see doc note above
    let green = t::GREEN.to_hsla();
    let red = t::RED.to_hsla();
    let blue = t::BLUE.to_hsla();
    let yellow = t::YELLOW.to_hsla();

    // ---- Core surfaces (direct 1:1 token fields) ----------------------------
    c.background = bg;
    c.foreground = fg;
    c.border = border;
    c.input = input;
    c.ring = t::RING.to_hsla();
    c.muted = t::MUTED.to_hsla();
    c.muted_foreground = muted_foreground;
    c.accent = accent;
    c.accent_foreground = t::ACCENT_FOREGROUND.to_hsla();
    c.popover = t::POPOVER.to_hsla();
    c.popover_foreground = t::POPOVER_FOREGROUND.to_hsla();

    // ---- Primary / secondary (+ derived hover/active) -----------------------
    c.primary = primary;
    c.primary_foreground = primary_foreground;
    c.primary_hover = bg.blend(primary.opacity(hover_opacity));
    c.primary_active = primary.darken(active_darken);
    c.secondary = secondary;
    c.secondary_foreground = t::SECONDARY_FOREGROUND.to_hsla();
    c.secondary_hover = bg.blend(secondary.opacity(hover_opacity));
    c.secondary_active = secondary.darken(active_darken);

    // ---- Sidebar (direct tokens; accent-fg/primary derived like web) --------
    c.sidebar = t::SIDEBAR.to_hsla();
    c.sidebar_foreground = t::SIDEBAR_FOREGROUND.to_hsla();
    c.sidebar_accent = t::SIDEBAR_ACCENT.to_hsla();
    // web --sidebar-accent-foreground == --accent-foreground in the zinc theme
    c.sidebar_accent_foreground = t::ACCENT_FOREGROUND.to_hsla();
    c.sidebar_border = t::SIDEBAR_BORDER.to_hsla();
    // web --sidebar-primary(-foreground) == --primary(-foreground) in dark zinc
    c.sidebar_primary = primary;
    c.sidebar_primary_foreground = primary_foreground;

    // ---- Buttons (component dark formulas; ghost/base button = input mixes) -
    c.button = input.mix_oklab(transparent, 0.3);
    c.button_foreground = fg;
    c.button_hover = input.mix_oklab(transparent, 0.5);
    c.button_active = input.mix_oklab(transparent, 0.7);
    c.button_primary = primary;
    c.button_primary_foreground = primary_foreground;
    c.button_primary_hover = c.primary_hover;
    c.button_primary_active = c.primary_active;
    c.button_secondary = secondary;
    c.button_secondary_foreground = c.secondary_foreground;
    c.button_secondary_hover = c.secondary_hover;
    c.button_secondary_active = c.secondary_active;

    // ---- Danger (web `--destructive`; white text like web destructive btn) --
    c.danger = danger;
    c.danger_foreground = fg;
    c.danger_hover = bg.blend(danger.opacity(hover_opacity));
    c.danger_active = danger.darken(active_darken);
    c.button_danger = danger.mix_oklab(transparent, 0.2);
    c.button_danger_foreground = danger;
    c.button_danger_hover = danger.mix_oklab(transparent, 0.3);
    c.button_danger_active = danger.mix_oklab(transparent, 0.4);

    // ---- Success / info / warning (no web tokens; semantic accents + the
    //      component's own derivation formulas). info uses BLUE — tokens carry
    //      no cyan, and blue is the web's informational accent. ---------------
    c.success = green;
    c.success_foreground = primary_foreground;
    c.success_hover = bg.blend(green.opacity(hover_opacity));
    c.success_active = green.darken(active_darken);
    c.button_success = green.mix_oklab(transparent, 0.2);
    c.button_success_foreground = green;
    c.button_success_hover = green.mix_oklab(transparent, 0.3);
    c.button_success_active = green.mix_oklab(transparent, 0.4);
    c.info = blue;
    c.info_foreground = primary_foreground;
    c.info_hover = bg.blend(blue.opacity(hover_opacity));
    c.info_active = blue.darken(active_darken);
    c.button_info = blue.mix_oklab(transparent, 0.2);
    c.button_info_foreground = blue;
    c.button_info_hover = blue.mix_oklab(transparent, 0.3);
    c.button_info_active = blue.mix_oklab(transparent, 0.4);
    c.warning = yellow;
    c.warning_foreground = primary_foreground;
    c.warning_hover = bg.blend(yellow.opacity(hover_opacity));
    // NB: warning_active blends with background in the component formula
    // (unlike the other *_active) — copied exactly.
    c.warning_active = bg.blend(yellow.darken(active_darken));
    c.button_warning = yellow.mix_oklab(transparent, 0.2);
    c.button_warning_foreground = yellow;
    c.button_warning_hover = yellow.mix_oklab(transparent, 0.3);
    c.button_warning_active = yellow.mix_oklab(transparent, 0.4);

    // ---- Lists — web issue-list parity (EXP-1 #4: the list background must be
    //      the REAL surfaces, not a wrong card color). tokens.json has no
    //      list/list_head — mapped from apps/web/src/components/issue-list.tsx:
    //      rows sit on the page background; group header = `bg-accent/20`;
    //      row hover = `hover:bg-accent/30`; selection (command/select lists) =
    //      solid accent like web `data-[selected]:bg-accent`. -----------------
    c.list = bg;
    c.list_head = accent.opacity(0.2);
    c.list_hover = accent.opacity(0.3);
    c.list_even = bg; // web has no row striping
    c.list_active = accent;
    c.list_active_border = t::RING.to_hsla();

    // ---- Tables mirror the list surfaces (component's own fallback rule) ----
    c.table = c.list;
    c.table_head = c.list_head;
    c.table_head_foreground = muted_foreground;
    c.table_hover = c.list_hover;
    c.table_even = c.list_even;
    c.table_active = c.list_active;
    c.table_active_border = c.list_active_border;
    c.table_foot = c.list_head;
    c.table_foot_foreground = muted_foreground;
    c.table_row_border = border;

    // ---- Tabs — web filter-bar pills (issue-filter-bar.tsx): active =
    //      `bg-accent text-foreground`, inactive = transparent +
    //      `text-muted-foreground`. tab_bar is desktop-only dock chrome → the
    //      web card surface (also gpui-component's stock-dark intent);
    //      segmented = web `bg-muted`. ------------------------------------------
    c.tab = transparent;
    c.tab_foreground = muted_foreground;
    c.tab_active = accent;
    c.tab_active_foreground = fg;
    c.tab_bar = card;
    c.tab_bar_segmented = t::MUTED.to_hsla();

    // ---- Window chrome (desktop-only; web card surface, stock-dark parity) --
    c.title_bar = card;
    c.title_bar_border = t::SIDEBAR_BORDER.to_hsla();
    c.status_bar = card;
    c.status_bar_border = t::SIDEBAR_BORDER.to_hsla();
    c.tiles = card;
    c.window_border = border; // Linux CSD only

    // ---- Overlay / selection / caret ----------------------------------------
    // web dialog overlay is `bg-black/50` (components/ui/dialog.tsx)
    c.overlay = gpui::black().opacity(0.5);
    // text selection: token-locked semantic BLUE at the component's 0.3 alpha
    // clamp (stock dark uses blue-700; a near-white primary selection is
    // illegible over dark surfaces)
    c.selection = blue.opacity(0.3);
    // web caret is currentColor → foreground (component fallback uses primary;
    // same near-white family, foreground is the web-true pick)
    c.caret = fg;

    // ---- Remaining component surfaces (dark formulas, token-locked) ---------
    c.accordion = bg;
    c.accordion_hover = accent.opacity(0.8);
    c.group_box = bg.blend(secondary.opacity(0.3));
    c.group_box_foreground = fg;
    c.description_list_label = bg.blend(border.opacity(0.2));
    c.description_list_label_foreground = muted_foreground;
    c.drag_border = primary.opacity(0.65);
    c.drop_target = primary.opacity(0.2);
    c.link = primary;
    c.link_hover = primary;
    c.link_active = primary;
    c.progress_bar = primary;
    // web skeleton is `bg-accent` (components/ui/skeleton.tsx)
    c.skeleton = accent;
    // scrollbar: transparent track (stock-dark behavior — a solid track would
    // stripe the #171717 sidebar/popover surfaces); thumb from RING so it is
    // token-locked and visible on every surface
    c.scrollbar = transparent;
    c.scrollbar_thumb = t::RING.to_hsla().opacity(0.7);
    c.scrollbar_thumb_hover = t::RING.to_hsla();
    // slider: filled bar = primary, thumb = background (web thumb is
    // `bg-background border-primary`; also readable on the near-white bar)
    c.slider_bar = primary;
    c.slider_thumb = bg;
    // switch: single thumb token serves checked (primary, near-white) AND
    // unchecked tracks, so the thumb must be dark → background (stock-dark
    // scheme). The unchecked track lightens MUTED toward stock's neutral-700 so
    // the dark thumb stays visible on it.
    c.switch = t::MUTED.to_hsla().lighten(0.15);
    c.switch_thumb = bg;

    // ---- Charts (no web analogue; component's blue ladder, token-locked) ----
    c.chart_1 = blue.lighten(0.4);
    c.chart_2 = blue.lighten(0.2);
    c.chart_3 = blue;
    c.chart_4 = blue.darken(0.2);
    c.chart_5 = blue.darken(0.4);
    c.chart_bullish = green;
    c.chart_bearish = red;

    // ---- Base ANSI-ish colors (semantic tokens + component light-variant
    //      formula). magenta/cyan have no design token and no product surface —
    //      they keep the stock dark values (documented intentional inherit;
    //      charts-only in gpui-component). ------------------------------------
    c.red = red;
    c.red_light = bg.blend(red.opacity(0.8));
    c.green = green;
    c.green_light = bg.blend(green.opacity(0.8));
    c.blue = blue;
    c.blue_light = bg.blend(blue.opacity(0.8));
    c.yellow = yellow;
    c.yellow_light = bg.blend(yellow.opacity(0.8));
    // c.magenta / c.cyan (+ _light): intentionally stock — see note above.

    c
}

/// Overwrite the global gpui-component [`Theme`] with the Exponential Dark
/// palette + compact density (§4.3 / §4.4).
///
/// MUST run AFTER `gpui_component::init(cx)` and after the theme global exists
/// in dark mode — `Theme::change(Dark)` reassigns both `theme.colors` and
/// `theme.tokens` from the stock config and would clobber this palette if it
/// ran later. Use [`init`] for the correctly-ordered pair.
pub fn apply_exponential_dark(cx: &mut App) {
    let colors = exponential_dark();
    let theme = Theme::global_mut(cx);
    theme.mode = ThemeMode::Dark;
    theme.colors = colors;
    // MUST rebuild tokens whenever colors change, else half the widgets (which
    // read theme.tokens, not theme.colors) desync from the palette.
    theme.tokens = ThemeTokens::from(&colors);

    // Web-parity type: the web app renders `"Inter", ui-sans-serif, …`
    // (apps/web/src/styles.css). The app shell embeds the Inter TTFs via its
    // AssetSource and registers them with `cx.text_system().add_fonts(...)`
    // BEFORE any window opens (§3.2 — no runtime font path), so the family is
    // always resolvable. Mono stays the component's platform default (web's
    // mono stack is ui-monospace/system too).
    theme.font_family = "Inter".into();
    // Compact density (§4.4): base font one notch under web's 14px text-sm;
    // radius from the generated token scale — theme.radius drives the general
    // controls (web `rounded-md`-ish chrome at compact density → the SM step),
    // radius_lg the Dialog/Notification chrome (proportionally small → MD).
    theme.font_size = px(FONT_SIZE_PX);
    theme.radius = px(t::radius::SM);
    theme.radius_lg = px(t::radius::MD);
    // window.refresh() on the next frame / first window open picks up the new
    // palette — no live window exists at bootstrap time.
}

/// One-call bootstrap entry (§3.6): force dark mode, then apply the
/// Exponential palette. ORDER IS LOAD-BEARING — `Theme::change` installs the
/// Theme global (if missing) and resets colors AND tokens from the stock dark
/// config, so it must run first and never again after.
///
/// Call AFTER `gpui_component::init(cx)`, BEFORE opening any window. The app
/// is dark-only (web `html.dark` parity): never call `sync_system_appearance`,
/// never register an appearance observer.
pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    apply_exponential_dark(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn assert_hsla_eq(actual: Hsla, expected: Hsla, what: &str) {
        assert!(
            approx(actual.h, expected.h)
                && approx(actual.s, expected.s)
                && approx(actual.l, expected.l)
                && approx(actual.a, expected.a),
            "{what}: {actual:?} != {expected:?}"
        );
    }

    #[test]
    fn srgb8_to_rgba_maps_bytes_to_unit_floats() {
        let c = Srgb8 { r: 255, g: 0, b: 127, a: 51 };
        let rgba = c.to_rgba();
        assert!(approx(rgba.r, 1.0));
        assert!(approx(rgba.g, 0.0));
        assert!(approx(rgba.b, 127. / 255.));
        assert!(approx(rgba.a, 0.2));
    }

    #[test]
    fn srgb8_to_hsla_matches_gpui_conversion() {
        // BACKGROUND is pure gray: hue/sat 0, lightness 10/255.
        let h = tokens::BACKGROUND.to_hsla();
        assert!(approx(h.s, 0.0), "gray has no saturation: {h:?}");
        assert!(approx(h.l, 10. / 255.), "lightness from byte: {h:?}");
        assert!(approx(h.a, 1.0));
        // BORDER carries token alpha (white @ 10%).
        let b = tokens::BORDER.to_hsla();
        assert!(approx(b.a, 26. / 255.), "alpha survives the bridge: {b:?}");
        assert!(approx(b.l, 1.0), "white: {b:?}");
    }

    #[test]
    fn palette_uses_generated_tokens_not_stock() {
        let c = exponential_dark();
        assert_hsla_eq(c.background, tokens::BACKGROUND.to_hsla(), "background");
        assert_hsla_eq(c.foreground, tokens::FOREGROUND.to_hsla(), "foreground");
        assert_hsla_eq(c.primary, tokens::PRIMARY.to_hsla(), "primary");
        assert_hsla_eq(c.muted_foreground, tokens::MUTED_FOREGROUND.to_hsla(), "muted_foreground");
        assert_hsla_eq(c.border, tokens::BORDER.to_hsla(), "border");
        assert_hsla_eq(c.sidebar, tokens::SIDEBAR.to_hsla(), "sidebar");
        assert_hsla_eq(c.danger, tokens::DESTRUCTIVE.to_hsla(), "danger");
        assert_hsla_eq(c.popover, tokens::POPOVER.to_hsla(), "popover");

        // The web zinc border is translucent white — categorically different
        // from stock neutral-800; proves we are not on component defaults.
        let stock = *ThemeColor::dark();
        assert!(
            !approx(c.border.a, stock.border.a) || !approx(c.border.l, stock.border.l),
            "border must differ from stock dark"
        );
        assert!(
            !approx(c.list_hover.a, stock.list_hover.a),
            "list_hover must be the web accent/30 surface"
        );
    }

    #[test]
    fn list_surfaces_mirror_web_issue_list() {
        // EXP-1 #4: rows on page background, header accent/20, hover accent/30.
        let c = exponential_dark();
        assert_hsla_eq(c.list, tokens::BACKGROUND.to_hsla(), "list");
        assert_hsla_eq(c.list_head, tokens::ACCENT.to_hsla().opacity(0.2), "list_head");
        assert_hsla_eq(c.list_hover, tokens::ACCENT.to_hsla().opacity(0.3), "list_hover");
        assert_hsla_eq(c.table, c.list, "table mirrors list");
        assert_hsla_eq(c.table_hover, c.list_hover, "table_hover mirrors list_hover");
    }

    #[test]
    fn no_field_left_at_stock_light_default() {
        // Sanity: the builder starts from ThemeColor::dark(), never ::default()
        // (all-zero) — background must be a real color, not transparent black.
        let c = exponential_dark();
        assert!(c.background.a > 0.9);
        assert!(c.foreground.l > 0.9);
    }
}
