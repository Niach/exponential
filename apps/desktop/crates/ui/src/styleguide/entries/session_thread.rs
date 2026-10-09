//! EXP-1175: `session-thread` (Special components): the Run face's THREAD,
//! the IDE half of the styleguide entry the web page draws.
//!
//! The shared reader's own output (`domain::session_results::session_thread`)
//! through the run view's row renderers: a topic's text (the muted caption
//! over its markdown), a picture tile, a second text, and the Summary drawn
//! LAST as the agent's reply. EXP-1245: below it the OWNER's thread as turns
//! (`session_turns` over a feed): the first turn's settled status row and
//! reply, the person's bubble, the second turn's live row and its text. The
//! image cache has no transport, so the tile paints its placeholder box.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use domain::session_results::{
    session_result_tile_caption, session_thread, session_turns, SessionThread, SessionTurnEvent,
    SessionTurns, ThreadItem, SESSION_INLINE_TILE_HEIGHT,
};

use crate::markdown::{ImageCache, MarkdownView};
use crate::steer_viewer::{narration_row, thread_text_row};

pub(crate) const ID: &str = "session-thread";
pub(crate) const OWNER: &str = "EXP-1175";

/// A run's published results as an agent files them: the Summary first in
/// the blob, drawn last.
pub(crate) fn thread() -> SessionThread {
    session_thread(Some(&serde_json::json!([
        {"topic": "Summary", "label": null, "attachmentId": null,
         "text": "The Run face opens on what the run published; Show work brings the transcript back."},
        {"topic": "Status row", "label": null, "attachmentId": null,
         "text": "One row over the thread: the run mark, where it builds and for how long, the newest tool call."},
        {"topic": "Status row", "label": "desktop", "attachmentId": "sg-thread-shot", "width": 1440, "height": 900},
        {"topic": "Thread", "label": null, "attachmentId": null,
         "text": "Pictures and report texts keep the order the agent filed them in."},
    ])))
}

/// EXP-1245: the owner's two turns (the fixture's "two turns and the
/// person's bubble" case).
pub(crate) fn turns() -> SessionTurns {
    session_turns(
        Some(&serde_json::json!([
            {"topic": "Summary", "text": "The × is gone, so the trailing cluster is just Create.", "at": 1_760_000_300_000_i64},
            {"topic": "Reviews", "text": "Reviews resolves PRs by the exact pr_url now.", "at": 1_760_000_900_000_i64},
        ])),
        &[
            SessionTurnEvent::TurnStarted { at: 1_760_000_000_000 },
            SessionTurnEvent::TurnEnded { at: 1_760_000_437_000 },
            SessionTurnEvent::UserMessage {
                at: 1_760_000_600_000,
                text: "why is this PR not connected to the issue? please fix this as well.".into(),
                images: Vec::new(),
            },
            SessionTurnEvent::TurnStarted { at: 1_760_000_601_000 },
        ],
    )
}

fn prose(key: String, text: String, images: &gpui::Entity<ImageCache>) -> MarkdownView {
    MarkdownView::new(SharedString::from(key), text)
        .chat(true)
        .selectable(true)
        .images(images.clone())
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("styleguide-session-thread-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let thread = thread();
    let mut rows = Vec::new();
    for (index, item) in thread.items.into_iter().enumerate() {
        rows.push(match item {
            ThreadItem::Text { topic, text } => {
                thread_text_row(topic, prose(format!("sg-thread-text-{index}"), text, &images), cx)
            }
            ThreadItem::Picture(entry) => crate::session_results::tile(
                &entry,
                SESSION_INLINE_TILE_HEIGHT,
                SharedString::from(format!("sg-thread-picture-{index}")),
                session_result_tile_caption(&entry).to_string(),
                &images,
                cx,
            ),
        });
    }
    if let Some(reply) = thread.reply {
        rows.push(narration_row(prose("sg-thread-reply".to_string(), reply, &images), cx));
    }
    // EXP-1245: the owner's turns, through the same rows the run view draws.
    let now = 1_760_000_900_000 + 422_000;
    for (index, turn) in turns().turns.into_iter().enumerate() {
        if let Some(message) = turn.message.as_ref() {
            let body = prose(format!("sg-turn-bubble-{index}"), message.text.clone(), &images);
            rows.push(crate::steer_viewer::user_bubble(
                gpui::IntoElement::into_any_element(body),
                px(380.),
                "Danny · 21:40 · from mint".to_string(),
                cx,
            ));
        }
        if let Some((caption, tone)) = crate::session_results::turn_row_caption(
            turn.started_at,
            turn.ended_at,
            crate::run_rows::RunRowState::Working,
            "macbook",
            None,
            now,
            true,
        ) {
            let settled = turn.ended_at.is_some();
            use crate::queries::CodingSessionDisplay as D;
            rows.push(crate::run_rows::run_status_row(
                crate::run_rows::RunStatusRowSpec {
                    id: SharedString::from(format!("sg-turn-status-{index}")),
                    agent: Some(coding::CodingAgent::default()),
                    mark: if settled {
                        crate::run_rows::RunStatusMark::Ended
                    } else {
                        crate::run_rows::RunStatusMark::Live(Some(D::Working))
                    },
                    caption: SharedString::from(caption),
                    tone,
                    tool_line: (!settled).then(|| SharedString::from("Bash grep")),
                    show_work: false,
                    on_toggle: None,
                },
                cx,
            ));
        }
        for (item_ix, item) in turn.items.into_iter().enumerate() {
            if let ThreadItem::Text { topic, text } = item {
                rows.push(thread_text_row(
                    topic,
                    prose(format!("sg-turn-text-{index}-{item_ix}"), text, &images),
                    cx,
                ));
            }
        }
        if let Some(reply) = turn.reply {
            rows.push(narration_row(prose(format!("sg-turn-reply-{index}"), reply, &images), cx));
        }
    }
    div().flex().flex_col().gap_3().pt_2().w(px(560.)).children(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo is the shared reader's output: three items in publish
    /// order, the Summary as the reply.
    #[test]
    fn the_demo_reads_as_a_thread() {
        let thread = thread();
        let kinds: Vec<&str> = thread
            .items
            .iter()
            .map(|item| match item {
                ThreadItem::Text { .. } => "text",
                ThreadItem::Picture(_) => "picture",
            })
            .collect();
        assert_eq!(kinds, vec!["text", "picture", "text"]);
        assert!(thread.reply.is_some_and(|reply| reply.starts_with("The Run face")));
        // EXP-1245: the owner's island is two turns, the second opened by
        // the person's message and still running.
        let turns = turns();
        assert!(turns.per_turn);
        assert_eq!(turns.turns.len(), 2);
        assert!(turns.turns[0].message.is_none() && turns.turns[0].reply.is_some());
        assert!(turns.turns[1].message.is_some() && turns.turns[1].ended_at.is_none());
    }
}
