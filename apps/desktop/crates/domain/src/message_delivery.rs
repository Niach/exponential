//! #FEED-237: a person's OWN message, WhatsApp-style: one bubble at the
//! bottom of the transcript from the press, its delivery mark (the clock, one
//! check, two checks, the red mark) and its menu. Locked by
//! `packages/domain-contract/fixtures/message-delivery.json` (its `$comment`
//! holds the rules); mirrors TS `src/message-delivery.ts`, iOS
//! `MessageDelivery.swift`, Android `MessageDelivery.kt`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::instant::parse_instant;

/// How far before an unacked entry's press its synced row may be stamped and
/// still match it by text (`matchWindowMs`).
pub const MATCH_WINDOW_MS: i64 = 60_000;
/// The mark's size beside the bubble's `HH:mm` (`markSize`).
pub const MARK_SIZE: f32 = 14.0;
/// The mark's test id (`testIds.status`).
pub const TEST_ID_STATUS: &str = "message-status";
/// The bubble menu's test id (`testIds.menu`).
pub const TEST_ID_MENU: &str = "message-menu";

/// genUI's `MessageStatus`: the clock, one check, two checks, the red mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeliveryState {
    Sending,
    Sent,
    Read,
    Failed,
}

/// The mark's ink (`marks[state].tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkTone {
    /// The time's ink.
    Muted,
    /// The read blue.
    Info,
    /// Destructive.
    Danger,
}

impl DeliveryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Read => "read",
            Self::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "sending" => Self::Sending,
            "sent" => Self::Sent,
            "read" => Self::Read,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// The mark's accessibility label (`copy.status[state]`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Sending => "Sending",
            Self::Sent => "Sent",
            Self::Read => "Read",
            Self::Failed => "Not sent",
        }
    }

    /// The mark's icon concept (`marks[state].icon`).
    pub fn icon_concept(self) -> &'static str {
        match self {
            Self::Sending => "ui-clock",
            Self::Sent => "ui-check",
            Self::Read => "ui-read",
            Self::Failed => "ui-error",
        }
    }

    pub fn tone(self) -> MarkTone {
        match self {
            Self::Sending | Self::Sent => MarkTone::Muted,
            Self::Read => MarkTone::Info,
            Self::Failed => MarkTone::Danger,
        }
    }
}

/// An outbox entry's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalState {
    Sending,
    Failed,
}

/// `deliveryState`'s input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeliveryInput {
    /// The outbox entry's state; None = a synced row alone.
    pub local: Option<LocalState>,
    /// The server holds the row (an entry's messageId, or a synced row).
    pub stored: bool,
    pub delivered_at: Option<String>,
    pub consumed_at: Option<String>,
    /// The bubble IS the live transcript's echo of the turn.
    pub echoed: bool,
}

/// First match wins: echoed / consumed = read, delivered = sent, a failed
/// unstored entry = failed, anything else = the clock.
pub fn delivery_state(input: &DeliveryInput) -> DeliveryState {
    let set = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    if input.echoed || set(&input.consumed_at) {
        return DeliveryState::Read;
    }
    if set(&input.delivered_at) {
        return DeliveryState::Sent;
    }
    if input.local == Some(LocalState::Failed) && !input.stored {
        return DeliveryState::Failed;
    }
    DeliveryState::Sending
}

/// One press of a session composer's Send, persisted on the device until its
/// synced row takes its place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxEntry {
    /// A fresh UUID per press; also the `sessions.send` `clientMessageId`.
    pub client_id: String,
    pub text: String,
    /// The press (RFC 3339).
    pub created_at: String,
    pub state: LocalState,
    /// The row the server stored it as (None until the send answers).
    #[serde(default)]
    pub message_id: Option<String>,
}

/// The slice of a synced message row the merge reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxRow {
    pub id: String,
    pub author_kind: String,
    pub author_id: Option<String>,
    pub body: String,
    pub created_at: String,
}

/// `outboxMerge`'s answer: the entries a synced row replaced and the ones
/// that still draw (oldest first), as clientIds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutboxMerge {
    pub drop: Vec<String>,
    pub keep: Vec<String>,
}

fn millis(text: &str) -> i64 {
    parse_instant(text).unwrap_or(0)
}

/// The entries oldest first (ties by clientId).
pub fn sort_entries(entries: &mut [OutboxEntry]) {
    entries.sort_by(|a, b| millis(&a.created_at).cmp(&millis(&b.created_at)).then_with(|| a.client_id.cmp(&b.client_id)));
}

pub fn outbox_merge(viewer_id: &str, outbox: &[OutboxEntry], rows: &[OutboxRow]) -> OutboxMerge {
    let mut entries = outbox.to_vec();
    sort_entries(&mut entries);
    let row_ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let mut matched: HashSet<String> = HashSet::new();
    let mut dropped: HashSet<String> = HashSet::new();
    // By id first: an acked entry's row is its row, whatever its text.
    for entry in &entries {
        if let Some(id) = entry.message_id.as_deref() {
            if row_ids.contains(id) {
                dropped.insert(entry.client_id.clone());
                matched.insert(id.to_string());
            }
        }
    }
    let mut own: Vec<&OutboxRow> = rows
        .iter()
        .filter(|row| row.author_kind == "user" && row.author_id.as_deref() == Some(viewer_id))
        .collect();
    own.sort_by(|a, b| millis(&a.created_at).cmp(&millis(&b.created_at)).then_with(|| a.id.cmp(&b.id)));
    for entry in &entries {
        if dropped.contains(&entry.client_id) || entry.message_id.is_some() || entry.state == LocalState::Failed {
            continue;
        }
        let from = millis(&entry.created_at) - MATCH_WINDOW_MS;
        let Some(row) = own
            .iter()
            .find(|row| !matched.contains(&row.id) && row.body == entry.text && millis(&row.created_at) >= from)
        else {
            continue;
        };
        matched.insert(row.id.clone());
        dropped.insert(entry.client_id.clone());
    }
    let (drop, keep): (Vec<&OutboxEntry>, Vec<&OutboxEntry>) =
        entries.iter().partition(|entry| dropped.contains(&entry.client_id));
    OutboxMerge {
        drop: drop.into_iter().map(|entry| entry.client_id.clone()).collect(),
        keep: keep.into_iter().map(|entry| entry.client_id.clone()).collect(),
    }
}

/// A bubble menu item (`copy.menu`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleAction {
    Retry,
    Delete,
    Cancel,
}

impl BubbleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Delete => "delete",
            Self::Cancel => "cancel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Retry => "Retry",
            Self::Delete => "Delete",
            Self::Cancel => "Cancel sending",
        }
    }
}

/// The toast when the server refuses a cancel (the device already took it).
pub const CANCEL_FAILED_TOAST: &str = "Couldn't remove the message";

/// The bubble's menu, in order (empty = no menu).
pub fn bubble_actions(state: DeliveryState, stored: bool) -> Vec<BubbleAction> {
    match state {
        DeliveryState::Failed => vec![BubbleAction::Retry, BubbleAction::Delete],
        DeliveryState::Read => Vec::new(),
        _ if stored => vec![BubbleAction::Cancel],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    const FIXTURE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/message-delivery.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct StateInput {
        local: Option<String>,
        stored: bool,
        delivered_at: Option<String>,
        consumed_at: Option<String>,
        echoed: bool,
    }

    #[derive(Deserialize)]
    struct StateCase {
        name: String,
        input: StateInput,
        expected: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        id: String,
        author_kind: String,
        author_id: Option<String>,
        body: String,
        created_at: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OutboxInput {
        viewer_id: String,
        outbox: Vec<OutboxEntry>,
        rows: Vec<Row>,
    }

    #[derive(Deserialize)]
    struct OutboxExpected {
        drop: Vec<String>,
        keep: Vec<String>,
    }

    #[derive(Deserialize)]
    struct OutboxCase {
        name: String,
        input: OutboxInput,
        expected: OutboxExpected,
    }

    #[derive(Deserialize)]
    struct ActionInput {
        state: String,
        stored: bool,
    }

    #[derive(Deserialize)]
    struct ActionCase {
        name: String,
        input: ActionInput,
        expected: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Mark {
        icon: String,
        tone: String,
    }

    #[derive(Deserialize)]
    struct Copy {
        status: BTreeMap<String, String>,
        menu: BTreeMap<String, String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TestIds {
        status: String,
        menu: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        copy: Copy,
        marks: BTreeMap<String, Mark>,
        mark_size: f32,
        match_window_ms: i64,
        test_ids: TestIds,
        state_cases: Vec<StateCase>,
        outbox_cases: Vec<OutboxCase>,
        action_cases: Vec<ActionCase>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE_JSON).expect("message-delivery.json parses")
    }

    fn state(text: &str) -> DeliveryState {
        DeliveryState::parse(text).unwrap_or_else(|| panic!("unknown state {text}"))
    }

    #[test]
    fn constants_and_copy_match_the_fixture() {
        let fixture = fixture();
        assert_eq!(fixture.match_window_ms, MATCH_WINDOW_MS);
        assert_eq!(fixture.mark_size, MARK_SIZE);
        assert_eq!(fixture.test_ids.status, TEST_ID_STATUS);
        assert_eq!(fixture.test_ids.menu, TEST_ID_MENU);
        for (key, label) in &fixture.copy.status {
            assert_eq!(state(key).label(), label, "copy.status.{key}");
        }
        for (key, mark) in &fixture.marks {
            let state = state(key);
            assert_eq!(state.icon_concept(), mark.icon, "marks.{key}.icon");
            let tone = match mark.tone.as_str() {
                "muted" => MarkTone::Muted,
                "info" => MarkTone::Info,
                "danger" => MarkTone::Danger,
                other => panic!("unknown tone {other}"),
            };
            assert_eq!(state.tone(), tone, "marks.{key}.tone");
        }
        for action in [BubbleAction::Retry, BubbleAction::Delete, BubbleAction::Cancel] {
            assert_eq!(fixture.copy.menu.get(action.as_str()).map(String::as_str), Some(action.label()));
        }
        assert!(fixture_comment_names_the_toast());
    }

    fn fixture_comment_names_the_toast() -> bool {
        FIXTURE_JSON.contains(CANCEL_FAILED_TOAST)
    }

    #[test]
    fn replays_every_state_case() {
        let cases = fixture().state_cases;
        assert!(!cases.is_empty());
        for case in cases {
            let local = case.input.local.as_deref().map(|text| match text {
                "sending" => LocalState::Sending,
                "failed" => LocalState::Failed,
                other => panic!("{}: unknown local {other}", case.name),
            });
            let input = DeliveryInput {
                local,
                stored: case.input.stored,
                delivered_at: case.input.delivered_at,
                consumed_at: case.input.consumed_at,
                echoed: case.input.echoed,
            };
            assert_eq!(delivery_state(&input), state(&case.expected), "{}", case.name);
        }
    }

    #[test]
    fn replays_every_outbox_case() {
        let cases = fixture().outbox_cases;
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<OutboxRow> = case
                .input
                .rows
                .into_iter()
                .map(|row| OutboxRow {
                    id: row.id,
                    author_kind: row.author_kind,
                    author_id: row.author_id,
                    body: row.body,
                    created_at: row.created_at,
                })
                .collect();
            let merged = outbox_merge(&case.input.viewer_id, &case.input.outbox, &rows);
            assert_eq!(merged.drop, case.expected.drop, "{} (drop)", case.name);
            assert_eq!(merged.keep, case.expected.keep, "{} (keep)", case.name);
        }
    }

    #[test]
    fn replays_every_action_case() {
        let cases = fixture().action_cases;
        assert!(!cases.is_empty());
        for case in cases {
            let actions: Vec<&str> =
                bubble_actions(state(&case.input.state), case.input.stored).into_iter().map(BubbleAction::as_str).collect();
            assert_eq!(actions, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn an_entry_round_trips_through_its_persisted_json() {
        let entry = OutboxEntry {
            client_id: "c1".into(),
            text: "ship it".into(),
            created_at: "2026-11-13T10:00:00.000Z".into(),
            state: LocalState::Sending,
            message_id: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"clientId\":\"c1\"") && json.contains("\"state\":\"sending\""));
        assert_eq!(serde_json::from_str::<OutboxEntry>(&json).unwrap(), entry);
    }
}
