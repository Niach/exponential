//! A vertically scrolling pane with an overlay scrollbar and CORRECT flex
//! sizing (EXP-67). gpui-component's `overflow_y_scrollbar` wrapper copies
//! only the wrapped element's `size` refinement onto its outer div and
//! defaults it to `size_full` — the element's `flex_1`/`min_h_0` are lost, so
//! a pane inside a flex column gets a 100%-basis wrapper and its inner scroll
//! area never resolves to the visible height (the Source Control history was
//! unscrollable this way). This helper owns the outer flex sizing itself and
//! composes the same primitives the wrapper does: a `track_scroll`ed
//! `overflow_y_scroll` area with a sibling absolute scrollbar layer.

use gpui::{
    div, px, Div, ElementId, InteractiveElement as _, IntoElement, ListState, ParentElement as _,
    Pixels, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
};
use gpui_component::scroll::{Scrollbar, ScrollbarAxis};

/// Build a `flex_1`/`min_h_0` pane whose content wheel-scrolls vertically and
/// shows an overlay scrollbar. The caller owns `handle` (view state) so the
/// scroll position survives re-renders.
pub(crate) fn v_scroll_pane(
    id: impl Into<ElementId>,
    handle: &ScrollHandle,
    content: impl IntoElement,
) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id(id.into())
                .size_full()
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(content),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(Scrollbar::new(handle).axis(ScrollbarAxis::Vertical)),
        )
}

/// EXP-1155: a CONTENT-HEIGHT band that scrolls only past `max_height` — the
/// same scroll area + overlay scrollbar as [`v_scroll_pane`], but sized by
/// its content instead of by a flex parent. For a growing region INSIDE a
/// layout that must stay whole (the composer dialog's issue chips and action
/// inputs). The caller sizes its WIDTH (`w_full`, or `flex_1` in a row).
pub(crate) fn capped_v_scroll(
    id: impl Into<ElementId>,
    handle: &ScrollHandle,
    max_height: Pixels,
    content: impl IntoElement,
) -> Div {
    div()
        .relative()
        .min_w_0()
        .flex_shrink_0()
        .child(
            div()
                .id(id.into())
                .w_full()
                .max_h(max_height)
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(content),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(Scrollbar::new(handle).axis(ScrollbarAxis::Vertical)),
        )
}

/// EXP-776: the same `flex_1`/`min_h_0` shell around a VIRTUALISED
/// [`gpui::list`] — the list scrolls itself (its state owns the offset and
/// the wheel handling), so all this adds is the flex sizing and the overlay
/// scrollbar driven by that same state (`gpui_base` implements
/// `ScrollbarHandle for ListState`).
pub(crate) fn v_list_pane(list: gpui::List, state: &ListState) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(list.size_full())
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(Scrollbar::new(state).axis(ScrollbarAxis::Vertical)),
        )
}

// ---------------------------------------------------------------------------
// The full-page scaffold (moved here from the retired Actions page, NEW-6)
// ---------------------------------------------------------------------------

/// The page column's width cap — the web page's `md:max-w-5xl`.
const PAGE_COLUMN_W: f32 = 1024.;

/// The shared full-page scaffold every rail-navigated page uses (EXP-686 —
/// Devices, Drafts, Chats): ONE scroll pane holding one centered
/// column capped at [`PAGE_COLUMN_W`] (block wrapper + `mx_auto`, the
/// EXP-179-safe centering recipe).
///
/// NO `w_full` on the column's CHILDREN (EXP-508): a percent width there
/// resolves against the UNCLAMPED ancestor available width, so sections
/// shrink-wrap and content runs off the window (the EXP-436 leak; EXP-179 has
/// the same drop-`w_full` fix). Auto width + the column's flex-col stretch
/// resolve the capped column width at every panel width. `min_w_0` rides
/// every flex hop for the same reason.
pub(crate) fn page_scaffold(
    id: &'static str,
    scroll: &ScrollHandle,
    column: gpui::Div,
) -> impl IntoElement {
    page_scaffold_with(id, scroll, column, PAGE_COLUMN_W)
}

/// [`page_scaffold`] with an explicit column cap — a list page (EXP-706)
/// reads as a narrower list than the settings-shaped pages, matching the web
/// route's `max-w-3xl`. Same flex rules apply verbatim; only the cap moves.
pub(crate) fn page_scaffold_with(
    id: &'static str,
    scroll: &ScrollHandle,
    column: gpui::Div,
    width: f32,
) -> impl IntoElement {
    gpui_component::v_flex()
        .size_full()
        .min_h_0()
        .min_w_0()
        .child(crate::scroll_pane::v_scroll_pane(
            id,
            scroll,
            div().w_full().min_w_0().child(
                column
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .py_4()
                    .max_w(px(width))
                    .mx_auto(),
            ),
        ))
}
