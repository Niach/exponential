//! eApps R6 (VAPP-222, docs/spec/apps.md §3.3/§3.4): a kind's state machine
//! on ONE enum field (`records.<kind>.transitions`). A line-by-line port of
//! `src/host/record-transitions.ts`; `fixtures/record-transitions.json` is
//! the spec (`tests/record_transitions.rs` replays it).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Map, Value};

use super::package::PackageIssue;
use super::record_policy::record_author_of;

/// The key a records row of a kind with transitions carries its legal next states in.
pub const RECORD_TRANSITIONS_ROW_KEY: &str = "transitions";
pub const TRANSITION_LEVELS: &[&str] = &["member", "author", "owners"];
const KEYS: &[&str] = &["field", "initial", "from", "who"];
const ARROW: &str = "->";

/// A kind's state machine (the well-formed parts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordTransitions {
    pub field: String,
    pub initial: Option<Vec<String>>,
    pub from: BTreeMap<String, Vec<String>>,
    pub who: BTreeMap<String, String>,
}

/// Who moves a record: the member id and their team role.
#[derive(Clone, Copy, Debug, Default)]
pub struct TransitionViewer<'a> {
    pub id: Option<&'a str>,
    pub role: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransitionRefusal {
    pub code: &'static str,
    pub message: String,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn utf16_sorted<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut v: Vec<&String> = keys.collect();
    v.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    v
}

fn string_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?.iter().map(|s| s.as_str().map(str::to_string)).collect()
}

/// A kind's transitions when well-formed enough to apply (`field` a string,
/// `from` an object of lists); else `None`. Bad entries are dropped.
pub fn kind_transitions(kind: Option<&Value>) -> Option<RecordTransitions> {
    let t = kind?.get("transitions")?.as_object()?;
    let field = t.get("field")?.as_str().filter(|f| !f.is_empty())?.to_string();
    let from: BTreeMap<String, Vec<String>> = t.get("from")?.as_object()?.iter().filter_map(|(k, v)| string_list(v).map(|l| (k.clone(), l))).collect();
    let who: BTreeMap<String, String> = t.get("who").and_then(Value::as_object).map(|w| w.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
    Some(RecordTransitions { field, initial: t.get("initial").and_then(string_list), from, who })
}

/// The states a new record may start in: `initial`, else every `from` state
/// no edge enters (else every `from` state), in code-unit order.
pub fn initial_states(t: &RecordTransitions) -> Vec<String> {
    if let Some(initial) = &t.initial {
        return initial.clone();
    }
    let states: Vec<String> = utf16_sorted(t.from.keys()).into_iter().cloned().collect();
    let entered: BTreeSet<&String> = t.from.values().flatten().collect();
    let sources: Vec<String> = states.iter().filter(|s| !entered.contains(s)).cloned().collect();
    if sources.is_empty() {
        states
    } else {
        sources
    }
}

fn state_of<'a>(data: Option<&'a Value>, field: &str) -> Option<&'a str> {
    data?.get(field)?.as_str().filter(|s| !s.is_empty())
}

/// The level of the edge a → b (`who["a->b"]`, else `who["b"]`, else member).
fn edge_level<'a>(t: &'a RecordTransitions, a: &str, b: &str) -> &'a str {
    t.who.get(&format!("{a}{ARROW}{b}")).or_else(|| t.who.get(b)).map_or("member", String::as_str)
}

/// Why the viewer may not take an edge of `level` (`None` = they may).
fn level_refusal(level: &str, field: &str, to: &str, viewer: &TransitionViewer, before: Option<&Value>, row: Option<&Value>) -> Option<String> {
    if level == "member" || viewer.role == Some("owner") {
        return None;
    }
    if level == "owners" {
        return Some(format!("Only team owners can move {field} to {to}"));
    }
    if level == "author" {
        let author = record_author_of(row);
        return if author.is_some() && author == viewer.id { None } else { Some(format!("Only the record's author or a team owner can move {field} to {to}")) };
    }
    if let Some(role_field) = level.strip_prefix("role:") {
        let holder = state_of(before, role_field);
        return if holder.is_some() && holder == viewer.id { None } else { Some(format!("Only the record's {role_field} or a team owner can move {field} to {to}")) };
    }
    Some(format!("Nobody can move {field} to {to}"))
}

/// Why a write of a kind with transitions `t` may not land: `before` = the
/// stored data (`None` = a create), `after` = the data as it would be
/// stored, `row` = the record's ownership. `None` = it may.
pub fn transition_refusal(t: Option<&RecordTransitions>, before: Option<&Value>, after: &Value, viewer: &TransitionViewer, row: Option<&Value>) -> Option<TransitionRefusal> {
    let t = t?;
    let from = state_of(before, &t.field);
    let to = state_of(Some(after), &t.field);
    if from == to {
        return None;
    }
    let Some(from) = from else {
        let initial = initial_states(t);
        return match to {
            Some(to) if !initial.iter().any(|s| s == to) => Some(TransitionRefusal { code: "BAD_REQUEST", message: format!("{}: a new record starts in {}", t.field, initial.join(", ")) }),
            _ => None,
        };
    };
    let declared = to.is_some_and(|to| t.from.get(from).is_some_and(|next| next.iter().any(|n| n == to)));
    let Some(to) = to.filter(|_| declared) else {
        return Some(TransitionRefusal { code: "BAD_REQUEST", message: format!("{}: {from} → {} is not a declared transition", t.field, to.unwrap_or("(none)")) });
    };
    level_refusal(edge_level(t, from, to), &t.field, to, viewer, before, row).map(|message| TransitionRefusal { code: "FORBIDDEN", message })
}

/// The next states a record may move to (in the declared order; `None` = a
/// new record: the initial states), only those the viewer may take when one
/// is given.
pub fn transitions_from(t: Option<&RecordTransitions>, record: Option<&Value>, viewer: Option<&TransitionViewer>, row: Option<&Value>) -> Vec<String> {
    let Some(t) = t else { return Vec::new() };
    let Some(from) = state_of(record, &t.field) else { return initial_states(t) };
    let next = t.from.get(from).cloned().unwrap_or_default();
    match viewer {
        Some(viewer) => next.into_iter().filter(|to| level_refusal(edge_level(t, from, to), &t.field, to, viewer, record, row).is_none()).collect(),
        None => next,
    }
}

/// eApps R6: a kind's `transitions`, each issue under `<at>/transitions`.
pub fn validate_record_transitions(kind: &Value, at: &str, kind_name: &str) -> Vec<PackageIssue> {
    let Some(t) = kind.get("transitions") else { return Vec::new() };
    let tp = format!("{at}/transitions");
    let Some(t) = t.as_object() else { return vec![issue(tp, "transitions is {field, from, initial?, who?}")] };
    let mut issues = Vec::new();
    for key in utf16_sorted(t.keys()) {
        if !KEYS.contains(&key.as_str()) {
            issues.push(issue(format!("{tp}/{key}"), format!("unknown key {key}")));
        }
    }
    let empty = Map::new();
    let props = kind.get("schema").and_then(|s| s.get("properties")).and_then(Value::as_object).unwrap_or(&empty);
    let field = t.get("field").and_then(Value::as_str);
    let mut states: Option<Vec<String>> = None;
    match field {
        None | Some("") => issues.push(issue(format!("{tp}/field"), "field is the name of an enum field")),
        Some(f) => match props.get(f).and_then(Value::as_object) {
            None => issues.push(issue(format!("{tp}/field"), format!("{f} is not a declared field"))),
            Some(def) => match (def.get("type").and_then(Value::as_str), def.get("enum").and_then(string_list)) {
                (Some("string"), Some(values)) => states = Some(values),
                _ => issues.push(issue(format!("{tp}/field"), format!("{f} is not an enum field"))),
            },
        },
    }
    let field_name = field.unwrap_or("");
    let check_state = |s: &Value, p: String, issues: &mut Vec<PackageIssue>| match s.as_str() {
        None => issues.push(issue(p, "a state is a string")),
        Some(s) => {
            if states.as_ref().is_some_and(|st| !st.iter().any(|x| x == s)) {
                issues.push(issue(p, format!("{s} is not a state of {field_name}")));
            }
        }
    };
    let from = t.get("from").and_then(Value::as_object);
    match from {
        None => issues.push(issue(format!("{tp}/from"), "from is {state: [next state…]}")),
        Some(from) => {
            for s in utf16_sorted(from.keys()) {
                check_state(&Value::String(s.clone()), format!("{tp}/from/{s}"), &mut issues);
                match from[s].as_array() {
                    None => issues.push(issue(format!("{tp}/from/{s}"), "the next states are a list")),
                    Some(next) => {
                        for (i, n) in next.iter().enumerate() {
                            check_state(n, format!("{tp}/from/{s}/{i}"), &mut issues);
                        }
                    }
                }
            }
        }
    }
    if let Some(initial) = t.get("initial") {
        match initial.as_array().filter(|l| !l.is_empty()) {
            None => issues.push(issue(format!("{tp}/initial"), "initial is a non-empty list of states")),
            Some(list) => {
                for (i, s) in list.iter().enumerate() {
                    check_state(s, format!("{tp}/initial/{i}"), &mut issues);
                }
            }
        }
    }
    if let Some(who) = t.get("who") {
        match who.as_object() {
            None => issues.push(issue(format!("{tp}/who"), r#"who is {"a->b" | state: level}"#)),
            Some(who) => {
                let refs = kind.get("refs").and_then(Value::as_object);
                for key in utf16_sorted(who.keys()) {
                    let wp = format!("{tp}/who/{key}");
                    if let Some(arrow) = key.find(ARROW) {
                        let (a, b) = (&key[..arrow], &key[arrow + ARROW.len()..]);
                        let declared = from.and_then(|f| f.get(a)).and_then(Value::as_array).is_some_and(|next| next.iter().any(|n| n.as_str() == Some(b)));
                        if !declared {
                            issues.push(issue(&wp, format!("{a} → {b} is not a declared transition")));
                        }
                    } else {
                        check_state(&Value::String(key.clone()), wp.clone(), &mut issues);
                    }
                    match who[key].as_str() {
                        Some(level) if level.starts_with("role:") => {
                            let role_field = &level[5..];
                            if refs.and_then(|r| r.get(role_field)).and_then(Value::as_str) != Some("member") {
                                issues.push(issue(&wp, format!("{level}: {role_field} is not a member ref of {kind_name}")));
                            }
                        }
                        Some(level) if TRANSITION_LEVELS.contains(&level) => {}
                        _ => issues.push(issue(&wp, "a level is member, author, owners or role:<field>")),
                    }
                }
            }
        }
    }
    let computed = kind.get("computed").and_then(Value::as_object);
    if props.contains_key(RECORD_TRANSITIONS_ROW_KEY) || computed.is_some_and(|c| c.contains_key(RECORD_TRANSITIONS_ROW_KEY)) {
        issues.push(issue(tp, format!("a kind with transitions declares no field {RECORD_TRANSITIONS_ROW_KEY} (its rows carry the next states there)")));
    }
    issues
}
