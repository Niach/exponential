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
    div, Div, ElementId, InteractiveElement as _, IntoElement, ListState, ParentElement as _,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _,
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
