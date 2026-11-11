//! #EXP-1629: the settings detail column's chrome, the web settings
//! route's own (`routes/t/$teamSlug/settings/route.tsx`
//! `WideSettingsLayout`): "Settings" (EXP-1550: no explanatory subtitle),
//! a hairline, then the selected pane, all in one centered `max-w-4xl`
//! column (EXP-771).
//!
//! This replaces the EXP-277 "no screen header" rule (the center tab
//! carried the title — but Settings is a TAB-LESS full-page screen, so
//! nothing named the page) and the EXP-282 "hug the left edge under a 672px
//! cap" one (the nav is the window's left column now; a column pinned to
//! its right edge left the rest of a wide window empty).
//!
//! The SCROLL REGION stays full width with the scrollbar at the viewport
//! edge — only the content column is capped and centered, the
//! page-scaffold recipe every other full-page screen uses
//! (`scroll_pane::page_scaffold_with`).

use std::rc::Rc;

use gpui::{div, px, AnyElement, App, Entity, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::v_flex;

/// The selected pane, built for the frame that draws it.
pub(crate) type PaneBuild = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A pane entity as the column's [`PaneBuild`].
pub(crate) fn pane_of<V: Render>(pane: &Entity<V>) -> PaneBuild {
    let view: gpui::AnyView = pane.clone().into();
    Rc::new(move |_, _| view.clone().into_any_element())
}

/// No pane (a hand-over in flight).
pub(crate) fn empty_pane() -> PaneBuild {
    Rc::new(|_, _| div().into_any_element())
}

/// The detail column: its ONE scroll offset (the header and the pane share
/// a scroll region, so a section switch rewinds it — the web's route change
/// lands at the top of the page, never mid-list).
pub(crate) struct DetailShell {
    scroll: gpui::ScrollHandle,
    /// EXP-1558 (DEV-ONLY, `EXP_DEV_SCROLL_TO=end`): the column's max offset
    /// when it was last parked at its end ([`crate::screens::dev_park_at_end`]).
    dev_parked_end: std::cell::Cell<gpui::Pixels>,
}

impl DetailShell {
    pub(crate) fn new(_window: &mut Window, _cx: &mut App) -> Self {
        Self { scroll: gpui::ScrollHandle::new(), dev_parked_end: Default::default() }
    }

    /// Back to the top (a section switch).
    pub(crate) fn rewind(&self) {
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
    }

    /// The column showing `pane`; `key` names the scroll region (one per
    /// section).
    pub(crate) fn render(&mut self, key: SharedString, pane: PaneBuild, window: &mut Window, cx: &mut App) -> AnyElement {
        let header = div()
            .w_full()
            .min_w_0()
            .text_2xl()
            .font_weight(FontWeight::BOLD)
            .child("Settings");
        // `flex_shrink_0`: a 1px child in a flex column is the first thing
        // the layout eats.
        let separator = div()
            .w_full()
            .h(px(1.))
            .flex_shrink_0()
            .bg(super::row_stroke(cx));

        // EXP-1558 (DEV-ONLY): `settings-assistant-meeting` = the Assistant
        // pane at its end (the Meeting mode rows), web's `scrollToMeetingMode`.
        crate::screens::dev_park_at_end(&self.scroll, &self.dev_parked_end, window);
        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(crate::scroll_pane::v_scroll_pane(
                key,
                &self.scroll,
                // EXP-1557: the column lays out at the web's 16px rem, so the
                // panes' type, rows and controls match web's settings page
                // size for size (the IDE's 14px rem drew them ~12% small).
                div().w_full().min_w_0().child(crate::controls::web_rem(
                    detail_column().child(header).child(separator).child(pane(window, cx)),
                )),
            ))
            .into_any_element()
    }
}

/// EXP-771: the shared settings detail column — the web route's
/// `mx-auto w-full max-w-4xl space-y-6 p-6` grid. It used to be a
/// left-aligned 672px column (EXP-282); the nav lives in the window's left
/// column since EXP-456, so hugging its edge just left the rest of a wide
/// window blank. The `space-y-6` is the gap between the header, its hairline
/// and the pane — every pane spaces its OWN sections.
fn detail_column() -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .max_w(px(896.))
        .mx_auto()
        .p(px(24.))
        .gap(px(24.))
}
