//! #EDE-96 — the device answers a transcript SEARCH over a session's journal
//! (`packages/domain-contract/fixtures/steer-enet.json` `search`, whose
//! `$comment` is the spec; the TS reference is `steerSearch`).
//!
//! A viewer searches what it holds itself and asks the device that holds the
//! journal for the rest: `search {requestId, query, beforeSeq?, limit}` →
//! ONE `search_result` to the asker. Both the live publisher and STORED
//! REPLAY answer off the journal FILE ([`answer`]).

use std::collections::HashMap;
use std::path::Path;

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

use crate::frames::{ActivityEvent, ClientFrame, SearchHit};

/// Query code points kept (after the whitespace collapse).
pub const QUERY_MAX_CHARS: usize = 200;
/// The most hits one answer carries (`limit` is clamped to it).
pub const HITS_MAX: usize = 50;
/// The searched text (UTF-8 bytes, collapsed) one answer scans at most.
pub const SCAN_MAX_BYTES: usize = 1_048_576;
/// A snippet's width in code points (more only for a longer match).
pub const SNIPPET_CHARS: usize = 120;
/// Code points of context a snippet keeps before the match.
pub const SNIPPET_LEAD_CHARS: usize = 40;

/// What [`search`] answers (the frame minus its request id).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub total: Option<usize>,
    pub done: bool,
    pub scanned_to_seq: u64,
}

impl SearchOutcome {
    /// Nothing to search: no hits, done.
    pub fn empty() -> SearchOutcome {
        SearchOutcome { done: true, ..Default::default() }
    }

    /// The `search_result` frame, serialized.
    pub fn frame(self, request_id: &str) -> String {
        ClientFrame::SearchResult {
            request_id: request_id.to_string(),
            hits: self.hits,
            total: self.total,
            done: self.done,
            scanned_to_seq: self.scanned_to_seq,
        }
        .to_json()
    }
}

fn is_search_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}')
}

/// Every run of ASCII whitespace (space, tab, LF, CR, FF, VT) = one space,
/// trimmed.
pub fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if is_search_space(ch) {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

/// The fold, per code point: NFD, marks (General_Category M) dropped,
/// lowercased. Returns the folded chars and, per folded char, the index of
/// the code point of `text` it came from.
pub fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut source = Vec::with_capacity(text.len());
    for (index, ch) in text.chars().enumerate() {
        decompose_canonical(ch, |part| {
            if is_combining_mark(part) {
                return;
            }
            for lower in part.to_lowercase() {
                chars.push(lower);
                source.push(index);
            }
        });
    }
    (chars, source)
}

/// The searchable text of one event, its hit kind and the row it lands on.
fn searchable<'e>(
    event: &'e ActivityEvent,
    seq: u64,
    tool_rows: &HashMap<&str, u64>,
) -> Option<(&'static str, std::borrow::Cow<'e, str>, u64)> {
    match event {
        ActivityEvent::UserMessage { text, .. } => Some(("user_message", text.as_str().into(), seq)),
        ActivityEvent::Narration { text, .. } => Some(("narration", text.as_str().into(), seq)),
        ActivityEvent::Tool { name, detail, .. } => {
            let title = match detail.as_deref().filter(|detail| !detail.is_empty()) {
                Some(detail) => format!("{name} {detail}").into(),
                None => name.as_str().into(),
            };
            Some(("tool", title, seq))
        }
        ActivityEvent::ToolUpdate { id, output, diff, .. } => {
            let text = output.as_deref().or(diff.as_deref())?;
            let row = *tool_rows.get(id.as_str())?;
            Some(("tool_output", text.into(), row))
        }
        _ => None,
    }
}

/// The first match of the folded `query` in `text` (already collapsed), as a
/// hit body (snippet, offsets).
fn first_match(text: &str, query: &[char]) -> Option<(String, usize, usize)> {
    let (folded, source) = fold(text);
    if query.is_empty() || folded.len() < query.len() {
        return None;
    }
    let at = (0..=folded.len() - query.len()).find(|&i| folded[i..i + query.len()] == *query)?;
    let start = source[at];
    let end = source[at + query.len() - 1] + 1;
    let chars: Vec<char> = text.chars().collect();
    let from = start.saturating_sub(SNIPPET_LEAD_CHARS);
    let to = chars.len().min((from + SNIPPET_CHARS).max(end));
    let mut snippet = String::new();
    let mut lead = 0;
    if from > 0 {
        snippet.push('…');
        lead = 1;
    }
    snippet.extend(&chars[from..to]);
    if to < chars.len() {
        snippet.push('…');
    }
    Some((snippet, start - from + lead, end - start))
}

/// The SEARCH rule (steer-enet.json): `events` = the journal as read
/// (oldest first, with seqs); newest first below `before_seq`, at most
/// `limit` (clamped to [1, [`HITS_MAX`]]) hits and `scan_max_bytes` of text.
pub fn search<E: std::borrow::Borrow<ActivityEvent>>(
    events: &[(u64, E)],
    query: &str,
    before_seq: Option<u64>,
    limit: usize,
    scan_max_bytes: usize,
) -> SearchOutcome {
    let query: String = collapse(query).chars().take(QUERY_MAX_CHARS).collect();
    let (query, _) = fold(&query);
    if query.is_empty() {
        return SearchOutcome::empty();
    }
    let limit = limit.clamp(1, HITS_MAX);
    let mut tool_rows: HashMap<&str, u64> = HashMap::new();
    for (seq, event) in events {
        if let ActivityEvent::Tool { id: Some(id), .. } = event.borrow() {
            let row = tool_rows.entry(id.as_str()).or_insert(*seq);
            *row = (*row).min(*seq);
        }
    }
    // Newest first BY SEQ: a journal read lists its latest-wins slots after
    // the log, at their own (older or newer) seqs.
    let mut candidates: Vec<&(u64, E)> =
        events.iter().filter(|(seq, _)| before_seq.map_or(true, |before| *seq < before)).collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let mut hits = Vec::new();
    let mut scanned = 0usize;
    let mut scanned_to_seq = 0;
    for (index, (seq, event)) in candidates.iter().enumerate() {
        scanned_to_seq = *seq;
        if let Some((kind, text, row)) = searchable(event.borrow(), *seq, &tool_rows) {
            let text = collapse(&text);
            scanned += text.len();
            if let Some((snippet, match_start, match_len)) = first_match(&text, &query) {
                hits.push(SearchHit { seq: row, kind: kind.to_string(), snippet, match_start, match_len });
            }
        }
        if (hits.len() >= limit || scanned >= scan_max_bytes) && index + 1 < candidates.len() {
            return SearchOutcome { hits, total: None, done: false, scanned_to_seq };
        }
    }
    let total = before_seq.is_none().then_some(hits.len());
    SearchOutcome { hits, total, done: true, scanned_to_seq }
}

/// One `search` ask answered off the session's journal FILE (the live
/// publisher's and STORED REPLAY's): no journal here = no hits, done.
pub fn answer(data_dir: Option<&Path>, session_id: &str, request_id: &str, query: &str, before_seq: Option<u64>, limit: u64) -> String {
    let events = data_dir.and_then(|dir| crate::history::read_journal_seq(dir, session_id));
    let outcome = match events {
        Some(events) => search(&events, query, before_seq, limit.min(HITS_MAX as u64) as usize, SCAN_MAX_BYTES),
        None => SearchOutcome::empty(),
    };
    outcome.frame(request_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json");

    fn spec() -> Value {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        fixture["search"].clone()
    }

    #[test]
    fn constants_match_the_contract() {
        let f = spec();
        assert_eq!(f["queryMaxChars"], QUERY_MAX_CHARS as u64);
        assert_eq!(f["hitsMax"], HITS_MAX as u64);
        assert_eq!(f["scanMaxBytes"], SCAN_MAX_BYTES as u64);
        assert_eq!(f["snippetChars"], SNIPPET_CHARS as u64);
        assert_eq!(f["snippetLeadChars"], SNIPPET_LEAD_CHARS as u64);
    }

    /// Every `search.cases` entry, through the device's own rule, and its
    /// `search_result` frame body.
    #[test]
    fn search_cases() {
        for case in spec()["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let events: Vec<(u64, ActivityEvent)> = case["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| (entry["seq"].as_u64().unwrap(), serde_json::from_value(entry["event"].clone()).unwrap()))
                .collect();
            let outcome = search(
                &events,
                case["query"].as_str().unwrap(),
                case["beforeSeq"].as_u64(),
                case["limit"].as_u64().unwrap() as usize,
                case["scanMaxBytes"].as_u64().map_or(SCAN_MAX_BYTES, |max| max as usize),
            );
            let mut frame: Value = serde_json::from_str(&outcome.frame("r1")).unwrap();
            assert_eq!(frame["t"], "search_result", "{name}");
            assert_eq!(frame["requestId"], "r1", "{name}");
            let object = frame.as_object_mut().unwrap();
            object.remove("t");
            object.remove("requestId");
            assert_eq!(frame, case["expect"], "{name}");
        }
    }

    /// The device side end to end: a journal file on disk, searched by
    /// [`answer`]; no journal = an empty, done answer.
    #[test]
    fn answers_off_the_journal_file() {
        let dir = std::env::temp_dir().join(format!("ede96-search-{}", uuid::Uuid::new_v4()));
        let journal = crate::history::journal_path(&dir, "sess-1").unwrap();
        std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
        let lines = [
            ActivityEvent::UserMessage { text: "Fix the café".into(), subagent_id: None, at: Some(1) },
            ActivityEvent::Narration {
                text: "Looking at the CAFE menu".into(),
                before_question_id: None,
                message_id: Some("m1".into()),
                subagent_id: None,
                at: Some(2),
            },
        ];
        let body: String = lines.iter().map(|event| format!("{}\n", serde_json::to_string(event).unwrap())).collect();
        std::fs::write(&journal, body).unwrap();

        let frame: Value = serde_json::from_str(&answer(Some(&dir), "sess-1", "q1", "cafe", None, 10)).unwrap();
        assert_eq!(frame["requestId"], "q1");
        assert_eq!(frame["done"], true);
        assert_eq!(frame["total"], 2);
        let kinds: Vec<&str> = frame["hits"].as_array().unwrap().iter().map(|hit| hit["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["narration", "user_message"]);

        let none: Value = serde_json::from_str(&answer(Some(&dir), "sess-2", "q2", "cafe", None, 10)).unwrap();
        assert_eq!(none["hits"], serde_json::json!([]));
        assert_eq!(none["done"], true);
        assert_eq!(none["scannedToSeq"], 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
