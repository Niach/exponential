//! Feedback surfaces: the empty state, the skeleton and the inline alert.

use std::time::Duration;

use gpui::{
    bounce, div, px, Animation, App, Div, FontWeight, IntoElement,
    ParentElement as _, SharedString, Styled, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon};

use super::animation::{SteppedAnimationExt as _, SMOOTH_STEP};
use crate::chrome::{ease, Chrome};

/// EXP-1274 — ONE empty state, two sizes (the web `EmptyState size`; the ×4
/// copy contract is `packages/domain-contract/fixtures/empty-state.json`).
///
/// PAGE ([`empty_state`]): what a whole screen or pane says when it has
/// nothing — a centred column of at most 448px with 48px of vertical
/// padding: the 48px primary-tinted icon disc (24px glyph), a semibold
/// title, an optional muted sentence, and an optional actions row
/// ([`empty_state_actions`]) the caller appends.
///
/// LIST ([`empty_state_list`]): what a list inside a section, a sidebar or a
/// picker says — a bare 20px muted glyph over ONE muted 14px line, 24px of
/// vertical padding. It never carries a description or a button: the
/// section band's trailing pill (or the picker's own field) IS the call to
/// action.
///
/// Copy rules: a title is at most four words and never explains (`No <noun>
/// yet` for a list that will grow, `No <noun>` for a filter result, `All
/// caught up` for the inbox), no trailing period; a description exists only
/// on a page, only when it teaches a next step that is not a visible button.
pub fn empty_state(
    icon: Icon,
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .max_w(px(448.))
        .mx_auto()
        .items_center()
        .gap_3()
        .px_6()
        .py_12()
        .text_center()
        .child(
            div()
                .size(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme.primary.opacity(0.1))
                .child(icon.size(px(24.)).text_color(theme.primary)),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(description)
        }))
}

/// The row a PAGE [`empty_state`]'s buttons go into (web `mt-2 flex
/// flex-wrap items-center justify-center gap-2`):
/// `empty_state(..).child(empty_state_actions().child(button))`.
pub fn empty_state_actions() -> Div {
    div()
        .mt_2()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_center()
        .gap_2()
}

/// The LIST size of [`empty_state`]: a bare 20px glyph at
/// `muted_foreground` / 60% over one muted `text_sm` line, centred, 24px of
/// vertical padding. No description, no button.
pub fn empty_state_list(icon: Icon, title: impl Into<SharedString>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .w_full()
        .items_center()
        .gap_1p5()
        .px_3()
        .py_6()
        .text_center()
        .child(icon.size(px(20.)).flex_shrink_0().text_color(muted.opacity(0.6)))
        .child(div().text_sm().text_color(muted).child(title.into()))
}

/// The skeleton's pulse period — the web `animate-pulse` (2s). Not a motion
/// duration token: those are the transition rungs (120/180/280), and a
/// placeholder's breathing is a different kind of time; the CURVE is the
/// ladder's standard one.
pub const SKELETON_PULSE: Duration = Duration::from_secs(2);

/// EXP-970 — the skeleton, the web `Skeleton` twin: a block standing in for
/// text that is still loading, at the SHAPE of what will arrive. The row
/// rung's corner (`radius_md`, web `rounded-md`), the theme's skeleton fill
/// (the opaque accent in the Exponential theme), breathing between full and
/// half opacity on the chrome's standard curve over [`SKELETON_PULSE`]. A
/// 16px-tall full-width bar by default; the caller sizes it like any
/// element (`.h_3p5().w_40()`, `.size_4().rounded_full()` for an avatar
/// stand-in, `.flex_1()` for a fill): a row's worth of bars, never a
/// spinner in a list.
///
/// gpui-component's own `Skeleton` is not used: its radius is the crate's
/// and its pulse is `bounce(ease_in_out)` over its own 2s, neither on the
/// ladder (the IDE's `only_controls_constructs_skeletons` keeps it out).
/// Construct with [`skeleton`].
#[derive(IntoElement)]
pub struct Skeleton {
    style: gpui::StyleRefinement,
}

/// A [`Skeleton`] block.
pub fn skeleton() -> Skeleton {
    Skeleton {
        style: gpui::StyleRefinement::default(),
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl gpui::RenderOnce for Skeleton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_component::StyledExt as _;
        let chrome = Chrome::global(cx);
        let (radius, curve) = (chrome.radius_md, chrome.ease_standard);
        div()
            .flex_shrink_0()
            .w_full()
            .h_4()
            .rounded(px(radius))
            .bg(cx.theme().skeleton)
            .refine_style(&self.style)
            // EXP-1325: a placeholder lives as long as its load — stepped.
            .with_stepped_animation(
                "skeleton-pulse",
                Animation::new(SKELETON_PULSE)
                    .repeat()
                    .with_easing(bounce(ease(curve))),
                SMOOTH_STEP,
                |block, delta| block.opacity(1. - 0.5 * delta),
            )
    }
}

/// The two [`alert`] variants (web `Alert variant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertVariant {
    /// A notice: the card fill under the card hairline, foreground text.
    Default,
    /// A failure: the danger tint at a tenth under its stroke at half,
    /// danger text.
    Destructive,
}

/// EXP-970 — the inline alert, the web `Alert` twin: a message that belongs
/// to the page it interrupts, not a toast and not a dialog. The row rung's
/// corner, `px_3 py_2`, `text_sm`; the leading glyph (16px, nudged 2px down
/// to sit on the first line) earns its column only when one is passed. The
/// caller appends its content — [`alert_title`] and/or a description — as
/// children; a banner that wants to wrap pills chains `.flex_wrap()`.
pub fn alert(variant: AlertVariant, glyph: Option<Icon>, cx: &App) -> Div {
    let chrome = Chrome::global(cx);
    let theme = cx.theme();
    let (fill, stroke, text) = match variant {
        AlertVariant::Default => (chrome.fill_card, chrome.stroke_card, theme.foreground),
        AlertVariant::Destructive => (
            theme.danger.opacity(0.1),
            theme.danger.opacity(0.5),
            theme.danger,
        ),
    };
    div()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .px_3()
        .py_2()
        .rounded(px(chrome.radius_md))
        .border_1()
        .border_color(stroke)
        .bg(fill)
        .text_sm()
        .text_color(text)
        .children(glyph.map(|glyph| {
            div()
                .flex_shrink_0()
                .pt_0p5()
                .child(glyph.size(px(16.)).flex_shrink_0())
        }))
}

/// An [`alert`]'s one-line title (web `AlertTitle`): medium weight, tight.
pub fn alert_title(title: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .font_weight(FontWeight::MEDIUM)
        .truncate()
        .child(title.into())
}
