//! #EXP-1674: `entity-chip` (2 General components): THE entity chip (EXP-920),
//! case for case with the web entry: the REAL [`crate::entity_chip::entity_chip`]
//! per kind. No store here, so every chip is its unsynced look; agent prose
//! paints a bare in-app URL as this chip (`prose-links.json`).

use gpui::{div, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};
use steer::EntityRef;

use crate::entity_chip::entity_chip;

pub(crate) const ID: &str = "entity-chip";
pub(crate) const OWNER: &str = "EXP-1674";

fn demo(kind: &str, id: &str, identifier: Option<&str>, title: &str) -> EntityRef {
    EntityRef {
        kind: kind.to_string(),
        id: id.to_string(),
        identifier: identifier.map(str::to_string),
        title: Some(title.to_string()),
        count: None,
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let captioned = |id: &'static str, caption: &'static str, r#ref: EntityRef| {
        v_flex()
            .gap_2()
            .child(entity_chip(SharedString::from(format!("sg-entity-chip-{id}")), &r#ref, &[]).into_any_element())
            .child(div().text_xs().text_color(muted).child(caption))
    };
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_6()
        .child(captioned(
            "issue",
            "an issue: its status glyph, identifier and title",
            demo("issue", "i-1", Some("EXP-42"), "Fix the flicker"),
        ))
        .child(captioned("board", "a board", demo("board", "b-1", None, "Web")))
        .child(captioned("session", "a session", demo("session", "s-1", None, "Fix the flicker")))
        .child(captioned("agent", "an agent", demo("agent", "a-1", None, "Triage")))
}
