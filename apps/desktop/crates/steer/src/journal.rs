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
//! a chatty worktree cannot evict the whole transcript.

use crate::frames::ActivityEvent;

/// Event-count cap — mirrors the relay's `ACTIVITY_LOG_CAP`.
pub const JOURNAL_EVENT_CAP: usize = 2000;
/// Byte budget over the serialized events — mirrors `ACTIVITY_BYTE_CAP`.
pub const JOURNAL_BYTE_CAP: usize = 4 * 1024 * 1024;

struct Entry {
    event: ActivityEvent,
    bytes: usize,
    /// Question id while the card is still answerable (pin key).
    pinned_question: Option<String>,
}

/// The replay buffer described in the module docs.
#[derive(Default)]
pub struct ActivityJournal {
    entries: Vec<Entry>,
    bytes: usize,
}

impl ActivityJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one published event.
    pub fn push(&mut self, event: ActivityEvent) {
        match &event {
            ActivityEvent::Diff { .. } => {
                if let Some(pos) = self
                    .entries
                    .iter()
                    .position(|entry| matches!(entry.event, ActivityEvent::Diff { .. }))
                {
                    self.bytes -= self.entries[pos].bytes;
                    self.entries.remove(pos);
                }
            }
            ActivityEvent::QuestionResolved { id, ask_id, .. } => {
                self.unpin(id.as_deref(), ask_id.as_deref());
            }
            _ => {}
        }
        // A re-emitted question REPLACES its earlier card IN PLACE (the
        // options grew) — live clients upsert by id without moving the card
        // (EXP-483), so a replay must keep the same position too; moving it
        // to the tail would reorder it behind later narration.
        let re_emitted = match &event {
            ActivityEvent::Question { id: Some(id), .. } => self
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
                pinned_question,
            };
            self.evict();
            return;
        }
        let bytes = serialized_bytes(&event);
        let pinned_question = match &event {
            ActivityEvent::Question { id, .. } => id.clone(),
            _ => None,
        };
        self.entries.push(Entry {
            event,
            bytes,
            pinned_question,
        });
        self.bytes += bytes;
        self.evict();
    }

    /// The history to re-publish, oldest first.
    pub fn replay(&self) -> impl Iterator<Item = &ActivityEvent> {
        self.entries.iter().map(|entry| &entry.event)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

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

    fn evict(&mut self) {
        while self.entries.len() > JOURNAL_EVENT_CAP
            || (self.bytes > JOURNAL_BYTE_CAP && self.entries.len() > 1)
        {
            let Some(pos) = self
                .entries
                .iter()
                .position(|entry| entry.pinned_question.is_none())
            else {
                return; // only live question cards left — never drop those
            };
            self.bytes -= self.entries[pos].bytes;
            self.entries.remove(pos);
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
    use crate::frames::QuestionOption;

    fn question(id: &str) -> ActivityEvent {
        ActivityEvent::Question {
            text: "pick".to_string(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: Some(id.to_string()),
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
        assert_eq!(journal.len(), 2);
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
            id: Some("toolu_1#0".to_string()),
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
