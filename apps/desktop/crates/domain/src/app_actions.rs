//! VAPP-120 — typed host ACTIONS, the Rust twin of
//! `packages/domain-contract/src/app-actions.ts` (iOS `AppActions.swift`,
//! Android `AppActions.kt`), byte-locked by `fixtures/app-actions.json`
//! (`parseCases`, `grantCases`, `declaredCases`, `consentCases`,
//! `ownerOnlyCases`): which
//! `exponential.<noun>.<op>` functions exist, what a name resolves to, what a
//! package declares, what an app's grants allow, and the consent card's
//! words. Pure: the app host feeds it names, grants and args.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::contract::{EXP_TOOL_NAMES, EXP_TOOL_PREFIX, EXP_TOOL_SUBJECT_KEYS, EXP_TOOL_TITLES};

const ACTIONS_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-actions.json");

/// The fixture's `copy`: the app settings page's Tools group.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ActionsCopy {
    pub title: String,
    pub hint: String,
    pub none: String,
    pub granted: String,
    /// The row of a destructive or owner-only action, in place of its switch.
    pub always: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ToolRow {
    tool: String,
    op: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ActionsSpec {
    prefix: String,
    #[serde(rename = "grantRole")]
    grant_role: String,
    #[serde(rename = "destructiveWords")]
    destructive_words: Vec<String>,
    #[serde(rename = "destructiveArgs")]
    destructive_args: BTreeMap<String, Vec<String>>,
    #[serde(rename = "ownerActions")]
    owner_actions: Vec<String>,
    functions: Vec<String>,
    tools: BTreeMap<String, ToolRow>,
    copy: ActionsCopy,
}

static SPEC: LazyLock<ActionsSpec> =
    LazyLock::new(|| serde_json::from_str(ACTIONS_JSON).expect("app-actions.json parses"));

/// `exponential.`
pub static APP_ACTION_PREFIX: LazyLock<&'static str> = LazyLock::new(|| SPEC.prefix.as_str());
/// Every function name, sorted (the fixture's ×4 list).
pub static APP_ACTIONS: LazyLock<&'static [String]> = LazyLock::new(|| SPEC.functions.as_slice());
/// The settings page's Tools group copy.
pub static APP_ACTIONS_COPY: LazyLock<&'static ActionsCopy> = LazyLock::new(|| &SPEC.copy);

/// What an action runs: the MCP tool and the noun's op (None for a verb tool).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTarget {
    pub tool: String,
    pub op: Option<String>,
}

/// `exponential.issues.update_status` → the tool and op it runs; None for
/// anything that is not an offered action.
pub fn parse_action_name(name: &str) -> Option<ActionTarget> {
    SPEC.tools.get(name).map(|row| ActionTarget { tool: row.tool.clone(), op: row.op.clone() })
}

/// VAPP-107's pattern rule: an exact name or a `prefix*`.
fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// The actions a package's `functions` list names (patterns expanded), in
/// the fixture's order; anything that is not an offered action is dropped.
pub fn declared_actions<S: AsRef<str>>(functions: &[S]) -> Vec<String> {
    APP_ACTIONS
        .iter()
        .filter(|name| functions.iter().any(|pattern| matches(pattern.as_ref(), name)))
        .cloned()
        .collect()
}

/// The team role that sets or changes an app's grants (the fixture's
/// `grantRole`, `owner`): the creator gets no grant power from creating it.
pub const GRANT_ROLE: &str = "owner";

/// May a member with `role` set an app's grants? Owners only.
pub fn can_grant_actions(role: Option<&str>) -> bool {
    role == Some(GRANT_ROLE)
}

/// A destructive action: a word of its tool name or op is delete, merge,
/// close, end, remove or trash. Never grantable; always asks.
pub fn action_destructive(name: &str) -> bool {
    let Some(target) = parse_action_name(name) else { return false };
    let words = format!("{} {}", target.tool, target.op.as_deref().unwrap_or("")).to_lowercase();
    words
        .split(|c: char| !c.is_ascii_lowercase())
        .any(|w| SPEC.destructive_words.iter().any(|d| d == w))
}

/// VAPP-158: an operation the server runs for a team owner only (members,
/// invites, MCP servers, prompts, the team's settings). Never grantable;
/// always asks (the plain card).
pub fn action_owner_only(name: &str) -> bool {
    SPEC.owner_actions.iter().any(|a| a == name)
}

/// May `name` be granted at all: an offered action that is neither
/// destructive nor owner-only.
pub fn action_grantable(name: &str) -> bool {
    parse_action_name(name).is_some() && !action_destructive(name) && !action_owner_only(name)
}

/// A destructive CALL: a destructive action, or a grantable one whose args
/// carry a `destructiveArgs` key (present and not false/null):
/// `exponential.sessions.send` with `stop` = prompts.json's stop-run.
pub fn call_destructive(name: &str, args: &Map<String, Value>) -> bool {
    if action_destructive(name) {
        return true;
    }
    SPEC.destructive_args.get(name).is_some_and(|keys| {
        keys.iter()
            .any(|key| args.get(key).is_some_and(|v| !v.is_null() && *v != Value::Bool(false)))
    })
}

/// Does the app's `granted_tools` list let this call run without asking?
/// Exact names only (a grant is a person's choice, never a pattern), and a
/// destructive action or call ignores a stored grant: it always asks.
pub fn grant_allows<S: AsRef<str>>(granted: &[S], name: &str, args: &Map<String, Value>) -> bool {
    action_grantable(name) && !call_destructive(name, args) && granted.iter().any(|g| g.as_ref() == name)
}

/// The consent card's words for a typed action call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionConsent {
    pub tool: String,
    pub title: String,
    pub destructive: bool,
    /// The arguments as compact JSON, None when there are none.
    pub args: Option<String>,
}

/// The contract `expToolDisplay` row (`<noun>_<op>`) an action shows as:
/// (title, subjectKey).
fn display_row(target: &ActionTarget) -> Option<(&'static str, &'static str)> {
    let noun = target.tool.strip_prefix(EXP_TOOL_PREFIX).unwrap_or(&target.tool);
    let row_name = match &target.op {
        Some(op) => format!("{noun}_{op}"),
        None => noun.to_string(),
    };
    let index = EXP_TOOL_NAMES.iter().position(|row| *row == row_name)?;
    Some((EXP_TOOL_TITLES[index], EXP_TOOL_SUBJECT_KEYS[index]))
}

/// The display row's title for an action (`Set issue status`), the name
/// itself for anything else: the settings page's row label.
pub fn action_title(name: &str) -> String {
    parse_action_name(name)
        .and_then(|target| display_row(&target))
        .map(|(title, _)| title.to_string())
        .unwrap_or_else(|| name.to_string())
}

/// The consent card's words for a typed action call: the display row's
/// title lower-cased after "Allow this surface to", the subject arg when the
/// row names one, the destructive flag from the tool name and op, the args
/// as compact JSON (the given key order). None for a name that is not an
/// offered action (the generic `exponential-ui-consent` copy applies).
pub fn action_consent(name: &str, args: &Map<String, Value>) -> Option<ActionConsent> {
    let target = parse_action_name(name)?;
    let row = display_row(&target);
    let base = match row {
        Some((title, _)) => {
            let mut chars = title.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
                None => String::new(),
            }
        }
        None => name.to_string(),
    };
    let subject = row
        .filter(|(_, key)| !key.is_empty())
        .and_then(|(_, key)| args.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let title = match subject {
        Some(subject) => format!("Allow this surface to {base} {subject}?"),
        None => format!("Allow this surface to {base}?"),
    };
    let destructive = call_destructive(name, args);
    let args = (!args.is_empty()).then(|| Value::Object(args.clone()).to_string());
    Some(ActionConsent { tool: target.tool, title, destructive, args })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(ACTIONS_JSON).unwrap();
        fixture[key].as_array().unwrap().clone()
    }

    fn strings(v: &Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(*APP_ACTION_PREFIX, "exponential.");
        let fixture: Value = serde_json::from_str(ACTIONS_JSON).unwrap();
        assert_eq!(APP_ACTIONS.to_vec(), strings(&fixture["functions"]));
        assert!(APP_ACTIONS.iter().all(|name| name.starts_with(*APP_ACTION_PREFIX)));
        assert!(APP_ACTIONS.iter().all(|name| parse_action_name(name).is_some()));
        assert_eq!(APP_ACTIONS.len(), SPEC.tools.len());
        assert!(!APP_ACTIONS_COPY.title.is_empty() && !APP_ACTIONS_COPY.none.is_empty());
        // Every offered action has a display row (the settings label).
        for name in APP_ACTIONS.iter() {
            assert_ne!(action_title(name), *name, "{name} has no expToolDisplay row");
        }
    }

    #[test]
    fn parse_cases() {
        for case in cases("parseCases") {
            let name = case["name"].as_str().unwrap();
            let got = parse_action_name(name).map(|t| {
                serde_json::json!({ "tool": t.tool, "op": t.op })
            });
            let expected = (!case["expected"].is_null()).then(|| case["expected"].clone());
            assert_eq!(got, expected, "{name}");
        }
    }

    #[test]
    fn grant_cases() {
        for case in cases("grantCases") {
            let name = case["name"].as_str().unwrap();
            let empty = Map::new();
            let args = case["args"].as_object().unwrap_or(&empty);
            assert_eq!(
                grant_allows(&strings(&case["granted"]), name, args),
                case["expected"].as_bool().unwrap(),
                "{name}"
            );
        }
    }

    #[test]
    fn grantable_cases() {
        for case in cases("grantableCases") {
            let name = case["name"].as_str().unwrap();
            assert_eq!(action_grantable(name), case["expected"].as_bool().unwrap(), "{name}");
        }
        assert_eq!(SPEC.grant_role, GRANT_ROLE);
        assert!(!SPEC.destructive_words.is_empty() && SPEC.destructive_args.contains_key("exponential.sessions.send"));
    }

    #[test]
    fn owner_only_cases() {
        for case in cases("ownerOnlyCases") {
            let name = case["name"].as_str().unwrap();
            assert_eq!(action_owner_only(name), case["expected"].as_bool().unwrap(), "{name}");
        }
        // Every owner action is an offered one, and the card stays plain.
        for name in &SPEC.owner_actions {
            assert!(parse_action_name(name).is_some(), "{name}");
            assert!(!action_grantable(name), "{name}");
            assert!(!call_destructive(name, &Map::new()), "{name}");
        }
    }

    #[test]
    fn can_grant_cases() {
        for case in cases("canGrantCases") {
            let role = case["role"].as_str().unwrap();
            assert_eq!(can_grant_actions(Some(role)), case["expected"].as_bool().unwrap(), "{role}");
        }
        assert!(!can_grant_actions(None));
    }

    #[test]
    fn declared_cases() {
        for case in cases("declaredCases") {
            assert_eq!(
                declared_actions(&strings(&case["functions"])),
                strings(&case["expected"]),
                "{}",
                case["functions"]
            );
        }
    }

    #[test]
    fn consent_cases() {
        for case in cases("consentCases") {
            let name = case["name"].as_str().unwrap();
            let got = action_consent(name, case["args"].as_object().unwrap()).map(|c| {
                serde_json::json!({ "tool": c.tool, "title": c.title, "destructive": c.destructive, "args": c.args })
            });
            let expected = (!case["expected"].is_null()).then(|| case["expected"].clone());
            assert_eq!(got, expected, "{name}");
        }
    }
}
