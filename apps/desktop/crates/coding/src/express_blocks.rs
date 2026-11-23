//! #EXP-2067 (`fixtures/chat-express.json`): Express blocks inline in an
//! agent's text, the SDK half of packages/domain-contract/src/chat-express.ts
//! over eui-core: the split (`express::unwrap`), each block ONE unwrapped
//! `ExpressDecoder` whose surface id is its key ([`ExpressRow`], the IDE's
//! transcript), the failure note the engine prepends to the agent's next
//! input ([`failure_note`]). The
//! pure half (gate, held tail, keys, prose, actions) is
//! [`domain::chat_express`].

use std::collections::{HashMap, HashSet};

use domain::chat_express::{has_express_block, issue_category, without_held_tail, CHAT_EXPRESS};
use eui_core::express::{unwrap, RawPart};
use eui_core::host::{Decoded, ExpressDecoder};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    Open,
    Ok,
    Failed,
    TooMany,
    TooLarge,
}

impl BlockStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockStatus::Open => "open",
            BlockStatus::Ok => "ok",
            BlockStatus::Failed => "failed",
            BlockStatus::TooMany => "tooMany",
            BlockStatus::TooLarge => "tooLarge",
        }
    }

    /// Draws THE inline marker `copy.blockFailed` under its surfaces.
    pub fn marks(self) -> bool {
        matches!(self, BlockStatus::Failed | BlockStatus::TooMany | BlockStatus::TooLarge)
    }
}

/// One block of a row, as its renderer draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockState {
    pub key: String,
    pub raw: String,
    pub closed: bool,
    pub status: BlockStatus,
    /// The SDK's category or `Unclosed` (failed only).
    pub error: Option<String>,
    pub error_message: Option<String>,
    /// Surface ids created and not deleted, in creation order.
    pub surfaces: Vec<String>,
    /// EVERY A2UI message since the block's stream (re)started.
    pub messages: Vec<Value>,
    /// Bumps when the stream restarts (the renderer resets its surfaces).
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowSegment {
    Prose(String),
    Block(BlockState),
}

struct LiveBlock {
    raw: String,
    decoder: ExpressDecoder,
    messages: Vec<Value>,
    issue: Option<String>,
    ended: bool,
    generation: u32,
}

const UNCLOSED_MESSAGE: &str = "The block has no </a2ui> line.";

fn tally(messages: &[Value]) -> (Vec<String>, usize) {
    let mut surfaces: Vec<String> = Vec::new();
    let mut comps: HashMap<String, HashSet<String>> = HashMap::new();
    for m in messages {
        let create = m.get("createSurface");
        let update = m.get("updateComponents");
        if let Some(c) = create {
            let id = c["surfaceId"].as_str().unwrap_or_default().to_string();
            surfaces.push(id.clone());
            comps.insert(id, HashSet::new());
        }
        if let Some(c) = create.or(update) {
            let id = c["surfaceId"].as_str().unwrap_or_default();
            if let (Some(set), Some(list)) = (comps.get_mut(id), c["components"].as_array()) {
                for k in list {
                    if let Some(kid) = k["id"].as_str() {
                        set.insert(kid.to_string());
                    }
                }
            }
        }
        if let Some(d) = m.get("deleteSurface") {
            let id = d["surfaceId"].as_str().unwrap_or_default();
            surfaces.retain(|s| s != id);
            comps.remove(id);
        }
    }
    let components = comps.values().map(HashSet::len).sum();
    (surfaces, components)
}

/// One row's blocks across its updates (`streamCases`).
pub struct ExpressRow {
    row_key: String,
    blocks: HashMap<String, LiveBlock>,
}

impl ExpressRow {
    pub fn new(row_key: impl Into<String>) -> Self {
        ExpressRow { row_key: row_key.into(), blocks: HashMap::new() }
    }

    /// The row's segments for its text so far; None = no block (draw as today).
    pub fn update(&mut self, text: &str, is_final: bool) -> Option<Vec<RowSegment>> {
        let shown = without_held_tail(text, is_final);
        if !has_express_block(shown) {
            self.blocks.clear();
            if shown.len() == text.len() {
                return None;
            }
            let t = shown.trim();
            return Some(if t.is_empty() { vec![] } else { vec![RowSegment::Prose(t.to_string())] });
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut n = 0usize;
        for part in unwrap(shown) {
            match part {
                RawPart::Text { text } => {
                    if !text.trim().is_empty() {
                        out.push(RowSegment::Prose(text));
                    }
                }
                RawPart::Block { a2ui_raw, is_final: closed } => {
                    n += 1;
                    let key = format!("{}#{n}", self.row_key);
                    seen.insert(key.clone());
                    out.push(RowSegment::Block(self.block(key, a2ui_raw, closed, n - 1, is_final)));
                }
            }
        }
        self.blocks.retain(|k, _| seen.contains(k));
        Some(out)
    }

    fn block(&mut self, key: String, raw: String, closed: bool, index: usize, is_final: bool) -> BlockState {
        let limits = &CHAT_EXPRESS.limits;
        let base = |status, generation| BlockState {
            key: key.clone(),
            raw: raw.clone(),
            closed,
            status,
            error: None,
            error_message: None,
            surfaces: vec![],
            messages: vec![],
            generation,
        };
        if index >= limits.blocks_per_row_max {
            return base(BlockStatus::TooMany, 0);
        }
        if raw.len() > limits.block_max_bytes {
            return base(BlockStatus::TooLarge, 0);
        }
        let restart = match self.blocks.get(&key) {
            Some(live) => !raw.starts_with(&live.raw) || (live.ended && raw != live.raw),
            None => false,
        };
        if restart || !self.blocks.contains_key(&key) {
            let generation = self.blocks.get(&key).map(|l| l.generation + 1).unwrap_or(0);
            self.blocks.insert(
                key.clone(),
                LiveBlock { raw: String::new(), decoder: ExpressDecoder::new(false, &key), messages: vec![], issue: None, ended: false, generation },
            );
        }
        let live = self.blocks.get_mut(&key).expect("inserted");
        if !live.ended && raw.len() > live.raw.len() {
            let d = live.decoder.push(&raw[live.raw.len()..]);
            take(live, d);
            live.raw = raw.clone();
        }
        if !live.ended && (closed || is_final) {
            let d = live.decoder.end();
            take(live, d);
            live.ended = true;
        }
        let (surfaces, components) = tally(&live.messages);
        if surfaces.len() > limits.block_max_surfaces || components > limits.block_max_components {
            return base(BlockStatus::TooLarge, live.generation);
        }
        let mut state = base(BlockStatus::Ok, live.generation);
        state.surfaces = surfaces;
        state.messages = live.messages.clone();
        if let Some(issue) = &live.issue {
            let (category, message) = issue_category(issue);
            state.status = BlockStatus::Failed;
            state.error = Some(category);
            state.error_message = Some(message);
        } else if !closed {
            if is_final {
                state.status = BlockStatus::Failed;
                state.error = Some("Unclosed".into());
                state.error_message = Some(UNCLOSED_MESSAGE.into());
            } else {
                state.status = BlockStatus::Open;
            }
        }
        state
    }
}

fn take(live: &mut LiveBlock, d: Decoded) {
    if live.issue.is_some() {
        return;
    }
    live.messages.extend(d.messages);
    if let Some(issue) = d.issues.into_iter().next() {
        live.issue = Some(issue.message);
    }
}

/// The drawn prose (`segmentCases.prose`).
pub fn express_prose(segments: Option<&[RowSegment]>, text: &str) -> String {
    match segments {
        None => text.to_string(),
        Some(s) => s
            .iter()
            .filter_map(|seg| match seg {
                RowSegment::Prose(t) => Some(t.as_str()),
                RowSegment::Block(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

/// `failureNoteCases`: the lines a finished turn's failed blocks leave for
/// the agent's NEXT input (None = none). `turn_texts` = the turn's
/// main-lane narration texts in order, each final.
pub fn failure_note<S: AsRef<str>>(turn_texts: &[S]) -> Option<String> {
    let limits = &CHAT_EXPRESS.limits;
    let mut notes = Vec::new();
    let mut n = 0usize;
    for text in turn_texts {
        let text = text.as_ref();
        if !has_express_block(text) {
            continue;
        }
        let mut in_row = 0usize;
        for part in unwrap(text) {
            let RawPart::Block { a2ui_raw: raw, is_final: closed } = part else { continue };
            n += 1;
            in_row += 1;
            let err = if in_row > limits.blocks_per_row_max {
                Some(("Limit".to_string(), format!("More than {} blocks in one message.", limits.blocks_per_row_max), 1))
            } else if raw.len() > limits.block_max_bytes {
                Some(("Limit".to_string(), format!("More than {} bytes in one block.", limits.block_max_bytes), 1))
            } else {
                block_error(&raw, closed, &format!("note#{n}"))
            };
            let Some((category, message, line)) = err else { continue };
            let mut line_text = raw.split('\n').nth(line - 1).unwrap_or_default().trim().to_string();
            if line_text.chars().count() > limits.note_line_max {
                line_text = format!("{}…", line_text.chars().take(limits.note_line_max - 1).collect::<String>());
            }
            notes.push(format!("[view] Block {n} of your last turn did not render ({category}): {message} Line {line}: {line_text}"));
        }
    }
    (!notes.is_empty()).then(|| notes.join("\n"))
}

fn block_error(raw: &str, closed: bool, key: &str) -> Option<(String, String, usize)> {
    let limits = &CHAT_EXPRESS.limits;
    let lines: Vec<&str> = raw.split('\n').collect();
    let mut decoder = ExpressDecoder::new(false, key);
    let mut messages: Vec<Value> = Vec::new();
    let check = |d: Decoded, line: usize, messages: &mut Vec<Value>| -> Option<(String, String, usize)> {
        for m in d.messages {
            messages.push(m);
            let (surfaces, components) = tally(messages);
            if surfaces.len() > limits.block_max_surfaces {
                return Some(("Limit".into(), format!("More than {} surfaces in one block.", limits.block_max_surfaces), line));
            }
            if components > limits.block_max_components {
                return Some(("Limit".into(), format!("More than {} components in one block.", limits.block_max_components), line));
            }
        }
        d.issues.into_iter().next().map(|issue| {
            let (category, message) = issue_category(&issue.message);
            (category, message, line)
        })
    };
    for (i, l) in lines.iter().enumerate() {
        let chunk = if i + 1 < lines.len() { format!("{l}\n") } else { (*l).to_string() };
        if let Some(err) = check(decoder.push(&chunk), i + 1, &mut messages) {
            return Some(err);
        }
    }
    if let Some(err) = check(decoder.end(), lines.len(), &mut messages) {
        return Some(err);
    }
    (!closed).then(|| ("Unclosed".to_string(), UNCLOSED_MESSAGE.to_string(), lines.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::chat_express::fixture;
    use serde_json::json;

    fn locked(segments: Option<Vec<RowSegment>>) -> Value {
        match segments {
            None => Value::Null,
            Some(list) => Value::Array(
                list.into_iter()
                    .map(|s| match s {
                        RowSegment::Prose(text) => json!({"kind": "prose", "text": text}),
                        RowSegment::Block(b) => {
                            let mut v = json!({"kind": "block", "key": b.key, "raw": b.raw, "closed": b.closed, "status": b.status.as_str(), "surfaces": b.surfaces});
                            if let Some(e) = b.error {
                                v["error"] = json!(e);
                            }
                            v
                        }
                    })
                    .collect(),
            ),
        }
    }

    /// The fixture's block objects with their keys in OUR order (serde
    /// preserve_order compares maps by order, so normalise through sorting).
    fn norm(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<_> = m.keys().cloned().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), norm(&m[&k]))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
            other => other.clone(),
        }
    }

    #[test]
    fn segment_cases() {
        let f = fixture();
        for c in f["segmentCases"].as_array().unwrap() {
            let text = c["text"].as_str().unwrap();
            let mut row = ExpressRow::new(c["rowKey"].as_str().unwrap());
            let out = row.update(text, c["final"].as_bool().unwrap());
            assert_eq!(express_prose(out.as_deref(), text), c["prose"].as_str().unwrap(), "{}", c["name"]);
            assert_eq!(norm(&locked(out)), norm(&c["expected"]), "{}", c["name"]);
        }
    }

    #[test]
    fn stream_cases() {
        let f = fixture();
        let steps = f["streamCases"].as_array().unwrap();
        let mut row = ExpressRow::new(steps[0]["rowKey"].as_str().unwrap());
        for step in steps {
            let out = row.update(step["text"].as_str().unwrap(), step["final"].as_bool().unwrap());
            assert_eq!(norm(&locked(out)), norm(&step["expected"]), "{}", step["text"]);
        }
    }

    #[test]
    fn any_chunking_reaches_the_segment_states() {
        let f = fixture();
        for c in f["segmentCases"].as_array().unwrap() {
            let text = c["text"].as_str().unwrap();
            let chars: Vec<char> = text.chars().collect();
            for size in [1usize, 3, 17] {
                let mut row = ExpressRow::new(c["rowKey"].as_str().unwrap());
                let mut i = size;
                while i < chars.len() + size {
                    let upto: String = chars[..i.min(chars.len())].iter().collect();
                    row.update(&upto, false);
                    i += size;
                }
                let out = row.update(text, c["final"].as_bool().unwrap());
                assert_eq!(norm(&locked(out)), norm(&c["expected"]), "{} @{size}", c["name"]);
            }
        }
    }

    #[test]
    fn failure_note_cases() {
        let f = fixture();
        for c in f["failureNoteCases"].as_array().unwrap() {
            let texts: Vec<&str> = c["texts"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
            assert_eq!(failure_note(&texts), c["expected"].as_str().map(str::to_string), "{}", c["name"]);
        }
    }
}
