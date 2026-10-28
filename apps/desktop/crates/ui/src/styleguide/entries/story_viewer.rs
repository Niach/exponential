//! S-3: `story-viewer` (`fixtures/stories.json` SPECIMEN; web entry
//! `story-viewer`, iOS + Android twins): THE story viewer's panel
//! ([`crate::story_viewer`]) as the IDE's centred modal over a scrim, open
//! on ring `viewer.ring` (0), story `viewer.story` (0) of
//! `stories(specimen.input)`: the board's waiting question over its
//! Exponential UI surface (Text, ChoiceTiles, StatTile) with two described
//! options, the progress frozen at `viewer.fraction` (0). No `sync::Store`
//! here: the card is built inline from the same hardcoded input.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::json;

use domain::rows::MessageRow;
use domain::stories::STORIES;

use super::story_ring::{board_question_card, specimen_avatar, specimen_rings};
use crate::markdown::ImageCache;
use crate::story_ring::RING_GLYPH_PX;
use crate::story_viewer::{story_content, story_panel, PanelHandlers};
use crate::thread_screen::ThreadText;

pub(crate) const ID: &str = "story-viewer";
pub(crate) const OWNER: &str = "S-3";

/// The open story's row (stories.json `specimen.input` board-question).
fn message() -> MessageRow {
    serde_json::from_value(json!({
        "id": "board-question",
        "conversation_id": "c-board",
        "session_id": "board",
        "author_session_id": "board",
        "created_at": "2027-01-15T07:56:00.000Z",
        "card": board_question_card()
    }))
    .expect("the specimen message deserializes")
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-story-viewer-images", cx, |_, _| ImageCache::new(None)).clone();
    let rings = specimen_rings();
    let ring = &rings[0];
    let story = &ring.stories[0];
    let message = message();
    let content = story_content(story, Some(&message), 0, &ThreadText::default(), &images, window, cx);
    let avatar = specimen_avatar(ring, RING_GLYPH_PX * 0.75, cx);
    let spec = &STORIES.viewer;
    let panel = story_panel(ring, story, 0, 0., avatar, content, PanelHandlers::default(), cx)
        .w(px(spec.width))
        .h(px(spec.height))
        .rounded(px(spec.radius))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .shadow_2xl();
    // The scrim behind the modal (the list under it is the page's).
    v_flex()
        .p_8()
        .items_center()
        .bg(gpui::black().opacity(0.6))
        .child(div().child(panel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_viewer_opens_on_the_board_question() {
        let rings = specimen_rings();
        let story = &rings[0].stories[0];
        assert_eq!(story.id, "board-question");
        assert_eq!(story.title.as_deref(), Some("Which accent for the board header?"));
        assert!(story.surface);
        assert_eq!(message().id, story.id);
    }
}
