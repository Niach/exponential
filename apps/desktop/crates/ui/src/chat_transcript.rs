//! #EXP-1380 — a chat's transcript rules ×4 (`fixtures/chats.json` WINDOW,
//! FOLLOW + JUMP, `transcript`), shared by the thread screen and the agent
//! chat: both paint their rows through a virtualised `gpui::list` whose
//! [`ListState`] follows the tail.
//!
//! WINDOW: the newest `transcript.window` rows; a `Load earlier` ghost button
//! above them widens it by `transcript.windowStep` (the run view's look; the
//! front splice keeps the reader's scroll anchor). While the list follows the
//! tail the window slides with the newest rows (what drops off the top is
//! out of view); once the reader leaves the tail the start is PINNED, so a
//! new arrival never shrinks it.
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

/// The window's start over a projection of rows (indices into it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptWindow {
    /// `None` = the newest `transcript.window` rows (sliding); `Some` = the
    /// pinned first row.
    pinned: Option<usize>,
}

impl TranscriptWindow {
    /// The first row painted of `total`.
    pub(crate) fn start(&self, total: usize) -> usize {
        let tail = domain::chats::transcript_window_start(total, CHATS.transcript.window);
        match self.pinned {
            Some(start) => start.min(tail),
            None => tail,
        }
    }

    /// Slide with the tail while `following`, else pin where it stands.
    pub(crate) fn reanchor(&mut self, following: bool, total: usize) {
        if following {
            self.pinned = None;
        } else if self.pinned.is_none() {
            self.pinned = Some(self.start(total));
        }
    }

    /// `Load earlier`: widen by `transcript.windowStep`.
    pub(crate) fn load_earlier(&mut self, total: usize) {
        self.pinned = Some(self.start(total).saturating_sub(CHATS.transcript.window_step));
    }
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

/// Before a list sync: a reader inside the slack is still on the tail, so
/// the arrival keeps them there.
pub(crate) fn rearm_near_tail(list: &ListState) {
    if !list.is_following_tail() && near_tail(list) {
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
    fn the_window_slides_then_pins_then_widens() {
        let mut window = TranscriptWindow::default();
        assert_eq!(window.start(1500), 1400);
        assert_eq!(window.start(40), 0);
        // Following: it slides with every arrival.
        window.reanchor(true, 1501);
        assert_eq!(window.start(1501), 1401);
        // Scrolled up: pinned, an arrival never shrinks it.
        window.reanchor(false, 1501);
        assert_eq!(window.start(1510), 1401);
        // Load earlier widens by the step.
        window.load_earlier(1510);
        assert_eq!(window.start(1510), 1301);
        window.load_earlier(1510);
        window.load_earlier(1510);
        assert_eq!(window.start(1510), 1101);
        // Back on the tail: the newest window again.
        window.reanchor(true, 1510);
        assert_eq!(window.start(1510), 1410);
    }
}
