//! #EDE-129 FIND IN CHAT (`packages/domain-contract/src/find-in-chat.ts`'s
//! twin; the IDE and eDE share it): the `…` entry, the query, the stops in
//! reading order, the counter, the steps, keeping the current stop while
//! answers arrive, what a landing does, the status line and the keys.
//! Byte-locked by `fixtures/find-in-chat.json` (its `$comment` is the spec);
//! pure.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const FIND_IN_CHAT_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/find-in-chat.json");

/// The fixture's `copy`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindInChatCopy {
    pub menu: String,
    pub placeholder: String,
    pub counter: String,
    pub counter_compact: String,
    pub no_results: String,
    pub searching_older: String,
    pub searching_device: String,
    pub device_offline: String,
    pub output_hint_one: String,
    pub output_hint_many: String,
    pub previous: String,
    pub next: String,
    pub close: String,
}

/// The fixture's `testIds`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindInChatTestIds {
    pub bar: String,
    pub input: String,
    pub counter: String,
    pub previous: String,
    pub next: String,
    pub close: String,
    pub status: String,
    pub output_hint: String,
}

/// The fixture's `shortcut` hints.
#[derive(Debug, Deserialize)]
pub struct FindInChatShortcut {
    pub mac: String,
    pub other: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    query_max_chars: usize,
    hits_max: usize,
    debounce_ms: u64,
    copy: FindInChatCopy,
    shortcut: FindInChatShortcut,
    test_ids: FindInChatTestIds,
}

static FIXTURE: LazyLock<Fixture> =
    LazyLock::new(|| serde_json::from_str(FIND_IN_CHAT_JSON).expect("find-in-chat.json parses"));

pub fn copy() -> &'static FindInChatCopy {
    &FIXTURE.copy
}

pub fn test_ids() -> &'static FindInChatTestIds {
    &FIXTURE.test_ids
}

/// `hitsMax`: what one store / device search asks for.
pub fn hits_max() -> usize {
    FIXTURE.hits_max
}

/// `debounceMs`: typing re-searches after this.
pub fn debounce_ms() -> u64 {
    FIXTURE.debounce_ms
}

/// Whether a screen kind has a transcript to find in.
pub fn has_transcript(kind: &str) -> bool {
    matches!(kind, "session" | "agent" | "assistant")
}

/// THE `…` with `search` first in its own group (an app: unchanged).
pub fn menu<E: Clone>(kind: &str, entries: &[E], search: E, separator: E) -> Vec<E> {
    if !has_transcript(kind) {
        return entries.to_vec();
    }
    let mut out = vec![search];
    if !entries.is_empty() {
        out.push(separator);
        out.extend(entries.iter().cloned());
    }
    out
}

/// The query as the chat store takes it: ASCII whitespace runs = one space,
/// trimmed, the first `queryMaxChars` code points.
pub fn query(raw: &str) -> String {
    let collapsed = raw
        .split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c' | '\x0b'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.chars().take(FIXTURE.query_max_chars).collect()
}

/// A loaded row, top to bottom. `count` = its painted highlight count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRow {
    pub key: String,
    pub count: usize,
    #[serde(default)]
    pub group: Option<String>,
    /// A tool row whose output matches: one more stop, ringing the row.
    #[serde(default)]
    pub output_hit: bool,
}

/// An older hit (store, then device), newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FindOlderHit {
    pub key: String,
    #[serde(default)]
    pub output: bool,
}

/// #EXP-1998: a store hit as [`older`] reads it (an event's carries its
/// `seq`, a synced message's its `message_id`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindStoreHit {
    pub session_id: String,
    #[serde(default)]
    pub seq: Option<i64>,
    #[serde(default)]
    pub message_id: Option<String>,
    pub kind: String,
}

/// #EXP-1998: a device's hit (always the shown session's event).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FindDeviceHit {
    pub seq: i64,
    pub kind: String,
}

/// #EXP-1998: the synced messages the host draws as bubbles (`None` = a
/// live transcript, which holds them as its own rows): `loaded` = every
/// loaded synced message of the session, `drawable` = those it would draw
/// with no cap, `more` = older ones exist unloaded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FindSynced {
    pub loaded: Vec<String>,
    pub drawable: Vec<String>,
    pub more: bool,
}

/// #EXP-1998 (`olderCases`): the store's and the device's hits OLDER than
/// the loaded rows, store first, first key wins.
pub fn older(session_id: &str, lowest_seq: Option<i64>, store: &[FindStoreHit], device: &[FindDeviceHit], synced: Option<&FindSynced>) -> Vec<FindOlderHit> {
    let loaded: HashSet<&str> = synced.map(|s| s.loaded.iter().map(String::as_str).collect()).unwrap_or_default();
    let drawable: HashSet<&str> = synced.map(|s| s.drawable.iter().map(String::as_str).collect()).unwrap_or_default();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |key: String, output: bool| {
        if seen.insert(key.clone()) {
            out.push(FindOlderHit { key, output });
        }
    };
    let below = |seq: i64| lowest_seq.is_none_or(|lowest| seq < lowest);
    for hit in store.iter().filter(|hit| hit.session_id == session_id) {
        if let Some(seq) = hit.seq {
            if below(seq) {
                push(event_key(session_id, seq), hit.kind == "tool_output");
            }
        } else if let (Some(id), Some(synced)) = (hit.message_id.as_deref(), synced) {
            if if loaded.contains(id) { drawable.contains(id) } else { synced.more } {
                push(message_key(id), false);
            }
        }
    }
    for hit in device.iter().filter(|hit| below(hit.seq)) {
        push(event_key(session_id, hit.seq), hit.kind == "tool_output");
    }
    out
}

/// One stop of the counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindStop {
    pub key: String,
    pub local: usize,
    pub loaded: bool,
    pub group: Option<String>,
    pub output: bool,
}

/// Every stop in reading order (newest last).
pub fn stops(rows: &[FindRow], older: &[FindOlderHit]) -> Vec<FindStop> {
    let loaded: HashSet<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    let mut seen = HashSet::new();
    let mut out: Vec<FindStop> = Vec::new();
    for hit in older {
        if loaded.contains(hit.key.as_str()) || !seen.insert(hit.key.as_str()) {
            continue;
        }
        out.push(FindStop { key: hit.key.clone(), local: 0, loaded: false, group: None, output: hit.output });
    }
    out.reverse();
    for row in rows {
        for local in 0..row.count {
            out.push(FindStop { key: row.key.clone(), local, loaded: true, group: row.group.clone(), output: false });
        }
        if row.output_hit {
            out.push(FindStop { key: row.key.clone(), local: 0, loaded: true, group: row.group.clone(), output: true });
        }
    }
    out
}

/// The counter text (`""` = none shown). `current` is 0-based.
pub fn counter(current: Option<usize>, total: usize, done: bool, compact: bool) -> String {
    let Some(current) = current.filter(|_| total > 0) else {
        return if done && total == 0 { FIXTURE.copy.no_results.clone() } else { String::new() };
    };
    let template = if compact { &FIXTURE.copy.counter_compact } else { &FIXTURE.copy.counter };
    template.replace("{n}", &(current + 1).to_string()).replace("{m}", &total.to_string())
}

/// A step through the stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindStep {
    Start,
    Previous,
    Next,
}

impl FindStep {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "start" => Some(Self::Start),
            "previous" => Some(Self::Previous),
            "next" => Some(Self::Next),
            _ => None,
        }
    }
}

/// The stop after a step (wraps); `Start` = the newest.
pub fn step(current: Option<usize>, total: usize, step: FindStep) -> Option<usize> {
    if total == 0 {
        return None;
    }
    let Some(current) = current.filter(|_| step != FindStep::Start) else {
        return Some(total - 1);
    };
    Some(match step {
        FindStep::Previous => (current + total - 1) % total,
        _ => (current + 1) % total,
    })
}

/// The same stop in the new list (same key, local, output), else the newest.
pub fn keep(before: &[FindStop], current: Option<usize>, after: &[FindStop]) -> Option<usize> {
    if after.is_empty() {
        return None;
    }
    if let Some(was) = current.and_then(|current| before.get(current)) {
        if let Some(at) = after.iter().position(|s| s.key == was.key && s.local == was.local && s.output == was.output) {
            return Some(at);
        }
    }
    Some(after.len() - 1)
}

/// What landing on a stop does. `current` = the row's `highlight.current`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindLanding {
    pub open_group: Option<String>,
    pub page_in: bool,
    /// Always `centre`.
    pub scroll: &'static str,
    pub ring: bool,
    pub current: Option<usize>,
}

pub fn land(stop: &FindStop) -> FindLanding {
    if !stop.loaded {
        return FindLanding { open_group: None, page_in: true, scroll: "centre", ring: true, current: None };
    }
    FindLanding {
        open_group: stop.group.clone(),
        page_in: false,
        scroll: "centre",
        ring: stop.output,
        current: (!stop.output).then_some(stop.local),
    }
}

/// Where one search (the store, the device) is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FindSearchPhase {
    #[default]
    None,
    Searching,
    Done,
    Offline,
}

impl FindSearchPhase {
    pub fn parse(word: &str) -> Self {
        match word {
            "searching" => Self::Searching,
            "done" => Self::Done,
            "offline" => Self::Offline,
            _ => Self::None,
        }
    }
}

/// The status line under the bar (`None` = none).
pub fn status(store: FindSearchPhase, device: FindSearchPhase, device_name: &str) -> Option<String> {
    match (store, device) {
        (_, FindSearchPhase::Offline) => Some(FIXTURE.copy.device_offline.replace("{device}", device_name)),
        (_, FindSearchPhase::Searching) => Some(FIXTURE.copy.searching_device.replace("{device}", device_name)),
        (FindSearchPhase::Searching, _) => Some(FIXTURE.copy.searching_older.clone()),
        _ => None,
    }
}

/// What a key press does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindKeyAction {
    Find,
    Global,
    Previous,
    Next,
    Close,
}

impl FindKeyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Global => "global",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Close => "close",
        }
    }
}

/// A key press: `mod_` = Cmd on Apple, else Ctrl. `key` is a single
/// character or a DOM-style name (`Enter`, `Escape`).
pub fn key(key: &str, mod_: bool, shift: bool, in_chat: bool, in_field: bool) -> Option<FindKeyAction> {
    let key = if key.chars().count() == 1 { key.to_lowercase() } else { key.to_string() };
    if mod_ && key == "k" {
        return Some(FindKeyAction::Global);
    }
    if mod_ && key == "f" {
        return Some(if in_chat { FindKeyAction::Find } else { FindKeyAction::Global });
    }
    if !in_field || mod_ {
        return None;
    }
    match key.as_str() {
        "Enter" => Some(if shift { FindKeyAction::Next } else { FindKeyAction::Previous }),
        "Escape" => Some(FindKeyAction::Close),
        _ => None,
    }
}

/// The tool row's hint beside its title (`n` = its output's matches).
pub fn output_hint(n: usize) -> String {
    if n == 1 {
        FIXTURE.copy.output_hint_one.clone()
    } else {
        FIXTURE.copy.output_hint_many.replace("{n}", &n.to_string())
    }
}

/// The menu's shortcut hint (phones show none).
pub fn shortcut(apple: bool) -> &'static str {
    if apple {
        &FIXTURE.shortcut.mac
    } else {
        &FIXTURE.shortcut.other
    }
}

/// An event hit's key (seq = the tool row's for output).
pub fn event_key(session_id: &str, seq: i64) -> String {
    format!("e:{session_id}:{seq}")
}

/// A synced message's key.
pub fn message_key(message_id: &str) -> String {
    format!("m:{message_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(FIND_IN_CHAT_JSON).unwrap()
    }

    fn cases(name: &str) -> Vec<Value> {
        let cases = fixture()[name].as_array().unwrap().clone();
        assert!(!cases.is_empty(), "{name}");
        cases
    }

    fn index(value: &Value) -> Option<usize> {
        value.as_u64().map(|n| n as usize)
    }

    fn stops_of(value: &Value) -> Vec<FindStop> {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn menu_cases() {
        for case in cases("menuCases") {
            let entries: Vec<String> = serde_json::from_value(case["entries"].clone()).unwrap();
            let got = menu(case["kind"].as_str().unwrap(), &entries, "search".to_string(), "separator".to_string());
            assert_eq!(serde_json::json!(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn query_cases() {
        for case in cases("queryCases") {
            assert_eq!(query(case["raw"].as_str().unwrap()), case["query"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn match_cases() {
        for case in cases("matchCases") {
            let rows: Vec<FindRow> = serde_json::from_value(case["rows"].clone()).unwrap();
            let older: Vec<FindOlderHit> = serde_json::from_value(case["older"].clone()).unwrap();
            assert_eq!(stops(&rows, &older), stops_of(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn older_cases() {
        for case in cases("olderCases") {
            let store: Vec<FindStoreHit> = serde_json::from_value(case["store"].clone()).unwrap();
            let device: Vec<FindDeviceHit> = serde_json::from_value(case["device"].clone()).unwrap();
            let synced: Option<FindSynced> = serde_json::from_value(case["synced"].clone()).unwrap();
            let expected: Vec<FindOlderHit> = serde_json::from_value(case["expected"].clone()).unwrap();
            let got = older(case["sessionId"].as_str().unwrap(), case["lowestSeq"].as_i64(), &store, &device, synced.as_ref());
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn counter_cases() {
        for case in cases("counterCases") {
            let got = counter(
                index(&case["current"]),
                index(&case["total"]).unwrap(),
                case["done"].as_bool().unwrap(),
                case["compact"].as_bool().unwrap(),
            );
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn step_cases() {
        for case in cases("stepCases") {
            let kind = FindStep::parse(case["step"].as_str().unwrap()).unwrap();
            let got = step(index(&case["current"]), index(&case["total"]).unwrap(), kind);
            assert_eq!(got, index(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn result_cases() {
        for case in cases("resultCases") {
            let got = keep(&stops_of(&case["before"]), index(&case["current"]), &stops_of(&case["after"]));
            assert_eq!(got, index(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn land_cases() {
        for case in cases("landCases") {
            let stop: FindStop = serde_json::from_value(case["match"].clone()).unwrap();
            assert_eq!(serde_json::to_value(land(&stop)).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn status_cases() {
        for case in cases("statusCases") {
            let input = &case["input"];
            let got = status(
                FindSearchPhase::parse(input["store"].as_str().unwrap()),
                FindSearchPhase::parse(input["device"].as_str().unwrap()),
                input["deviceName"].as_str().unwrap(),
            );
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn key_cases() {
        for case in cases("keyCases") {
            let input = &case["input"];
            let got = key(
                input["key"].as_str().unwrap(),
                input["mod"].as_bool().unwrap(),
                input["shift"].as_bool().unwrap(),
                input["inChat"].as_bool().unwrap(),
                input["inField"].as_bool().unwrap(),
            );
            assert_eq!(got.map(FindKeyAction::as_str), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn copy_and_hints_read_the_fixture() {
        assert_eq!(copy().menu, "Search in chat");
        assert_eq!(test_ids().bar, "find-in-chat");
        assert_eq!(shortcut(true), "⌘F");
        assert_eq!(shortcut(false), "Ctrl F");
        assert_eq!(output_hint(1), "1 match in output");
        assert_eq!(output_hint(3), "3 matches in output");
        assert_eq!(event_key("s1", 4), "e:s1:4");
        assert_eq!(message_key("a1"), "m:a1");
        assert_eq!(hits_max(), 50);
        assert_eq!(debounce_ms(), 150);
    }
}
