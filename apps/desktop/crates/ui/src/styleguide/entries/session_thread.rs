//! EXP-1293/EXP-1294: `session-thread` (Special components): a thread's
//! Conversation, the IDE half of the styleguide entry the web page draws —
//! the thread screen's REAL row painters (`crate::thread_screen`, through
//! [`super::thread_parts::row`]) over the contract model
//! (`domain::session_tree::build_session_thread`) on `session-thread.json`'s
//! first two cases, like the web entry: the root's own session card first,
//! then the root's messages oldest first (a person's bubble, `queued` until
//! consumed; the agent's prose; event lines; child session cards; questions
//! and plans answered in the card; a subagent; a picture) and EXP-1323's
//! dispatcher (the suggestions card, a report line, an aside's card).

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use crate::markdown::ImageCache;

use super::thread_parts::{case, case_named, row, DISPATCHER_CASE};

pub(crate) const ID: &str = "session-thread";
pub(crate) const OWNER: &str = "EXP-1293";

/// The fixture cases the entry draws (the web entry's): the first two,
/// then EXP-1323's dispatcher (suggestions in every state, a child's
/// question, a report, a `/btw` aside card with its caption).
fn cases() -> [usize; 3] {
    [0, 1, case_named(DISPATCHER_CASE)]
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-session-thread-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let mut column = v_flex().w(px(720.)).gap_10();
    for index in cases() {
        let case = case(index);
        let mut conversation = v_flex().w_full().gap_5();
        for item in &case.view.rows {
            conversation = conversation.child(row(&case, item, &images, window, cx));
        }
        column = column.child(conversation);
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every drawn case opens on the root's own session card.
    #[test]
    fn each_case_opens_on_the_roots_session_card() {
        for index in cases() {
            let case = case(index);
            assert!(
                matches!(
                    case.view.rows.first(),
                    Some(domain::session_tree::SessionThreadRow::Session { .. })
                ),
                "case {index}"
            );
        }
    }
}
