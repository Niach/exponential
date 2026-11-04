//! #FEED-191 THE RESULTS TAB ×4 (`packages/domain-contract/src/results-tab.ts`'s
//! twin, iOS ExpCore `ResultsTab.swift`, Android `domain/ResultsTab.kt`): a chat
//! tree's results, the session screen's last tab. Byte-locked by
//! `fixtures/results-tab.json` (its `$comment` is the spec); pure.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const RESULTS_TAB_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/results-tab.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResultsCopy {
    pub now: String,
    pub untitled: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResultsTabCopy {
    pub label: String,
    pub empty: String,
    pub changes: String,
    pub stack: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResultsTabTestIds {
    pub tab: String,
    pub list: String,
    pub section: String,
    pub changes: String,
    pub stack: String,
}

/// A per-form-factor length (`wide` = web md+ and the IDE, `phone`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ResultsFormFactorLength {
    pub wide: f32,
    pub phone: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsTileSpec {
    pub thumb_height: ResultsFormFactorLength,
    pub thumb_max_width: ResultsFormFactorLength,
    pub gap: f32,
    /// Line 1 → the description.
    pub text_gap: f32,
    pub radius: f32,
    pub list_gap: f32,
    pub lead_gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ResultsTabSpecimen {
    pub case: usize,
}

/// The `tab` block: the session screen's Results tab.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsTabBlock {
    pub copy: ResultsTabCopy,
    pub test_ids: ResultsTabTestIds,
    pub tile: ResultsTileSpec,
    pub specimen: ResultsTabSpecimen,
}

/// `fixtures/results-tab.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ResultsTabSpec {
    pub copy: ResultsCopy,
    pub tab: ResultsTabBlock,
}

pub static RESULTS_TAB: LazyLock<ResultsTabSpec> =
    LazyLock::new(|| serde_json::from_str(RESULTS_TAB_JSON).expect("results-tab.json parses"));

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsSessionInput {
    pub id: String,
    pub root_id: String,
    /// The session's display name (session-title).
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsMessageInput {
    pub id: String,
    pub session_id: String,
    pub author_session_id: Option<String>,
    /// Epoch ms.
    pub created_at: i64,
    /// The raw `messages.card`.
    pub card: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultsTabInput {
    pub now: i64,
    pub root_id: String,
    pub sessions: Vec<ResultsSessionInput>,
    pub messages: Vec<ResultsMessageInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPicture {
    pub label: String,
    pub attachment_id: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub caption: Option<String>,
}

/// One result (TS `Result`; not `Result` here, std's owns the name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabResult {
    pub id: String,
    pub session_id: String,
    pub session_name: String,
    pub created_at: i64,
    /// `<m>m` / `<h>h` / `<d>d`, `now` under a minute.
    pub age: String,
    pub title: String,
    pub description: Option<String>,
    pub pictures: Vec<ResultPicture>,
    pub surface: bool,
}

fn text<'a>(card: &'a Value, key: &str) -> Option<&'a str> {
    card.get(key).and_then(Value::as_str)
}

fn number(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_f64().map(|v| v as i64))
}

fn resolved_pictures(card: &Value) -> Vec<ResultPicture> {
    let Some(list) = card.get("pictures").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter(|raw| raw.is_object())
        .filter_map(|picture| {
            let attachment_id = text(picture, "attachmentId").filter(|id| !id.is_empty())?;
            Some(ResultPicture {
                label: text(picture, "label").unwrap_or_default().to_string(),
                attachment_id: attachment_id.to_string(),
                width: picture.get("width").and_then(number),
                height: picture.get("height").and_then(number),
                caption: text(picture, "caption").map(str::to_string),
            })
        })
        .collect()
}

/// The card's flag: `result`, else a card from before the rename's `story`.
fn explicit_flag(card: &Value) -> Option<bool> {
    card.get("result")
        .and_then(Value::as_bool)
        .or_else(|| card.get("story").and_then(Value::as_bool))
}

/// A surface card that goes on the Results tab: the flag, else any resolved
/// picture. Plans and questions never do.
pub fn is_result_card(card: &Value) -> bool {
    if text(card, "kind") != Some("surface") || text(card, "questionId").is_some_and(|id| !id.is_empty()) {
        return false;
    }
    explicit_flag(card).unwrap_or_else(|| !resolved_pictures(card).is_empty())
}

/// `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `copy.now`.
pub fn result_age(created_at: i64, now: i64) -> String {
    let minutes = (now - created_at).max(0) / 60_000;
    if minutes < 1 {
        return RESULTS_TAB.copy.now.clone();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

fn trimmed(card: &Value, key: &str) -> Option<String> {
    text(card, key).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

/// THE fold: every result the root's tree posted, newest first.
pub fn results_tab(input: &ResultsTabInput) -> Vec<TabResult> {
    let sessions: HashMap<&str, &ResultsSessionInput> = input.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut out: Vec<TabResult> = input
        .messages
        .iter()
        .filter(|message| message.author_session_id.as_deref() == Some(message.session_id.as_str()))
        .filter(|message| is_result_card(&message.card))
        .filter_map(|message| {
            let session = *sessions.get(message.session_id.as_str())?;
            if session.root_id != input.root_id {
                return None;
            }
            let card = &message.card;
            Some(TabResult {
                id: message.id.clone(),
                session_id: session.id.clone(),
                session_name: session.name.clone(),
                created_at: message.created_at,
                age: result_age(message.created_at, input.now),
                title: trimmed(card, "title").unwrap_or_else(|| RESULTS_TAB.copy.untitled.clone()),
                description: trimmed(card, "description"),
                pictures: resolved_pictures(card),
                surface: card.get("surface").is_some_and(Value::is_object),
            })
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// THE styleguide specimen ×4 (`tab.specimen`): the fold of that case's input.
pub fn results_tab_specimen() -> Vec<TabResult> {
    let fixture: Value = serde_json::from_str(RESULTS_TAB_JSON).expect("results-tab.json parses");
    let case = &fixture["cases"][RESULTS_TAB.tab.specimen.case];
    let input: ResultsTabInput = serde_json::from_value(case["input"].clone()).expect("specimen case input");
    results_tab(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_tab_spec_parses() {
        assert_eq!(RESULTS_TAB.copy.now, "now");
        assert_eq!(RESULTS_TAB.copy.untitled, "Result");
        assert_eq!(RESULTS_TAB.tab.copy.label, "Results");
        assert_eq!(RESULTS_TAB.tab.test_ids.tab, "thread-results-tab");
        assert_eq!(RESULTS_TAB.tab.tile.thumb_height.phone, 240.0);
        assert_eq!(RESULTS_TAB.tab.tile.list_gap, 32.0);
    }

    #[test]
    fn results_tab_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(RESULTS_TAB_JSON).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input: ResultsTabInput = serde_json::from_value(case["input"].clone()).unwrap();
            let actual = serde_json::to_value(results_tab(&input)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
        let specimen = serde_json::to_value(results_tab_specimen()).unwrap();
        assert_eq!(specimen, cases[RESULTS_TAB.tab.specimen.case]["expected"]);
    }

    #[test]
    fn results_tab_age_floors() {
        assert_eq!(result_age(0, 59_999), "now");
        assert_eq!(result_age(0, 60_000), "1m");
        assert_eq!(result_age(0, 3_600_000), "1h");
        assert_eq!(result_age(0, 86_400_000), "1d");
        assert_eq!(result_age(10, 0), "now");
    }
}
