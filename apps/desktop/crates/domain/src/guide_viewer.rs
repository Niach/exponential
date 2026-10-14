//! EXP-1283: THE Guide picture viewer ×4 — the Rust twin of
//! `packages/domain-contract/src/guide-viewer.ts`, byte-locked by
//! `fixtures/session-results.json` `viewer.cases` (its `$comment` holds the
//! rules). Pure over the raw `sessions.guide` jsonb.

use serde::Serialize;
use serde_json::Value;

/// The viewer's state over one picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideViewerState {
    /// 1-based place of the picture among every Guide picture.
    pub index: usize,
    pub count: usize,
    pub topic: String,
    pub label: String,
    /// The picture's topic text (its first non-blank text entry), trimmed.
    pub text: Option<String>,
    pub prev_id: Option<String>,
    pub next_id: Option<String>,
}

fn non_blank<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key)?.as_str().filter(|value| !value.trim().is_empty())
}

fn is_summary(topic: &str) -> bool {
    topic.trim().eq_ignore_ascii_case("summary")
}

/// The viewer over `attachment_id`: every picture of the Guide in the
/// Guide's order (the first Summary topic leads, then topics first-seen,
/// pictures in publish order), no wrap; `None` when the attachment is not a
/// Guide picture.
pub fn guide_viewer(guide: Option<&Value>, attachment_id: &str) -> Option<GuideViewerState> {
    let mut topics: Vec<&str> = Vec::new();
    let mut texts: Vec<(&str, String)> = Vec::new();
    let mut pictures: Vec<(&str, &str, &str)> = Vec::new();
    for entry in guide.and_then(Value::as_array).into_iter().flatten() {
        if !entry.is_object() {
            continue;
        }
        let Some(topic) = non_blank(entry, "topic") else {
            continue;
        };
        let id = non_blank(entry, "attachmentId");
        let label = non_blank(entry, "label");
        let text = non_blank(entry, "text");
        if let (Some(id), Some(label)) = (id, label) {
            if !topics.contains(&topic) {
                topics.push(topic);
            }
            pictures.push((topic, label, id));
        } else if let Some(text) = text {
            if !topics.contains(&topic) {
                topics.push(topic);
            }
            if !texts.iter().any(|(seen, _)| *seen == topic) {
                texts.push((topic, text.trim().to_string()));
            }
        }
    }
    let lead = topics.iter().copied().find(|topic| is_summary(topic));
    let ordered: Vec<&str> = lead
        .into_iter()
        .chain(topics.iter().copied().filter(|topic| Some(*topic) != lead))
        .collect();
    let flat: Vec<(&str, &str, &str)> = ordered
        .iter()
        .flat_map(|topic| pictures.iter().copied().filter(move |(own, _, _)| own == topic))
        .collect();
    let at = flat.iter().position(|(_, _, id)| *id == attachment_id)?;
    let (topic, label, _) = flat[at];
    Some(GuideViewerState {
        index: at + 1,
        count: flat.len(),
        topic: topic.to_string(),
        label: label.to_string(),
        text: texts.iter().find(|(own, _)| *own == topic).map(|(_, text)| text.clone()),
        prev_id: at.checked_sub(1).map(|prev| flat[prev].2.to_string()),
        next_id: flat.get(at + 1).map(|(_, _, id)| id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `session-results.json` `viewer.cases`, whole values.
    #[test]
    fn guide_viewer_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let cases = fixture["viewer"]["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let actual = guide_viewer(Some(&case["guide"]), case["attachmentId"].as_str().unwrap());
            assert_eq!(serde_json::to_value(actual).unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
