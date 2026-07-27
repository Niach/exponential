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

pub mod terminal;

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
/// real web issue-list surfaces (see inline notes).
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
    // EXP-269: the unified product accent (mobile Accent.indigo / web --brand).
    let indigo = t::BRAND.to_hsla();

    // ---- Core surfaces (direct 1:1 token fields) ----------------------------
    c.background = bg;
    c.foreground = fg;
    c.border = border;
    c.input = input;
    // EXP-269: focus rings carry the brand accent (mobile-style interaction
    // color); the neutral RING token still drives the scrollbar thumb below.
    c.ring = indigo;
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

    // ---- Lists — glass surfaces (EXP-269, GlassTheme parity). Transparent
    //      rows sit directly on the page gradient (the opaque
    //      `.bg(colors.list)` panel paints become no-ops); hover = row fill,
    //      selection = active fill + active stroke — the exact mobile
    //      row/section/active alphas. EXP-285: headers are transparent too —
    //      ONE glassy surface, no grey band between groups and content. -------
    c.list = transparent;
    c.list_head = transparent;
    c.list_hover = t::glass::FILL_ROW.to_hsla();
    c.list_even = transparent; // web has no row striping
    c.list_active = t::glass::FILL_ACTIVE.to_hsla();
    c.list_active_border = t::glass::STROKE_ACTIVE.to_hsla();

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
    c.tab_active = t::glass::FILL_ACTIVE.to_hsla();
    c.tab_active_foreground = fg;
    c.tab_bar = transparent; // glass: tab strips float on the gradient
    c.tab_bar_segmented = t::glass::FILL_CARD.to_hsla();

    // ---- Window chrome — glass (EXP-269): the titlebar/status bar are
    //      transparent strips over the page gradient. EXP-277 drops the
    //      hairline strokes between chrome and content — the bars and the
    //      panels below read as one blended surface; the Linux CSD frame
    //      keeps the strong active stroke. -------------------------------------
    c.title_bar = transparent;
    c.title_bar_border = transparent;
    c.status_bar = transparent;
    c.status_bar_border = transparent;
    c.tiles = card;
    c.window_border = t::glass::STROKE_ACTIVE.to_hsla(); // Linux CSD only

    // ---- Overlay / selection / caret ----------------------------------------
    // web dialog overlay is `bg-black/60` (components/ui/dialog.tsx)
    c.overlay = gpui::black().opacity(0.6);
    // text selection: brand indigo at the component's 0.3 alpha clamp
    // (EXP-269 — the mobile selection accent; a near-white primary selection
    // is illegible over dark surfaces)
    c.selection = indigo.opacity(0.3);
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
    // EXP-269: interaction accents go brand indigo (links/progress/drag/slider
    // match the mobile accent usage; primary stays near-white for buttons).
    c.drag_border = indigo.opacity(0.65);
    c.drop_target = indigo.opacity(0.2);
    // Links are the one brand accent that is BODY TEXT, so it needs the 4.5:1
    // AA floor rather than the 3:1 non-text one. Raw indigo lands at ~4.0-4.5:1
    // over the zinc gradient; lightening it clears the floor while reading as
    // the same accent. That applies to the PRESSED state too — it is still body
    // text — so `link_active` only reads darker than `link`, it never drops to
    // raw indigo. The non-text accents below do stay on raw indigo.
    c.link = indigo.lighten(0.12);
    c.link_hover = indigo.lighten(0.22);
    c.link_active = indigo.lighten(0.06);
    c.progress_bar = indigo;
    // web skeleton is `bg-accent` (components/ui/skeleton.tsx)
    c.skeleton = accent;
    // scrollbar: transparent track (stock-dark behavior — a solid track would
    // stripe the #171717 sidebar/popover surfaces); thumb from RING so it is
    // token-locked and visible on every surface
    c.scrollbar = transparent;
    c.scrollbar_thumb = t::RING.to_hsla().opacity(0.7);
    c.scrollbar_thumb_hover = t::RING.to_hsla();
    // slider: filled bar = brand indigo (EXP-269), thumb = background
    // (readable on the indigo bar)
    c.slider_bar = indigo;
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
    // mono stack is ui-monospace/system too); the TERMINAL grid uses its own
    // bundled JetBrains Mono family (`terminal::FONT_FAMILY`, §6.9).
    theme.font_family = "Inter".into();
    // Compact density (§4.4): base font one notch under web's 14px text-sm;
    // radius from the generated token scale — EXP-269 glass ladder: general
    // controls take the mobile row radius (MD = 10), Dialog/Notification
    // chrome the section radius (LG = 12).
    theme.font_size = px(FONT_SIZE_PX);
    theme.radius = px(t::radius::MD);
    theme.radius_lg = px(t::radius::LG);
    // Inline code/mono runs match the terminal's bundled JetBrains Mono
    // (EXP-269 — beats the platform-default mono for glass-dark contrast; the
    // TTFs are embedded by the app shell alongside Inter).
    theme.mono_font_family = terminal::FONT_FAMILY.into();
    // window.refresh() on the next frame / first window open picks up the new
    // palette — no live window exists at bootstrap time.
}

/// EXP-277: how far the desktop lifts the gradient's top stop toward the
/// bottom stop. The token `BACKGROUND_TOP` (#09090B) is near-black and read as
/// a hard band behind the titlebar against the white-alpha panel washes below;
/// mixing 60% toward `BACKGROUND_BOTTOM` (≈ #121215) blends chrome and panels
/// into one surface. Desktop-only presentation — the token values stay
/// mobile-verbatim in tokens.json, and web/mobile keep the full ramp.
const GRADIENT_TOP_MIX: f32 = 0.6;

/// Channel-wise sRGB lerp used to derive the desktop gradient's top stop.
fn mix_rgba(a: Rgba, b: Rgba, k: f32) -> Rgba {
    Rgba {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: a.a + (b.a - a.a) * k,
    }
}

/// The desktop gradient's top stop as raw sRGB (the lerp endpoint that both
/// [`blended_background_top`] and [`background_gradient_color_at`] start from).
fn blended_background_top_rgba() -> Rgba {
    mix_rgba(
        t::glass::BACKGROUND_TOP.to_rgba(),
        t::glass::BACKGROUND_BOTTOM.to_rgba(),
        GRADIENT_TOP_MIX,
    )
}

/// The desktop gradient's top stop: `BACKGROUND_TOP` mixed
/// [`GRADIENT_TOP_MIX`] of the way toward `BACKGROUND_BOTTOM`.
pub fn blended_background_top() -> Hsla {
    blended_background_top_rgba().into()
}

/// The SOLID color of [`background_gradient`] at vertical fraction `t` (0 =
/// window top, 1 = window bottom).
///
/// EXP-285 dropped the opaque fills so everything sits on the one page
/// gradient — but an element that genuinely must occlude what is behind it
/// (the terminal's IME composition underlay, which covers the grid cells the
/// composition text overlays) still needs an OPAQUE paint. Stamping a flat
/// `palette.background` there banded visibly against the ramp; this samples
/// the ramp at the quad's own y instead, so the occluder disappears into the
/// page. `t` is clamped, so a caller passing a NaN/─ve ratio still gets the
/// top stop rather than a garbage color.
///
/// EXP-290 made the painted gradient translucent ([`glass_content_alpha`]);
/// this sampler stays FULLY OPAQUE on purpose — an occluder that lets the cells
/// behind it through is not an occluder. It therefore reads slightly more solid
/// than the page it sits on, which is the correct trade for one IME quad.
pub fn background_gradient_color_at(t: f32) -> Hsla {
    let k = if t.is_finite() { t.clamp(0., 1.) } else { 0. };
    mix_rgba(
        blended_background_top_rgba(),
        t::glass::BACKGROUND_BOTTOM.to_rgba(),
        k,
    )
    .into()
}

/// Whether this platform really puts a BLURRED backdrop behind a non-opaque
/// window — the precondition for the EXP-290 glass look. Translucency without
/// blur is not glass: it is a sharp, unsmeared ghost of the desktop showing
/// through the page (EXP-293 — wallpaper icons and filenames were legible
/// through the Linux sidebar), so where this is false the page paints FULLY
/// OPAQUE and the glass reduces to the web's subtle top→bottom gradient.
///
/// The three platform answers, at the pinned gpui rev:
/// - **macOS: yes.** `WindowBackgroundAppearance::Blurred` installs a real
///   `NSVisualEffectView` behind the window (`gpui_macos::window`).
/// - **Windows: no.** Our windows deliberately stay `Opaque` (its `Blurred` is
///   the legacy `ACCENT_ENABLE_BLURBEHIND`, a non-opaque window loses ClearType,
///   and the DirectX renderer clears an Opaque target to WHITE — a translucent
///   page there washes out instead of revealing anything).
/// - **Linux: only KDE/KWin on Wayland.** gpui's Wayland backend asks the
///   compositor's `org_kde_kwin_blur_manager`, an OPTIONAL global bind
///   (`gpui_linux::linux::wayland`), and the X11 backend has no blur code at all
///   — there `Blurred` only picks a 32-bit ARGB visual. So Mutter/GNOME,
///   wlroots, Cinnamon/muffin and every X11 session get transparency with no
///   blur, and must fall through to opaque. Setting
///   `_KDE_NET_WM_BLUR_BEHIND_REGION` ourselves is not an option: gpui exposes
///   no raw X11 window handle, and we do not patch gpui (LD: no fork).
///
/// The Linux answer is an env probe (there is no gpui API for "is the blur
/// manager bound"), cached because it cannot change within a process.
pub fn blur_backdrop_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(target_os = "windows")]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        use std::sync::OnceLock;
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            let wayland = std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty());
            let kde = std::env::var("XDG_CURRENT_DESKTOP")
                .is_ok_and(|v| v.to_ascii_uppercase().contains("KDE"));
            wayland && kde
        })
    }
}

/// EXP-293: how opaque the page paints under the SIDEBAR — the icon/nav rail,
/// the most transparent surface in the app. This is the swap the issue asks
/// for: the rail used to read as the solid column (its white-4% `FILL_SECTION`
/// wash) over a uniformly translucent page, and now it is the glass one while
/// the content next to it is nearly solid — the native macOS Finder/Mail idiom.
///
/// 1.0 without a real blur backdrop ([`blur_backdrop_available`]).
pub fn glass_sidebar_alpha() -> f32 {
    if blur_backdrop_available() {
        0.72
    } else {
        1.0
    }
}

/// How opaque the page paints under the MAIN CONTENT — everything right of
/// the rail (issue list, tabs, detail sidebar, terminal dock) and every
/// standalone window (undocked views, dialogs, the login/update surfaces).
/// 0.88 is EXP-290's proven mechanism, one notch more open than its 0.92
/// (dialed in visually over the restored frost): the lightly frosted content
/// the EXP-293 flip accidentally LOST — its base+top-up layering stacked two
/// translucent gradients, which never composite to the intended alpha (the
/// content rendered fully opaque at every top-up value; the commit's own
/// "not verified visually" note). EXP-303 restored the EXP-290 mechanism —
/// each region paints ONE gradient at its own alpha ([`background_gradient`]
/// for content, [`sidebar_background_gradient`] for the rail column) — so
/// this value reaches the screen again. The sidebar stays 0.72, deliberately
/// the glassier region.
///
/// 1.0 without a real blur backdrop ([`blur_backdrop_available`]) — Windows
/// and non-KDE-Wayland Linux have no glassy blur and stay fully opaque.
pub fn glass_content_alpha() -> f32 {
    if blur_backdrop_available() {
        0.88
    } else {
        1.0
    }
}

/// The glass page background (EXP-269): the mobile `AppBackground` gradient —
/// top to bottom, at [`glass_content_alpha`]. Paint it on the root content
/// element of every window that is ALL content (undocked windows, dialogs, the
/// login + update-required surfaces); panel surfaces above it are transparent
/// or white-alpha glass fills so the ramp shows through. EXP-277 softens the
/// desktop's top stop (see [`blended_background_top`]) so the titlebar band no
/// longer steps hard against the content panels.
///
/// The Shell window splits the page into the rail column
/// ([`sidebar_background_gradient`]) and the content column (this gradient),
/// painted SIDE BY SIDE — never stacked (EXP-303: stacked translucent
/// gradients do not composite to the intended alpha).
///
/// EXP-290/EXP-293: the stops carry the region's alpha — the ONE place the
/// window turns translucent. It pairs with
/// `WindowBackgroundAppearance::Blurred` on the window itself; without the blur
/// this would just be a smeared raw desktop (hence
/// [`blur_backdrop_available`]), and without the alpha the blur would be
/// invisible behind an opaque page. Deliberately NOT applied to
/// [`background_gradient_color_at`], which must stay an opaque occluder.
pub fn background_gradient() -> gpui::Background {
    gradient_at_alpha(glass_content_alpha())
}

/// The page gradient under the sidebar/rail column, at
/// [`glass_sidebar_alpha`]. EXP-303: the Shell paints this on the RAIL COLUMN
/// ONLY (the rail's white wash sits on top of it), side by side with the
/// content column's [`background_gradient`] — never stacked. The EXP-293
/// base+top-up layering (this gradient full-window, a second "top-up"
/// gradient over the content) is gone: two stacked translucent gradients do
/// not composite to the intended alpha, which left the content fully opaque.
pub fn sidebar_background_gradient() -> gpui::Background {
    gradient_at_alpha(glass_sidebar_alpha())
}

fn gradient_at_alpha(alpha: f32) -> gpui::Background {
    let (top, bottom) = gradient_stops_at_alpha(alpha);
    gpui::linear_gradient(
        180.,
        gpui::linear_color_stop(top, 0.),
        gpui::linear_color_stop(bottom, 1.),
    )
}

/// The page gradient's stop colors as painted (top, bottom) — the exact pair
/// [`background_gradient`] hands gpui, factored out because `Background`'s
/// fields are private at the pinned gpui rev, so the EXP-290/EXP-293 alpha
/// contract is otherwise untestable.
pub fn background_gradient_stops() -> (Hsla, Hsla) {
    gradient_stops_at_alpha(glass_content_alpha())
}

/// [`background_gradient_stops`] for the sidebar base layer.
pub fn sidebar_background_gradient_stops() -> (Hsla, Hsla) {
    gradient_stops_at_alpha(glass_sidebar_alpha())
}

fn gradient_stops_at_alpha(alpha: f32) -> (Hsla, Hsla) {
    (
        blended_background_top().opacity(alpha),
        t::glass::BACKGROUND_BOTTOM.to_hsla().opacity(alpha),
    )
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
    fn list_surfaces_are_glass_fills() {
        // EXP-269: transparent rows on the gradient, row-fill hover,
        // active-fill selection — the mobile glass alphas. EXP-285 flattened
        // the section header band to transparent: one glassy surface.
        let c = exponential_dark();
        assert!(approx(c.list.a, 0.0), "list rows are transparent: {:?}", c.list);
        assert!(approx(c.list_head.a, 0.0), "list_head is transparent (EXP-285): {:?}", c.list_head);
        assert_hsla_eq(c.list_hover, tokens::glass::FILL_ROW.to_hsla(), "list_hover");
        assert_hsla_eq(c.list_active, tokens::glass::FILL_ACTIVE.to_hsla(), "list_active");
        assert_hsla_eq(c.table, c.list, "table mirrors list");
        assert_hsla_eq(c.table_hover, c.list_hover, "table_hover mirrors list_hover");
    }

    #[test]
    fn glass_chrome_and_brand_accent() {
        // EXP-269: transparent titlebar strips (the gradient shows through the
        // gpui-component TitleBar via theme.tokens); EXP-277 dropped the
        // chrome hairlines so bars and panels blend into one surface. The
        // indigo brand accent stays on the interaction colors.
        let c = exponential_dark();
        assert!(approx(c.title_bar.a, 0.0), "title_bar is transparent: {:?}", c.title_bar);
        assert!(approx(c.tab_bar.a, 0.0), "tab_bar is transparent: {:?}", c.tab_bar);
        assert!(
            approx(c.title_bar_border.a, 0.0),
            "title_bar_border is transparent (EXP-277): {:?}",
            c.title_bar_border
        );
        assert!(
            approx(c.status_bar_border.a, 0.0),
            "status_bar_border is transparent (EXP-277): {:?}",
            c.status_bar_border
        );
        assert_hsla_eq(c.window_border, tokens::glass::STROKE_ACTIVE.to_hsla(), "window_border");
        assert_hsla_eq(c.ring, tokens::BRAND.to_hsla(), "ring");
        // link is the lightened brand (body text needs the 4.5:1 AA floor);
        // ring/selection keep raw brand since they are non-text accents.
        assert_hsla_eq(c.link, tokens::BRAND.to_hsla().lighten(0.12), "link");
        assert_hsla_eq(c.selection, tokens::BRAND.to_hsla().opacity(0.3), "selection");
        // primary deliberately stays near-white (web/mobile primary button).
        assert_hsla_eq(c.primary, tokens::PRIMARY.to_hsla(), "primary");
    }

    #[test]
    fn background_gradient_is_a_linear_gradient() {
        // Smoke test: the helper builds a gradient background (not a solid).
        // Background's tag field is private at the pinned gpui rev, so probe
        // the Debug representation.
        let bg = format!("{:?}", background_gradient());
        assert!(bg.contains("LinearGradient"), "{bg}");
    }

    #[test]
    fn page_gradient_stops_carry_their_regions_alpha() {
        // EXP-290/EXP-293: the page is the ONE translucent layer — both stops
        // of both regions carry that region's alpha so the window's
        // behind-window blur shows the desktop through. Hues/lightnesses are
        // untouched, and the two regions share the ramp exactly.
        for (label, (top, bottom), alpha) in [
            ("content", background_gradient_stops(), glass_content_alpha()),
            ("sidebar", sidebar_background_gradient_stops(), glass_sidebar_alpha()),
        ] {
            assert!(approx(top.a, alpha), "{label} top stop alpha: {top:?}");
            assert!(approx(bottom.a, alpha), "{label} bottom stop alpha: {bottom:?}");
            assert!(approx(top.l, blended_background_top().l), "{label} top color unchanged");
            assert!(
                approx(bottom.l, tokens::glass::BACKGROUND_BOTTOM.to_hsla().l),
                "{label} bottom color unchanged"
            );
        }
        // The occluder sampler is deliberately NOT translucent (it hides the
        // grid cells behind IME composition text) — the two must not drift.
        assert!(approx(background_gradient_color_at(0.).a, 1.0));
    }

    #[test]
    fn glass_alphas_keep_the_sidebar_the_glassier_region() {
        // EXP-293 flip + EXP-303 restore: the rail is the GLASS column
        // (untouched at 0.72) and the content next to it is lightly frosted
        // (EXP-290's 0.92). Guard rails: the sidebar never goes below 0.7
        // (its own labels must stay legible over a bright blurred backdrop)
        // and the content never below 0.85 (body text, code, diffs).
        let (sidebar, content) = (glass_sidebar_alpha(), glass_content_alpha());
        assert!(sidebar <= content, "sidebar {sidebar} must be the more transparent region");
        assert!((0.7..=1.0).contains(&sidebar), "sidebar alpha: {sidebar}");
        assert!((0.85..=1.0).contains(&content), "content alpha: {content}");
        if blur_backdrop_available() {
            // A real blurred backdrop exists — BOTH regions must actually be
            // see-through (EXP-303: the content is glassy too), and the two
            // must differ visibly.
            assert!(content < 1.0, "the content must actually be see-through");
            assert!(sidebar < content - 0.05, "the regions must differ perceptibly");
        } else {
            // EXP-293: no blur means translucency would be a sharp ghost of the
            // desktop (the Linux/X11 report), so the page is FULLY opaque and
            // the glass reduces to the ramp itself.
            assert!(approx(sidebar, 1.0), "no-blur sidebar must be opaque: {sidebar}");
            assert!(approx(content, 1.0), "no-blur content must be opaque: {content}");
        }
    }

    #[test]
    fn blur_backdrop_availability_matches_the_platform_backends() {
        // Locks the per-platform answer documented on the fn: only macOS
        // (NSVisualEffectView) and KDE-on-Wayland (org_kde_kwin_blur_manager)
        // blur at the pinned gpui rev.
        #[cfg(target_os = "macos")]
        assert!(blur_backdrop_available());
        #[cfg(target_os = "windows")]
        assert!(!blur_backdrop_available());
        #[cfg(target_os = "linux")]
        {
            let wayland = std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty());
            let kde = std::env::var("XDG_CURRENT_DESKTOP")
                .is_ok_and(|v| v.to_ascii_uppercase().contains("KDE"));
            assert_eq!(blur_backdrop_available(), wayland && kde);
        }
    }

    #[test]
    fn gradient_top_is_lifted_between_the_token_stops() {
        // EXP-277: the desktop top stop sits strictly between the token stops
        // — lighter than the near-black BACKGROUND_TOP (no hard titlebar
        // band), darker than BACKGROUND_BOTTOM (the ramp still ramps).
        let top = blended_background_top();
        assert!(top.l > tokens::glass::BACKGROUND_TOP.to_hsla().l);
        assert!(top.l < tokens::glass::BACKGROUND_BOTTOM.to_hsla().l);
        assert!(approx(top.a, 1.0));
    }

    #[test]
    fn gradient_sample_matches_the_stops_and_stays_opaque() {
        // EXP-285: the terminal's IME underlay paints this instead of a flat
        // `palette.background`, so it must reproduce the ramp's endpoints and
        // stay fully opaque (it is an occluder — see the fn docs).
        let top = background_gradient_color_at(0.);
        let bottom = background_gradient_color_at(1.);
        assert_hsla_eq(top, blended_background_top(), "sample at t=0");
        assert_hsla_eq(
            bottom,
            tokens::glass::BACKGROUND_BOTTOM.to_hsla(),
            "sample at t=1",
        );
        // Monotonic in between, and never see-through.
        let mid = background_gradient_color_at(0.5);
        assert!(mid.l > top.l && mid.l < bottom.l, "mid={mid:?}");
        for t in [0., 0.5, 1.] {
            assert!(approx(background_gradient_color_at(t).a, 1.0));
        }
        // Out-of-range / non-finite ratios clamp instead of producing garbage.
        assert_hsla_eq(background_gradient_color_at(-3.), top, "clamped low");
        assert_hsla_eq(background_gradient_color_at(9.), bottom, "clamped high");
        assert_hsla_eq(background_gradient_color_at(f32::NAN), top, "NaN ratio");
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
