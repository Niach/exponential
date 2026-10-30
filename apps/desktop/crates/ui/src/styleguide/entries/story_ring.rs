//! S-3: `story-ring` (`fixtures/stories.json` SPECIMEN; web entry
//! `story-ring`, iOS + Android twins): the rings of
//! `stories(specimen.input)` in fold order at the bubble size with their
//! titles under them, then the same rings at `smallRing` px without titles,
//! then a StoryProgress (4 segments, the second at 0.4). No state words.
//! The input is hardcoded identically ×4 (the fixture's last case replays
//! it).

use gpui::{App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex};
use serde_json::{json, Value};

use domain::chats::CHATS;
use domain::stories::{stories, StoriesInput, StoryRing};

use crate::story_ring::{ring_avatar, story_ring, story_ring_button, RingArc, RING_GLYPH_PX};
use crate::story_viewer::story_progress;

pub(crate) const ID: &str = "story-ring";
pub(crate) const OWNER: &str = "S-3";

/// stories.json `specimen.smallRing`.
const SMALL_RING: f32 = 36.;

/// stories.json `specimen.input`, verbatim.
pub(crate) fn specimen_input() -> Value {
    let text = |text: &str| json!({ "root": { "id": "r", "component": "Text", "props": { "text": text } } });
    json!({
        "now": 1_800_000_000_000_i64,
        "sessions": [
            { "id": "triage", "rootId": "triage", "name": "Triage", "agentId": "ag1", "startedByAgentId": null },
            { "id": "board", "rootId": "board", "name": "Board header polish", "agentId": null, "startedByAgentId": null, "cli": "claude" },
            { "id": "designer", "rootId": "designer", "name": "Designer", "agentId": "ag2", "startedByAgentId": null },
            { "id": "ship", "rootId": "ship", "name": "Ship the launch checklist", "agentId": null, "startedByAgentId": null, "cli": "claude" }
        ],
        "agents": [
            { "id": "ag1", "name": "Triage", "assistant": false, "icon": "bug", "color": "#a855f7" },
            { "id": "ag2", "name": "Designer", "assistant": false, "icon": "bot", "color": "#6366f1" }
        ],
        "messages": [
            {
                "id": "triage-plan", "sessionId": "triage", "authorSessionId": "triage",
                "createdAt": 1_799_999_460_000_i64, "answeredAt": null,
                "card": {
                    "kind": "plan",
                    "plan": "## Close the duplicates\n\nMerge FB-8 into FB-3 and label the rest.",
                    "decision": null
                }
            },
            {
                "id": "triage-result", "sessionId": "triage", "authorSessionId": "triage",
                "createdAt": 1_799_998_200_000_i64, "answeredAt": null,
                "card": { "kind": "surface", "story": true, "title": "Today's triage", "surface": text("3 labelled") }
            },
            {
                "id": "board-question", "sessionId": "board", "authorSessionId": "board",
                "createdAt": 1_799_999_760_000_i64, "answeredAt": null,
                "card": board_question_card()
            },
            {
                "id": "designer-result", "sessionId": "designer", "authorSessionId": "designer",
                "createdAt": 1_799_996_400_000_i64, "answeredAt": null,
                "card": { "kind": "surface", "story": true, "title": "Spacing review", "description": "Paddings match on all four platforms.", "surface": text("No drift") }
            },
            {
                "id": "ship-result", "sessionId": "ship", "authorSessionId": "ship",
                "createdAt": 1_799_992_800_000_i64, "answeredAt": null,
                "card": { "kind": "surface", "story": true, "title": "Checklist done", "surface": text("12 of 12") }
            }
        ],
        "reads": [
            { "sessionId": "ship", "readAt": 1_799_996_400_000_i64, "storySeenAt": null }
        ]
    })
}

/// The board's waiting question (the viewer specimen's story).
pub(crate) fn board_question_card() -> Value {
    json!({
        "kind": "question",
        "question": "Which accent for the board header?",
        "options": ["Indigo", "Emerald"],
        "optionDescriptions": ["The brand accent", "Matches the merged state"],
        "custom": false,
        "surface": {
            "root": {
                "id": "root",
                "component": "Stack",
                "props": { "gap": "md" },
                "children": [
                    { "id": "title", "component": "Text", "props": { "text": "Board header" }, "children": [] },
                    { "id": "accent", "component": "ChoiceTiles", "props": {
                        "name": "accent",
                        "options": [{ "label": "Indigo", "value": "indigo" }, { "label": "Emerald", "value": "emerald" }],
                        "value": { "path": "/accent" }
                    }, "children": [] },
                    { "id": "stat", "component": "StatTile", "props": {
                        "label": "Header height", "value": "48 px", "delta": "\u{2212}8 px", "tone": "success"
                    }, "children": [] }
                ]
            },
            "data": { "accent": "indigo" }
        }
    })
}

/// The specimen's rings: `stories(specimen.input)`.
pub(crate) fn specimen_rings() -> Vec<StoryRing> {
    let input: StoriesInput = serde_json::from_value(specimen_input()).expect("the specimen input deserializes");
    stories(&input)
}

/// A specimen ring's avatar: an agent ring = its agent's icon + colour, a
/// chat ring = the chat row's bare brand mark (the specimen roots run `cli`
/// claude).
pub(crate) fn specimen_avatar(ring: &StoryRing, glyph: f32, cx: &App) -> gpui::AnyElement {
    let input = specimen_input();
    let agent = ring.agent_id.as_deref().and_then(|id| {
        input["agents"].as_array()?.iter().find(|agent| agent["id"] == id).cloned()
    });
    match agent {
        Some(agent) => ring_avatar(Some((agent["icon"].as_str(), agent["color"].as_str())), None, glyph, cx),
        None => ring_avatar(None, Some(coding::CodingAgent::Claude), glyph, cx),
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let rings = specimen_rings();
    let small_glyph = RING_GLYPH_PX * SMALL_RING / CHATS.shell.bubble;
    v_flex()
        .gap_6()
        // THE STORY ROW geometry (#EXP-1461, stories.json `row`).
        .child(h_flex().gap(gpui::px(domain::stories::STORIES.row.gap)).items_start().children(
            rings.iter().map(|ring| story_ring_button(ring, specimen_avatar(ring, RING_GLYPH_PX, cx), cx)),
        ))
        .child(h_flex().gap_3().items_center().children(rings.iter().map(|ring| {
            let arcs: Vec<RingArc> = ring.stories.iter().map(RingArc::from).collect();
            story_ring(&arcs, SMALL_RING, specimen_avatar(ring, small_glyph, cx), cx).into_any_element()
        })))
        .child(v_flex().w(gpui::px(280.)).child(story_progress(4, 1, 0.4, Some(&[0, 0, 1, 2]), cx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hardcoded input IS stories.json's specimen and folds to its
    /// replay case, in fold order.
    #[test]
    fn the_specimen_is_the_fixture() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../../../../../packages/domain-contract/fixtures/stories.json"))
                .unwrap();
        assert_eq!(specimen_input(), fixture["specimen"]["input"]);
        let titles: Vec<String> = specimen_rings().into_iter().map(|ring| ring.title).collect();
        assert_eq!(titles, ["Board header polish", "Triage", "Designer", "Ship the launch checklist"]);
        assert_eq!(fixture["specimen"]["smallRing"].as_f64(), Some(SMALL_RING as f64));
    }
}
