//! S-3: `story-viewer` (`fixtures/stories.json` SPECIMEN; web entry
//! `story-viewer`, iOS + Android twins): THE story viewer's panel
//! ([`crate::story_viewer`]) as the IDE's centred modal over a scrim, open
//! on ring 0's FIRST FRAME (#EXP-1493 one frame per story, `story_frames`)
//! of `stories(specimen.input)`: THE kind badge + the board's waiting
//! question's bare Exponential UI surface (Text, ChoiceTiles, StatTile) with
//! two described options, the progress (one segment per story) frozen at 0. No `sync::Store` here: the card is
//! built inline from the same hardcoded input.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::json;

use domain::rows::MessageRow;
use domain::stories::{story_frames, STORIES};

use super::story_ring::{board_question_card, specimen_avatar, specimen_rings};
use crate::markdown::ImageCache;
use crate::story_viewer::{story_content, story_panel, PanelHandlers};

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
    let frames = story_frames(&ring.stories);
    let frame = &frames[0];
    let story = &ring.stories[frame.story];
    let message = message();
    let content = story_content(story, frame, 0, Some(&message), &PanelHandlers::default(), &images, window, cx);
    let avatar = specimen_avatar(ring, crate::story_viewer::HEADER_GLYPH, cx);
    let spec = &STORIES.viewer;
    // #EXP-1463: a surface frame = the fixed frame (`story_viewer_size`
    // without a picture) in the 1440 × 900 capture window.
    let size = domain::stories::story_viewer_size(None, (1440., 900.));
    let panel = story_panel(ring, &frames, 0, 0, 0., avatar, content, PanelHandlers::default(), cx)
        .w(px(size.width as f32))
        .h(px(size.height as f32))
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
        // #EXP-1493: one frame per story; the first is the question's bare
        // surface with its choices footer.
        let frames = story_frames(&rings[0].stories);
        assert_eq!(frames.len(), rings[0].stories.len());
        assert_eq!(frames[0].kind, domain::stories::StoryFrameKind::Surface);
        assert_eq!(frames[0].footer, domain::stories::StoryFooter::Choices);
    }
}
