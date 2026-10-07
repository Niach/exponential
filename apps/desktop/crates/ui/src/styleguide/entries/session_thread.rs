//! EXP-1175: `session-thread` (Special components): the Run face's THREAD,
//! the IDE half of the styleguide entry the web page draws.
//!
//! The shared reader's own output (`domain::session_results::session_thread`)
//! through the run view's row renderers: a topic's text (the muted caption
//! over its markdown), a picture tile, a second text, and the Summary drawn
//! LAST as the agent's reply. The image cache has no transport, so the tile
//! paints its placeholder box.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use domain::session_results::{
    session_result_tile_caption, session_thread, SessionThread, ThreadItem,
    SESSION_INLINE_TILE_HEIGHT,
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
    }
}
