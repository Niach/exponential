//! The agent chat's adapter lives in `domain::chat_document` (#EDE-65 lane O:
//! the IDE draws the same chat document); this keeps the goldens' demo data.

pub use domain::chat_document::*;

/// Lane Q: the chat view as `/chatting` binds it; a card's stored 0.9
/// surface (no `version`) paints through the upgrade, as the IDE's
/// `painted_surface` does (Round 9 §14; the Embed parses 1.0 only).
pub fn chatting(view: &uicore_model::chat::ChatView) -> serde_json::Value {
    let mut value = serde_json::to_value(view).unwrap_or_default();
    for row in value["rows"].as_array_mut().into_iter().flatten() {
        let surface = &mut row["surface"];
        if surface.is_object() && surface.get("version").is_none() {
            let card = eui_core::upgrade09::upgrade_card09(&serde_json::json!({"root": surface["root"]}));
            surface["root"] = card["root"].clone();
            surface["version"] = serde_json::json!(eui_core::catalog::A2UI_VERSION);
        }
    }
    value
}

/// #EDE-65 lane O: the goldens' rows grouped like the IDE's transcript
/// (`domain::chat_document::group` for hand-made rows): the viewer's own
/// out, the agent's in under `avatar` with `name` over a group's first
/// bubble; 3 px inside a group, 12 between, none above the first row.
pub fn demo_group(rows: &mut [uicore_model::chat::ChatRowInput], avatar: &str, agent_name: &str) {
    use uicore_model::chat::{ChatGroup, ChatRowInput};
    let layout = domain::transcript_grouping::layout();
    let mut prev: Option<String> = None;
    for (index, row) in rows.iter_mut().enumerate() {
        let ChatRowInput::Message { author, name, group, .. } = row else {
            prev = None;
            continue;
        };
        let lead = prev.as_deref() != Some(author.as_str());
        let gap = if index == 0 { 0.0 } else if lead { layout.gap_between } else { layout.gap_in_group };
        let inbound = author != "user";
        if inbound {
            *name = lead.then(|| agent_name.to_string());
        }
        *group = Some(ChatGroup {
            side: Some(if inbound { "in" } else { "out" }.into()),
            avatar: (inbound && lead).then(|| format!("{AVATAR_ICON_PREFIX}{avatar}")),
            indent: inbound,
            link: None,
            gap: gap as u32,
        });
        prev = Some(author.clone());
    }
}

/// The goldens' people.
pub fn demo_members() -> Vec<Member> {
    [("Dana Reyes", "dana@example.com"), ("Alex Kim", "alex@example.com"), ("Danny", "danny@example.com")].into_iter().map(|(name, email)| Member { name: name.into(), email: email.into() }).collect()
}

/// The goldens' ranked issues.
pub fn demo_issues() -> Vec<(String, String)> {
    [("FB-8", "Editor takes seconds to open on older laptops"), ("FB-9", "Screenshots attached to feedback are cropped")].into_iter().map(|(i, t)| (i.into(), t.into())).collect()
}

/// The goldens' team prompts.
pub fn demo_prompts() -> Vec<domain::rows::PromptRow> {
    [("p1", "Release notes", "rocket"), ("p2", "Triage the inbox", "")]
        .into_iter()
        .map(|(id, name, icon)| serde_json::from_value(serde_json::json!({"id": id, "name": name, "icon": icon})).expect("a prompt row"))
        .collect()
}

#[cfg(test)]
mod tests {
    use uicore_model::chat::{chat_view, ChatInput, ChatRowInput};

    #[test]
    fn a_stored_0_9_card_paints_as_1_0_and_a_1_0_card_stays() {
        let card = |id: &str, surface: serde_json::Value| ChatRowInput::Card { id: id.into(), icon: "ui-widget".into(), title: "T".into(), text: String::new(), target: None, surface: Some(surface) };
        let input = ChatInput {
            session_id: Some("s".into()),
            rows: vec![
                card("old", serde_json::json!({"root": {"id": "r", "component": "Text", "props": {"text": "hi"}}, "data": {"a": 1}})),
                card("new", serde_json::json!({"version": "1.0", "root": {"id": "r", "component": "Text", "props": {"text": "hi"}}})),
            ],
            ..Default::default()
        };
        let value = super::chatting(&chat_view(&input));
        assert_eq!(value["rows"][0]["surface"]["version"], eui_core::catalog::A2UI_VERSION);
        assert_eq!(value["rows"][0]["surface"]["data"]["a"], 1, "its data stays");
        assert_eq!(value["rows"][1]["surface"], input_surface(&input, 1));
        assert!(value["rows"][0]["surface"]["root"]["component"].is_string());
    }

    fn input_surface(input: &ChatInput, at: usize) -> serde_json::Value {
        match &input.rows[at] {
            ChatRowInput::Card { surface, .. } => surface.clone().unwrap_or_default(),
            _ => serde_json::Value::Null,
        }
    }
}
