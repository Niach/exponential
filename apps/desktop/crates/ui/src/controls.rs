//! Web-parity control metrics (EXP-525) — the shadcn sizing layer.
//!
//! gpui-component sizes its controls in rem off this app's 13px root, which
//! lands every button/input well under the web's shadcn boxes (web Button
//! default h-9/px-4, sm h-8/px-3 rounded-full, xs h-6/px-2 rounded-full;
//! inputs h-9). These helpers are pure `Styled` refinements — gpui-component
//! applies caller refinements after its own base styles (`refine_style` runs
//! last and is replayed inside the selected/disabled state closures), so they
//! win without forking the component. `cursor_pointer` rides the same
//! refinement: gpui-component buttons default to `cursor_default`, the web
//! (and every native toolkit convention we mirror) points on hover.

use gpui::{
    div, px, App, Div, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    Styled,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon, Sizable, Size};
use theme::tokens as t;

/// Web `h-9` (Button default / Input).
pub(crate) const CTL_MD_H: f32 = 36.;
/// Web `h-8` (Button sm / small inputs).
pub(crate) const CTL_SM_H: f32 = 32.;
/// Web `h-6` (Button xs).
pub(crate) const CTL_XS_H: f32 = 24.;

/// One import per file: `use crate::controls::WebControl as _;`
/// `with_size` keeps the component's own label/icon typography mapping; the
/// explicit height/padding overrides the too-small rem-derived boxes.
pub(crate) trait WebControl: Styled + Sizable + Sized {
    /// Web Button default: h-9 px-4, theme radius.
    fn web_md(self) -> Self {
        self.with_size(Size::Medium)
            .h(px(CTL_MD_H))
            .px(px(16.))
            .cursor_pointer()
    }

    /// Web Button `sm`: h-8 px-3, capsule.
    fn web_sm(self) -> Self {
        self.with_size(Size::Small)
            .h(px(CTL_SM_H))
            .px(px(12.))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button `xs`: h-6 px-2, capsule.
    fn web_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .h(px(CTL_XS_H))
            .px(px(8.))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button icon-sm: size-8 circle.
    fn web_icon_sm(self) -> Self {
        self.with_size(Size::Small)
            .size(px(CTL_SM_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button `icon-xs`: size-6 circle.
    fn web_icon_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .size(px(CTL_XS_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Input: h-9 (radius stays the component's).
    fn web_input(self) -> Self {
        self.h(px(CTL_MD_H))
    }

    /// Web small input/select: h-8.
    fn web_input_sm(self) -> Self {
        self.h(px(CTL_SM_H))
    }
}

impl<T: Styled + Sizable> WebControl for T {}

/// Web `EmptyState` (`components/empty-state.tsx`): centered column, a 48px
/// primary-tinted icon disc, semibold title, muted description.
pub(crate) fn empty_state(
    icon: Icon,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
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
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(description.into()),
        )
}

/// Web segmented `TabsList` capsule (`components/ui/tabs.tsx`): h-9 full-width
/// capsule with a 3px inset. Pair with [`segmented_item`] children.
pub(crate) fn segmented(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_row()
        .w_full()
        .h(px(CTL_MD_H))
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(t::glass::STROKE_SECTION.to_hsla())
        .bg(t::glass::FILL_SECTION.to_hsla())
        .p(px(3.))
        .text_color(theme.muted_foreground)
}

/// A web `TabsTrigger`: equal-width capsule segment, active = glass active
/// fill + stroke.
pub(crate) fn segmented_item(active: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let item = div()
        .flex()
        .flex_row()
        .flex_1()
        .h_full()
        .items_center()
        .justify_center()
        .gap_1p5()
        .rounded_full()
        .border_1()
        .border_color(gpui::transparent_black())
        .px_2()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer();
    if active {
        item.bg(t::glass::FILL_ACTIVE.to_hsla())
            .border_color(t::glass::STROKE_ACTIVE.to_hsla())
            .text_color(theme.foreground)
    } else {
        item.hover(|style| style.text_color(theme.foreground))
    }
}
