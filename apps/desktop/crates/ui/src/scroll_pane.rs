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
    div, px, AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ListState,
    ParentElement as _, Pixels, RenderOnce, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, Window,
};
use gpui_component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle, ScrollbarStyles};

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

// ---------------------------------------------------------------------------
// EXP-1095: the SIDEBAR's slim scrollbar
// ---------------------------------------------------------------------------

/// The sidebar thumb's width at rest (px) — a hairline, not a control.
pub(crate) const SIDEBAR_THUMB_WIDTH: f32 = 3.;
/// Its width while hovered or dragged (px).
pub(crate) const SIDEBAR_THUMB_ACTIVE_WIDTH: f32 = 4.;
/// Its gap to the column's right edge (px).
pub(crate) const SIDEBAR_THUMB_INSET: f32 = 1.;
/// The hit strip (px): the vendored 16px track swallowed clicks on the rows'
/// trailing badges. 8px = the column's `px_2` gutter, so it never reaches a
/// row.
pub(crate) const SIDEBAR_TRACK_WIDTH: f32 = 8.;

/// EXP-1095: per-INSTANCE styles for every sidebar scroller. The vendored
/// scrollbar (gpui-component `da4f936`) draws a 6px thumb inset 4px (8px
/// when active) inside a 16px track, so it sat 4-12px in from the edge and
/// overlapped the rows. The sidebar's thumb hugs the right edge
/// (`SIDEBAR_THUMB_INSET`), is 3-4px wide, rounded, and dimmer than the
/// global thumb. The theme's `scrollbar_thumb` tokens stay untouched: every
/// other scrollbar in the app keeps them.
pub(crate) fn sidebar_scrollbar_styles(styles: ScrollbarStyles) -> ScrollbarStyles {
    let ring = theme::tokens::RING.to_hsla();
    let width = px(SIDEBAR_THUMB_WIDTH);
    let active: Pixels = px(SIDEBAR_THUMB_ACTIVE_WIDTH);
    let inset = px(SIDEBAR_THUMB_INSET);
    styles
        .track(|t| t.width(px(SIDEBAR_TRACK_WIDTH)))
        .thumb(|t| {
            t.width(width)
                .inset(inset)
                .radius(width / 2.)
                .bg(ring.opacity(0.35))
        })
        .thumb_hover(|t| {
            t.width(active)
                .inset(inset)
                .radius(active / 2.)
                .bg(ring.opacity(0.6))
        })
        .thumb_active(|t| {
            t.width(active)
                .inset(inset)
                .radius(active / 2.)
                .bg(ring.opacity(0.75))
        })
}

/// The absolute overlay layer holding a sidebar-styled vertical scrollbar
/// over `handle` — for a scroller that already owns its scroll area (the
/// `ListNav` virtual list).
pub(crate) fn sidebar_scrollbar_layer<H: ScrollbarHandle + Clone>(
    id: impl Into<ElementId>,
    handle: &H,
) -> Div {
    div().absolute().top_0().left_0().right_0().bottom_0().child(
        Scrollbar::new(handle)
            .id(id)
            .axis(ScrollbarAxis::Vertical)
            .styles(sidebar_scrollbar_styles),
    )
}

/// [`v_scroll_pane`] with the sidebar's slim scrollbar (EXP-1095).
pub(crate) fn sidebar_scroll_pane(
    id: impl Into<ElementId>,
    handle: &ScrollHandle,
    content: impl IntoElement,
) -> Div {
    let id: ElementId = id.into();
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id(id.clone())
                .size_full()
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(content),
        )
        .child(sidebar_scrollbar_layer((id, "scrollbar"), handle))
}

/// A [`sidebar_scroll_pane`] whose scroll handle lives in the window's keyed
/// state under `id` — the drop-in for gpui-component's `overflow_y_scrollbar`
/// (which takes no styles) on a sidebar list that has no handle of its own.
/// `flex_1`/`min_h_0`/`min_w_0`, like the call sites it replaces.
#[derive(IntoElement)]
pub(crate) struct SidebarScrollArea {
    id: ElementId,
    content: AnyElement,
}

impl SidebarScrollArea {
    pub(crate) fn new(id: impl Into<ElementId>, content: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            content: content.into_any_element(),
        }
    }
}

impl RenderOnce for SidebarScrollArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handle = window
            .use_keyed_state((self.id.clone(), "handle"), cx, |_, _| {
                ScrollHandle::default()
            })
            .read(cx)
            .clone();
        sidebar_scroll_pane(self.id, &handle, self.content).min_w_0()
    }
}
