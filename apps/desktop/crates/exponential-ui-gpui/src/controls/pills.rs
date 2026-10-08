//! The ONE pill (EXP-698), its Button twins, the colour dot, the live dot
//! and the count badge.

use gpui::{
    div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, AnyElement, App, Div,
    ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Stateful, Styled,
};
use gpui_component::ActiveTheme as _;

use super::buttons::custom_variant_fill;
use super::sizing::{WebControl as _, CTL_LG_H, CTL_MD_H, CTL_SM_H};
use crate::chrome::{ease, Chrome, EASE_DECELERATE};

/// The two pill rungs of the control ladder: `Md` is the 32px control box
/// (`size::CONTROL_MD`), `Sm` the 24px one (`size::CONTROL_SM`). There is no
/// third rung — a capsule smaller than 24 stops being a hit target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillSize {
    /// EXP-926 — the toggle's own height (`CONTROL_LG`, the web `h-9`
    /// `TabsList`). The ONE place it is worn: a work-header action standing
    /// BESIDE the face toggle, where a shorter capsule read as a stray chip
    /// floating next to the control.
    Lg,
    Md,
    Sm,
}

impl PillSize {
    /// The capsule's height in px.
    pub fn height(self) -> f32 {
        match self {
            PillSize::Lg => CTL_LG_H,
            PillSize::Md => CTL_MD_H,
            PillSize::Sm => CTL_SM_H,
        }
    }

    /// The size a LEADING glyph renders at inside the capsule.
    pub fn glyph(self) -> f32 {
        match self {
            PillSize::Lg | PillSize::Md => 16.,
            PillSize::Sm => 12.,
        }
    }
}

/// What a pill DOES, which is the only thing that varies its chrome:
///
/// - `Action` — it runs something on click (a header button, a picker
///   trigger, a filter pill's ✕). Hover lifts it to the active fill.
/// - `Select { selected }` — it is one option of a set: the sidebar's tool
///   tabs (Inbox / My issues) and the issue composer's "Reply to reporter"
///   toggle. The selected one wears the active fill + stroke.
/// - `Readonly` — it only LABELS something (a role, a label, an attachment,
///   a count badge). No hover, no pointer cursor.
///
/// Orthogonal to all three is the PRIMARY paint flag (EXP-698, mirrored ×4 as
/// the web `<Pill primary>` / mobile `GlassPill(primary:)` prop): the ONE
/// emphasised capsule of a surface — solid `theme.primary`,
/// `primary_foreground` text, no stroke, a darker hover. It changes nothing
/// but paint: geometry, type and glyph size stay the pill's. A surface gets at
/// most one (the issue header's Start coding, the coding-now card's Watch);
/// everything else beside it stays the default glass fill, which is what makes
/// the primary one read as the action.
///
/// It rides [`glass_pill_button_primary`] rather than a `.primary()` chained
/// onto [`glass_pill_button`]: the name is already taken on `Button` by
/// gpui-component's `ButtonVariants::primary`, which every call site imports,
/// so a same-named extension method would only make every call ambiguous.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillMode {
    Action,
    Select { selected: bool },
    Readonly,
}

/// EXP-698 — the ONE capsule of the desktop, the twin of the web
/// `@exp/ui pill.tsx` and the mobile `GlassPill`s. Every chip, tag,
/// badge, filter pill, header button and picker trigger that used to be its
/// own recipe (`glass_chip`, the old two-arg `glass_pill`, `pickers::
/// chip_button`, `issue_list::label_chip`, `settings/members::role_chip`, the
/// two `file_chip`s, `pending_chip`) is this function with a different
/// [`PillSize`] / [`PillMode`].
///
/// Chrome: capsule, `fill_card` over a 1px `stroke_card` hairline, label at
/// 70% foreground. `Sm` is `text_xs` MEDIUM with 8px of side padding and a
/// 4px gap; `Md` is `text_sm` with 12/6. A leading glyph (sized
/// [`PillSize::glyph`]) or a [`pill_dot`] is the CALLER's child — the pill
/// only owns the box.
///
/// Returns a `Stateful<Div>` in every mode, `Readonly` included: the id is
/// free (gpui needs one for any element that may host a tooltip or a
/// context menu) and it keeps one return type, so a caller can flip a pill
/// between modes without rewriting the element chain. `Readonly` simply
/// carries no hover style and no pointer cursor.
pub fn glass_pill(id: impl Into<ElementId>, size: PillSize, mode: PillMode, cx: &App) -> Stateful<Div> {
    let chrome = Chrome::global(cx);
    let foreground = cx.theme().foreground;
    let selected = matches!(mode, PillMode::Select { selected: true });
    let (fill, stroke) = if selected {
        (chrome.fill_active, chrome.stroke_active)
    } else {
        (chrome.fill_card, chrome.stroke_card)
    };
    let hover_fill = chrome.fill_active;
    let (px_pad, gap) = match size {
        PillSize::Lg | PillSize::Md => (12., 6.),
        PillSize::Sm => (8., 4.),
    };
    let pill = div()
        .id(id)
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(px(size.height()))
        .px(px(px_pad))
        .gap(px(gap))
        .rounded_full()
        .border_1()
        .border_color(stroke)
        .bg(fill)
        .whitespace_nowrap()
        .when(size == PillSize::Sm, |pill| {
            pill.text_xs().font_weight(FontWeight::MEDIUM)
        })
        .when(size == PillSize::Md, |pill| pill.text_sm());
    match mode {
        PillMode::Readonly => pill.text_color(foreground.opacity(0.7)),
        PillMode::Select { selected: true } => pill.cursor_pointer().text_color(foreground),
        PillMode::Select { selected: false } => pill
            .cursor_pointer()
            .text_color(foreground.opacity(0.7))
            .hover(|style| style.text_color(foreground)),
        PillMode::Action => pill
            .cursor_pointer()
            .text_color(foreground.opacity(0.7))
            .hover(|style| style.bg(hover_fill).text_color(foreground)),
    }
}

/// The [`glass_pill`] chrome on a gpui-component `Button`.
///
/// Most capsules in the app are plain elements and take [`glass_pill`]. The
/// ones that are MENU or POPOVER triggers cannot: `DropdownMenu` is
/// implemented only for `Button` upstream (`Selectable + InteractiveElement`
/// bounds a `Stateful<Div>` does not satisfy), and every picker chip in the
/// issue header and the create dialog is such a trigger. So they stay
/// `Button`s wearing the pill's paint.
///
/// The paint rides a `ButtonCustomVariant`, not a `ghost` base plus a caller
/// refinement: the built-in variants paint their own hover fill from the
/// interactivity layer, which is applied AFTER `refine_style` and would win
/// over any glass tokens set here (and a second `.hover()` on the button
/// trips gpui's "hover style already set" assertion). The custom variant owns
/// bg/hover/active; only the stroke rides as a refinement. Same trick as
/// [`crate::controls::glass_icon_button`], which is this pill's icon-only
/// sibling.
pub fn glass_pill_button(
    id: impl Into<ElementId>,
    size: PillSize,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    let chrome = Chrome::global(cx);
    let foreground = cx.theme().foreground;
    let variant = ButtonCustomVariant::new(cx)
        .color(custom_variant_fill(chrome.fill_card))
        .hover(custom_variant_fill(chrome.fill_active))
        .active(custom_variant_fill(chrome.fill_active))
        .foreground(foreground.opacity(0.7));
    let button = gpui_component::button::Button::new(id)
        .custom(variant)
        .border_1()
        .border_color(chrome.stroke_card);
    // EXP-1176: the controls are rectangles now; the capsule shape is this
    // recipe's, so every size puts the radius back on.
    match size {
        PillSize::Sm => button.web_xs().rounded_full(),
        PillSize::Md => button.web_sm().rounded_full(),
        PillSize::Lg => button.web_md().rounded_full(),
    }
}

/// The PRIMARY paint of [`glass_pill_button`] (see [`PillMode`]): the same
/// capsule geometry, filled solid with `theme.primary` under
/// `primary_foreground` text and NO stroke.
///
/// The fill rides gpui-component's own `Primary` variant instead of a
/// [`custom_variant_fill`]ed `ButtonCustomVariant`: the variant already owns
/// the accent's hover/active pair (`button_primary_hover/_active`, a notch
/// darker) and paints its border in the fill colour, i.e. strokeless. That is
/// also why this is a separate recipe rather than a flag threaded through
/// [`glass_pill_button`] — that one has to override the border to get the
/// glass hairline, and an override cannot un-set itself.
pub fn glass_pill_button_primary(
    id: impl Into<ElementId>,
    size: PillSize,
) -> gpui_component::button::Button {
    use gpui_component::button::ButtonVariants as _;
    let button = gpui_component::button::Button::new(id).primary();
    match size {
        PillSize::Sm => button.web_xs().rounded_full(),
        PillSize::Md => button.web_sm().rounded_full(),
        PillSize::Lg => button.web_md().rounded_full(),
    }
}

/// The 6px colour dot a [`glass_pill`] carries instead of a glyph when the
/// thing it names IS a colour (an issue label, a custom status, a session's
/// liveness tone).
pub fn pill_dot(color: Hsla) -> Div {
    div().flex_shrink_0().size(px(6.)).rounded_full().bg(color)
}

/// The live dot's disc (web `size-2`).
pub const LIVE_DOT_PX: f32 = 8.;
/// One ripple of the ping halo — the web `animate-ping` period (1s).
const LIVE_DOT_PING: std::time::Duration = std::time::Duration::from_secs(1);

/// EXP-970 — the live dot, the web `LiveDot` twin (iOS `SessionStateDot`,
/// Android `LiveDot`): a session's state in one 8px disc, tinted by the
/// host's tone table, with the ATTENTION halo behind it when `ping` is set.
/// The halo is a second disc of the same tone that grows from the dot to
/// twice its size while fading from 60% to nothing, once a second on the
/// decelerate curve — the CSS `animate-ping` recipe (`cubic-bezier(0, 0,
/// 0.2, 1)` IS the ladder's `DECELERATE`, [`EASE_DECELERATE`]), so the
/// desktop ripples exactly as the web does. gpui animates by repainting, so
/// a pinging dot is a live element: reserve it for something happening
/// right now (the agent mid-turn), never for `running` alone — a
/// live-but-idle run draws the steady disc.
///
/// The container is exactly the disc's box; the halo is an absolute child,
/// so the ripple paints OVER the neighbours without moving them.
pub fn live_dot(tone: Hsla, ping: bool) -> AnyElement {
    let disc = div().size(px(LIVE_DOT_PX)).rounded_full().bg(tone);
    if !ping {
        return disc.flex_shrink_0().into_any_element();
    }
    div()
        .relative()
        .flex_shrink_0()
        .size(px(LIVE_DOT_PX))
        .child(
            div().absolute().rounded_full().bg(tone).with_animation(
                "live-dot-ping",
                Animation::new(LIVE_DOT_PING)
                    .repeat()
                    .with_easing(ease(EASE_DECELERATE)),
                |halo, delta| {
                    let grow = LIVE_DOT_PX * delta;
                    halo.left(px(-grow / 2.))
                        .top(px(-grow / 2.))
                        .size(px(LIVE_DOT_PX + grow))
                        .opacity(0.6 * (1. - delta))
                },
            ),
        )
        .child(disc.absolute().left_0().top_0())
        .into_any_element()
}

/// The count badge's tone (EXP-963, web `Badge tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// A count you parked (the rail's drafts): muted fill.
    Muted,
    /// A count that wants you (unread): the accent fill.
    Primary,
}

/// Past this a [`count_badge`] reads `99+` (web `Badge max`).
pub const COUNT_BADGE_MAX: usize = 99;

/// EXP-963 — the COUNT badge, the web `Badge` twin: the smallest chip there
/// is, a 16px capsule carrying a NUMBER and nothing else, 10px semibold so
/// a count can climb without the box twitching. Zero renders NOTHING (a
/// badge is a signal, and an empty signal is noise), past
/// [`COUNT_BADGE_MAX`] it reads `99+`. PLACEMENT stays at the call site — a
/// row's trailing edge, a rail glyph's corner — the badge owns only its
/// shape. A `Pill` `Sm` is 24 tall and carries a word; this carries a
/// quantity.
pub fn count_badge(count: usize, tone: BadgeTone, cx: &App) -> Option<Div> {
    if count == 0 {
        return None;
    }
    let theme = cx.theme();
    let (fill, ink) = match tone {
        BadgeTone::Muted => (theme.muted, theme.muted_foreground),
        BadgeTone::Primary => (theme.primary, theme.primary_foreground),
    };
    let label = if count > COUNT_BADGE_MAX {
        format!("{COUNT_BADGE_MAX}+")
    } else {
        count.to_string()
    };
    Some(
        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .h(px(16.))
            .min_w(px(16.))
            .px(px(4.))
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(fill)
            .text_color(ink)
            .text_size(px(10.))
            .line_height(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(SharedString::from(label)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-698: the pill rungs ARE the control ladder's rungs.
    #[test]
    fn pill_sizes_are_the_control_rungs() {
        assert_eq!(PillSize::Lg.height(), 36.);
        assert_eq!(PillSize::Md.height(), 32.);
        assert_eq!(PillSize::Sm.height(), 24.);
        assert_eq!(PillSize::Md.glyph(), 16.);
        assert_eq!(PillSize::Sm.glyph(), 12.);
    }
}
