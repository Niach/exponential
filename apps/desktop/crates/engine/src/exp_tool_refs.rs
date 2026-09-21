//! EXP-920 — what an Exponential MCP tool's RESULT points at.
//!
//! The engine holds the raw answer of every MCP call the agent made, so it is
//! the one place that can distil the entities the answer NAMED into
//! [`EntityRef`]s and publish them on the settle's `tool_update` preview
//! (`refs`). Every client — web, desktop, iOS, Android — then renders the SAME
//! chips from the same refs and resolves the hover card (an issue's status, a
//! board's issues, a comment's body) from its own synced rows; `title` /
//! `identifier` ride along as the display fallback for a row the viewer has
//! not synced.
//!
//! [`exp_tool_refs`] is THE rule, mirrored ×2 with web
//! `lib/mcp/preview.ts` `toolResultPreview` and locked field-for-field by
//! `packages/domain-contract/fixtures/tool-result-preview.json`. It is driven
//! by the contract's per-tool spec (`expToolPreview.tools[].refs`, `kind@path`
//! terms): a term reads "the `<kind>` that node names", a `list:<member>@path`
//! term yields ONE `list` ref (its `id` = the member kind, `title` = the plural
//! noun, `count` = the row count) followed by the members, a `$`-rooted path
//! walks the call's INPUT instead of its answer (`pr_open` answers with a url,
//! the input named the issue).

use std::collections::HashSet;

use serde_json::{Map, Value};
use steer::EntityRef;

/// The cap on refs per preview (contract `expToolPreview.maxRefs`).
const MAX_REFS: usize = steer::TOOL_PREVIEW_MAX_REFS;
/// The cap on every string a ref carries (contract `expToolPreview.textMax`).
const TEXT_MAX: usize = domain::contract::EXP_TOOL_PREVIEW_TEXT_MAX;

/// The display string an entity row carries, in the order a row is known
/// by: a title, a name, an action's snapshot name (a session), a repo's full
/// name, a file's name, an invite's email, a device's label, a session's
/// issue title, a comment's body (cut).
const TITLE_KEYS: [&str; 9] = [
    "title",
    "name",
    "actionName",
    "issueTitle",
    "fullName",
    "filename",
    "email",
    "label",
    "body",
];

/// A session/comment/node row carries its issue's descriptors under these
/// twins; an `issue@` read over such a row takes them and nothing else.
const ISSUE_IDENTIFIER_TWIN: &str = "issueIdentifier";
const ISSUE_TITLE_TWIN: &str = "issueTitle";

/// EXP-846: the JSON an MCP answer actually carries. MCP returns a
/// `{content:[{type:"text",text:"…"}]}` envelope and the text is usually the
/// JSON itself, so both shapes (and a bare value) resolve to one value here;
/// anything unparseable is no payload. An `isError` envelope is never one.
/// Web `toolResultPayload`.
pub fn tool_result_payload(result: &Value) -> Option<Value> {
    match result {
        Value::String(text) => serde_json::from_str(text).ok(),
        Value::Object(map) => {
            if map.get("isError") == Some(&Value::Bool(true)) {
                return None;
            }
            if let Some(content) = map.get("content").and_then(Value::as_array) {
                // The first text block that parses as JSON is the payload.
                return content
                    .iter()
                    .filter_map(|block| block.as_object()?.get("text")?.as_str())
                    .find_map(|text| serde_json::from_str::<Value>(text).ok());
            }
            Some(Value::Object(map.clone()))
        }
        Value::Array(_) => Some(result.clone()),
        _ => None,
    }
}

/// The plural noun a `list` ref carries as its title — `kind + s`, with the
/// two irregulars. Web `listRefNoun`.
pub fn list_ref_noun(member_kind: &str) -> String {
    match member_kind {
        "status" => "statuses".to_string(),
        "repository" => "repositories".to_string(),
        other => format!("{other}s"),
    }
}

/// THE rule: the refs a tool's answer yields, in spec order, deduplicated
/// per kind by id AND identifier (an input's `EXP-12` next to the row it
/// resolved to is one issue), capped at `maxRefs`. Empty for an error
/// result, an unreadable payload, an unknown tool or a tool whose spec is
/// empty. `input` is the call's own arguments, for the tools whose answer
/// names no row (`pr_open`, the relation and label links); `{}` when the
/// call carried none.
pub fn exp_tool_refs(tool_name: &str, result: &Value, input: &Value) -> Vec<EntityRef> {
    let terms = preview_terms(tool_name);
    if terms.is_empty() {
        return vec![];
    }
    let payload = match tool_result_payload(result) {
        Some(Value::Null) | None => return vec![],
        Some(payload) => payload,
    };
    let mut refs: Vec<EntityRef> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |entity: Option<EntityRef>| {
        let Some(entity) = entity else { return };
        if refs.len() >= MAX_REFS {
            return;
        }
        let mut keys = vec![format!("{}:{}", entity.kind, entity.id)];
        if let Some(identifier) = &entity.identifier {
            keys.push(format!("{}:{identifier}", entity.kind));
        }
        if keys.iter().any(|key| seen.contains(key)) {
            return;
        }
        seen.extend(keys);
        refs.push(entity);
    };
    for term in &terms {
        let (root, path) = match term.path.strip_prefix('$') {
            Some(path) => (input, path),
            None => (&payload, term.path.as_str()),
        };
        if let Some(member) = &term.member {
            let Some(Value::Array(rows)) = nodes_at(root, path).into_iter().next() else {
                continue;
            };
            let count = root
                .as_object()
                .and_then(|map| map.get("total"))
                .and_then(Value::as_f64)
                .filter(|total| total.is_finite() && *total >= 0.0)
                .map_or_else(
                    || u32::try_from(rows.len()).unwrap_or(u32::MAX),
                    |total| u32::try_from(total.round() as u64).unwrap_or(u32::MAX),
                );
            push(Some(EntityRef {
                kind: "list".to_string(),
                id: member.clone(),
                identifier: None,
                title: Some(list_ref_noun(member)),
                count: Some(count),
            }));
            for row in rows {
                push(ref_from_node(member, row));
            }
            continue;
        }
        for node in nodes_at(root, path) {
            push(ref_from_node(&term.kind, node));
        }
    }
    refs
}

/// One parsed `kind@path` term of a contract spec.
struct Term {
    kind: String,
    /// `list:<member>` terms: the member kind.
    member: Option<String>,
    path: String,
}

/// The contract spec for a tool, parsed. Empty = not one of ours or nothing
/// to preview. Web `previewSpec`.
fn preview_terms(tool_name: &str) -> Vec<Term> {
    let Some(spec) = steer::exp_tool::exp_tool_preview_spec(tool_name) else {
        return vec![];
    };
    spec.split_whitespace().filter_map(parse_term).collect()
}

fn parse_term(term: &str) -> Option<Term> {
    let (head, path) = term.split_once('@')?;
    let is_kind = |kind: &str| domain::contract::ENTITY_REF_KIND_VALUES.contains(&kind);
    if let Some(member) = head.strip_prefix("list:") {
        if !is_kind(member) {
            return None;
        }
        return Some(Term {
            kind: "list".to_string(),
            member: Some(member.to_string()),
            path: path.to_string(),
        });
    }
    if !is_kind(head) {
        return None;
    }
    Some(Term { kind: head.to_string(), member: None, path: path.to_string() })
}

/// Walk a dotted path; a segment ending in `[]` fans out over an array. The
/// empty path is the root itself. Web `nodesAt`.
fn nodes_at<'a>(root: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut nodes = vec![root];
    if path.is_empty() {
        return nodes;
    }
    for segment in path.split('.') {
        let (key, fan) = match segment.strip_suffix("[]") {
            Some(key) => (key, true),
            None => (segment, false),
        };
        let mut next = Vec::new();
        for node in nodes {
            let value = if key.is_empty() {
                Some(node)
            } else {
                node.as_object().and_then(|map| map.get(key))
            };
            let Some(value) = value else { continue };
            if value.is_null() {
                continue;
            }
            if fan {
                if let Value::Array(items) = value {
                    next.extend(items.iter());
                }
            } else {
                next.push(value);
            }
        }
        nodes = next;
    }
    nodes
}

/// `kind@path` reads "the <kind> that node NAMES". On an object the id is
/// its `<kind>Id` column first (an issue's `statusId`, a comment's `issueId`,
/// a session's `issueId`; a member's `userId`), then its own `id` — so
/// `issue@` over a comment row is the comment's issue and over an issue row
/// the issue itself.
fn named_id_key(kind: &str) -> String {
    if kind == "member" {
        "userId".to_string()
    } else {
        format!("{kind}Id")
    }
}

/// One ref out of one node: an object naming the kind, or a bare id string
/// (an input's `issueId`, which may be an identifier). Web `refFromNode`.
fn ref_from_node(kind: &str, node: &Value) -> Option<EntityRef> {
    if let Value::String(raw) = node {
        let id = raw.trim();
        if id.is_empty() {
            return None;
        }
        let mut entity = EntityRef::new(kind, clamp(id));
        if kind == "issue" && is_issue_identifier(id) {
            entity.identifier = Some(entity.id.clone());
        }
        return Some(entity);
    }
    let row = node.as_object()?;
    let own_id = string_at(row, "id");
    let named_key = named_id_key(kind);
    // A node that CARRIES the `<kind>Id` column names that row — or, when the
    // column is null, nothing at all (an action run's `issueId`, an issue
    // with no `statusId`): the fall-through to its own `id` is for rows that
    // never had the column.
    let named = row.contains_key(&named_key).then(|| string_at(row, &named_key));
    let id = match &named {
        None => own_id,
        Some(named) => *named,
    }?;
    let mut entity = EntityRef::new(kind, clamp(id));
    let foreign = matches!(named, Some(Some(named)) if Some(named) != own_id);
    let mut identifier_keys: Vec<&str> = Vec::new();
    let mut title_keys: Vec<&str> = Vec::new();
    if kind == "issue" {
        identifier_keys.push(ISSUE_IDENTIFIER_TWIN);
        title_keys.push(ISSUE_TITLE_TWIN);
    }
    if !foreign || own_id.is_none() {
        identifier_keys.push("identifier");
        title_keys.extend(TITLE_KEYS);
    }
    let mut identifier = first_string(row, &identifier_keys);
    // A workflow node (`{id, issueId, identifier, title}`) carries its issue's
    // identifier under the plain key; a foreign read takes it only when it
    // LOOKS like one, so a row's own identifier never lands on another kind.
    if identifier.is_none() && kind == "issue" && foreign {
        identifier = string_at(row, "identifier").filter(|plain| is_issue_identifier(plain));
    }
    if let Some(identifier) = identifier {
        entity.identifier = Some(clamp(identifier));
    }
    if let Some(title) = first_string(row, &title_keys) {
        entity.title = Some(clamp(title));
    }
    Some(entity)
}

/// The trimmed string under `key`, when it is one and not blank.
fn string_at<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    let trimmed = row.get(key)?.as_str()?.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn first_string<'a>(row: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| string_at(row, key))
}

/// A human issue identifier (`EXP-42`), the web's `^[A-Z][A-Z0-9]*-\d+$` —
/// an input that names an issue by identifier rather than by uuid yields a
/// ref carrying both.
fn is_issue_identifier(text: &str) -> bool {
    let Some((head, number)) = text.split_once('-') else {
        return false;
    };
    let mut head = head.chars();
    head.next().is_some_and(|first| first.is_ascii_uppercase())
        && head.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// Trim, then cut to `textMax` code points — web `clamp`.
fn clamp(text: &str) -> String {
    text.trim().chars().take(TEXT_MAX).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    const FIXTURE: &str = include_str!(
        "../../../../../packages/domain-contract/fixtures/tool-result-preview.json"
    );

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        tool: String,
        result: Value,
        #[serde(default)]
        input: Option<Value>,
        refs: Vec<EntityRef>,
    }

    /// The rule is the fixture's, field for field, for every case — the web
    /// port is locked by the same file.
    #[test]
    fn the_fixture_locks_every_case() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert!(fixture.cases.len() >= 40, "the fixture lost its cases");
        for case in &fixture.cases {
            let input = case.input.clone().unwrap_or_else(|| json!({}));
            let refs = exp_tool_refs(&case.tool, &case.result, &input);
            assert_eq!(refs, case.refs, "case: {}", case.name);
        }
    }

    /// The identifier check is the web regex, hand-rolled: an uppercase
    /// head, digits after the ONE dash, nothing else.
    #[test]
    fn the_identifier_rule_is_the_regex() {
        for yes in ["EXP-1", "A-0", "FEED2-42", "REV2-103"] {
            assert!(is_issue_identifier(yes), "{yes}");
        }
        for no in ["exp-1", "EXP-", "-1", "EXP-1a", "EXP--1", "EXP-1-2", "2XP-1", "EXP 1", ""] {
            assert!(!is_issue_identifier(no), "{no}");
        }
    }

    /// The payload rule: an error envelope is no payload, a non-JSON text
    /// block is none, a bare value is itself.
    #[test]
    fn the_payload_is_the_answer_behind_the_envelope() {
        assert_eq!(
            tool_result_payload(&json!({"isError": true, "content": [{"type": "text", "text": "{\"id\":\"i-1\"}"}]})),
            None
        );
        assert_eq!(
            tool_result_payload(&json!({"content": [{"type": "text", "text": "nope"}, {"type": "text", "text": "[1]"}]})),
            Some(json!([1]))
        );
        assert_eq!(tool_result_payload(&json!("{\"a\":1}")), Some(json!({"a": 1})));
        assert_eq!(tool_result_payload(&json!(7)), None);
        assert_eq!(tool_result_payload(&json!({"content": "text"})), Some(json!({"content": "text"})));
    }

    /// The plural noun: `+s`, with the two irregulars.
    #[test]
    fn the_list_noun_pluralises() {
        assert_eq!(list_ref_noun("issue"), "issues");
        assert_eq!(list_ref_noun("status"), "statuses");
        assert_eq!(list_ref_noun("repository"), "repositories");
    }
}
