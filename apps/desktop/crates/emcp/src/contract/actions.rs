//! The slice of `packages/domain-contract/fixtures/app-actions.json` the
//! exposes rules read: which `exponential.<noun>.<op>` actions exist, which
//! are destructive (by word or by argument) and which are owner-only.
//! Read from the fixture itself: emcp links no app crate (the app's
//! `domain::app_actions` replays the same file).

use serde_json::Value;

use super::fixture;

/// `exponential.issues.update_status` → (tool, op); `None` for anything
/// that is not an offered action.
pub fn parse_action_name(name: &str) -> Option<(String, Option<String>)> {
    let target = fixture::app_actions()["tools"].get(name)?;
    Some((target["tool"].as_str()?.to_string(), target["op"].as_str().map(str::to_string)))
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.to_lowercase().split(|c: char| !c.is_ascii_lowercase()).map(str::to_string).collect::<Vec<_>>().into_iter()
}

pub(crate) fn destructive_words() -> Vec<String> {
    fixture::app_actions()["destructiveWords"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// A destructive action: a word of its tool or op is a destructive word.
pub fn action_destructive(name: &str) -> bool {
    let Some((tool, op)) = parse_action_name(name) else { return false };
    let destructive = destructive_words();
    words(&format!("{tool} {}", op.unwrap_or_default())).any(|word| destructive.contains(&word))
}

/// An operation the server runs for a team owner only: never grantable.
pub fn action_owner_only(name: &str) -> bool {
    fixture::app_actions()["ownerActions"]
        .as_array()
        .is_some_and(|list| list.iter().any(|entry| entry.as_str() == Some(name)))
}

/// A destructive CALL: a destructive action, or one whose args carry a
/// `destructiveArgs` key (present and not false/null).
pub fn call_destructive(name: &str, args: Option<&Value>) -> bool {
    if action_destructive(name) {
        return true;
    }
    let keys = fixture::app_actions()["destructiveArgs"][name].as_array().cloned().unwrap_or_default();
    keys.iter().filter_map(Value::as_str).any(|key| {
        args.and_then(|args| args.get(key)).is_some_and(|value| !matches!(value, Value::Null | Value::Bool(false)))
    })
}
