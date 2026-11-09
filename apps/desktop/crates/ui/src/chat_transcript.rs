//! #EXP-1380 — a transcript's FOLLOW rule (`fixtures/chats.json`
//! `transcript`): a list follows the tail (gpui `FollowMode::Tail`); a reader
//! within `transcript.followSlack` px of the bottom is still on it (an
//! arrival re-arms the follow). #EXP-1732: the chat screen's own list (its
//! window, `Load earlier` and jump overlay) is gone: the Chat tab is the
//! root's session view.

use gpui::{px, FollowMode, ListState};

use domain::chats::CHATS;

use crate::transcript_rows::ListOp;

/// How far (px) the list sits above its bottom; `None` before a layout.
pub(crate) fn distance_from_bottom(list: &ListState) -> Option<f32> {
    if list.viewport_bounds().size.height <= px(0.) {
        return None;
    }
    let max = f32::from(list.max_offset_for_scrollbar().y);
    let offset = -f32::from(list.scroll_px_offset_for_scrollbar().y);
    Some((max - offset).max(0.))
}

/// The reader is within `transcript.followSlack` of the bottom (or the list
/// follows already).
pub(crate) fn near_tail(list: &ListState) -> bool {
    list.is_following_tail() || distance_from_bottom(list).is_none_or(|d| d <= CHATS.transcript.follow_slack)
}

/// Before a list sync's `ops` apply: when rows ARRIVE (a splice inserting
/// rows after the first), a reader inside the slack is still on the tail, so
/// the arrival keeps them there. #EXP-1444: only an arrival re-arms; run on
/// every frame (or every unrelated store change) it snapped a reader back to
/// the tail before a trackpad's small steps ever left the slack, and the
/// transcript could not be scrolled.
pub(crate) fn rearm_on_arrival(list: &ListState, ops: &[ListOp]) {
    let arrival = ops
        .iter()
        .any(|op| matches!(op, ListOp::Splice { range, count } if *count > 0 && range.start > 0));
    if arrival && !list.is_following_tail() && near_tail(list) {
        list.set_follow_mode(FollowMode::Tail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, IntoElement as _, Styled as _, Window};

    struct Rows(ListState);

    impl gpui::Render for Rows {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
            gpui::list(self.0.clone(), |_, _, _| div().h(px(50.)).w_full().into_any_element()).w_full().h_full()
        }
    }

    /// #EXP-1444: a reader who wheels up inside the slack STAYS there until
    /// a row arrives; re-arming on every frame (or every store change, or a
    /// row that only re-measured) snapped a trackpad's small steps back to
    /// the tail and the transcript never scrolled.
    #[gpui::test]
    fn only_an_arrival_rearms_the_follow(cx: &mut gpui::TestAppContext) {
        use gpui::{point, size, AppContext as _, ScrollDelta, ScrollEvent};
        let cx = cx.add_empty_window();
        let list = ListState::new(20, gpui::ListAlignment::Top, px(1000.));
        list.set_follow_mode(FollowMode::Tail);
        let draw = |cx: &mut gpui::VisualTestContext| {
            let state = list.clone();
            cx.draw(point(px(0.), px(0.)), size(px(200.), px(300.)), move |_, cx| {
                cx.new(|_| Rows(state)).into_any_element()
            });
        };
        draw(cx);
        assert!(list.is_following_tail());
        cx.simulate_event(ScrollEvent {
            position: point(px(10.), px(10.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(10.))),
            ..Default::default()
        });
        draw(cx);
        assert!(!list.is_following_tail(), "the wheel released the follow");
        assert!(near_tail(&list), "still inside the slack");
        rearm_on_arrival(&list, &[]);
        rearm_on_arrival(&list, &[ListOp::Remeasure(19..20)]);
        rearm_on_arrival(&list, &[ListOp::Splice { range: 0..0, count: 5 }]);
        assert!(!list.is_following_tail(), "no arrival, the reader keeps their place");
        rearm_on_arrival(&list, &[ListOp::Splice { range: 20..20, count: 1 }]);
        assert!(list.is_following_tail(), "a row arrived: back on the tail");
    }
}
