//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.
//!
//! VAPP-90: the recipe BODIES (group bands, flat rows, cards, inset groups and
//! their hairlines, the form rows, the property row, the bulk bar, the pill
//! and its Button twins, the dots, the count badge, the markdown styles, the
//! edge-fade shapes) live in the Exponential UI SDK's
//! `exponential_ui_gpui::controls`, keyed on a host-installed `Chrome`. This
//! module re-exports them under their old names; the few whose SDK twin takes
//! the `&Chrome` explicitly (they have no `cx` to read it from) or a host
//! value (the edge fades' window-ramp fill) are one-line wrappers that pass
//! [`crate::controls::ide_chrome`]. What stays here is IDE-only: the window
//! ramp sampling (`panel_fill_at`, [`scrim_ground`]) and the rich tab, which
//! carries app types (`coding::CodingAgent`, the run lead).

use exponential_ui_gpui::controls as sdk;
use gpui::{
    div, prelude::FluentBuilder as _, px, App, Div, ElementId, Hsla, InteractiveElement as _,
    ParentElement as _, SharedString, Stateful, Styled,
};
use gpui_component::{text::TextViewStyle, ActiveTheme as _};
use theme::tokens as t;

#[allow(unused_imports)] // the shared API; not every recipe has an IDE caller today
pub(crate) use exponential_ui_gpui::controls::{
    bare_code_markdown_style, count_badge, custom_variant_fill, glass_bar,
    glass_input_row, glass_picker_row, glass_picker_select, glass_pill, glass_pill_button,
    glass_pill_button_primary, glass_row_input, glass_row_shell, glass_section_band,
    glass_section_band_fold, glass_section_band_fold_loading, glass_section_band_loading,
    glass_section_header, glass_section_header_loading, glass_tab_item, glass_tabs_row,
    glass_toggle_row, live_dot, picker_row_chevron, picker_value_label, pill_dot, property_row, BadgeTone,
    PillMode, PillSize, COUNT_BADGE_MAX, FLAT_ROW_COMPACT_H, LIVE_DOT_PX, ROW_DESCRIPTION_MAX_W,
};

use crate::controls::ide_chrome;

/// EXP-818: ONE flat list row (SDK `controls::flat_row`).
pub(crate) fn flat_row() -> Div {
    sdk::flat_row(ide_chrome())
}

/// EXP-963: the COMPACT density of [`flat_row`] (SDK
/// `controls::flat_row_compact`).
pub(crate) fn flat_row_compact() -> Div {
    sdk::flat_row_compact(ide_chrome())
}

/// Card surface (SDK `controls::glass_card`).
pub(crate) fn glass_card() -> Div {
    sdk::glass_card(ide_chrome())
}

/// EXP-642: ONE row of a carded list (SDK `controls::glass_row_card`).
pub(crate) fn glass_row_card() -> Div {
    sdk::glass_row_card(ide_chrome())
}

/// EXP-694 — the inset-grouped card STACK (SDK `controls::glass_group`).
pub(crate) fn glass_group() -> Div {
    sdk::glass_group(ide_chrome())
}

/// A [`glass_group`] filled with `rows`, hairline-divided (SDK
/// `controls::glass_group_rows`).
pub(crate) fn glass_group_rows(rows: Vec<Div>) -> Div {
    sdk::glass_group_rows(ide_chrome(), rows)
}

/// The hairline a [`glass_group`] row draws above itself (SDK
/// `controls::glass_row_divider`).
pub(crate) fn glass_row_divider<T: Styled>(row: T) -> T {
    sdk::glass_row_divider(ide_chrome(), row)
}

/// EXP-1320 — one row of an entity list: a full-width wrapper, flat and
/// gapless, NO hairline (lists ×4 = a band over flat rows; only form groups,
/// [`glass_group_rows`], keep their dividers).
pub(crate) fn list_row(row: impl gpui::IntoElement) -> Div {
    gpui::div().w_full().min_w_0().child(row)
}

/// Shared markdown `TextView` style (EXP-282; SDK `controls::markdown_style`):
/// glass code blocks instead of the component's opaque `muted` panel.
pub(crate) fn markdown_style() -> TextViewStyle {
    sdk::markdown_style(ide_chrome())
}

/// EXP-1162 — where on the window ramp each edge strip samples the panel:
/// the top strip sits under the tab strip and the work header, the bottom
/// one just above the window's bottom edge. The ramp is a 12→17 grey step,
/// so a sample a few rows off is invisible.
const EDGE_TOP_RAMP_T: f32 = 0.15;
const EDGE_BOTTOM_RAMP_T: f32 = 0.95;

/// EXP-1256 — the cutout panel's hairline (the shell's card face,
/// `border_1`) and its INNER radius (`radius::LG` less that hairline). Every
/// edge strip and the floating bar's ground stay inside the one and round to
/// the other where they run flush to the card's edge: a strip painted over
/// the hairline cut a gap into the card's border, and its square corners
/// poked over the card's rounded ones.
pub(crate) const PANEL_BORDER: f32 = 1.;
pub(crate) const PANEL_INNER_RADIUS: f32 = t::radius::LG - PANEL_BORDER;

/// The cutout panel's colour as it paints at ramp fraction `t`: the card's
/// OWN ground, its two layers (the window gradient under the `FILL_PANEL`
/// wash) composited OPAQUE — the strip must hide the content it fades over,
/// not tint it. EXP-1256: measured on the reported capture, the strip and
/// the card read the same value; what showed was the strip cutting the
/// card's hairline, now [`PANEL_BORDER`] inset.
pub(crate) fn panel_fill_at(t: f32) -> Hsla {
    crate::picker::over(
        theme::background_gradient_color_at(t),
        t::glass::FILL_PANEL.to_hsla(),
    )
}

/// EXP-1162 — the TOP edge strip (contract `detail-chrome.json` `edgeTop`):
/// overlaid on the top of a scrolling body, directly under the work header,
/// it fades from the panel's colour to nothing, so content slides away under
/// the bar instead of being cut at a hairline (SDK `controls::edge_fade_top`
/// on the window ramp's panel colour). The caller's body wrapper must be
/// `relative()`; the strip is paint-only. EXP-1256: it hangs mid-card (under
/// a bar), so it keeps off the card's side hairlines and stays square.
pub(crate) fn edge_fade_top() -> Div {
    sdk::edge_fade_top(
        panel_fill_at(EDGE_TOP_RAMP_T),
        domain::detail_chrome::EDGE_TOP,
        PANEL_BORDER,
        0.,
    )
}

/// EXP-1162 — the floating bar's GROUND once the issue face's title has
/// scrolled under it: the panel colour, OPAQUE. Content really passes under
/// that bar, and with no blur to soften it the contract's 0.72 `scrim` left
/// the text readable behind the collapsed title — a client that cannot blur
/// paints the band solid and keeps only the fade under it.
pub(crate) fn scrim_ground() -> Hsla {
    panel_fill_at(EDGE_TOP_RAMP_T)
}

/// EXP-1256 — a band painted in [`scrim_ground`] (the floating bar's ground,
/// the pinned tray's) kept inside the card: off its side hairlines, and with
/// `flush_top` (the band starts at the card's top edge) its top corners
/// rounded to [`PANEL_INNER_RADIUS`]. The caller positions it; the inset is a
/// MARGIN, so it holds for an absolute band (`left_0().right_0()`) and an
/// in-flow one alike. A strip hung inside the band is already clear of the
/// hairlines: give it `left_0().right_0()`.
pub(crate) fn scrim_band(flush_top: bool) -> Div {
    let radius = if flush_top { PANEL_INNER_RADIUS } else { 0. };
    div()
        .mx(px(PANEL_BORDER))
        .rounded_tl(px(radius))
        .rounded_tr(px(radius))
        .bg(scrim_ground())
}

/// EXP-1271 — the panel's own ground mid-page (opaque): what the mark's
/// cut stripes paint in where it sits on the page itself (the composer's
/// STARTING row).
pub(crate) fn page_ground() -> Hsla {
    panel_fill_at(0.5)
}

/// The top strip hung under that ground: it starts at the ground's own
/// opaque colour and fades to nothing, so the band has no lower edge.
pub(crate) fn edge_fade_top_scrim() -> Div {
    edge_fade_top()
}

/// EXP-1162 — the BOTTOM edge strip (`edgeBottom`): the top strip mirrored,
/// fading upwards from the pane's bottom edge (the issue face's end; the
/// run's Dock wears [`dock_edge_fade`]). Same contract as
/// [`edge_fade_top`]. EXP-1256: that edge IS the card's bottom edge, so the
/// strip keeps off the hairlines AND rounds its bottom corners to the card's
/// inner radius.
pub(crate) fn edge_fade_bottom() -> Div {
    sdk::edge_fade_bottom(
        panel_fill_at(EDGE_BOTTOM_RAMP_T),
        domain::detail_chrome::EDGE_BOTTOM,
        PANEL_BORDER,
        PANEL_INNER_RADIUS,
    )
}

/// N-12 — THE Dock's edge fade behind its band: the panel's ground under
/// the band, fading out `above` px over it (web `-top-10`, the ground at
/// 72% from the bottom). Paint-only; the band's box is its parent.
pub(crate) fn dock_edge_fade(above: f32) -> Div {
    div()
        .absolute()
        .left(px(PANEL_BORDER))
        .right(px(PANEL_BORDER))
        .top(px(-above))
        .bottom_0()
        .bg(crate::dock::fade_gradient(panel_fill_at(EDGE_BOTTOM_RAMP_T)))
}

// ---------------------------------------------------------------------------
// The ONE rich tab (EXP-698)
// ---------------------------------------------------------------------------

/// The leading marker of a [`rich_tab`].
pub(crate) enum RichTabStatus {
    /// A status/agent glyph, already coloured by the caller
    /// (`icons::resolved_status_icon`, `ChipLead::icon`).
    Glyph(gpui_component::Icon),
    /// A liveness tone dot (`queries::session_dot_tone`) — every run chip.
    Dot(Hsla),
    None,
}

/// How wide a [`rich_tab`] may grow before its title truncates — the chip's
/// whole `max-w`, web `DockTab`'s 240px (EXP-877).
pub(crate) const RICH_TAB_MAX_W: f32 = 240.;

/// How wide a [`rich_tab`]'s title may grow before it truncates. The chip cap
/// above bounds the row; this bounds the title inside it.
pub(crate) const RICH_TAB_TITLE_MAX_W: f32 = 180.;

/// The content of a [`rich_tab`]. Handlers stay the CALLER's: the two strips
/// differ on middle-click, context menus and the hover-revealed undock, and
/// folding those in here would make the builder a switchboard.
pub(crate) struct RichTab {
    pub(crate) id: ElementId,
    pub(crate) selected: bool,
    pub(crate) status: RichTabStatus,
    /// The mono shortcode ahead of the title (`EXP-698`), muted.
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: Option<SharedString>,
    /// D8: a title still loading — a placeholder bar this many px wide in
    /// the title slot (never the text "Loading…").
    pub(crate) placeholder: Option<f32>,
    /// A tinted exit-code badge (a terminal chip whose child exited).
    pub(crate) badge: Option<(SharedString, Hsla)>,
    /// EXP-905: whether the caller appends the trailing ghost × cluster. A
    /// chip WITHOUT one (a live run, EXP-877) pads its right side like its
    /// left ([`rich_tab_padding`]) instead of jamming the label against the
    /// border.
    pub(crate) closable: bool,
}

/// EXP-905 — a [`rich_tab`]'s `(left, right)` padding in px: `pl 10 / pr 4`
/// when the 24px ghost × trails the label (its own box supplies the air),
/// `pl 10 / pr 10` when nothing does. Byte-identical with the web
/// `StripChip` (`px-2.5`, `pr-1` beside a trailing control, the ×'s `mr-1`);
/// the strip's width measurer reads the same pair, so a live chip is
/// measured exactly as wide as it paints.
pub(crate) fn rich_tab_padding(closable: bool) -> (f32, f32) {
    if closable {
        (STRIP_CHIP_PX, 4.)
    } else {
        (STRIP_CHIP_PX, STRIP_CHIP_PX)
    }
}

/// EXP-1334 — THE strip chip box (web `stripTabClass`, ONE for every strip
/// of chips: the thread header's issue + session chips, the work tabs strip
/// above the content card, the "+N" of a folded zone): 32px tall, 6px
/// radius, 10px sides, `gap 6`, `text_sm`. Three states and no more: at
/// rest a `bordered` chip keeps its hairline (a tab has no stroke) and reads
/// muted; the pointer only turns the text foreground (no wash); active =
/// the strong glass stroke over the glass active fill, foreground.
pub(crate) const STRIP_CHIP_H: f32 = 32.;
pub(crate) const STRIP_CHIP_PX: f32 = 10.;
/// The "+N" overflow button's sides (web `StripOverflowButton`, `px-2`).
pub(crate) const STRIP_OVERFLOW_PX: f32 = 8.;
/// The chrome [`strip_overflow_button`] adds around its label: the sides
/// and the 1px hairline, what a strip's width measurer reserves.
pub(crate) const STRIP_OVERFLOW_CHROME: f32 = 2. * STRIP_OVERFLOW_PX + 2.;

pub(crate) fn strip_chip_frame(id: impl Into<ElementId>, active: bool, bordered: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let chip = div()
        .id(id)
        .h(px(STRIP_CHIP_H))
        .min_w_0()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .px(px(STRIP_CHIP_PX))
        .rounded(px(6.))
        .border_1()
        .text_sm()
        .font_weight(gpui::FontWeight::NORMAL)
        .cursor_pointer();
    if active {
        chip.border_color(t::glass::STROKE_STRONG.to_hsla())
            .bg(t::glass::FILL_ACTIVE.to_hsla())
            .text_color(foreground)
    } else {
        chip.border_color(if bordered {
            t::glass::STROKE_ROW.to_hsla()
        } else {
            gpui::transparent_black()
        })
        .text_color(theme.muted_foreground)
        .hover(move |style| style.text_color(foreground))
    }
}

/// The "+N" of a folded zone (THE Menu's trigger, web
/// `StripOverflowButton`): the strip chip box at rest, the count alone.
pub(crate) fn strip_overflow_button(id: impl Into<ElementId>, count: usize, cx: &App) -> Stateful<Div> {
    strip_chip_frame(id, false, true, cx)
        .px(px(STRIP_OVERFLOW_PX))
        .child(SharedString::from(format!("+{count}")))
}

impl RichTab {
    pub(crate) fn new(id: impl Into<ElementId>, selected: bool) -> Self {
        Self {
            id: id.into(),
            selected,
            status: RichTabStatus::None,
            identifier: None,
            title: None,
            placeholder: None,
            badge: None,
            closable: true,
        }
    }
}

/// EXP-698/EXP-877 — the ONE RICH tab, byte-identical with the web chip
/// (EXP-1334: THE strip chip box, [`strip_chip_frame`], the same box the
/// thread header's chips wear): 32px tall, 6px radius, capped at
/// [`RICH_TAB_MAX_W`], `pl 10 / pr 4` (the short right side is the 24px
/// ghost × the caller appends; `pr 10` on a chip with no ×, EXP-905
/// [`rich_tab_padding`]), `gap 6`, a 14px lead box holding a 14px glyph or
/// an 8px dot, the mono `text_xs` muted identifier and the `text_sm`
/// truncating title.
///
/// Three states and no more (the frame's): rest = the hairline + muted
/// text, hover = foreground text, active = the strong stroke over the glass
/// active fill + foreground. EXP-877 retired the ` · machine` caption (a
/// tab is chrome, and the machine is on the run's own header), the paused
/// dimming (a chip that dims reads as disabled) and the working spinner
/// (the steady liveness dot carries it; the spinner is the LIST's, EXP-848).
///
/// gpui's rem is 14px ([`theme::FONT_SIZE_PX`]), so every one of these is a
/// `px()` literal — the spacing helpers would resolve 8px as `px_2` only by
/// coincidence of the rem.
pub(crate) fn rich_tab(tab: RichTab, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (pad_left, pad_right) = rich_tab_padding(tab.closable);
    let chip = strip_chip_frame(tab.id, tab.selected, true, cx)
        .max_w(px(RICH_TAB_MAX_W))
        .pl(px(pad_left))
        .pr(px(pad_right));
    // ONE 14px lead box, so a dot chip and a glyph chip line their titles up
    // (the strip's `lead_reserve_px` measures this box, not its content).
    chip.map(|chip| match tab.status {
        RichTabStatus::None => chip,
        status => chip.child(
            div()
                .flex_shrink_0()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .map(|slot| match status {
                    RichTabStatus::Glyph(icon) => {
                        slot.child(gpui_component::Sizable::with_size(icon, px(14.)))
                    }
                    // EXP-970: the shared disc; a chip never pings (EXP-877:
                    // the strip re-rendering on every turn edge was motion
                    // without information).
                    RichTabStatus::Dot(tone) => slot.child(live_dot(tone, false)),
                    RichTabStatus::None => slot,
                }),
        ),
    })
    .children(tab.identifier.map(|identifier| {
        div()
            .flex_shrink_0()
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(theme.muted_foreground)
            .whitespace_nowrap()
            .child(identifier)
    }))
    .children(
        tab.placeholder
            .filter(|_| tab.title.is_none())
            .map(|width| crate::feedback::placeholder_bar(width, cx)),
    )
    .children(tab.title.map(|title| {
        div()
            .max_w(px(RICH_TAB_TITLE_MAX_W))
            .truncate()
            .text_sm()
            .font_weight(gpui::FontWeight::NORMAL)
            .child(title)
    }))
    .children(tab.badge.map(|(label, color)| {
        div()
            .flex_shrink_0()
            .text_xs()
            .px_1()
            .rounded(px(3.))
            .bg(color.opacity(0.15))
            .text_color(color)
            .child(label)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use exponential_ui_gpui::controls::CUSTOM_VARIANT_ALPHA_FACTOR;

    /// EXP-698: the two pill rungs ARE the token ladder's control rungs — no
    /// hand-typed 20/26/28 heights, which is how the app ended up with six
    /// chip shapes before this sweep.
    #[test]
    fn pill_sizes_are_the_token_control_rungs() {
        assert_eq!(PillSize::Md.height(), t::size::CONTROL_MD);
        assert_eq!(PillSize::Sm.height(), t::size::CONTROL_SM);
        assert_eq!(PillSize::Md.height(), 32.);
        assert_eq!(PillSize::Sm.height(), 24.);
        // A leading glyph is half the capsule's height, both rungs.
        assert_eq!(PillSize::Md.glyph(), 16.);
        assert_eq!(PillSize::Sm.glyph(), 12.);
        assert!(PillSize::Sm.glyph() < PillSize::Md.glyph());
    }

    /// `Select` is the only mode whose chrome depends on state; `Action` and
    /// `Readonly` are single-valued. (The chrome itself needs a `Window` to
    /// render, so this pins the discriminants the builder switches on.)
    #[test]
    fn pill_select_mode_carries_its_selection() {
        assert_ne!(
            PillMode::Select { selected: true },
            PillMode::Select { selected: false }
        );
        assert_ne!(PillMode::Action, PillMode::Readonly);
        assert_ne!(PillMode::Action, PillMode::Select { selected: false });
    }

    /// The rich-tab builder starts EMPTY apart from its identity: every strip
    /// fills only the slots it has, and an unset slot must render nothing
    /// rather than a placeholder box (the bottom bar's terminal tabs carry no
    /// identifier, the top strip's chips no badge).
    #[test]
    fn rich_tab_builder_defaults_to_identity_only() {
        let tab = RichTab::new("t", true);
        assert!(tab.selected);
        assert!(matches!(tab.status, RichTabStatus::None));
        assert!(tab.identifier.is_none());
        assert!(tab.title.is_none());
        assert!(tab.badge.is_none());
        assert!(tab.closable, "a chip carries its × unless the caller says not");
    }

    /// EXP-905: a chip with no × (a live run) pads its right side like its
    /// left — the label used to touch the border — while a closable chip
    /// keeps the short right side its 24px × fills. Web `DockTab` twin:
    /// `pl-2 pr-2` / `pl-2 pr-1`.
    #[test]
    fn a_chip_without_a_close_button_pads_both_sides_equally() {
        assert_eq!(rich_tab_padding(false), (10., 10.));
        assert_eq!(rich_tab_padding(true), (10., 4.));
        // EXP-1334: the web `StripChip` box — `px-2.5`, the "+N" `px-2`.
        assert_eq!(STRIP_CHIP_PX, 10.);
        assert_eq!(STRIP_CHIP_H, 32.);
        assert_eq!(STRIP_OVERFLOW_CHROME, 18.);
    }

    /// EXP-698: a Button pill and a `Div` pill must paint the SAME surface.
    /// gpui-component runs a custom variant's colour through
    /// `mix_oklab(transparent, 0.2)` before painting it, so the pre-division
    /// in [`custom_variant_fill`] has to mix back to the token exactly —
    /// asserted with the crate's OWN mix, so an upstream change to either the
    /// factor or the mix semantics fails here instead of on screen.
    #[test]
    fn custom_variant_fills_mix_back_to_the_glass_tokens() {
        use gpui_component::theme::Colorize as _;
        let transparent = gpui::transparent_black();
        for token in [t::glass::FILL_CARD, t::glass::FILL_ACTIVE] {
            let want = token.to_hsla();
            let painted = custom_variant_fill(want).mix_oklab(transparent, CUSTOM_VARIANT_ALPHA_FACTOR);
            assert!(
                (painted.a - want.a).abs() < 0.001,
                "compensated fill must paint at the token alpha: painted {:?} vs token {:?}",
                painted,
                want,
            );
            assert!(
                (painted.l - want.l).abs() < 0.01,
                "the mix must leave lightness alone: painted {:?} vs token {:?}",
                painted,
                want,
            );
        }
        // The naive (uncompensated) hand-off is what this guards against.
        let naive = t::glass::FILL_CARD
            .to_hsla()
            .mix_oklab(transparent, CUSTOM_VARIANT_ALPHA_FACTOR);
        assert!(
            naive.a < t::glass::FILL_CARD.to_hsla().a * 0.5,
            "sanity: passing the token straight through paints it far too faint ({naive:?})"
        );
    }

    #[test]
    fn rich_tab_caps_are_shared_with_the_strip_measurement() {
        // `screens::measure_chip_width` reads these for its overflow
        // computation; a divergence collapses tabs into "+N" too early.
        // EXP-877: 240px is the WEB chip's cap, byte-identical ×2.
        assert_eq!(RICH_TAB_MAX_W, 240.);
        assert_eq!(RICH_TAB_TITLE_MAX_W, 180.);
        assert!(RICH_TAB_TITLE_MAX_W < RICH_TAB_MAX_W);
    }
}
