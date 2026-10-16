//! EXP-1293/EXP-1294: `session-thread` (Special components): a thread's
//! Conversation, the IDE half of the styleguide entry the web page draws —
//! the thread screen's REAL row painters (`crate::thread_screen`, through
//! [`super::thread_parts::row`]) over the contract model
//! (`domain::session_tree::build_session_thread`) on `session-thread.json`'s
//! first two cases, like the web entry: the root's messages oldest first
//! (EXP-1334: no card of its own — the conversation IS the dispatcher) (a person's bubble, `queued` until
//! consumed; the agent's prose; event lines; child session cards; questions
//! and plans answered in the card; a subagent; a picture) and EXP-1323's
//! dispatcher (the suggestions card, a report line, an aside's card) and
//! EXP-1334's (a child's question and plan, answered in place).

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use crate::markdown::ImageCache;

use super::thread_parts::{case, case_named, row, DISPATCHER_CASE, FOLD_CASE};

pub(crate) const ID: &str = "session-thread";
pub(crate) const OWNER: &str = "EXP-1293";

/// The fixture cases the entry draws (the web entry's): the first two,
/// then EXP-1323's dispatcher (suggestions in every state, a child's
/// question, a report, a `/btw` aside card with its caption).
fn cases() -> [usize; 4] {
    [0, 1, case_named(DISPATCHER_CASE), case_named(FOLD_CASE)]
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

    /// EXP-1334: no case opens on a root card; the fold case surfaces a
    /// child's question and plan with that child's id.
    #[test]
    fn the_dispatcher_has_no_card_and_children_ask_here() {
        use domain::session_tree::SessionThreadRow;
        for index in cases() {
            let case = case(index);
            let root = case.view.root_id.clone();
            assert!(
                !case.view.rows.iter().any(|row| matches!(row, SessionThreadRow::Session { id, .. } if *id == root)),
                "case {index}"
            );
        }
        let fold = case(case_named(FOLD_CASE));
        let root = fold.view.root_id.clone();
        assert!(fold.view.rows.iter().any(|row| matches!(row, SessionThreadRow::Question { session_id, .. } if *session_id != root)));
        assert!(fold.view.rows.iter().any(|row| matches!(row, SessionThreadRow::Plan { session_id, .. } if *session_id != root)));
    }
}
