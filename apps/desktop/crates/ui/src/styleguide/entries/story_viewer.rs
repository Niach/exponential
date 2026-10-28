//! S-3: `story-viewer` (`fixtures/stories.json` STORY VIEWER; web entry
//! `story-viewer`): THE story viewer's panel ([`crate::story_viewer`]) as
//! the IDE's centred modal over a scrim, on a draft QUESTION story whose
//! card carries an Exponential UI surface and two choices: the progress
//! (the second of three), the header, the surface through THE card's
//! renderer and the choices + Answer through THE card's controls. No
//! `sync::Store` here: the card is built inline.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::json;

use domain::rows::MessageRow;
use domain::stories::{StoryFooter, StoryKind, STORIES};

use crate::markdown::ImageCache;
use crate::story_ring::{ring_avatar, RING_GLYPH_PX};
use crate::story_viewer::{story_content, story_panel, PanelHandlers};
use crate::thread_screen::ThreadText;

pub(crate) const ID: &str = "story-viewer";
pub(crate) const OWNER: &str = "S-3";

const MESSAGE_ID: &str = "sg-story-question";

fn message() -> MessageRow {
    serde_json::from_value(json!({
        "id": MESSAGE_ID,
        "conversation_id": "c-sg-story",
        "session_id": "sg-story-session",
        "author_session_id": "sg-story-session",
        "created_at": "2026-10-28T10:00:00.000Z",
        "card": {
            "kind": "question",
            "question": "Which accent ships?",
            "options": ["Blue", "Green"],
            "custom": false,
            "surface": {
                "root": {
                    "id": "s",
                    "component": "Stack",
                    "props": { "direction": "vertical", "gap": "sm" },
                    "children": [
                        { "id": "t", "component": "Text", "props": { "text": "Checkout button, both accents" } },
                        { "id": "c", "component": "ChoiceTiles", "props": {
                            "name": "accent",
                            "options": [{ "label": "Blue", "value": "blue" }, { "label": "Green", "value": "green" }],
                            "value": { "path": "/accent" }
                        } }
                    ]
                },
                "data": { "accent": "blue" }
            }
        }
    }))
    .expect("the demo message deserializes")
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    use super::story_ring::{ring, story};
    let images = window.use_keyed_state("sg-story-viewer-images", cx, |_, _| ImageCache::new(None)).clone();
    let mut question = story(MESSAGE_ID, StoryKind::Question, true, false);
    question.title = Some("Which accent ships?".into());
    question.footer = StoryFooter::Choices;
    question.meta = format!("{} · 5m", StoryKind::Question.label());
    let demo = ring(
        "sg-viewer",
        "Fix the checkout",
        false,
        vec![story("v1", StoryKind::Plan, true, false), question.clone(), story("v3", StoryKind::Result, false, false)],
    );
    let message = message();
    let content = story_content(&question, Some(&message), 0, &ThreadText::default(), &images, window, cx);
    let avatar = ring_avatar(None, None, RING_GLYPH_PX * 0.75, cx);
    let spec = &STORIES.viewer;
    let panel = story_panel(&demo, &question, 1, 1., avatar, content, PanelHandlers::default(), cx)
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
