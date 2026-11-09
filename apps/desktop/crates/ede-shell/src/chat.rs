//! The agent chat's adapter lives in `domain::chat_document` (#EDE-65 lane O:
//! the IDE draws the same chat document); this keeps the goldens' demo data.

pub use domain::chat_document::*;

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
