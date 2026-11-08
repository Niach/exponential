//! #EXP-1710 — THE MERGE TRAIN on the clients (`packages/domain-contract/
//! fixtures/merge-train.json`, its `$comment` is the spec; TS reference
//! `packages/domain-contract/src/merge-train.ts`): the chat header's merge
//! STATUS pill ([`status_pill`], [`ordinal`]), the header `…`'s rows
//! ([`merge_menu_entries`]), the Needs you row of a stuck PR ([`stuck_row`])
//! and the `pr_merge` event lines ([`event_line`]). Pure: the IDE feeds it the
//! synced `pull_requests.merge_*` columns. The cases replay the fixture by
//! path (no registry: registering it breaks no leg).

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::contract;
use crate::rows::PullRequestRow;

const MERGE_TRAIN_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/merge-train.json");

#[derive(Debug, Clone, Deserialize)]
pub struct MergeTrainLimits {
    pub bounces: i64,
    #[serde(rename = "offlineHours")]
    pub offline_hours: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PillCopy {
    pub draft: String,
    pub checks_running: String,
    pub session_working: String,
    pub queued: String,
    pub queued_at: String,
    pub testing: String,
    pub testing_with: String,
    pub merged: String,
    pub conflict: String,
    pub failing: String,
    pub on_hold: String,
    pub stuck: String,
    pub needs_approval: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuCopy {
    pub merge_now: String,
    pub hold: String,
    pub resume: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouCopy {
    pub title: String,
    pub bounces: String,
    pub timeout: String,
    pub offline: String,
    pub retry: String,
    pub merge_now: String,
    pub close: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsCopy {
    pub label: String,
    pub auto: String,
    pub approval: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeTrainCopy {
    pub pill: PillCopy,
    pub menu: MenuCopy,
    #[serde(rename = "needsYou")]
    pub needs_you: NeedsYouCopy,
    pub settings: SettingsCopy,
    /// `pr_merge` lines by outcome (merged | conflict | failing | held).
    pub events: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTrainTestIds {
    pub status_pill: String,
    pub merge_now: String,
    pub hold: String,
    pub resume: String,
    pub stuck_row: String,
    pub settings_row: String,
}

/// `fixtures/merge-train.json` as far as the clients read it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTrainSpec {
    pub limits: MergeTrainLimits,
    pub copy: MergeTrainCopy,
    pub test_ids: MergeTrainTestIds,
}

pub static MERGE_TRAIN: LazyLock<MergeTrainSpec> =
    LazyLock::new(|| serde_json::from_str(MERGE_TRAIN_JSON).expect("merge-train.json parses"));

/// The settings row's label for a `teams.merge_mode` value (contract
/// `teamMergeMode`); an unknown value reads as Automatic (the default).
pub fn merge_mode_label(mode: &str) -> &'static str {
    let copy = &MERGE_TRAIN.copy.settings;
    if mode == contract::TEAM_MERGE_MODE_APPROVAL {
        copy.approval.as_str()
    } else {
        copy.auto.as_str()
    }
}

/// A PR's merge-train state as the pill, the `…` and Needs you read it
/// (web `prMergeState`): a merged or draft PR is that, whatever the train
/// last wrote; a row without the column reads `checks`.
pub fn pr_merge_state(pull: &PullRequestRow) -> &str {
    match pull.state.as_deref() {
        Some(contract::PR_STATE_MERGED) => contract::PR_MERGE_STATE_MERGED,
        Some(contract::PR_STATE_DRAFT) => contract::PR_MERGE_STATE_DRAFT,
        _ => pull.merge_state.as_deref().filter(|s| !s.is_empty()).unwrap_or(contract::PR_MERGE_STATE_CHECKS),
    }
}

// ── THE STATUS PILL ────────────────────────────────────────────────────────

/// The pill's glyph (icons.json concepts; `Spinner` = THE Spinner).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillGlyph {
    Spinner,
    PrDraft,
    CiQueued,
    PrMerged,
    PrConflict,
    CiFailed,
    PrHold,
}

impl PillGlyph {
    pub fn id(self) -> &'static str {
        match self {
            Self::Spinner => "spinner",
            Self::PrDraft => "pr-draft",
            Self::CiQueued => "ci-queued",
            Self::PrMerged => "pr-merged",
            Self::PrConflict => "pr-conflict",
            Self::CiFailed => "ci-failed",
            Self::PrHold => "pr-hold",
        }
    }
}

/// The glyph's tint (red tints the glyph only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillTone {
    Muted,
    Red,
}

impl PillTone {
    pub fn id(self) -> &'static str {
        match self {
            Self::Muted => "muted",
            Self::Red => "red",
        }
    }
}

/// What a tap does: `Approve` = the merge confirm, then the queue;
/// `Checks` = the Checks list. `None` on the view = readonly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillTap {
    Approve,
    Checks,
}

impl PillTap {
    pub fn id(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Checks => "checks",
        }
    }
}

/// Which confirm `Needs approval` asks: the plain merge or the stack one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillConfirm {
    Merge,
    Stack,
}

impl PillConfirm {
    pub fn id(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Stack => "stack",
        }
    }
}

/// [`status_pill`]'s input (merge-train.json `statusPill[].input`).
#[derive(Clone, Copy, Debug, Default)]
pub struct StatusPillInput<'a> {
    /// contract `prMergeState` ([`pr_merge_state`]).
    pub state: &'a str,
    /// `merge_wait`: what a not-yet-queued PR waits for.
    pub wait: Option<&'a str>,
    /// `merge_position`: the 1-based queue place.
    pub position: Option<i64>,
    /// The PRs of the testing batch, this one included.
    pub batch_size: Option<i64>,
    /// `merge_hold`: person | bounces | timeout.
    pub hold_reason: Option<&'a str>,
    /// The PR sits in an open stack.
    pub stack: bool,
    /// The PR's check rows (none = readonly).
    pub checks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusPillView {
    pub label: String,
    pub glyph: PillGlyph,
    pub tone: PillTone,
    pub primary: bool,
    pub tap: Option<PillTap>,
    pub confirm: Option<PillConfirm>,
}

/// `1st 2nd 3rd 4th 11th 12th 13th 21st`.
pub fn ordinal(n: i64) -> String {
    let rem100 = n % 100;
    if (11..=13).contains(&rem100) {
        return format!("{n}th");
    }
    let suffix = match n % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// `statusPill`: THE header pill of a PR's merge status. Not primary but
/// `Needs approval` (approval mode, ready, nobody approved), the only action.
pub fn status_pill(input: StatusPillInput<'_>) -> StatusPillView {
    let copy = &MERGE_TRAIN.copy.pill;
    let status = |label: String, glyph: PillGlyph, tone: PillTone| StatusPillView {
        label,
        glyph,
        tone,
        primary: false,
        tap: (input.checks > 0).then_some(PillTap::Checks),
        confirm: None,
    };
    use PillGlyph as G;
    use PillTone::{Muted, Red};
    match input.state {
        contract::PR_MERGE_STATE_DRAFT => status(copy.draft.clone(), G::PrDraft, Muted),
        contract::PR_MERGE_STATE_QUEUED => {
            let label = match input.position.filter(|p| *p > 0) {
                Some(position) => copy.queued_at.replace("{position}", &ordinal(position)),
                None => copy.queued.clone(),
            };
            status(label, G::CiQueued, Muted)
        }
        contract::PR_MERGE_STATE_TESTING => {
            let others = input.batch_size.unwrap_or(1) - 1;
            let label = if others > 0 {
                copy.testing_with.replace("{count}", &others.to_string())
            } else {
                copy.testing.clone()
            };
            status(label, G::Spinner, Muted)
        }
        contract::PR_MERGE_STATE_MERGED => status(copy.merged.clone(), G::PrMerged, Muted),
        contract::PR_MERGE_STATE_CONFLICT => status(copy.conflict.clone(), G::PrConflict, Red),
        contract::PR_MERGE_STATE_FAILING => status(copy.failing.clone(), G::CiFailed, Red),
        contract::PR_MERGE_STATE_HELD => {
            if input.hold_reason == Some("person") {
                status(copy.on_hold.clone(), G::PrHold, Muted)
            } else {
                status(copy.stuck.clone(), G::PrHold, Red)
            }
        }
        // `checks` (and any state a newer server adds).
        _ => {
            if input.wait == Some("approval") {
                return StatusPillView {
                    label: copy.needs_approval.clone(),
                    glyph: G::PrMerged,
                    tone: Muted,
                    primary: true,
                    tap: Some(PillTap::Approve),
                    confirm: Some(if input.stack { PillConfirm::Stack } else { PillConfirm::Merge }),
                };
            }
            let label = if input.wait == Some("session") {
                copy.session_working.clone()
            } else {
                copy.checks_running.clone()
            };
            status(label, G::Spinner, Muted)
        }
    }
}

// ── THE `…` MENU ───────────────────────────────────────────────────────────

/// One train row of the header `…`, in menu order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeMenuEntry {
    Checks,
    MergeNow,
    Hold,
    Resume,
    ClosePr,
}

impl MergeMenuEntry {
    pub fn id(self) -> &'static str {
        match self {
            Self::Checks => "checks",
            Self::MergeNow => "merge-now",
            Self::Hold => "hold",
            Self::Resume => "resume",
            Self::ClosePr => "close-pr",
        }
    }
}

/// `mergeMenuEntries` (the chat header's `…`): the
/// Checks submenu (any viewer, the PR has rows), Merge now (owners; a live
/// PR not draft, merged or conflict), Hold | Resume auto-merge (members; a
/// live PR not draft or merged: Resume while held), Close PR (members, live
/// PR). `state` = contract `prState`, None = no PR.
pub fn merge_menu_entries(
    state: Option<&str>,
    member: bool,
    owner: bool,
    checks: usize,
    merge_state: Option<&str>,
) -> Vec<MergeMenuEntry> {
    let Some(state) = state else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    if checks > 0 {
        entries.push(MergeMenuEntry::Checks);
    }
    let live = state == contract::PR_STATE_OPEN || state == contract::PR_STATE_DRAFT;
    if !live || !member {
        return entries;
    }
    let ms = merge_state.unwrap_or(contract::PR_MERGE_STATE_CHECKS);
    let settled = ms == contract::PR_MERGE_STATE_DRAFT || ms == contract::PR_MERGE_STATE_MERGED;
    if owner && !settled && ms != contract::PR_MERGE_STATE_CONFLICT {
        entries.push(MergeMenuEntry::MergeNow);
    }
    if !settled {
        entries.push(if ms == contract::PR_MERGE_STATE_HELD { MergeMenuEntry::Resume } else { MergeMenuEntry::Hold });
    }
    entries.push(MergeMenuEntry::ClosePr);
    entries
}

// ── NEEDS YOU ──────────────────────────────────────────────────────────────

/// A stuck row's actions (THE Button sm row under its meta).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StuckAction {
    Retry,
    MergeNow,
    ClosePr,
}

impl StuckAction {
    pub fn id(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::MergeNow => "merge-now",
            Self::ClosePr => "close-pr",
        }
    }

    /// The button's label (`copy.needsYou`).
    pub fn label(self) -> &'static str {
        let copy = &MERGE_TRAIN.copy.needs_you;
        match self {
            Self::Retry => copy.retry.as_str(),
            Self::MergeNow => copy.merge_now.as_str(),
            Self::ClosePr => copy.close.as_str(),
        }
    }
}

/// The opener's device as [`offline_since`] reads it: THE presence every
/// client shows, and its row's last heartbeat (None = unknown).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfflineDevice {
    pub online: bool,
    pub last_seen_at_ms: Option<i64>,
}

/// `offlineSince` (#EXP-1710): [`stuck_row`]'s `device_offline_since_ms`.
/// `opener` = the PR has an opener (its `session_id` session, else its root)
/// with a device; `device` = that device's synced row (None = none synced);
/// `updated_at_ms` = the PR's last write (the move to conflict/failing).
/// None while the device is online or there is none to wait for; else its
/// last heartbeat, and an UNKNOWN last-seen = the PR's `updated_at` (never
/// online). Times are UTC milliseconds.
pub fn offline_since(opener: bool, device: Option<OfflineDevice>, updated_at_ms: i64) -> Option<i64> {
    if !opener || device.is_some_and(|device| device.online) {
        return None;
    }
    Some(device.and_then(|device| device.last_seen_at_ms).unwrap_or(updated_at_ms))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StuckRow {
    pub title: String,
    pub meta: String,
    pub actions: Vec<StuckAction>,
}

/// `stuckRow`: the Needs you row of a stuck PR, or None: `held` by the
/// limits, or a `conflict`/`failing` whose opener's device is offline
/// `limits.offlineHours`+ (`device_offline_since_ms`, None while online). A
/// person's Hold is never a row. Times are UTC milliseconds.
pub fn stuck_row(
    number: i64,
    state: &str,
    hold_reason: Option<&str>,
    device_offline_since_ms: Option<i64>,
    now_ms: i64,
    owner: bool,
) -> Option<StuckRow> {
    let spec = &*MERGE_TRAIN;
    let copy = &spec.copy.needs_you;
    let held = state == contract::PR_MERGE_STATE_HELD;
    let meta = if held && hold_reason == Some("bounces") {
        copy.bounces.replace("{count}", &spec.limits.bounces.to_string())
    } else if held && hold_reason == Some("timeout") {
        copy.timeout.clone()
    } else if (state == contract::PR_MERGE_STATE_CONFLICT || state == contract::PR_MERGE_STATE_FAILING)
        && device_offline_since_ms.is_some_and(|since| now_ms - since >= spec.limits.offline_hours * 3_600_000)
    {
        copy.offline.clone()
    } else {
        return None;
    };
    Some(StuckRow {
        title: copy.title.replace("{number}", &number.to_string()),
        meta,
        actions: if owner {
            vec![StuckAction::Retry, StuckAction::MergeNow, StuckAction::ClosePr]
        } else {
            vec![StuckAction::Retry, StuckAction::ClosePr]
        },
    })
}

// ── pr_merge LINES ─────────────────────────────────────────────────────────

/// A `pr_merge` event card's template by its `mergeOutcome` (`copy.events`);
/// None = unknown or missing (the caller keeps session-chat's fallback).
pub fn event_line(outcome: Option<&str>) -> Option<&'static str> {
    MERGE_TRAIN.copy.events.get(outcome?).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(MERGE_TRAIN_JSON).expect("merge-train.json parses")
    }

    fn ms(value: &Value) -> Option<i64> {
        value.as_str().and_then(crate::instant::parse_instant)
    }

    #[test]
    fn the_copy_and_test_ids_are_the_fixtures() {
        let fixture = fixture();
        let spec = &*MERGE_TRAIN;
        assert_eq!(fixture["testIds"]["statusPill"], spec.test_ids.status_pill.as_str());
        assert_eq!(fixture["testIds"]["settingsRow"], "settings-merge-mode");
        assert_eq!(fixture["copy"]["menu"]["mergeNow"], spec.copy.menu.merge_now.as_str());
        assert_eq!(fixture["copy"]["settings"]["label"], spec.copy.settings.label.as_str());
        assert_eq!(merge_mode_label("approval"), "After approval");
        assert_eq!(merge_mode_label("auto"), "Automatic");
        assert_eq!(merge_mode_label("future"), "Automatic");
        assert_eq!(fixture["mergeNowPrompt"]["id"], crate::prompts::MERGE_NOW_PR.id);
    }

    #[test]
    fn ordinals() {
        let cases = [(1, "1st"), (2, "2nd"), (3, "3rd"), (4, "4th"), (11, "11th"), (12, "12th"), (13, "13th"), (21, "21st"), (23, "23rd"), (111, "111th")];
        for (n, expected) in cases {
            assert_eq!(ordinal(n), expected);
        }
    }

    #[test]
    fn status_pill_matches_the_fixture() {
        let cases = fixture()["statusPill"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let view = status_pill(StatusPillInput {
                state: input["state"].as_str().unwrap(),
                wait: input["wait"].as_str(),
                position: input["position"].as_i64(),
                batch_size: input["batchSize"].as_i64(),
                hold_reason: input["holdReason"].as_str(),
                stack: input["stack"].as_bool().unwrap(),
                checks: input["checks"].as_u64().unwrap() as usize,
            });
            let expected = &case["expected"];
            let name = case["name"].as_str().unwrap();
            assert_eq!(view.label, expected["label"].as_str().unwrap(), "{name}");
            assert_eq!(view.glyph.id(), expected["glyph"].as_str().unwrap(), "{name}");
            assert_eq!(view.tone.id(), expected["tone"].as_str().unwrap(), "{name}");
            assert_eq!(view.primary, expected["primary"].as_bool().unwrap(), "{name}");
            assert_eq!(view.tap.map(PillTap::id), expected["tap"].as_str(), "{name}");
            assert_eq!(view.confirm.map(PillConfirm::id), expected.get("confirm").and_then(Value::as_str), "{name}");
        }
    }

    #[test]
    fn the_menu_matches_the_fixture() {
        let cases = fixture()["menu"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let ids: Vec<&str> = merge_menu_entries(
                input["state"].as_str(),
                input["member"].as_bool().unwrap(),
                input["owner"].as_bool().unwrap(),
                input["checks"].as_u64().unwrap() as usize,
                input["mergeState"].as_str(),
            )
            .into_iter()
            .map(MergeMenuEntry::id)
            .collect();
            let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn the_stuck_row_matches_the_fixture() {
        let cases = fixture()["stuckRow"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let row = stuck_row(
                input["number"].as_i64().unwrap(),
                input["state"].as_str().unwrap(),
                input["holdReason"].as_str(),
                ms(&input["deviceOfflineSince"]),
                ms(&input["now"]).unwrap(),
                input["owner"].as_bool().unwrap(),
            );
            let got = row.map(|row| {
                serde_json::json!({
                    "title": row.title,
                    "meta": row.meta,
                    "actions": row.actions.iter().map(|a| a.id()).collect::<Vec<_>>(),
                })
            });
            assert_eq!(got.unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn offline_since_matches_the_fixture() {
        let cases = fixture()["offlineSince"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let device = input["device"].as_object().map(|device| OfflineDevice {
                online: device["online"].as_bool().unwrap(),
                last_seen_at_ms: ms(&device["lastSeenAt"]),
            });
            let got = offline_since(input["opener"].as_bool().unwrap(), device, ms(&input["updatedAt"]).unwrap());
            assert_eq!(got, ms(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn event_lines_read_by_outcome() {
        let fixture = fixture();
        for (outcome, text) in fixture["copy"]["events"].as_object().unwrap() {
            assert_eq!(event_line(Some(outcome)), text.as_str());
        }
        assert_eq!(event_line(Some("future")), None);
        assert_eq!(event_line(None), None);
    }

    #[test]
    fn a_merged_or_draft_pr_wins_over_the_train() {
        let row = |state: &str, merge_state: Option<&str>| -> PullRequestRow {
            serde_json::from_value(serde_json::json!({"id": "p", "state": state, "merge_state": merge_state})).unwrap()
        };
        assert_eq!(pr_merge_state(&row("merged", Some("queued"))), "merged");
        assert_eq!(pr_merge_state(&row("draft", Some("queued"))), "draft");
        assert_eq!(pr_merge_state(&row("open", Some("held"))), "held");
        assert_eq!(pr_merge_state(&row("open", None)), "checks");
    }
}
