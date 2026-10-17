//! EXP-1293/EXP-1294: `session-thread` (Special components): a thread's
//! Conversation, the IDE half of the styleguide entry the web page draws —
//! the thread screen's REAL row painters (`crate::thread_screen`, through
//! [`super::thread_parts::row`]) over the contract model
//! (`domain::session_tree::build_session_thread`) on `session-thread.json`'s
//! cases in the web entry's order: EXP-1334's dispatcher (a child's question
//! and plan, answered in place; no root card — the conversation IS the
//! dispatcher), the first two (a person's bubble, `queued` until consumed;
//! the agent's prose; event lines; child session cards; questions and plans
//! answered in the card; a subagent; a picture), then THE composer the
//! thread hosts, idle and while it works
//! ([`super::composer_dialog::ComposerSpecimen`]), EXP-1211's quick replies
//! above it (`repliesCases[1]`), then EXP-1323's ended
//! root and its live dispatcher (the suggestions card in every state, a
//! report line, an aside's card).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::markdown::ImageCache;

use super::composer_dialog::{composer_specimen, composer_specimen_with_replies, thread_placeholder, Phase};
use super::fixture_rows;
use super::thread_parts::{case, case_named, row, DISPATCHER_CASE, FOLD_CASE};

pub(crate) const ID: &str = "session-thread";
pub(crate) const OWNER: &str = "EXP-1293";

/// The EXP-1323 ended-root case's name prefix.
pub(super) const ENDED_CASE: &str = "EXP-1323: an ended";

/// The fixture cases the entry draws, in the web entry's order: EXP-1334's
/// dispatcher, the first two, then (after the two composers) EXP-1323's
/// ended root and its live dispatcher.
fn cases() -> [usize; 5] {
    [case_named(FOLD_CASE), 0, 1, case_named(ENDED_CASE), case_named(DISPATCHER_CASE)]
}

/// The two composers sit after this many cases (the web's: after the first
/// three).
const COMPOSERS_AFTER: usize = 3;

/// EXP-1211: the quick replies specimen's model, `repliesCases[1].expected`
/// (the web entry's).
fn sample_replies() -> domain::session_tree::ComposerReplies {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    serde_json::from_value(fixture["repliesCases"][1]["expected"].clone()).expect("repliesCases[1] hydrates")
}

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-session-thread-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let mut column = v_flex().w(px(720.)).gap_10();
    for (position, index) in cases().into_iter().enumerate() {
        if position == COMPOSERS_AFTER {
            let placeholder = thread_placeholder();
            column = column
                .child(
                    v_flex()
                        .gap_2()
                        .child(caption(
                            "The thread's composer = THE session composer (EXP-1334): the card with \"+\" (the options) and the inline tools, model · Plan · the context ring under it.",
                            cx,
                        ))
                        .child(composer_specimen("sg-session-thread-idle", Phase::Idle, &placeholder, window, cx)),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(caption("The same composer in a session while it works.", cx))
                        .child(composer_specimen("sg-session-thread-running", Phase::Running, &placeholder, window, cx)),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(caption(
                            "EXP-1211: the agent's quick replies (`sessions_suggest` replies) above the composer; a tap sends one as the next message, the others stay.",
                            cx,
                        ))
                        .child(composer_specimen_with_replies(
                            "sg-session-thread-replies",
                            Phase::Idle,
                            &placeholder,
                            Some(sample_replies()),
                            window,
                            cx,
                        )),
                );
        }
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

    /// The web entry's order: the fold case first, then 0 and 1, the ended
    /// root, the live dispatcher; five distinct cases.
    #[test]
    fn the_cases_are_the_webs_in_order() {
        let cases = cases();
        assert_eq!(cases[0], case_named(FOLD_CASE));
        assert_eq!(&cases[1..3], &[0, 1]);
        assert_eq!(cases[3], case_named(ENDED_CASE));
        assert_eq!(cases[4], case_named(DISPATCHER_CASE));
        let distinct: std::collections::BTreeSet<usize> = cases.into_iter().collect();
        assert_eq!(distinct.len(), 5);
        assert_eq!(COMPOSERS_AFTER, 3);
    }

    /// EXP-1211: the specimen is the fixture's two open replies.
    #[test]
    fn the_quick_replies_specimen_is_the_fixtures() {
        let replies = sample_replies();
        assert_eq!(replies.message_id, "m2");
        assert_eq!(replies.items.len(), 2);
    }
}
