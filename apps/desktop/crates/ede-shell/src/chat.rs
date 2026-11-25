//! The agent chat's adapter lives in `domain::chat_document` (#EDE-65 lane O:
//! the IDE draws the same chat document); this keeps the goldens' demo data.

pub use domain::chat_document::*;

/// Lane Q: the chat view as `/chatting` binds it.
pub fn chatting(view: &uicore_model::chat::ChatView) -> serde_json::Value {
    serde_json::to_value(view).unwrap_or_default()
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
            // #EXP-1876: the group's first bubble wears THE tail.
            tail: lead,
        });
        prev = Some(author.clone());
    }
}

/// The goldens' people.
pub fn demo_members() -> Vec<Member> {
    [("Dana Reyes", "dana@example.com"), ("Alex Kim", "alex@example.com"), ("Danny", "danny@example.com")].into_iter().map(|(name, email)| Member { name: name.into(), email: email.into(), image: None }).collect()
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
