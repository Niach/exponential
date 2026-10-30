//! #EXP-1380 — a chat's transcript rules ×4 (`fixtures/chats.json` WINDOW,
//! FOLLOW + JUMP, `transcript`), shared by the thread screen and the agent
//! chat: both paint their rows through a virtualised `gpui::list` whose
//! [`ListState`] follows the tail.
//!
//! WINDOW (review of #1218, [`domain::chats::transcript_window_start`]): the
//! newest `transcript.window` rows, the start reaching up to the oldest
//! WAITING plan / question card (never cut off); a `Load earlier` ghost
//! button above them moves the start up by `transcript.windowStep` in ONE
//! press (the run view's look; the front splice keeps the reader's scroll
//! anchor). Once painted the start is PINNED: arrivals append below it
//! whether the reader follows the tail or not; a reopened chat (a new view)
//! starts again at the newest window.
//!
//! FOLLOW + JUMP: the list follows the tail (gpui `FollowMode::Tail`); a
//! reader within `transcript.followSlack` px of the bottom is still on it (an
//! arrival re-arms the follow); further up, THE jump-to-bottom button shows,
//! and a click scrolls to the newest row and resumes following.

use gpui::{
    div, px, AnyElement, App, ClickEvent, FollowMode, IntoElement as _, ListState,
    ParentElement as _, Styled as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, ActiveTheme as _, Sizable as _};

use domain::chats::CHATS;
use domain::session_tree::SessionThreadRow;

use crate::transcript_rows::ListOp;

/// The window's start over a projection of rows (indices into it): the
/// fixture's `pinnedStart` + `loadedSteps`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptWindow {
    /// The start the first painted (non-empty) projection fixed; `None`
    /// before it.
    pinned: Option<usize>,
    /// `Load earlier` presses since the open.
    loaded_steps: usize,
}

impl TranscriptWindow {
    /// The first row painted of `total`, `waiting` = the projection's
    /// indexes of WAITING cards ([`waiting_indexes`]). The first call over
    /// a non-empty projection pins the start.
    pub(crate) fn start(&mut self, total: usize, waiting: &[usize]) -> usize {
        let start = domain::chats::transcript_window_start(total, waiting, self.pinned, self.loaded_steps);
        if self.pinned.is_none() && total > 0 {
            self.pinned = Some(start);
        }
        start
    }

    /// `Load earlier`: one press = one `transcript.windowStep`.
    pub(crate) fn load_earlier(&mut self) {
        self.loaded_steps += 1;
    }
}

/// The indexes (into `rows`) of WAITING plan / question cards: a plan with
/// no decision, an unanswered question (`ask_dock::thread_pending_asks`).
pub(crate) fn waiting_indexes<'a>(rows: impl IntoIterator<Item = &'a SessionThreadRow>) -> Vec<usize> {
    rows.into_iter()
        .enumerate()
        .filter(|(_, row)| {
            matches!(
                row,
                SessionThreadRow::Plan { decision: None, .. } | SessionThreadRow::Question { answered: false, .. }
            )
        })
        .map(|(index, _)| index)
        .collect()
}

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

/// Whether the jump button shows: off the tail by more than the slack.
pub(crate) fn jump_shown(list: &ListState, rows: usize) -> bool {
    rows > 0 && !near_tail(list)
}

/// THE jump-to-bottom overlay over a chat's list ([`crate::controls::jump_to_bottom_button`],
/// [`crate::controls::JUMP_TO_BOTTOM_GAP`] above the bottom chrome `band`):
/// shown while [`jump_shown`]; a click scrolls to the newest row and resumes
/// following. A paint-time probe repaints `entity_id` once when the answer
/// flipped (gpui settles the scroll in layout).
pub(crate) fn jump_overlay(
    id: &'static str,
    list: &ListState,
    rows: usize,
    band: f32,
    entity_id: gpui::EntityId,
    cx: &App,
) -> AnyElement {
    let shown = jump_shown(list, rows);
    let probe_list = list.clone();
    let probe = gpui::canvas(
        move |_, _, cx| {
            if jump_shown(&probe_list, rows) != shown {
                cx.defer(move |cx| cx.notify(entity_id));
            }
        },
        |_, _: (), _, _| {},
    )
    .absolute()
    .size_0();
    let target = list.clone();
    let button = shown.then(|| {
        h_flex()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(band + crate::controls::JUMP_TO_BOTTOM_GAP))
            .justify_center()
            .child(crate::controls::jump_to_bottom_button(
                id,
                move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    target.set_follow_mode(FollowMode::Tail);
                    target.scroll_to_end();
                    cx.notify(entity_id);
                },
                cx,
            ))
    });
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .child(probe)
        .children(button)
        .into_any_element()
}

/// The `Load earlier` ghost button over the window's first row (web
/// `agent-session.tsx`: ghost, sm, muted, centred).
pub(crate) fn load_earlier_button(
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    h_flex()
        .w_full()
        .justify_center()
        .pb_1()
        .child(
            Button::new(gpui::SharedString::from(CHATS.test_ids.load_earlier.clone()))
                .ghost()
                .small()
                .cursor_pointer()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .label(CHATS.copy.load_earlier.clone())
                .on_click(on_click),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_pins_on_first_paint_then_grows_by_one_step() {
        let mut window = TranscriptWindow::default();
        // Nothing synced yet: no pin.
        assert_eq!(window.start(0, &[]), 0);
        assert_eq!(window.start(250, &[]), 150);
        // Arrivals append, the start stays (following or not).
        assert_eq!(window.start(260, &[]), 150);
        // One press = one step.
        window.load_earlier();
        assert_eq!(window.start(270, &[]), 50);
        window.load_earlier();
        assert_eq!(window.start(270, &[]), 0);
        // A reopened chat (a new view) starts at the newest window again.
        assert_eq!(TranscriptWindow::default().start(260, &[]), 160);
    }

    #[test]
    fn a_waiting_card_is_never_cut_off() {
        let mut window = TranscriptWindow::default();
        assert_eq!(window.start(250, &[30]), 30);
        window.load_earlier();
        assert_eq!(window.start(250, &[30]), 0);
    }

    #[test]
    fn waiting_rows_are_open_plans_and_unanswered_questions() {
        let rows: Vec<SessionThreadRow> = vec![
            SessionThreadRow::Event { id: "e".into(), event: "linked".into(), text: "Linked".into(), session_id: None },
            plan("p1", None),
            plan("p2", Some("approved")),
            question("q1", false),
            question("q2", true),
        ];
        assert_eq!(waiting_indexes(&rows), vec![1, 3]);
    }

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
        use gpui::{point, size, AppContext as _, ScrollDelta, ScrollWheelEvent};
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
        cx.simulate_event(ScrollWheelEvent {
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

    fn plan(id: &str, decision: Option<&str>) -> SessionThreadRow {
        SessionThreadRow::Plan {
            id: id.into(),
            session_id: "s".into(),
            plan: "Plan".into(),
            questions: Vec::new(),
            surface: None,
            decision: decision.map(Into::into),
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        }
    }

    fn question(id: &str, answered: bool) -> SessionThreadRow {
        SessionThreadRow::Question {
            id: id.into(),
            session_id: "s".into(),
            question: "Which?".into(),
            options: Vec::new(),
            custom: true,
            multi: false,
            surface: None,
            answer: None,
            answers: None,
            surface_data: None,
            answered,
        }
    }
}
