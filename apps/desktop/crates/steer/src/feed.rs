//! The steering VIEWER's feed model (EXP-696) — a PURE, synchronous reducer
//! over [`ActivityEvent`]s, with no tokio, no gpui and no clock of its own.
//!
//! This is the desktop mirror of the web viewer's reducer
//! (`apps/web/src/lib/steer-session-store.ts` `handleActivity` + the pure
//! helpers in `apps/web/src/lib/agent-feed.ts`) and of the native ones
//! (`AgentSessionModel.swift`, `SteerConnection.kt`). [`viewer`] drives it
//! with what comes off the socket; the UI renders [`SteerFeed::items`] (or
//! the [`SteerFeed::rows`] projection) and never decides protocol semantics
//! itself.
//!
//! [`viewer`]: crate::viewer
//!
//! ## The rules that are NOT obvious from the wire
//!
//! * **A reset does not clear anything** (EXP-656). The relay answers every
//!   join with `activity_reset` + a full replay, and a publisher reconnect
//!   fans out the same pair. Clearing on the spot empties the feed under a
//!   reader who is halfway through a plan, and the replay then re-appends
//!   the whole history — the feed collapses and the reader is yanked to the
//!   bottom. So [`SteerFeed::apply_reset`] opens a STAGING window: activity
//!   buffers, the visible feed is frozen, and the whole thing swaps in as ONE
//!   commit on `activity_synced` ([`SteerFeed::apply_synced`]) or on the
//!   caller's quiet/deadline timer ([`SteerFeed::force_swap`]).
//! * **Questions are keyed by wire id, not by position.** A re-emitted
//!   question REPLACES its card in place (the desktop augments the options as
//!   it learns them) and keeps whatever resolution the card already had.
//! * **Narration can arrive after the card it was written before** (EXP-483):
//!   `beforeQuestionId` splices it back above the matching question, and an
//!   unmatched anchor just appends.
//! * **A message this client sent is echoed locally AND comes back** in the
//!   transcript as a `user_message` — the echo FIFO consumes the twin.
//! * **Diffs never enter the feed**: the latest replaces the previous one
//!   behind the pinned "Latest changes" strip, and an EMPTY diff clears it
//!   (EXP-688 — the branch no longer differs).
//!
//! ## Timers belong to the caller
//!
//! Nothing here reads a clock. The answer-ack deadline
//! ([`ANSWER_ACK_TIMEOUT`]), the staged-replay quiet window
//! ([`REPLAY_QUIET`]) and its hard cap ([`REPLAY_MAX`]) are published as
//! constants for the UI to arm; the feed only exposes the state they act on
//! ([`SteerFeed::is_staging`], [`SteerFeed::fail_answer`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use crate::frames::{ActivityEvent, QuestionOption, SubagentStatus};

/// Client-side feed cap — old items fall off the top. Matches the relay's
/// `ACTIVITY_LOG_CAP` so a full-history replay renders in full (web
/// `FEED_CAP`).
pub const FEED_CAP: usize = 2000;

/// At most this many un-matched local echoes are remembered (web `ECHO_CAP`).
pub const ECHO_CAP: usize = 8;

/// No `answer_ack` within this long re-enables a locked card with an inline
/// note. Derived from the desktop publisher's worst-case ack budget
/// (EXP-347): ANSWER_RETRY_TTL 4s + ANSWER_SETTLE 2s + PLAN_SUBMIT_PROBE 0.5s
/// + ~1.5s tick/relay margin. Web `ANSWER_ACK_TIMEOUT_MS` / iOS
/// `answerLockSeconds` / Android parity — move all four in lockstep. The
/// CALLER arms the timer and calls [`SteerFeed::fail_answer`].
pub const ANSWER_ACK_TIMEOUT: Duration = Duration::from_secs(8);

/// EXP-656 staged-replay fallback for a publisher-driven republish that
/// carries no `activity_synced` marker: the replay arrives as one burst, so
/// this much silence means it is over (iOS `replayQuietSeconds`).
pub const REPLAY_QUIET: Duration = Duration::from_millis(400);

/// The hard cap on a staging window — a stalled republish commits what it has
/// (and appends the rest) instead of holding the buffer forever (iOS
/// `replayMaxSeconds`).
pub const REPLAY_MAX: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// Feed items
// ---------------------------------------------------------------------------

/// A feed item's local identity — monotonic per feed, stable across
/// re-emissions of the same question, and preserved across the EXP-656 swap
/// for the unchanged prefix so the UI keeps its row identity (and the
/// reader's scroll anchor).
pub type FeedItemId = u64;

/// One rendered row's payload. `diff` is deliberately absent: diffs are not
/// feed items ([`SteerFeed::latest_diff`]).
#[derive(Clone, Debug, PartialEq)]
pub enum FeedKind {
    Narration {
        text: String,
    },
    Tool {
        name: String,
        detail: Option<String>,
        /// Set when the call came from a subagent's transcript — the row is
        /// nested under that subagent's card.
        subagent_id: Option<String>,
    },
    UserMessage {
        text: String,
    },
    /// Informational only — a permission prompt is never answerable remotely.
    Permission {
        tool: String,
        detail: Option<String>,
    },
    Subagent {
        subagent_id: String,
        agent_type: String,
        status: SubagentStatus,
        detail: Option<String>,
    },
    Question(QuestionCard),
}

/// One item of the visible feed.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedItem {
    pub id: FeedItemId,
    pub kind: FeedKind,
}

impl FeedItem {
    pub fn question(&self) -> Option<&QuestionCard> {
        match &self.kind {
            FeedKind::Question(card) => Some(card),
            _ => None,
        }
    }

    pub fn question_mut(&mut self) -> Option<&mut QuestionCard> {
        match &mut self.kind {
            FeedKind::Question(card) => Some(card),
            _ => None,
        }
    }

    /// The subagent a row belongs to — a tool call attributed to one, or the
    /// subagent's own lifecycle marker.
    pub fn subagent_id(&self) -> Option<&str> {
        match &self.kind {
            FeedKind::Tool { subagent_id, .. } => subagent_id.as_deref(),
            FeedKind::Subagent { subagent_id, .. } => Some(subagent_id.as_str()),
            _ => None,
        }
    }

    pub fn is_tool(&self) -> bool {
        matches!(self.kind, FeedKind::Tool { .. })
    }
}

/// An interactive question card. The wire identity fields are `None` on a
/// LEGACY card (a desktop that publishes no question ids), which is answerable
/// by raw keystroke only and retired positionally.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuestionCard {
    pub text: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
    /// An `ExitPlanMode` plan-approval picker (EXP-97) — a dedicated card.
    pub plan_mode: bool,
    /// The wire id (protocol v2): present ⇒ answerable through the semantic
    /// `answer` frame, and a re-emission replaces the card in place.
    pub question_id: Option<String>,
    /// Groups the steps of one multi-question ask. A card with `ask_id` and
    /// no `index` is that ask's final review/submit step.
    pub ask_id: Option<String>,
    pub index: Option<u32>,
    pub total: Option<u32>,
    pub header: Option<String>,
    /// Set once the question resolved — a resolved card renders its `answer`
    /// (or "Dismissed") and is never active again.
    pub resolved: bool,
    pub answer: Option<String>,
    pub dismissed: bool,
}

/// The key a card's answer state is tracked under: the wire question id when
/// the desktop publishes one, else the local feed id (web `answerKey`).
pub fn answer_key(item: &FeedItem) -> String {
    match item.question().and_then(|card| card.question_id.as_deref()) {
        Some(id) => id.to_string(),
        None => format!("#{}", item.id),
    }
}

// ---------------------------------------------------------------------------
// Answer lock state machine (protocol v2)
// ---------------------------------------------------------------------------

/// Where a submitted answer stands. `Sending` and `Acked` both LOCK the card;
/// `Error` re-enables it with an inline note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerStatus {
    /// Sent, awaiting `answer_ack`.
    Sending,
    /// The desktop confirmed it injected the answer — stay locked until the
    /// matching `question_resolved`.
    Acked,
    /// [`ANSWER_ACK_TIMEOUT`] passed with neither — the card is answerable
    /// again.
    Error,
}

/// What a card's lock renders while it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct AnswerState {
    /// What was sent — option keys for a semantic answer, keystrokes legacy.
    pub keys: Vec<String>,
    /// Option labels, rendered while the card is locked.
    pub labels: Vec<String>,
    pub status: AnswerStatus,
}

impl AnswerState {
    /// True while a card must stay locked — no button may fire twice.
    pub fn is_locked(&self) -> bool {
        matches!(self.status, AnswerStatus::Sending | AnswerStatus::Acked)
    }
}

// ---------------------------------------------------------------------------
// The feed
// ---------------------------------------------------------------------------

/// The staged half of an EXP-656 replay swap.
#[derive(Default)]
struct Staged {
    events: Vec<ActivityEvent>,
    /// Messages sent WHILE the replay was staging: the replay predates them,
    /// so the commit re-appends whatever it did not carry back.
    local_echoes: Vec<String>,
}

/// The viewer's whole rendering state. Feed it frames, read it for rendering.
#[derive(Default)]
pub struct SteerFeed {
    items: Vec<FeedItem>,
    latest_diff: Option<String>,
    answers: HashMap<String, AnswerState>,
    next_id: FeedItemId,
    /// Locally-echoed sent messages awaiting their transcript-derived twin.
    /// FIFO, capped at [`ECHO_CAP`]; deliberately clock-free (the web's 5min
    /// TTL is a refinement this port leaves out — the cap alone bounds it).
    echoes: VecDeque<String>,
    staged: Option<Staged>,
}

impl SteerFeed {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Reading ────────────────────────────────────────────────────────────

    pub fn items(&self) -> &[FeedItem] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The worktree diff behind the pinned "Latest changes" strip — the
    /// latest replaces the previous one, and an empty diff clears it.
    pub fn latest_diff(&self) -> Option<&str> {
        self.latest_diff.as_deref()
    }

    pub fn answer_state(&self, key: &str) -> Option<&AnswerState> {
        self.answers.get(key)
    }

    /// Whether a card's answer button must stay disabled.
    pub fn is_answer_locked(&self, key: &str) -> bool {
        self.answers.get(key).is_some_and(AnswerState::is_locked)
    }

    // ── Applying frames ────────────────────────────────────────────────────

    /// One `activity` frame. Buffered instead of applied while a replay is
    /// staging ([`SteerFeed::apply_reset`]).
    pub fn apply(&mut self, event: ActivityEvent) {
        if let Some(staged) = self.staged.as_mut() {
            staged.events.push(event);
            return;
        }
        self.handle_activity(event);
    }

    /// `activity_reset` — the relay/desktop is about to (re)publish the whole
    /// history. EXP-656: this CLEARS NOTHING. It opens (or restarts) a
    /// staging window; a second reset means the relay superseded the replay
    /// being buffered, not that the reader should lose what is on screen.
    pub fn apply_reset(&mut self) {
        self.staged = Some(Staged::default());
    }

    /// `activity_synced` (EXP-656) — "the picture is complete, commit it".
    /// A no-op when nothing is staging.
    pub fn apply_synced(&mut self) {
        self.commit_staged();
    }

    /// The caller's fallback for a replay that ends without a marker: commit
    /// once the stream has been quiet for [`REPLAY_QUIET`], and
    /// unconditionally at [`REPLAY_MAX`]. Identical to
    /// [`SteerFeed::apply_synced`] — named apart so the call sites read as
    /// what they are.
    pub fn force_swap(&mut self) {
        self.commit_staged();
    }

    /// Whether a replay is buffering right now. The caller arms its
    /// quiet/deadline timer off this (and disarms when it goes false).
    pub fn is_staging(&self) -> bool {
        self.staged.is_some()
    }

    /// How many events the in-flight replay has buffered (observability, and
    /// the caller's quiet-timer bookkeeping).
    pub fn staged_len(&self) -> usize {
        self.staged.as_ref().map_or(0, |s| s.events.len())
    }

    /// Drop a staged replay and KEEP the visible feed: the socket went away
    /// mid-burst, so the buffer is a partial history of a room this client is
    /// no longer joined to. The next join replays from scratch.
    pub fn discard_staging(&mut self) {
        self.staged = None;
    }

    // ── Local (client-originated) state ────────────────────────────────────

    /// A message this client just sent: it renders IMMEDIATELY, and the echo
    /// FIFO makes its transcript-derived `user_message` twin a no-op when it
    /// arrives. Returns the new item's id.
    pub fn push_local_message(&mut self, text: &str) -> FeedItemId {
        self.push_echo(text);
        if let Some(staged) = self.staged.as_mut() {
            // The replay predates this message; the commit re-appends it if
            // the replay did not carry it back.
            staged.local_echoes.push(text.to_string());
        }
        self.push_item(FeedKind::UserMessage {
            text: text.to_string(),
        })
    }

    /// Record a local echo WITHOUT rendering anything — for a message whose
    /// item the caller appends itself.
    pub fn note_local_echo(&mut self, text: &str) {
        self.push_echo(text);
    }

    /// Lock a card the instant its answer goes out (web `beginAnswer`). The
    /// caller then arms an [`ANSWER_ACK_TIMEOUT`] timer that calls
    /// [`SteerFeed::fail_answer`].
    pub fn note_answer_sent(&mut self, key: &str, keys: Vec<String>, labels: Vec<String>) {
        self.answers.insert(
            key.to_string(),
            AnswerState {
                keys,
                labels,
                status: AnswerStatus::Sending,
            },
        );
    }

    /// The ack never came — re-enable the card. An already-acked card stays
    /// locked (web `failAnswer`).
    pub fn fail_answer(&mut self, key: &str) {
        if let Some(state) = self.answers.get_mut(key) {
            if state.status == AnswerStatus::Sending {
                state.status = AnswerStatus::Error;
            }
        }
    }

    // ── Internals ──────────────────────────────────────────────────────────

    fn take_id(&mut self) -> FeedItemId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_item(&mut self, kind: FeedKind) -> FeedItemId {
        let id = self.take_id();
        self.items.push(FeedItem { id, kind });
        self.trim();
        id
    }

    fn trim(&mut self) {
        if self.items.len() > FEED_CAP {
            self.items.drain(..self.items.len() - FEED_CAP);
        }
    }

    fn push_echo(&mut self, text: &str) {
        self.echoes.push_back(text.trim().to_string());
        while self.echoes.len() > ECHO_CAP {
            self.echoes.pop_front();
        }
    }

    /// Whether an incoming `user_message` matches a recent local echo.
    /// Consumes the match — true means SKIP the event (web `consumeEcho`).
    fn consume_echo(&mut self, text: &str) -> bool {
        let needle = text.trim();
        match self.echoes.iter().position(|echo| echo == needle) {
            Some(index) => {
                self.echoes.remove(index);
                true
            }
            None => false,
        }
    }

    /// A resolved card carries its own answer — drop its lock so a stale ack
    /// deadline can't flip a finished card into the retry state (web
    /// `reconcileResolvedAnswers`). Only a resolution can newly resolve a
    /// card, so this runs on that edge (and on a replay commit) rather than
    /// per event.
    fn reconcile_resolved_answers(&mut self) {
        let stale: Vec<String> = self
            .items
            .iter()
            .filter(|item| item.question().is_some_and(|card| card.resolved))
            .map(answer_key)
            .filter(|key| self.answers.contains_key(key))
            .collect();
        for key in stale {
            self.answers.remove(&key);
        }
    }

    fn handle_activity(&mut self, event: ActivityEvent) {
        match event {
            ActivityEvent::Narration {
                text,
                before_question_id,
                ..
            } => {
                if text.trim().is_empty() {
                    return;
                }
                // EXP-483: prose from the withheld ask/plan entry flushes
                // AFTER its already-published card — splice it back above.
                if let Some(anchor) = before_question_id {
                    if let Some(at) = self.question_position(&anchor) {
                        let id = self.take_id();
                        self.items.insert(
                            at,
                            FeedItem {
                                id,
                                kind: FeedKind::Narration { text },
                            },
                        );
                        self.trim();
                        return;
                    }
                }
                self.push_item(FeedKind::Narration { text });
            }
            ActivityEvent::Tool {
                name,
                detail,
                subagent_id,
                ..
            } => {
                self.push_item(FeedKind::Tool {
                    name,
                    detail: non_blank(detail),
                    subagent_id,
                });
            }
            ActivityEvent::UserMessage { text, .. } => {
                if text.trim().is_empty() {
                    return;
                }
                // A message this client just sent was already echoed locally
                // — skip its transcript-derived twin.
                if self.consume_echo(&text) {
                    return;
                }
                self.push_item(FeedKind::UserMessage { text });
            }
            ActivityEvent::Question {
                text,
                options,
                multi_select,
                plan_mode,
                id,
                ask_id,
                index,
                total,
                header,
                ..
            } => {
                if text.trim().is_empty() || options.is_empty() {
                    return;
                }
                let card = QuestionCard {
                    text,
                    options,
                    multi_select: multi_select == Some(true),
                    plan_mode: plan_mode == Some(true),
                    question_id: id,
                    ask_id,
                    index,
                    total,
                    header,
                    resolved: false,
                    answer: None,
                    dismissed: false,
                };
                // A re-emission of a known id replaces the card IN PLACE (the
                // desktop augments the options as it learns them), keeping
                // the feed position, the local id and any resolution.
                if let Some(question_id) = card.question_id.clone() {
                    if let Some(existing) = self.items.iter_mut().find(|item| {
                        item.question()
                            .is_some_and(|c| c.question_id.as_deref() == Some(question_id.as_str()))
                    }) {
                        let previous = existing
                            .question()
                            .cloned()
                            .expect("the item matched as a question");
                        *existing.question_mut().expect("still a question") = QuestionCard {
                            resolved: previous.resolved,
                            answer: previous.answer,
                            dismissed: previous.dismissed,
                            ..card
                        };
                        return;
                    }
                }
                self.push_item(FeedKind::Question(card));
            }
            ActivityEvent::QuestionResolved {
                id,
                ask_id,
                answers,
                dismissed,
                ..
            } => {
                self.apply_question_resolved(id, ask_id, answers.unwrap_or_default(), dismissed);
                self.reconcile_resolved_answers();
            }
            ActivityEvent::AnswerAck { id, .. } => {
                if id.is_empty() {
                    return;
                }
                if let Some(state) = self.answers.get_mut(&id) {
                    if state.status != AnswerStatus::Acked {
                        state.status = AnswerStatus::Acked;
                    }
                }
            }
            ActivityEvent::Subagent {
                id,
                agent_type,
                status,
                detail,
                ..
            } => {
                if id.is_empty() {
                    return;
                }
                self.push_item(FeedKind::Subagent {
                    subagent_id: id,
                    agent_type,
                    status,
                    detail: non_blank(detail),
                });
            }
            ActivityEvent::Permission { tool, detail, .. } => {
                if tool.trim().is_empty() {
                    return;
                }
                self.push_item(FeedKind::Permission {
                    tool,
                    detail: non_blank(detail),
                });
            }
            ActivityEvent::Diff { diff, .. } => {
                // Diffs never enter the feed — the latest replaces the
                // previous one behind the pinned strip. EXP-688: an EMPTY
                // frame is the publisher saying the branch no longer differs.
                self.latest_diff = if diff.trim().is_empty() {
                    None
                } else {
                    Some(diff)
                };
            }
        }
    }

    /// Index of the first question card matching `anchor` by `ask_id` or wire
    /// `question_id` — resolved cards match too (the withheld prose normally
    /// flushes post-answer).
    fn question_position(&self, anchor: &str) -> Option<usize> {
        self.items.iter().position(|item| {
            item.question().is_some_and(|card| {
                card.ask_id.as_deref() == Some(anchor)
                    || card.question_id.as_deref() == Some(anchor)
            })
        })
    }

    /// Retire the card with the matching wire id, else EVERY card of `ask_id`,
    /// else every still-pending card (web `applyQuestionResolved`). Answers
    /// land positionally on the answer-consuming cards; a by-id resolution
    /// folds all of its answers into that one card.
    fn apply_question_resolved(
        &mut self,
        id: Option<String>,
        ask_id: Option<String>,
        answers: Vec<String>,
        dismissed: Option<bool>,
    ) {
        let dismissed = dismissed == Some(true);
        let joined = answers.join(", ");
        let mut cursor = 0usize;
        for item in self.items.iter_mut() {
            let Some(card) = item.question_mut() else {
                continue;
            };
            let matches = match (&id, &ask_id) {
                (Some(id), _) => card.question_id.as_deref() == Some(id.as_str()),
                (None, Some(ask)) => card.ask_id.as_deref() == Some(ask.as_str()),
                (None, None) => !card.resolved,
            };
            if !matches {
                continue;
            }
            // The submit step of an ask (ask_id, no index) is a confirmation,
            // not a question — it never consumes one of the ask's answers.
            let consumes_answer = card.ask_id.is_none() || card.index.is_some();
            let mut answer = None;
            if !dismissed && consumes_answer {
                answer = if id.is_some() {
                    (!joined.is_empty()).then(|| joined.clone())
                } else {
                    let taken = answers.get(cursor).cloned();
                    cursor += 1;
                    taken
                };
            }
            card.resolved = true;
            card.dismissed = card.dismissed || dismissed;
            if answer.is_some() {
                card.answer = answer;
            }
        }
    }

    /// Swap a staged replay in as ONE change: the old feed and the replayed
    /// one never coexist and the feed is never momentarily empty, so no
    /// scroll observer sees the collapse that used to yank the reader.
    fn commit_staged(&mut self) {
        let Some(staged) = self.staged.take() else {
            return;
        };
        // Locks still waiting for their `answer_ack` when the replay started:
        // a tap made DURING the staging window must not be undone by the swap
        // (the replay predates it and brings the card back unanswered).
        let carried: Vec<(String, AnswerState)> = self
            .answers
            .iter()
            .filter(|(_, state)| state.is_locked())
            .map(|(key, state)| (key.clone(), state.clone()))
            .collect();
        // The oldest visible item's id: replaying the same history from here
        // hands the unchanged prefix the ids it already had, so the UI keeps
        // every row's identity (and the reader's anchor) across the swap.
        let anchor_id = self.items.first().map(|item| item.id);

        self.items.clear();
        self.latest_diff = None;
        self.answers.clear();
        self.echoes.clear();
        if let Some(anchor) = anchor_id {
            self.next_id = anchor;
        }

        for event in staged.events {
            self.handle_activity(event);
        }
        for text in staged.local_echoes {
            if self.tail_carries_echo(&text) {
                continue;
            }
            // Not in the replay: re-show it, and re-arm the dedupe so its
            // transcript-derived twin doesn't render a second copy.
            self.push_echo(&text);
            self.push_item(FeedKind::UserMessage { text });
        }
        for (key, state) in carried {
            if self.carries_question(&key) {
                self.answers.insert(key, state);
            }
        }
        self.reconcile_resolved_answers();
    }

    /// Whether the committed feed already ends with this echo — the replay is
    /// authoritative, so anything it carried back must not be duplicated.
    fn tail_carries_echo(&self, text: &str) -> bool {
        let needle = text.trim();
        self.items
            .iter()
            .rev()
            .take(ECHO_CAP)
            .any(|item| match &item.kind {
                FeedKind::UserMessage { text } => text.trim() == needle,
                _ => false,
            })
    }

    fn carries_question(&self, key: &str) -> bool {
        self.items
            .iter()
            .any(|item| item.question().is_some() && answer_key(item) == key)
    }

    // ── Projections ────────────────────────────────────────────────────────

    /// Ids of the question cards still answerable ([`active_question_ids`]).
    pub fn active_question_ids(&self) -> HashSet<FeedItemId> {
        active_question_ids(&self.items)
    }

    /// The feed grouped into render rows ([`group_feed_rows`]).
    pub fn rows(&self) -> Vec<FeedRow<'_>> {
        group_feed_rows(&self.items)
    }

    /// Every subagent seen in the feed ([`collect_subagents`]).
    pub fn subagents(&self) -> Vec<SubagentSummary> {
        collect_subagents(&self.items)
    }
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

// ---------------------------------------------------------------------------
// Pure projections (ports of agent-feed.ts)
// ---------------------------------------------------------------------------

/// Ids of the `question` items still answerable.
///
/// Protocol v2 cards (a wire `question_id`) are IDENTITY-scoped: they stay
/// answerable until an explicit `question_resolved` retires them, no matter
/// what flushes in behind them.
///
/// Legacy cards keep the EXP-174 heuristic: the TRAILING consecutive question
/// run (a multi-question batch lands back-to-back and the TUI auto-advances in
/// order), PLUS any plan-approval card with no resolution signal after it —
/// plan questions are published from the live terminal grid the moment the
/// picker appears while the transcript tail lags, so tool rows and narration
/// flush in BEHIND a picker that is still on screen. Only a newer question
/// proves a plan picker resolved — a human message does NOT (steering mid-plan
/// leaves the picker up).
pub fn active_question_ids(items: &[FeedItem]) -> HashSet<FeedItemId> {
    let mut ids = HashSet::new();
    for item in items {
        if let Some(card) = item.question() {
            if card.question_id.is_some() && !card.resolved {
                ids.insert(item.id);
            }
        }
    }
    // Still inside the trailing consecutive question run.
    let mut trailing = true;
    // A resolution signal lies after the current position.
    let mut retired = false;
    for item in items.iter().rev() {
        match item.question() {
            Some(card) => {
                if card.resolved || card.question_id.is_some() {
                    // An answered/dismissed card is itself a resolution signal
                    // (it proves the TUI moved past it), as is any newer
                    // question.
                    trailing = false;
                    retired = true;
                } else {
                    if trailing || (card.plan_mode && !retired) {
                        ids.insert(item.id);
                    }
                    retired = true;
                }
            }
            None => trailing = false,
        }
        if retired && !trailing {
            break;
        }
    }
    ids
}

/// A render row over the flat feed: one item, a run of ≥2 CONSECUTIVE plain
/// tool items collapsed into a "N tool calls" row (EXP-97), one ask's question
/// cards collapsed into a stepper, or a subagent's events plus the tool calls
/// it made. A group's `id` is its FIRST item's id, so the row key (and its
/// expanded state) stays stable while the group keeps growing.
#[derive(Clone, Debug, PartialEq)]
pub enum FeedRow<'a> {
    Single(&'a FeedItem),
    ToolRun {
        id: FeedItemId,
        items: Vec<&'a FeedItem>,
    },
    Ask {
        id: FeedItemId,
        ask_id: String,
        items: Vec<&'a FeedItem>,
    },
    Subagent {
        id: FeedItemId,
        subagent_id: String,
        items: Vec<&'a FeedItem>,
    },
}

impl FeedRow<'_> {
    /// The row's stable key.
    pub fn id(&self) -> FeedItemId {
        match self {
            FeedRow::Single(item) => item.id,
            FeedRow::ToolRun { id, .. }
            | FeedRow::Ask { id, .. }
            | FeedRow::Subagent { id, .. } => *id,
        }
    }
}

/// Group the flat feed into render rows — a PURE projection: the feed (and
/// [`active_question_ids`] over it) is never restructured, so answerability is
/// unaffected. Grouped items are pulled out of their in-place position into
/// the row their group opened (web `groupFeedRows`).
pub fn group_feed_rows(items: &[FeedItem]) -> Vec<FeedRow<'_>> {
    let mut rows: Vec<FeedRow<'_>> = Vec::new();
    // Row index of the open group, keyed by ask / subagent id.
    let mut ask_rows: HashMap<String, usize> = HashMap::new();
    let mut subagent_rows: HashMap<String, usize> = HashMap::new();
    let mut i = 0usize;
    while i < items.len() {
        let item = &items[i];
        if let Some(ask_id) = item.question().and_then(|card| card.ask_id.clone()) {
            match ask_rows.get(&ask_id) {
                Some(&row) => {
                    if let FeedRow::Ask { items, .. } = &mut rows[row] {
                        items.push(item);
                    }
                }
                None => {
                    ask_rows.insert(ask_id.clone(), rows.len());
                    rows.push(FeedRow::Ask {
                        id: item.id,
                        ask_id,
                        items: vec![item],
                    });
                }
            }
            i += 1;
            continue;
        }
        if let Some(subagent_id) = item.subagent_id().map(str::to_string) {
            match subagent_rows.get(&subagent_id) {
                Some(&row) => {
                    if let FeedRow::Subagent { items, .. } = &mut rows[row] {
                        items.push(item);
                    }
                }
                None => {
                    subagent_rows.insert(subagent_id.clone(), rows.len());
                    rows.push(FeedRow::Subagent {
                        id: item.id,
                        subagent_id,
                        items: vec![item],
                    });
                }
            }
            i += 1;
            continue;
        }
        if !item.is_tool() {
            rows.push(FeedRow::Single(item));
            i += 1;
            continue;
        }
        let mut end = i;
        while end + 1 < items.len()
            && items[end + 1].is_tool()
            && items[end + 1].subagent_id().is_none()
        {
            end += 1;
        }
        if end == i {
            rows.push(FeedRow::Single(item));
        } else {
            rows.push(FeedRow::ToolRun {
                id: item.id,
                items: items[i..=end].iter().collect(),
            });
        }
        i = end + 1;
    }
    rows
}

/// `subagent.agent_type` when the desktop's hook payload carried none — old
/// builds also stamp it onto the COMPLETED edge, so it is a sentinel the label
/// selection must skip past, never a type to prefer (EXP-350).
pub const SUBAGENT_FALLBACK_TYPE: &str = "agent";

/// One subagent's summary for tab navigation (EXP-356).
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentSummary {
    pub subagent_id: String,
    pub agent_type: String,
    pub done: bool,
    pub detail: Option<String>,
    pub tool_count: usize,
}

/// Every subagent seen in the feed, in first-appearance order, each summarized
/// like its group row (web `collectSubagents`).
pub fn collect_subagents(items: &[FeedItem]) -> Vec<SubagentSummary> {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Vec<&FeedItem>> = HashMap::new();
    for item in items {
        let Some(subagent_id) = item.subagent_id() else {
            continue;
        };
        let bucket = by_id.entry(subagent_id.to_string()).or_insert_with(|| {
            order.push(subagent_id.to_string());
            Vec::new()
        });
        bucket.push(item);
    }
    order
        .into_iter()
        .map(|subagent_id| {
            let rows = by_id.get(&subagent_id).map(Vec::as_slice).unwrap_or(&[]);
            let summary = summarize_subagent_row(rows);
            SubagentSummary {
                subagent_id,
                agent_type: summary.agent_type,
                done: summary.done,
                detail: summary.detail,
                tool_count: summary.tool_count,
            }
        })
        .collect()
}

/// What a subagent group row displays (EXP-350) — one place for the label /
/// status / detail selection so all clients mirror it:
/// - `agent_type`: the first marker's REAL type — a later marker carrying the
///   fallback (an old desktop's completed edge) can never degrade the label;
/// - `done`: any marker completed;
/// - `detail`: the LATEST non-empty detail (the completed edge restates the
///   freshest);
/// - `tool_count`: the tool calls attributed to the subagent.
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentRowSummary {
    pub agent_type: String,
    pub done: bool,
    pub detail: Option<String>,
    pub tool_count: usize,
}

pub fn summarize_subagent_row(items: &[&FeedItem]) -> SubagentRowSummary {
    let markers: Vec<(&str, SubagentStatus, Option<&str>)> = items
        .iter()
        .filter_map(|item| match &item.kind {
            FeedKind::Subagent {
                agent_type,
                status,
                detail,
                ..
            } => Some((agent_type.trim(), *status, detail.as_deref())),
            _ => None,
        })
        .collect();
    let types: Vec<&str> = markers
        .iter()
        .map(|(agent_type, _, _)| *agent_type)
        .filter(|agent_type| !agent_type.is_empty())
        .collect();
    SubagentRowSummary {
        agent_type: types
            .iter()
            .find(|agent_type| **agent_type != SUBAGENT_FALLBACK_TYPE)
            .or(types.first())
            .map(|agent_type| agent_type.to_string())
            .unwrap_or_else(|| SUBAGENT_FALLBACK_TYPE.to_string()),
        done: markers
            .iter()
            .any(|(_, status, _)| *status == SubagentStatus::Completed),
        detail: markers
            .iter()
            .rev()
            .find_map(|(_, _, detail)| detail.filter(|d| !d.trim().is_empty()))
            .map(str::to_string),
        tool_count: items.iter().filter(|item| item.is_tool()).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from apps/web/src/lib/steer-session-store.test.ts (the reducer
    // half — the socket half lives in `viewer`) and the agent-feed helpers.

    fn question(id: Option<&str>, text: &str) -> ActivityEvent {
        ActivityEvent::Question {
            text: text.into(),
            options: vec![QuestionOption::new("Yes", "1"), QuestionOption::new("No", "2")],
            multi_select: None,
            plan_mode: None,
            id: id.map(str::to_string),
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        }
    }

    fn texts(feed: &SteerFeed) -> Vec<String> {
        feed.items()
            .iter()
            .map(|item| match &item.kind {
                FeedKind::Narration { text } | FeedKind::UserMessage { text } => text.clone(),
                FeedKind::Tool { name, .. } => name.clone(),
                FeedKind::Permission { tool, .. } => tool.clone(),
                FeedKind::Subagent { subagent_id, .. } => subagent_id.clone(),
                FeedKind::Question(card) => card.text.clone(),
            })
            .collect()
    }

    // ── Appending, trimming, blanks ────────────────────────────────────────

    #[test]
    fn applies_activity_frames_to_the_feed_and_caps_it() {
        let mut feed = SteerFeed::new();
        for i in 0..FEED_CAP + 10 {
            feed.apply(ActivityEvent::narration(format!("line {i}")));
        }
        assert_eq!(feed.len(), FEED_CAP);
        assert_eq!(
            texts(&feed).last().unwrap(),
            &format!("line {}", FEED_CAP + 9)
        );
        // The oldest survivor is the one right after the dropped prefix.
        assert_eq!(texts(&feed).first().unwrap(), "line 10");
    }

    #[test]
    fn blank_events_never_enter_the_feed() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("   "));
        feed.apply(ActivityEvent::user_message("\n"));
        feed.apply(ActivityEvent::Permission {
            tool: " ".into(),
            detail: None,
            at: None,
        });
        // A question with no options is not answerable, so it is not a card.
        feed.apply(ActivityEvent::Question {
            text: "Which?".into(),
            options: vec![],
            multi_select: None,
            plan_mode: None,
            id: None,
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        });
        assert!(feed.is_empty());
        // A blank tool detail is dropped, but the call itself renders.
        feed.apply(ActivityEvent::tool("Edit", Some("  ".into())));
        assert_eq!(
            feed.items()[0].kind,
            FeedKind::Tool {
                name: "Edit".into(),
                detail: None,
                subagent_id: None,
            }
        );
    }

    // ── Diff: latest replaces, empty clears (EXP-688) ──────────────────────

    #[test]
    fn diffs_replace_the_previous_one_and_never_enter_the_feed() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::diff("--- a\n+++ b\n"));
        assert_eq!(feed.latest_diff(), Some("--- a\n+++ b\n"));
        assert!(feed.is_empty());
        feed.apply(ActivityEvent::diff("--- c\n+++ d\n"));
        assert_eq!(feed.latest_diff(), Some("--- c\n+++ d\n"));
        // EXP-688: an empty diff means the branch no longer differs.
        feed.apply(ActivityEvent::diff("   "));
        assert_eq!(feed.latest_diff(), None);
    }

    // ── Echo dedupe (EXP-78) ───────────────────────────────────────────────

    #[test]
    fn a_local_message_swallows_its_transcript_twin_once() {
        let mut feed = SteerFeed::new();
        feed.push_local_message("do the thing");
        assert_eq!(texts(&feed), vec!["do the thing"]);
        // The transcript echo of the SAME text is skipped…
        feed.apply(ActivityEvent::user_message("do the thing"));
        assert_eq!(feed.len(), 1);
        // …but only once: a genuinely repeated turn still renders.
        feed.apply(ActivityEvent::user_message("do the thing"));
        assert_eq!(feed.len(), 2);
    }

    #[test]
    fn the_echo_fifo_is_capped_at_eight() {
        let mut feed = SteerFeed::new();
        for i in 0..ECHO_CAP + 2 {
            feed.note_local_echo(&format!("msg {i}"));
        }
        // The two oldest fell off the FIFO, so their twins render.
        feed.apply(ActivityEvent::user_message("msg 0"));
        feed.apply(ActivityEvent::user_message("msg 1"));
        assert_eq!(texts(&feed), vec!["msg 0", "msg 1"]);
        // A still-remembered one is deduped away.
        feed.apply(ActivityEvent::user_message("msg 5"));
        assert_eq!(feed.len(), 2);
    }

    // ── Question identity (protocol v2) ────────────────────────────────────

    #[test]
    fn a_re_emitted_question_replaces_its_card_in_place() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("toolu_01"), "Which color?"));
        feed.apply(ActivityEvent::narration("thinking"));
        let card_id = feed.items()[0].id;

        // The desktop learned another option and re-publishes the same id.
        feed.apply(ActivityEvent::Question {
            text: "Which color?".into(),
            options: vec![
                QuestionOption::new("Yes", "1"),
                QuestionOption::new("No", "2"),
                QuestionOption {
                    free_text: true,
                    ..QuestionOption::new("Type something.", "3")
                },
            ],
            multi_select: None,
            plan_mode: None,
            id: Some("toolu_01".into()),
            ask_id: None,
            index: None,
            total: None,
            header: Some("Color".into()),
            at: None,
        });
        assert_eq!(feed.len(), 2, "replaced, not appended");
        assert_eq!(feed.items()[0].id, card_id, "the card keeps its identity");
        let card = feed.items()[0].question().unwrap();
        assert_eq!(card.options.len(), 3);
        assert_eq!(card.header.as_deref(), Some("Color"));
        // Position holds: the narration is still after it.
        assert_eq!(texts(&feed)[1], "thinking");
    }

    #[test]
    fn a_re_emission_never_un_resolves_a_card() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("toolu_01"), "Which color?"));
        feed.apply(ActivityEvent::QuestionResolved {
            id: Some("toolu_01".into()),
            ask_id: None,
            answers: Some(vec!["Red".into()]),
            dismissed: None,
            at: None,
        });
        feed.apply(question(Some("toolu_01"), "Which color?"));
        let card = feed.items()[0].question().unwrap();
        assert!(card.resolved);
        assert_eq!(card.answer.as_deref(), Some("Red"));
    }

    // ── question_resolved ──────────────────────────────────────────────────

    #[test]
    fn a_by_id_resolution_folds_every_answer_into_that_card() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        feed.apply(question(Some("q2"), "And?"));
        feed.apply(ActivityEvent::QuestionResolved {
            id: Some("q1".into()),
            ask_id: None,
            answers: Some(vec!["Red".into(), "Blue".into()]),
            dismissed: None,
            at: None,
        });
        let first = feed.items()[0].question().unwrap();
        assert!(first.resolved);
        assert_eq!(first.answer.as_deref(), Some("Red, Blue"));
        assert!(!feed.items()[1].question().unwrap().resolved);
    }

    #[test]
    fn an_ask_resolution_lands_answers_positionally_and_skips_the_submit_step() {
        let mut feed = SteerFeed::new();
        for index in 1..=2u32 {
            feed.apply(ActivityEvent::Question {
                text: format!("Step {index}"),
                options: vec![QuestionOption::new("Yes", "1")],
                multi_select: None,
                plan_mode: None,
                id: Some(format!("ask#{index}")),
                ask_id: Some("ask".into()),
                index: Some(index),
                total: Some(2),
                header: None,
                at: None,
            });
        }
        // The ask's final review/submit step: askId, no index.
        feed.apply(ActivityEvent::Question {
            text: "Submit answers?".into(),
            options: vec![QuestionOption::new("Submit", "\r")],
            multi_select: None,
            plan_mode: None,
            id: Some("ask#submit".into()),
            ask_id: Some("ask".into()),
            index: None,
            total: None,
            header: None,
            at: None,
        });
        feed.apply(ActivityEvent::QuestionResolved {
            id: None,
            ask_id: Some("ask".into()),
            answers: Some(vec!["Red".into(), "Blue".into()]),
            dismissed: None,
            at: None,
        });
        let answers: Vec<Option<String>> = feed
            .items()
            .iter()
            .filter_map(|item| item.question())
            .map(|card| card.answer.clone())
            .collect();
        assert_eq!(
            answers,
            vec![
                Some("Red".to_string()),
                Some("Blue".to_string()),
                None, // the submit step consumes none
            ]
        );
        assert!(feed.items().iter().all(|item| item
            .question()
            .is_none_or(|card| card.resolved)));
    }

    #[test]
    fn a_dismissal_retires_every_card_of_the_ask_with_no_answer() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Question {
            text: "Step 1".into(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: Some("ask#1".into()),
            ask_id: Some("ask".into()),
            index: Some(1),
            total: Some(1),
            header: None,
            at: None,
        });
        feed.apply(ActivityEvent::QuestionResolved {
            id: None,
            ask_id: Some("ask".into()),
            answers: None,
            dismissed: Some(true),
            at: None,
        });
        let card = feed.items()[0].question().unwrap();
        assert!(card.resolved && card.dismissed);
        assert_eq!(card.answer, None);
    }

    #[test]
    fn an_id_less_ask_less_resolution_retires_every_pending_card() {
        let mut feed = SteerFeed::new();
        feed.apply(question(None, "Legacy one"));
        feed.apply(question(None, "Legacy two"));
        feed.apply(ActivityEvent::QuestionResolved {
            id: None,
            ask_id: None,
            answers: Some(vec!["A".into(), "B".into()]),
            dismissed: None,
            at: None,
        });
        let answers: Vec<Option<String>> = feed
            .items()
            .iter()
            .filter_map(|item| item.question())
            .map(|card| card.answer.clone())
            .collect();
        assert_eq!(answers, vec![Some("A".into()), Some("B".into())]);
    }

    // ── Narration splicing (EXP-483) ───────────────────────────────────────

    #[test]
    fn anchored_narration_splices_above_its_question_by_ask_or_question_id() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("before"));
        feed.apply(question(Some("toolu_01"), "Approve?"));
        feed.apply(ActivityEvent::Narration {
            text: "the prose that was withheld".into(),
            before_question_id: Some("toolu_01".into()),
            at: None,
        });
        assert_eq!(
            texts(&feed),
            vec!["before", "the prose that was withheld", "Approve?"]
        );
    }

    #[test]
    fn an_unmatched_anchor_appends_as_ever() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("before"));
        feed.apply(ActivityEvent::Narration {
            text: "orphan".into(),
            before_question_id: Some("evicted".into()),
            at: None,
        });
        assert_eq!(texts(&feed), vec!["before", "orphan"]);
    }

    // ── Answer locks ───────────────────────────────────────────────────────

    #[test]
    fn an_answer_locks_its_card_until_the_ack_or_the_timeout() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]);
        assert!(!feed.is_answer_locked(&key));

        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);
        assert!(feed.is_answer_locked(&key));
        assert_eq!(feed.answer_state(&key).unwrap().status, AnswerStatus::Sending);

        feed.apply(ActivityEvent::AnswerAck {
            id: "q1".into(),
            ask_id: None,
            at: None,
        });
        assert_eq!(feed.answer_state(&key).unwrap().status, AnswerStatus::Acked);
        assert!(feed.is_answer_locked(&key));

        // An acked card stays locked even if a stale deadline fires.
        feed.fail_answer(&key);
        assert_eq!(feed.answer_state(&key).unwrap().status, AnswerStatus::Acked);
    }

    #[test]
    fn a_missing_ack_re_enables_the_card() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]);
        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);
        feed.fail_answer(&key);
        assert_eq!(feed.answer_state(&key).unwrap().status, AnswerStatus::Error);
        assert!(!feed.is_answer_locked(&key));
    }

    #[test]
    fn a_resolution_drops_the_lock_so_a_stale_deadline_cannot_reopen_it() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]);
        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);
        feed.apply(ActivityEvent::QuestionResolved {
            id: Some("q1".into()),
            ask_id: None,
            answers: Some(vec!["Yes".into()]),
            dismissed: None,
            at: None,
        });
        assert!(feed.answer_state(&key).is_none());
        feed.fail_answer(&key);
        assert!(feed.answer_state(&key).is_none());
    }

    #[test]
    fn a_legacy_card_is_keyed_by_its_local_id() {
        let mut feed = SteerFeed::new();
        feed.apply(question(None, "Legacy"));
        let item = &feed.items()[0];
        assert_eq!(answer_key(item), format!("#{}", item.id));
    }

    // ── EXP-656 staged replay ──────────────────────────────────────────────

    #[test]
    fn a_reset_stages_instead_of_blanking_the_feed() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("already reading this"));

        feed.apply_reset();
        assert!(feed.is_staging());
        // The visible feed does NOT move while the replay buffers.
        feed.apply(ActivityEvent::narration("replayed 1"));
        feed.apply(ActivityEvent::narration("replayed 2"));
        assert_eq!(texts(&feed), vec!["already reading this"]);
        assert_eq!(feed.staged_len(), 2);

        feed.apply_synced();
        assert!(!feed.is_staging());
        assert_eq!(texts(&feed), vec!["replayed 1", "replayed 2"]);
    }

    #[test]
    fn the_quiet_timer_fallback_commits_the_same_swap() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("old"));
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("replayed"));
        // No `activity_synced` (a publisher-driven republish carries none).
        feed.force_swap();
        assert!(!feed.is_staging());
        assert_eq!(texts(&feed), vec!["replayed"]);
    }

    #[test]
    fn a_second_reset_restarts_the_window_and_keeps_the_visible_feed() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("visible"));
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("superseded"));
        feed.apply_reset();
        assert_eq!(feed.staged_len(), 0);
        assert_eq!(texts(&feed), vec!["visible"]);
        feed.apply(ActivityEvent::narration("the real replay"));
        feed.apply_synced();
        assert_eq!(texts(&feed), vec!["the real replay"]);
    }

    #[test]
    fn a_discarded_staging_keeps_the_visible_feed() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("visible"));
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("half a replay"));
        feed.discard_staging();
        assert!(!feed.is_staging());
        assert_eq!(texts(&feed), vec!["visible"]);
        // A synced with nothing staged is a no-op, never a wipe.
        feed.apply_synced();
        assert_eq!(texts(&feed), vec!["visible"]);
    }

    #[test]
    fn the_swap_preserves_the_leading_ids_so_rows_keep_their_identity() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("one"));
        feed.apply(ActivityEvent::narration("two"));
        let ids: Vec<FeedItemId> = feed.items().iter().map(|item| item.id).collect();

        feed.apply_reset();
        feed.apply(ActivityEvent::narration("one"));
        feed.apply(ActivityEvent::narration("two"));
        feed.apply(ActivityEvent::narration("three"));
        feed.apply_synced();

        let after: Vec<FeedItemId> = feed.items().iter().map(|item| item.id).collect();
        assert_eq!(after[..2], ids[..], "the unchanged prefix keeps its ids");
    }

    #[test]
    fn a_message_sent_during_a_replay_survives_the_swap_exactly_once() {
        let mut feed = SteerFeed::new();
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("history"));
        feed.push_local_message("steer me");
        assert_eq!(texts(&feed), vec!["steer me"], "it renders immediately");

        feed.apply_synced();
        // The replay predates the message, so the commit re-appends it.
        assert_eq!(texts(&feed), vec!["history", "steer me"]);
        // …and re-arms the dedupe, so the transcript twin is still swallowed.
        feed.apply(ActivityEvent::user_message("steer me"));
        assert_eq!(feed.len(), 2);
    }

    #[test]
    fn a_message_the_replay_carried_back_is_not_duplicated() {
        let mut feed = SteerFeed::new();
        feed.apply_reset();
        feed.push_local_message("steer me");
        // The desktop's republish already contains the message.
        feed.apply(ActivityEvent::narration("working"));
        feed.apply(ActivityEvent::user_message("steer me"));
        feed.apply_synced();
        assert_eq!(texts(&feed), vec!["working", "steer me"]);
    }

    #[test]
    fn an_in_flight_answer_lock_survives_the_swap() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]);
        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);

        // The reconnect replays the card as UNANSWERED — the tap must stand.
        feed.apply_reset();
        feed.apply(question(Some("q1"), "Which?"));
        feed.apply_synced();
        assert!(feed.is_answer_locked(&key));
        assert_eq!(feed.answer_state(&key).unwrap().labels, vec!["Yes"]);
    }

    #[test]
    fn a_lock_for_a_card_the_replay_dropped_is_not_carried() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]);
        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("the card aged out"));
        feed.apply_synced();
        assert!(feed.answer_state(&key).is_none());
    }

    #[test]
    fn the_swap_clears_the_stale_diff_before_replaying() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::diff("--- old\n"));
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("no diff this time"));
        feed.apply_synced();
        assert_eq!(feed.latest_diff(), None);
    }

    // ── Projections ────────────────────────────────────────────────────────

    #[test]
    fn consecutive_plain_tool_calls_collapse_into_one_row() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("prose"));
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::tool("Edit", None));
        feed.apply(ActivityEvent::tool("Bash", None));
        feed.apply(ActivityEvent::narration("more prose"));
        feed.apply(ActivityEvent::tool("Grep", None));

        let rows = feed.rows();
        assert_eq!(rows.len(), 4);
        assert!(matches!(rows[0], FeedRow::Single(_)));
        match &rows[1] {
            FeedRow::ToolRun { items, .. } => assert_eq!(items.len(), 3),
            other => panic!("expected a tool run, got {other:?}"),
        }
        assert!(matches!(rows[2], FeedRow::Single(_)));
        // A LONE tool call is its own single row, never a run of one.
        assert!(matches!(rows[3], FeedRow::Single(_)));
    }

    #[test]
    fn ask_cards_and_subagent_events_collect_into_their_own_rows() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Subagent {
            id: "agent_01".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Started,
            detail: Some("Map the crate".into()),
            at: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            subagent_id: Some("agent_01".into()),
            at: None,
        });
        feed.apply(ActivityEvent::narration("meanwhile"));
        for index in 1..=2u32 {
            feed.apply(ActivityEvent::Question {
                text: format!("Step {index}"),
                options: vec![QuestionOption::new("Yes", "1")],
                multi_select: None,
                plan_mode: None,
                id: Some(format!("ask#{index}")),
                ask_id: Some("ask".into()),
                index: Some(index),
                total: Some(2),
                header: None,
                at: None,
            });
        }
        // A later attributed call joins the subagent row it already opened.
        feed.apply(ActivityEvent::Tool {
            name: "Read".into(),
            detail: None,
            subagent_id: Some("agent_01".into()),
            at: None,
        });

        let rows = feed.rows();
        assert_eq!(rows.len(), 3);
        match &rows[0] {
            FeedRow::Subagent {
                subagent_id, items, ..
            } => {
                assert_eq!(subagent_id, "agent_01");
                assert_eq!(items.len(), 3, "marker + both attributed calls");
            }
            other => panic!("expected a subagent row, got {other:?}"),
        }
        assert!(matches!(rows[1], FeedRow::Single(_)));
        match &rows[2] {
            FeedRow::Ask { ask_id, items, .. } => {
                assert_eq!(ask_id, "ask");
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected an ask row, got {other:?}"),
        }
    }

    #[test]
    fn subagents_are_summarized_in_first_appearance_order() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Started,
            detail: Some("first".into()),
            at: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            subagent_id: Some("a1".into()),
            at: None,
        });
        feed.apply(ActivityEvent::Subagent {
            id: "a2".into(),
            agent_type: "plan".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
        });
        // EXP-350: an old desktop stamps the FALLBACK type on the completed
        // edge — it must never degrade the label, and its detail wins.
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: SUBAGENT_FALLBACK_TYPE.into(),
            status: SubagentStatus::Completed,
            detail: Some("done exploring".into()),
            at: None,
        });

        let agents = feed.subagents();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].subagent_id, "a1");
        assert_eq!(agents[0].agent_type, "explore");
        assert!(agents[0].done);
        assert_eq!(agents[0].detail.as_deref(), Some("done exploring"));
        assert_eq!(agents[0].tool_count, 1);
        assert_eq!(agents[1].subagent_id, "a2");
        assert_eq!(agents[1].agent_type, "plan");
        assert!(!agents[1].done);
    }

    #[test]
    fn v2_cards_stay_answerable_until_they_are_resolved() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::narration("still thinking"));
        let card_id = feed.items()[0].id;
        // Identity-scoped: whatever flushes in behind it, it stays active.
        assert!(feed.active_question_ids().contains(&card_id));

        feed.apply(ActivityEvent::QuestionResolved {
            id: Some("q1".into()),
            ask_id: None,
            answers: Some(vec!["Yes".into()]),
            dismissed: None,
            at: None,
        });
        assert!(!feed.active_question_ids().contains(&card_id));
    }

    #[test]
    fn a_legacy_plan_card_survives_prose_behind_it_but_not_a_newer_question() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Question {
            text: "The plan".into(),
            options: vec![QuestionOption::new("Approve", "1")],
            multi_select: None,
            plan_mode: Some(true),
            id: None,
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        });
        let plan_id = feed.items()[0].id;
        // The transcript tail lags the live grid: rows flush in BEHIND a
        // picker that is still on screen.
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::narration("prose"));
        assert!(feed.active_question_ids().contains(&plan_id));

        // Only a NEWER question proves the picker resolved.
        feed.apply(question(None, "Something else"));
        let newer_id = feed.items().last().unwrap().id;
        let active = feed.active_question_ids();
        assert!(active.contains(&newer_id));
        assert!(!active.contains(&plan_id));
    }
}
