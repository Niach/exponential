//! The publisher's activity journal (EXP-249) — the session's full published
//! history, replayed after every (re)connect.
//!
//! The removed PTY mirror had a 256 KiB byte ring the relay asked for with
//! `resync`. Its replacement is semantic: the publisher keeps every activity
//! event it ever sent and, on each new socket, sends `activity_reset` +
//! the whole journal before resuming live. A viewer that joins mid-session (or
//! after a relay restart) therefore sees the session from its first event —
//! the relay's own log is only a cache in front of this one.
//!
//! Bounds mirror the relay's room caps exactly ([`JOURNAL_EVENT_CAP`] /
//! [`JOURNAL_BYTE_CAP`]) so a replay can never exceed what the relay will
//! hold. Eviction is oldest-first with one exception: an unresolved
//! [`ActivityEvent::Question`] is PINNED — dropping the card a steerer is
//! looking at would make the session unanswerable, and there are never more
//! than a handful pending.
//!
//! Diffs replace rather than append (the relay keeps only the latest too), so
//! a chatty worktree cannot evict the whole transcript. EXP-746 put
//! `config_state` and `usage` under the same rule — they are latest-wins
//! STATE on every client, not transcript rows.
//!
//! EXP-758: those three live in their own SLOTS, outside `entries` and outside
//! both budgets, exactly like the relay's `room.lastByKind` (hub.ts) and the
//! engine's LocalFeed. Keeping them as tail entries was a slow leak: a run
//! that never toggles model or mode publishes ONE `config_state`, at index 0,
//! and after 2000 events it is the FIRST row evicted — so the next publisher
//! reconnect (relay restart, the 90 s idle watchdog) replays a history with no
//! config in it and every viewer nulls the slot: chips, mode switcher and `/`
//! commands gone for the rest of the run. Replay therefore yields the log
//! first and then the slots in the relay's own `LATEST_REPLAY_ORDER`
//! (`config_state`, `usage`, `diff`).
//!
//! EXP-748 adds the other pressure valve, the relay's `hub.ts` rule mirrored
//! here: a SUBAGENT's tool calls are second-class transcript. One fan-out of
//! parallel agents publishes thousands of them, and every client collapses
//! them to a "N tool calls" caption anyway, so they must never push the main
//! line (the user's messages, the narration, the questions) out of a replay.
//! Two tiers: each subagent keeps at most [`JOURNAL_SUBAGENT_TOOL_CAP`] tool
//! entries, and once the journal is over its count cap the OLDEST subagent
//! tool entry goes before any main-line row does. The byte budget stays the
//! plain oldest-first hard bound underneath both.

use std::collections::HashMap;

use crate::frames::ActivityEvent;

/// Event-count cap — mirrors the relay's `ACTIVITY_LOG_CAP`.
pub const JOURNAL_EVENT_CAP: usize = 2000;
/// Byte budget over the serialized events — mirrors `ACTIVITY_BYTE_CAP`.
pub const JOURNAL_BYTE_CAP: usize = 4 * 1024 * 1024;
/// How many tool entries ONE subagent may hold — mirrors the relay's
/// `SUBAGENT_TOOL_CAP` (EXP-748). Past it the subagent drops its own oldest
/// call rather than anything else's.
pub const JOURNAL_SUBAGENT_TOOL_CAP: usize = 50;

struct Entry {
    event: ActivityEvent,
    bytes: usize,
    /// EXP-783: the publisher's monotonic index for this event, carried onto
    /// the wire so a viewer can splice a replay onto a prefix it already has.
    seq: Option<u64>,
    /// Question id while the card is still answerable (pin key).
    pinned_question: Option<String>,
    /// EXP-748: the subagent this tool call was attributed to — the
    /// second-class-transcript key. `None` on every main-line entry.
    subagent_tool: Option<String>,
}

/// EXP-758: the latest-wins slots, in the relay's replay order
/// (`LATEST_REPLAY_ORDER` in hub.ts: `config_state`, `usage`, `rate_limit`,
/// `diff` — the diff stays LAST, where it replayed before any of this became
/// a map). EXP-784 added `rate_limit` beside `usage`.
const SLOT_CONFIG_STATE: usize = 0;
const SLOT_USAGE: usize = 1;
const SLOT_RATE_LIMIT: usize = 2;
const SLOT_DIFF: usize = 3;
const SLOT_COUNT: usize = 4;

/// Which slot an event owns, if any. The ONE place the four latest-wins
/// kinds are named.
fn slot_of(event: &ActivityEvent) -> Option<usize> {
    match event {
        ActivityEvent::ConfigState { .. } => Some(SLOT_CONFIG_STATE),
        ActivityEvent::Usage { .. } => Some(SLOT_USAGE),
        ActivityEvent::RateLimit { .. } => Some(SLOT_RATE_LIMIT),
        ActivityEvent::Diff { .. } => Some(SLOT_DIFF),
        _ => None,
    }
}

/// The replay buffer described in the module docs.
#[derive(Default)]
pub struct ActivityJournal {
    entries: Vec<Entry>,
    /// EXP-758: latest-wins STATE by kind, held OUTSIDE `entries`, `len()` and
    /// `bytes()` so eviction can never reach it. Each payload is already
    /// capped upstream (a diff at 512 KiB, a config at 8 options), so three
    /// slots are a bounded overhead on top of the budgets, not a hole in them.
    slots: [Option<ActivityEvent>; SLOT_COUNT],
    /// EXP-783: the seq of whatever currently occupies each slot.
    slot_seqs: [Option<u64>; SLOT_COUNT],
    bytes: usize,
    /// Live tool-entry count per subagent (an id with none left is removed).
    subagent_tool_counts: HashMap<String, usize>,
    /// How many entries carry a `subagent_tool` — the "is there one to drop"
    /// test the count tier asks on every push.
    subagent_tool_entries: usize,
    /// Scan hint: an index at or below the OLDEST subagent tool entry, so a
    /// long main-line head is not re-walked on every eviction.
    subagent_scan_from: usize,
}

impl ActivityJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one published event with no wire sequence (tests, and any
    /// caller that does not number its stream).
    #[cfg(test)]
    pub fn push(&mut self, event: ActivityEvent) {
        self.push_seq(None, event);
    }

    /// Record one published event under its EXP-783 wire sequence.
    pub fn push_seq(&mut self, seq: Option<u64>, event: ActivityEvent) {
        // EXP-758: latest-wins STATE, exactly like the relay's per-kind slots
        // — the newest snapshot DROPS ITS PREDECESSOR into its own slot and
        // never enters `entries` at all. Position among the log is irrelevant
        // (every client treats all three as slots, not ordered rows); what
        // matters is that a chatty worktree, a per-turn usage meter (EXP-746)
        // or a mode toggle can neither evict the transcript out of the
        // 2000-event / 4 MiB budget nor be evicted BY it.
        if let Some(slot) = slot_of(&event) {
            self.slots[slot] = Some(event);
            self.slot_seqs[slot] = seq;
            return;
        }
        if let ActivityEvent::QuestionResolved { id, ask_id, .. } = &event {
            self.unpin(id.as_deref(), ask_id.as_deref());
        }
        // A re-emitted question REPLACES its earlier card IN PLACE (the
        // options grew) — live clients upsert by id without moving the card
        // (EXP-483), so a replay must keep the same position too; moving it
        // to the tail would reorder it behind later narration.
        let re_emitted = match &event {
            ActivityEvent::Question { id, .. } => self
                .entries
                .iter()
                .position(|entry| entry.pinned_question.as_deref() == Some(id.as_str())),
            _ => None,
        };
        if let Some(pos) = re_emitted {
            let bytes = serialized_bytes(&event);
            self.bytes = self.bytes + bytes - self.entries[pos].bytes;
            let pinned_question = self.entries[pos].pinned_question.clone();
            self.entries[pos] = Entry {
                event,
                bytes,
                seq,
                pinned_question,
                subagent_tool: None,
            };
            self.evict();
            return;
        }
        let bytes = serialized_bytes(&event);
        let pinned_question = match &event {
            ActivityEvent::Question { id, .. } => Some(id.clone()),
            _ => None,
        };
        // EXP-748: a tool call the agent attributed to a subagent — the only
        // kind of entry the two-tier rule may drop early.
        let subagent_tool = match &event {
            ActivityEvent::Tool {
                id: None,
                tool_kind: None,
                subagent_id: Some(id),
                ..
            }
            // EXP-773: prose a subagent wrote renders INSIDE that subagent's
            // card, so it is second class exactly like the calls around it.
            | ActivityEvent::Narration {
                subagent_id: Some(id),
                ..
            } if !id.is_empty() => Some(id.clone()),
            _ => None,
        };
        self.entries.push(Entry {
            event,
            bytes,
            seq,
            pinned_question,
            subagent_tool: subagent_tool.clone(),
        });
        self.bytes += bytes;
        if let Some(subagent_id) = subagent_tool {
            if self.subagent_tool_entries == 0 {
                self.subagent_scan_from = self.entries.len() - 1;
            }
            self.subagent_tool_entries += 1;
            let held = self
                .subagent_tool_counts
                .entry(subagent_id.clone())
                .or_insert(0);
            *held += 1;
            // Tier one: this subagent alone is over its share — it drops its
            // OWN oldest call, so a runaway fan-out never costs anyone else a
            // row (nor the main line one).
            while self
                .subagent_tool_counts
                .get(&subagent_id)
                .is_some_and(|held| *held > JOURNAL_SUBAGENT_TOOL_CAP)
            {
                let Some(pos) = self
                    .entries
                    .iter()
                    .position(|entry| entry.subagent_tool.as_deref() == Some(subagent_id.as_str()))
                else {
                    break;
                };
                self.remove_entry(pos);
            }
        }
        self.evict();
    }

    /// The history to re-publish: the log oldest first, then the latest-wins
    /// slots in the relay's `LATEST_REPLAY_ORDER` (EXP-758) so a reconnecting
    /// publisher hands a joining viewer the same shape the relay's own join
    /// replay does.
    pub fn replay(&self) -> impl Iterator<Item = &ActivityEvent> {
        self.replay_seq().map(|(_, event)| event)
    }

    /// EXP-783: the replay with each event's wire sequence beside it — what a
    /// re-publish sends so a viewer can splice rather than swap.
    pub fn replay_seq(&self) -> impl Iterator<Item = (Option<u64>, &ActivityEvent)> {
        self.entries
            .iter()
            .map(|entry| (entry.seq, &entry.event))
            .chain(
                self.slots
                    .iter()
                    .zip(self.slot_seqs.iter())
                    .filter_map(|(event, seq)| event.as_ref().map(|event| (*seq, event))),
            )
    }

    /// LOG entries only — the latest-wins slots are outside the count budget
    /// (EXP-758), so a journal holding nothing but slots still replays three
    /// events at `len() == 0`.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// LOG bytes only — see [`ActivityJournal::len`].
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    fn unpin(&mut self, id: Option<&str>, ask_id: Option<&str>) {
        for entry in &mut self.entries {
            let Some(pinned) = entry.pinned_question.as_deref() else {
                continue;
            };
            let matched = match (id, ask_id) {
                (Some(id), _) => pinned == id,
                // Retire-by-ask: every card of the ask, including its
                // `<askId>#submit` step.
                (None, Some(ask)) => pinned == ask || pinned.starts_with(&format!("{ask}#")),
                (None, None) => false,
            };
            if matched {
                entry.pinned_question = None;
            }
        }
    }

    /// Drop one entry, keeping every index-, byte- and subagent-count book
    /// exact. The ONE removal path — nothing else touches `entries`.
    fn remove_entry(&mut self, pos: usize) {
        let entry = self.entries.remove(pos);
        self.bytes -= entry.bytes;
        if let Some(subagent_id) = entry.subagent_tool {
            self.subagent_tool_entries -= 1;
            if let Some(held) = self.subagent_tool_counts.get_mut(&subagent_id) {
                *held -= 1;
                if *held == 0 {
                    self.subagent_tool_counts.remove(&subagent_id);
                }
            }
        }
        // The hint is a LOWER bound: everything after `pos` shifted down one.
        if pos < self.subagent_scan_from {
            self.subagent_scan_from -= 1;
        }
        self.subagent_scan_from = self.subagent_scan_from.min(self.entries.len());
    }

    /// The oldest second-class row: a subagent's tool call. Advances the scan
    /// hint past the main-line head it just walked.
    fn oldest_subagent_tool(&mut self) -> Option<usize> {
        if self.subagent_tool_entries == 0 {
            return None;
        }
        let from = self.subagent_scan_from.min(self.entries.len());
        let found = self.entries[from..]
            .iter()
            .position(|entry| entry.subagent_tool.is_some())
            .map(|offset| from + offset)
            // A stale hint (never observed, but the bound is only ever
            // maintained downwards) falls back to the full scan.
            .or_else(|| {
                self.entries
                    .iter()
                    .position(|entry| entry.subagent_tool.is_some())
            })?;
        self.subagent_scan_from = found;
        Some(found)
    }

    fn first_unpinned(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.pinned_question.is_none())
    }

    fn evict(&mut self) {
        // Tier two (EXP-748): over the count cap, a subagent's tool call goes
        // before any main-line row — the mobile UIs render it as one line of a
        // "N tool calls" caption, while a dropped user_message/narration/
        // question is history no viewer can ever get back.
        while self.entries.len() > JOURNAL_EVENT_CAP {
            let Some(pos) = self.oldest_subagent_tool().or_else(|| self.first_unpinned()) else {
                return; // only live question cards left — never drop those
            };
            self.remove_entry(pos);
        }
        // The byte budget is the hard bound underneath: plain oldest-first,
        // pinned cards excepted, whatever the row is.
        while self.bytes > JOURNAL_BYTE_CAP && self.entries.len() > 1 {
            let Some(pos) = self.first_unpinned() else {
                return;
            };
            self.remove_entry(pos);
        }
    }
}

/// The event's size on the wire — what the relay's byte budget measures.
fn serialized_bytes(event: &ActivityEvent) -> usize {
    serde_json::to_string(event).map(|json| json.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{ConfigOption, QuestionOption, SubagentStatus};

    /// One tool call attributed to a subagent — second-class transcript.
    fn subagent_tool(subagent_id: &str, detail: &str) -> ActivityEvent {
        ActivityEvent::Tool {
            name: "Read".to_string(),
            detail: Some(detail.to_string()),
            id: None,
            tool_kind: None,
            subagent_id: Some(subagent_id.to_string()),
            at: None,
        }
    }

    fn subagent_marker(id: &str) -> ActivityEvent {
        ActivityEvent::Subagent {
            id: id.to_string(),
            agent_type: "explore".to_string(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        }
    }

    /// What [`ActivityJournal::bytes`] must equal at all times. EXP-758: the
    /// LOG only — the latest-wins slots are deliberately outside the budget,
    /// so this walks `entries` rather than `replay()`.
    fn replayed_bytes(journal: &ActivityJournal) -> usize {
        journal
            .entries
            .iter()
            .map(|entry| serialized_bytes(&entry.event))
            .sum()
    }

    fn config(value: &str) -> ActivityEvent {
        ActivityEvent::ConfigState {
            options: vec![ConfigOption {
                value: Some(value.to_string()),
                ..ConfigOption::new("model", "Model")
            }],
            current_mode: None,
            modes: None,
            commands: None,
            at: None,
        }
    }

    fn tool_details(journal: &ActivityJournal) -> Vec<String> {
        journal
            .replay()
            .filter_map(|event| match event {
                ActivityEvent::Tool { detail, .. } => detail.clone(),
                _ => None,
            })
            .collect()
    }

    fn question(id: &str) -> ActivityEvent {
        ActivityEvent::Question {
            text: "pick".to_string(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: id.to_string(),
            ask_id: id.split_once('#').map(|(ask, _)| ask.to_string()),
            index: None,
            total: None,
            header: None,
            at: None,
        }
    }

    #[test]
    fn replays_in_publication_order() {
        let mut journal = ActivityJournal::new();
        journal.push(ActivityEvent::narration("one"));
        journal.push(ActivityEvent::tool("Edit", Some("a.rs".into())));
        journal.push(ActivityEvent::user_message("go"));
        let replay: Vec<&ActivityEvent> = journal.replay().collect();
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0], &ActivityEvent::narration("one"));
        assert_eq!(replay[2], &ActivityEvent::user_message("go"));
    }

    #[test]
    fn only_the_latest_diff_survives() {
        let mut journal = ActivityJournal::new();
        journal.push(ActivityEvent::diff("--- v1"));
        journal.push(ActivityEvent::narration("between"));
        journal.push(ActivityEvent::diff("--- v2"));
        let diffs: Vec<&ActivityEvent> = journal
            .replay()
            .filter(|event| matches!(event, ActivityEvent::Diff { .. }))
            .collect();
        assert_eq!(diffs, vec![&ActivityEvent::diff("--- v2")]);
        // EXP-758: the diff lives in its own slot, so the LOG holds only the
        // narration.
        assert_eq!(journal.len(), 1);
    }

    /// EXP-746: `config_state` and `usage` follow the diff rule — the newest
    /// drops its predecessor. Without it a per-turn usage meter would evict
    /// the whole transcript out of the replay budget, and a reconnect would
    /// re-publish a truncated history.
    #[test]
    fn only_the_latest_config_state_and_usage_survive() {
        let mut journal = ActivityJournal::new();
        journal.push(config("sonnet"));
        journal.push(ActivityEvent::usage(10, 200, None));
        journal.push(ActivityEvent::narration("between"));
        journal.push(config("opus"));
        journal.push(ActivityEvent::usage(20, 200, Some(0.5)));

        let replay: Vec<&ActivityEvent> = journal.replay().collect();
        // EXP-758: only the narration is a LOG row now; the two state kinds
        // sit in slots outside the count budget.
        assert_eq!(journal.len(), 1, "one narration; the rest are slots");
        assert_eq!(
            replay,
            vec![
                &ActivityEvent::narration("between"),
                &config("opus"),
                &ActivityEvent::usage(20, 200, Some(0.5)),
            ],
            "the newest of each kind replays after the log, config then usage"
        );
    }

    /// EXP-784: `rate_limit` is the fourth slot, replayed between `usage`
    /// and the diff; a `tool_update` is a plain LOG row (EXP-785).
    #[test]
    fn rate_limit_is_a_slot_between_usage_and_diff_and_tool_update_is_a_row() {
        let mut journal = ActivityJournal::new();
        journal.push(ActivityEvent::diff("--- v1"));
        journal.push(ActivityEvent::rate_limit("allowed_warning", None, None));
        journal.push(ActivityEvent::usage(10, 200, None));
        journal.push(ActivityEvent::tool("Edit", None));
        journal.push(ActivityEvent::tool_update(
            "tc-1",
            Some(crate::ToolUpdateStatus::Completed),
            None,
        ));
        journal.push(ActivityEvent::rate_limit("rejected", Some(5), None));

        let replay: Vec<&ActivityEvent> = journal.replay().collect();
        assert_eq!(journal.len(), 2, "the tool row and its update are the log");
        assert_eq!(
            replay,
            vec![
                &ActivityEvent::tool("Edit", None),
                &ActivityEvent::tool_update("tc-1", Some(crate::ToolUpdateStatus::Completed), None),
                &ActivityEvent::usage(10, 200, None),
                &ActivityEvent::rate_limit("rejected", Some(5), None),
                &ActivityEvent::diff("--- v1"),
            ]
        );
    }

    /// EXP-758: the whole point of the slots — a run that publishes ONE
    /// `config_state` at the very start and then 2000 events used to lose it
    /// to the count cap, so the next reconnect replayed a history with no
    /// config in it and every viewer nulled its chips, mode switcher and `/`
    /// commands for the rest of the run.
    #[test]
    fn latest_wins_slots_survive_count_eviction() {
        let mut journal = ActivityJournal::new();
        journal.push(config("opus"));
        journal.push(ActivityEvent::usage(10, 200, None));
        journal.push(ActivityEvent::diff("--- v1"));
        for i in 0..JOURNAL_EVENT_CAP + 500 {
            journal.push(ActivityEvent::narration(format!("line {i}")));
        }

        assert_eq!(journal.len(), JOURNAL_EVENT_CAP);
        let replay: Vec<&ActivityEvent> = journal.replay().collect();
        assert_eq!(
            replay[replay.len() - 3..].to_vec(),
            vec![
                &config("opus"),
                &ActivityEvent::usage(10, 200, None),
                &ActivityEvent::diff("--- v1"),
            ],
            "the log first, then config_state, usage, diff — the relay's own \
             LATEST_REPLAY_ORDER (hub.ts)"
        );
    }

    /// The byte budget is the other eviction path, and it must not reach the
    /// slots either (EXP-758).
    #[test]
    fn latest_wins_slots_survive_byte_eviction() {
        let mut journal = ActivityJournal::new();
        journal.push(config("opus"));
        journal.push(ActivityEvent::usage(10, 200, None));
        journal.push(ActivityEvent::diff("--- v1"));
        let big = "x".repeat(600 * 1024);
        for i in 0..12 {
            journal.push(ActivityEvent::narration(format!("{i}{big}")));
        }

        assert!(journal.bytes() <= JOURNAL_BYTE_CAP, "{}", journal.bytes());
        let replay: Vec<&ActivityEvent> = journal.replay().collect();
        assert_eq!(
            replay[replay.len() - 3..].to_vec(),
            vec![
                &config("opus"),
                &ActivityEvent::usage(10, 200, None),
                &ActivityEvent::diff("--- v1"),
            ],
            "a 4 MiB flood of narration never costs the state slots"
        );
    }

    /// EXP-758: the slots are held OUTSIDE both budgets, like the relay's
    /// `room.lastByKind` and the engine's LocalFeed ring.
    #[test]
    fn latest_wins_slots_are_outside_len_and_bytes() {
        let mut journal = ActivityJournal::new();
        journal.push(config("opus"));
        journal.push(ActivityEvent::usage(10, 200, None));
        journal.push(ActivityEvent::diff("--- v1"));

        assert_eq!(journal.len(), 0);
        assert_eq!(journal.bytes(), 0);
        assert!(journal.is_empty(), "len/bytes count the LOG, not the slots");
        assert_eq!(
            journal.replay().count(),
            3,
            "…and the replay still carries all three"
        );
    }

    #[test]
    fn re_emitted_question_replaces_its_earlier_card_in_place() {
        let mut journal = ActivityJournal::new();
        journal.push(question("toolu_1#0"));
        // Narration lands in BETWEEN emit and re-emit — the card must keep
        // its original position, exactly like the clients' in-place upsert.
        journal.push(ActivityEvent::narration("between"));
        let augmented = ActivityEvent::Question {
            text: "pick".to_string(),
            options: vec![
                QuestionOption::new("Yes", "1"),
                QuestionOption::new("Type something", "2"),
            ],
            multi_select: None,
            plan_mode: None,
            id: "toolu_1#0".to_string(),
            ask_id: Some("toolu_1".to_string()),
            index: None,
            total: None,
            header: None,
            at: None,
        };
        journal.push(augmented.clone());
        assert_eq!(
            journal.replay().collect::<Vec<_>>(),
            vec![&augmented, &ActivityEvent::narration("between")]
        );

        // The replacement stays pinned: resolving it unpins as usual.
        journal.push(ActivityEvent::QuestionResolved {
            id: Some("toolu_1#0".to_string()),
            ask_id: None,
            answers: None,
            dismissed: None,
            at: None,
        });
        for i in 0..JOURNAL_EVENT_CAP + 10 {
            journal.push(ActivityEvent::narration(format!("flood {i}")));
        }
        assert!(
            !journal
                .replay()
                .any(|event| matches!(event, ActivityEvent::Question { .. })),
            "a resolved re-emitted card must evict normally"
        );
    }

    #[test]
    fn pending_questions_survive_count_eviction() {
        let mut journal = ActivityJournal::new();
        journal.push(question("toolu_1#0"));
        for i in 0..JOURNAL_EVENT_CAP + 50 {
            journal.push(ActivityEvent::narration(format!("line {i}")));
        }
        assert_eq!(journal.len(), JOURNAL_EVENT_CAP);
        assert!(
            journal
                .replay()
                .any(|event| matches!(event, ActivityEvent::Question { .. })),
            "an unresolved question card must never be evicted"
        );

        // Once resolved it is ordinary history and evicts like anything else.
        journal.push(ActivityEvent::QuestionResolved {
            id: None,
            ask_id: Some("toolu_1".to_string()),
            answers: Some(vec!["Yes".to_string()]),
            dismissed: None,
            at: None,
        });
        for i in 0..JOURNAL_EVENT_CAP {
            journal.push(ActivityEvent::narration(format!("after {i}")));
        }
        assert!(!journal
            .replay()
            .any(|event| matches!(event, ActivityEvent::Question { .. })));
    }

    /// EXP-748: one fan-out of parallel agents used to publish 2000 tool
    /// calls and leave a rejoining viewer with NOTHING else — no prompt, no
    /// narration, no card. The main line now outlives every one of them.
    #[test]
    fn subagent_tool_calls_evict_before_the_main_transcript() {
        let mut journal = ActivityJournal::new();
        for i in 0..100 {
            journal.push(ActivityEvent::narration(format!("line {i}")));
        }
        journal.push(subagent_marker("a0"));
        for i in 0..2_000 {
            journal.push(subagent_tool(&format!("a{}", i % 8), &format!("call {i}")));
        }

        assert!(journal.len() <= JOURNAL_EVENT_CAP, "{}", journal.len());
        let main_line: Vec<&ActivityEvent> = journal
            .replay()
            .filter(|event| !matches!(event, ActivityEvent::Tool { .. }))
            .collect();
        let expected: Vec<ActivityEvent> = (0..100)
            .map(|i| ActivityEvent::narration(format!("line {i}")))
            .chain(std::iter::once(subagent_marker("a0")))
            .collect();
        assert_eq!(
            main_line,
            expected.iter().collect::<Vec<_>>(),
            "every narration and the subagent marker replay, in order"
        );
        // Each of the eight subagents kept its own tail, nothing more.
        assert_eq!(
            tool_details(&journal).len(),
            8 * JOURNAL_SUBAGENT_TOOL_CAP,
            "tier one bounded every subagent before the count cap was reached"
        );
        assert_eq!(journal.bytes(), replayed_bytes(&journal));
    }

    #[test]
    fn a_subagent_keeps_at_most_fifty_tool_calls() {
        let mut journal = ActivityJournal::new();
        for i in 0..60 {
            journal.push(subagent_tool("a1", &format!("call {i}")));
        }
        assert_eq!(journal.len(), JOURNAL_SUBAGENT_TOOL_CAP);
        let details = tool_details(&journal);
        assert_eq!(details.first().unwrap(), "call 10", "the oldest ten are gone");
        assert_eq!(details.last().unwrap(), "call 59");
        assert_eq!(journal.bytes(), replayed_bytes(&journal));
    }

    #[test]
    fn pinned_questions_survive_subagent_eviction() {
        let mut journal = ActivityJournal::new();
        journal.push(question("toolu_1#0"));
        // 60 subagents keep 50 calls each — 3000 tool entries, well past the
        // count cap, so the second tier runs on them and only them.
        for i in 0..3_000 {
            journal.push(subagent_tool(&format!("a{}", i % 60), &format!("call {i}")));
        }
        assert_eq!(journal.len(), JOURNAL_EVENT_CAP);
        assert_eq!(
            journal.replay().next().unwrap(),
            &question("toolu_1#0"),
            "the unresolved card is still pinned at the head"
        );
        assert_eq!(journal.bytes(), replayed_bytes(&journal));
    }

    #[test]
    fn main_line_eviction_resumes_once_no_subagent_tools_remain() {
        let mut journal = ActivityJournal::new();
        for i in 0..40 {
            journal.push(subagent_tool("a1", &format!("call {i}")));
        }
        for i in 0..JOURNAL_EVENT_CAP + 100 {
            journal.push(ActivityEvent::narration(format!("line {i}")));
        }
        assert_eq!(journal.len(), JOURNAL_EVENT_CAP);
        assert!(
            !journal
                .replay()
                .any(|event| matches!(event, ActivityEvent::Tool { .. })),
            "all 40 subagent calls went first"
        );
        // …and then the oldest narration lines did, as they always have.
        assert_eq!(
            journal.replay().next().unwrap(),
            &ActivityEvent::narration("line 100")
        );
        assert_eq!(journal.bytes(), replayed_bytes(&journal));
    }

    /// The byte book is exact through BOTH tiers plus a latest-wins slot —
    /// a drift there would silently shrink (or blow) the replay budget.
    #[test]
    fn bytes_stay_exact_across_two_tier_eviction() {
        let mut journal = ActivityJournal::new();
        journal.push(question("toolu_1#0"));
        for i in 0..JOURNAL_EVENT_CAP {
            journal.push(ActivityEvent::narration(format!("line {i}")));
            journal.push(subagent_tool(&format!("a{}", i % 5), &format!("call {i}")));
        }
        journal.push(ActivityEvent::diff("--- v1"));
        journal.push(ActivityEvent::diff("--- v2"));

        assert_eq!(journal.len(), JOURNAL_EVENT_CAP);
        assert_eq!(journal.bytes(), replayed_bytes(&journal));
        assert!(journal.bytes() <= JOURNAL_BYTE_CAP);
        assert_eq!(
            journal
                .replay()
                .filter(|event| matches!(event, ActivityEvent::Diff { .. }))
                .collect::<Vec<_>>(),
            vec![&ActivityEvent::diff("--- v2")],
            "the latest-wins slot is untouched by the subagent tier"
        );
    }

    #[test]
    fn byte_budget_evicts_oldest_and_keeps_at_least_one() {
        let mut journal = ActivityJournal::new();
        let big = "x".repeat(600 * 1024);
        for i in 0..12 {
            journal.push(ActivityEvent::narration(format!("{i}{big}")));
        }
        assert!(journal.bytes() <= JOURNAL_BYTE_CAP, "{}", journal.bytes());
        assert!(!journal.is_empty());
        // The survivors are the NEWEST events.
        match journal.replay().last().unwrap() {
            ActivityEvent::Narration { text, .. } => assert!(text.starts_with("11")),
            other => panic!("expected narration, got {other:?}"),
        }
    }
}
