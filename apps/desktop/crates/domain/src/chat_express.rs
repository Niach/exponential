//! #EXP-2067 (`fixtures/chat-express.json`): Express blocks inline in an
//! agent's text: the PURE half of packages/domain-contract/src/chat-express.ts
//! (no SDK): the gate, the held tail, the row keys, the pure prose every
//! non-drawing read uses, the action card + its input line, the marker's
//! words, the issue category. The SDK half (the split, the per-block
//! streams, the failure note) lives in `coding::express_blocks`, which has
//! eui-core.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};

const FIXTURE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/chat-express.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub blocks_per_row_max: usize,
    pub block_max_bytes: usize,
    pub block_max_surfaces: usize,
    pub block_max_components: usize,
    pub action_context_max_bytes: usize,
    pub action_name_max: usize,
    pub note_line_max: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Copy {
    pub block_failed: String,
    pub block_only: String,
    pub action_marker: String,
    pub you: String,
    pub action_too_large: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Prompt {
    pub preamble: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub block_gap: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatExpressSpec {
    pub gate: String,
    pub reply_event: String,
    pub limits: Limits,
    pub copy: Copy,
    pub prompt: Prompt,
    pub layout: Layout,
}

pub static CHAT_EXPRESS: LazyLock<ChatExpressSpec> =
    LazyLock::new(|| serde_json::from_str(FIXTURE_JSON).expect("chat-express.json parses"));

/// The whole fixture (the replays read its cases).
pub fn fixture() -> Value {
    serde_json::from_str(FIXTURE_JSON).expect("chat-express.json parses")
}

static GATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("(?m){}", CHAT_EXPRESS.gate)).unwrap());
static OPEN_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&CHAT_EXPRESS.gate).unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*(```|~~~)[\w-]*[ \t]*$").unwrap());
static CLOSE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*</a2ui>[ \t]*$").unwrap());

/// A row's text holds blocks: one of its lines starts with the open tag.
pub fn has_express_block(text: &str) -> bool {
    GATE.is_match(text)
}

const TAG: &str = "<a2ui>";

/// While a row streams, a last line that is a proper prefix of `<a2ui>` is
/// held back (the tag may still be arriving).
pub fn without_held_tail(text: &str, is_final: bool) -> &str {
    if is_final {
        return text;
    }
    let start = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let last = text[start..].trim_start();
    if !last.is_empty() && last.len() < TAG.len() && TAG.starts_with(last) {
        let cut = &text[..start];
        return cut.strip_suffix('\n').unwrap_or(cut);
    }
    text
}

/// The key of a synced row.
pub fn synced_row_key(message_id: &str) -> String {
    format!("m:{message_id}")
}

/// The key of a feed narration row: its messageId, else its first seq.
pub fn feed_row_key(session_id: &str, message_id: Option<&str>, first_seq: u64) -> String {
    match message_id.filter(|id| !id.is_empty()) {
        Some(id) => format!("n:{session_id}:{id}"),
        None => format!("e:{session_id}:{first_seq}"),
    }
}

/// A decoder issue's message (`<category text>: <message>`) → (category,
/// the SDK's words).
pub fn issue_category(message: &str) -> (String, String) {
    for (prefix, category) in
        [("parse error: ", "ParseError"), ("validation error: ", "ValidationError"), ("catalog error: ", "CatalogError")]
    {
        if let Some(rest) = message.strip_prefix(prefix) {
            return (category.to_string(), rest.to_string());
        }
    }
    ("ParseError".to_string(), message.to_string())
}

/// The PURE prose (`plainProseCases`) for every read that does not draw.
pub fn plain_prose(text: &str) -> String {
    if !has_express_block(text) {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut keep: Vec<Option<&str>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !OPEN_LINE.is_match(line) {
            keep.push(Some(line));
            i += 1;
            continue;
        }
        if let Some(Some(prev)) = keep.last() {
            if FENCE.is_match(prev) {
                keep.pop();
            }
        }
        let rest = OPEN_LINE.replace(line, "");
        let mut j = i;
        if !rest.contains("</a2ui>") {
            j = i + 1;
            while j < lines.len() && !CLOSE_LINE.is_match(lines[j]) {
                j += 1;
            }
        }
        if j + 1 < lines.len() && FENCE.is_match(lines[j + 1]) {
            j += 1;
        }
        keep.push(None);
        i = j + 1;
    }
    let mut runs: Vec<String> = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    for l in keep {
        match l {
            None => runs.push(std::mem::take(&mut run).join("\n")),
            Some(l) => run.push(l),
        }
    }
    runs.push(run.join("\n"));
    runs.iter().map(|r| r.trim()).filter(|r| !r.is_empty()).collect::<Vec<_>>().join("\n\n")
}

/// THE `action` card.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionCard {
    pub kind: String,
    pub name: String,
    pub context: Option<Map<String, Value>>,
    pub surface_id: String,
    pub block: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOrigin {
    pub surface_id: String,
    pub block: Option<String>,
    pub message_id: Option<String>,
}

/// An exponential_show surface's origin.
pub fn show_action_origin(message_id: &str) -> ActionOrigin {
    ActionOrigin { surface_id: format!("show:{message_id}"), block: None, message_id: Some(message_id.to_string()) }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionPost {
    /// Post ONE person message: this body + this card.
    Post { body: String, card: ActionCard },
    /// Nothing posted: THE failed-action toast `copy.actionTooLarge`.
    TooLarge,
}

fn trimmed(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// `actionCases`: what an action event posts (None = no event name).
pub fn surface_action(name: Option<&str>, context: Option<&Value>, user_message: Option<&Value>, origin: &ActionOrigin) -> Option<ActionPost> {
    let name = name.filter(|n| !n.is_empty())?;
    let limits = &CHAT_EXPRESS.limits;
    let context = context.and_then(Value::as_object).cloned();
    if name.chars().count() > limits.action_name_max {
        return Some(ActionPost::TooLarge);
    }
    if let Some(c) = &context {
        if serde_json::to_string(c).map(|s| s.len()).unwrap_or(usize::MAX) > limits.action_context_max_bytes {
            return Some(ActionPost::TooLarge);
        }
    }
    let user = trimmed(user_message);
    let body = if name == CHAT_EXPRESS.reply_event {
        if user.is_empty() { trimmed(context.as_ref().and_then(|c| c.get("text"))) } else { user }
    } else {
        user
    };
    Some(ActionPost::Post {
        body,
        card: ActionCard {
            kind: "action".into(),
            name: name.to_string(),
            context,
            surface_id: origin.surface_id.clone(),
            block: origin.block.clone(),
            message_id: origin.message_id.clone(),
        },
    })
}

const ACTION_LEAD: &str = "[action] ";

/// `inputCases`: the ONE input the device hands the agent.
pub fn action_input(card: &ActionCard, body: &str) -> String {
    let mut head = Map::new();
    head.insert("name".into(), Value::String(card.name.clone()));
    head.insert("surfaceId".into(), Value::String(card.surface_id.clone()));
    head.insert("block".into(), card.block.clone().map(Value::String).unwrap_or(Value::Null));
    head.insert("messageId".into(), card.message_id.clone().map(Value::String).unwrap_or(Value::Null));
    head.insert("context".into(), card.context.clone().map(Value::Object).unwrap_or(Value::Null));
    let head = serde_json::to_string(&Value::Object(head)).unwrap_or_default();
    if body.is_empty() { format!("{ACTION_LEAD}{head}") } else { format!("{ACTION_LEAD}{head}\n{body}") }
}

/// A feed input back to its card and body.
pub fn parse_action_input(text: &str) -> Option<(ActionCard, String)> {
    let rest = text.strip_prefix(ACTION_LEAD)?;
    let (head, body) = match rest.find('\n') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    let v: Value = serde_json::from_str(head).ok()?;
    let name = v.get("name")?.as_str()?.to_string();
    let surface_id = v.get("surfaceId")?.as_str()?.to_string();
    Some((
        ActionCard {
            kind: "action".into(),
            name,
            context: v.get("context").and_then(Value::as_object).cloned(),
            surface_id,
            block: v.get("block").and_then(Value::as_str).map(str::to_string),
            message_id: v.get("messageId").and_then(Value::as_str).map(str::to_string),
        },
        body,
    ))
}

/// The marker's words of an action row without a body (`author` None = the viewer).
pub fn action_marker(name: &str, author: Option<&str>) -> String {
    CHAT_EXPRESS.copy.action_marker.replace("{author}", author.unwrap_or(&CHAT_EXPRESS.copy.you)).replace("{name}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prose_cases() {
        let f = fixture();
        for c in f["plainProseCases"].as_array().unwrap() {
            assert_eq!(plain_prose(c["text"].as_str().unwrap()), c["expected"].as_str().unwrap(), "{}", c["name"]);
        }
    }

    #[test]
    fn action_cases() {
        let f = fixture();
        for c in f["actionCases"].as_array().unwrap() {
            let e = &c["event"];
            let origin: ActionOrigin = serde_json::from_value(c["origin"].clone()).unwrap();
            let got = surface_action(e["name"].as_str(), e.get("context"), e.get("userMessage"), &origin).unwrap();
            match (&got, &c["expected"]) {
                (ActionPost::TooLarge, Value::String(s)) => assert_eq!(s, "tooLarge", "{}", c["name"]),
                (ActionPost::Post { body, card }, exp) => {
                    assert_eq!(body, exp["body"].as_str().unwrap(), "{}", c["name"]);
                    assert_eq!(serde_json::to_value(card).unwrap(), exp["card"], "{}", c["name"]);
                }
                _ => panic!("{}: {got:?}", c["name"]),
            }
        }
    }

    #[test]
    fn input_cases() {
        let f = fixture();
        for c in f["inputCases"].as_array().unwrap() {
            let card: ActionCard = serde_json::from_value(c["card"].clone()).unwrap();
            let body = c["body"].as_str().unwrap();
            let line = action_input(&card, body);
            assert_eq!(line, c["expected"].as_str().unwrap(), "{}", c["name"]);
            assert_eq!(parse_action_input(&line), Some((card, body.to_string())), "{}", c["name"]);
        }
        assert_eq!(parse_action_input("[action] not json"), None);
        assert_eq!(parse_action_input("Ship it"), None);
    }

    #[test]
    fn gate_and_held_tail() {
        assert!(has_express_block("a\n<a2ui>\nx"));
        assert!(has_express_block("  <a2ui surface=\"x\">"));
        assert!(!has_express_block("Wrap it in `<a2ui>` tags."));
        assert_eq!(without_held_tail("Checking.\n<a2", false), "Checking.");
        assert_eq!(without_held_tail("Checking.\n<a2", true), "Checking.\n<a2");
        assert_eq!(without_held_tail("a < b", false), "a < b");
    }

    #[test]
    fn markers_and_keys() {
        assert_eq!(action_marker("save_deal", None), "You pressed save_deal");
        assert_eq!(action_marker("save_deal", Some("Ana")), "Ana pressed save_deal");
        assert_eq!(feed_row_key("s1", Some("msg_07"), 3), "n:s1:msg_07");
        assert_eq!(feed_row_key("s1", None, 3), "e:s1:3");
        assert_eq!(issue_category("validation error: Nope"), ("ValidationError".into(), "Nope".into()));
    }
}
