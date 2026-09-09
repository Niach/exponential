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
//! * **`config_state` and `usage` are SLOTS too** (EXP-746): the publisher
//!   re-sends the whole snapshot on every change, so the newest replaces the
//!   previous one behind the composer chips / the context meter, and a
//!   zero-size usage clears the meter the way an empty diff clears the strip.
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

use crate::frames::{
    rate_limit_clears, ActivityEvent, CompactionPhase, ConfigCommand, ConfigMode, ConfigOption,
    QuestionOption, SubagentStatus, ToolKind, ToolUpdateStatus,
};

/// EXP-783: the transcript keeps the WHOLE run. These are safety ceilings on
/// a client's memory, not a display cap — the renderer paints a WINDOW over
/// the feed and grows it upward, so keeping everything costs nothing per
/// frame. Sized to the device journal (`JOURNAL_FILE_CAP`), because that file
/// is exactly what a replay reads back.
///
/// The relay's `ACTIVITY_LOG_CAP` is deliberately NOT matched any more: it
/// bounds the tail a joining viewer replays, and older pages are asked for
/// (`history_page`) rather than pushed.
///
/// The contract's `steerFeed` section (EXP-795): web, iOS and Android read
/// the same generated numbers, so the budgets and the trim target cannot
/// drift per client.
pub const FEED_BYTE_CAP: usize = domain::contract::STEER_FEED_BYTE_CAP;

/// The companion item ceiling: a run of 200k one-word events would sit far
/// under [`FEED_BYTE_CAP`] while costing a `Vec` entry each (web
/// `FEED_ITEM_CAP` / iOS `feedItemCap` / Android `FEED_ITEM_CAP`).
pub const FEED_ITEM_CAP: usize = domain::contract::STEER_FEED_ITEM_CAP;

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

/// EXP-724: a lone `compaction started` (its `ended` lost, or a replayed
/// start from long ago) drops the strip after this long. Web
/// `COMPACTION_TIMEOUT_MS` / iOS / Android parity — move all four in
/// lockstep. The CALLER arms the timer and calls
/// [`SteerFeed::clear_compaction`].
pub const COMPACTION_TIMEOUT: Duration = Duration::from_secs(180);

/// The strip's caption while [`SteerFeed::compacting`] is set — byte-identical
/// to the web/iOS/Android label.
pub const COMPACTING_LABEL: &str = "Compacting context…";

/// The feed marker left behind by `compaction ended` — byte-identical ×4.
pub const COMPACTED_LABEL: &str = "Context compacted";

// ---------------------------------------------------------------------------
// Feed items
// ---------------------------------------------------------------------------

/// A feed item's local identity — monotonic per feed, stable across
/// re-emissions of the same question, and preserved across the EXP-656 swap
/// for the unchanged prefix so the UI keeps its row identity (and the
/// reader's scroll anchor).
pub type FeedItemId = u64;

/// EXP-783 — where a feed's ids start.
///
/// Ids must stay STRICTLY INCREASING in feed order: the renderer prunes its
/// per-row state with `id >= oldest`, the list diff keys rows by id, and
/// [`SteerFeed::prepend_page`] gives an older page ids BELOW everything on
/// screen so no row on the reader's side is renumbered. That needs headroom
/// under the first id, and a feed that started at 0 has none. 2^32 of it is
/// four billion prepends' worth, and stays far from `u64::MAX` (the desktop's
/// synthetic "Working…" row key).
pub const FEED_ID_BASE: FeedItemId = 1 << 32;

/// One rendered row's payload. `diff` is deliberately absent: diffs are not
/// feed items ([`SteerFeed::latest_diff`]).
#[derive(Clone, Debug, PartialEq)]
pub enum FeedKind {
    Narration {
        text: String,
        /// EXP-772: the ACP coalescer's flush key. Consecutive narration
        /// events carrying the same id are ONE assistant message, so the
        /// second appends to this row instead of opening its own.
        message_id: Option<String>,
        /// EXP-773: set when the prose came from a subagent's transcript —
        /// the row is nested under that subagent's card, never the main feed.
        subagent_id: Option<String>,
    },
    Tool {
        name: String,
        detail: Option<String>,
        /// Set when the call came from a subagent's transcript — the row is
        /// nested under that subagent's card.
        subagent_id: Option<String>,
        /// EXP-785: the ACP tool-call id — the key a `tool_update` folds
        /// into this row by. Absent from a pre-EXP-785 publisher's rows,
        /// which then never settle.
        call_id: Option<String>,
        /// EXP-785: ACP's kind bucket (`edit`, `execute`, …), when the
        /// publisher sent one.
        tool_kind: Option<ToolKind>,
        /// EXP-785: a `tool_update` with a status landed — the call ENDED.
        settled: bool,
        /// EXP-785: that status was `failed` (a later `completed` clears it;
        /// the last word is the agent's). Group captions sort these last.
        failed: bool,
        /// EXP-786: the per-call unified diff an `edit` published, already
        /// cut to the contract's caps on the publisher.
        diff: Option<String>,
    },
    UserMessage {
        text: String,
        /// EXP-773: a turn addressed to a SUBAGENT, shown inside its card.
        subagent_id: Option<String>,
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
        /// EXP-748: the tool calls the publisher counted for this subagent.
        /// The journal drops a subagent's oldest calls long before the main
        /// transcript, so the marker's own number is what the caption falls
        /// back on when the rows themselves were evicted.
        tool_calls: Option<u32>,
    },
    Question(QuestionCard),
    /// EXP-724: the quiet "Context compacted" divider `compaction ended`
    /// leaves in the timeline (the strip itself is [`SteerFeed::compacting`],
    /// never an item).
    Compaction,
}

/// EXP-724: the in-flight compaction behind the pinned "Compacting context…"
/// strip. `trigger` is claude's `manual`/`auto` when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compaction {
    pub trigger: Option<String>,
}

/// EXP-746: the agent's live configuration behind the composer chips — the
/// vocabulary AND the values in force. A SLOT beside [`SteerFeed::latest_diff`],
/// never a feed row: the publisher always sends the FULL snapshot, so the
/// newest one replaces the previous whole.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionConfig {
    pub options: Vec<ConfigOption>,
    pub current_mode: Option<String>,
    pub modes: Vec<ConfigMode>,
    /// The agent's OWN slash commands — the `/` menu shows the contract
    /// catalog union these, contract first.
    pub commands: Vec<ConfigCommand>,
}

/// EXP-746: the run's context/spend meter. Tokens, not a percent (the
/// device-reported usage windows own the 0-100 rate-limit vocabulary).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionUsage {
    pub context_used: i64,
    pub context_size: i64,
    pub cost_usd: Option<f64>,
}

/// EXP-784: the agent's rate-limit window as it last reported it — the
/// fourth latest-wins slot beside [`SessionUsage`]. `status` is the agent's
/// own word (`allowed_warning`, `rejected`, …); the slot is CLEARED, never
/// filled, by an empty/`ok` status ([`rate_limit_clears`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRateLimit {
    pub status: String,
    /// Unix ms when the window resets, if the agent named one.
    pub resets_at: Option<i64>,
    pub message: Option<String>,
}

/// One item of the visible feed.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedItem {
    pub id: FeedItemId,
    pub kind: FeedKind,
    /// EXP-783: the publisher's wire sequence for the event behind this row,
    /// when it sent one. The only monotonic anchor a client has: it is what
    /// lets a replay be spliced onto a prefix already on screen, and what an
    /// older-page request is addressed relative to.
    pub seq: Option<u64>,
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

    /// The subagent a row belongs to — a tool call attributed to one, the
    /// subagent's own lifecycle marker, or (EXP-773) the prose and user turns
    /// the ACP mapper stamped with its parent tool call. Web `subagentIdOf`.
    pub fn subagent_id(&self) -> Option<&str> {
        match &self.kind {
            FeedKind::Tool { subagent_id, .. }
            | FeedKind::Narration { subagent_id, .. }
            | FeedKind::UserMessage { subagent_id, .. } => subagent_id.as_deref(),
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

/// The key a card's answer state is tracked under: its wire question id (web
/// `answerKey`).
///
/// `None` for an id-less card — EXP-730 retired the blind-keystroke answer
/// path, so such a card is read-only and can never carry answer state.
pub fn answer_key(item: &FeedItem) -> Option<String> {
    item.question()
        .and_then(|card| card.question_id.clone())
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
    events: Vec<(Option<u64>, ActivityEvent)>,
    /// Messages sent WHILE the replay was staging: the replay predates them,
    /// so the commit re-appends whatever it did not carry back.
    local_echoes: Vec<String>,
}

/// The viewer's whole rendering state. Feed it frames, read it for rendering.
#[derive(Default)]
pub struct SteerFeed {
    items: Vec<FeedItem>,
    latest_diff: Option<String>,
    /// EXP-724: set by `compaction started`, cleared by `ended`, the swap,
    /// the caller's [`COMPACTION_TIMEOUT`] and session end.
    compacting: Option<Compaction>,
    /// EXP-746: latest-wins state slots, exactly like `latest_diff` — the
    /// composer chips and the context meter, never feed rows.
    config: Option<SessionConfig>,
    usage: Option<SessionUsage>,
    /// EXP-784: the rate-limit banner's state, same slot rule.
    rate_limit: Option<SessionRateLimit>,
    answers: HashMap<String, AnswerState>,
    next_id: FeedItemId,
    /// Locally-echoed sent messages awaiting their transcript-derived twin.
    /// FIFO, capped at [`ECHO_CAP`]; deliberately clock-free (the web's 5min
    /// TTL is a refinement this port leaves out — the cap alone bounds it).
    echoes: VecDeque<String>,
    staged: Option<Staged>,
    /// EXP-783: the running size of `items`, maintained at every mutation so
    /// [`SteerFeed::trim`] is an integer compare rather than a walk.
    bytes: usize,
    /// EXP-783: the wire sequence of the event being folded in right now, so
    /// `push_item` can stamp it without threading it through every arm of
    /// `handle_activity`. Set for exactly the duration of one fold.
    seq: Option<u64>,
}

impl SteerFeed {
    pub fn new() -> Self {
        Self {
            next_id: FEED_ID_BASE,
            ..Self::default()
        }
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

    /// EXP-783: the feed's current byte weight, against [`FEED_BYTE_CAP`].
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The worktree diff behind the pinned "Latest changes" strip — the
    /// latest replaces the previous one, and an empty diff clears it.
    pub fn latest_diff(&self) -> Option<&str> {
        self.latest_diff.as_deref()
    }

    /// EXP-724: the compaction in flight, if any — renders the indeterminate
    /// [`COMPACTING_LABEL`] strip.
    pub fn compacting(&self) -> Option<&Compaction> {
        self.compacting.as_ref()
    }

    /// Drop the strip without an `ended` frame: the caller's
    /// [`COMPACTION_TIMEOUT`], or the session ending under it.
    pub fn clear_compaction(&mut self) {
        self.compacting = None;
    }

    /// EXP-746: the agent's live configuration behind the composer chips.
    /// `None` = this run publishes none (every PTY run), so no chips render.
    pub fn config(&self) -> Option<&SessionConfig> {
        self.config.as_ref()
    }

    /// EXP-746: the context/spend meter, `None` while unknown.
    /// EXP-784: the rate-limit slot; `None` = not limited (or cleared).
    pub fn rate_limit(&self) -> Option<&SessionRateLimit> {
        self.rate_limit.as_ref()
    }

    pub fn usage(&self) -> Option<SessionUsage> {
        self.usage
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
        self.apply_seq(None, event);
    }

    /// EXP-783: one `activity` frame WITH its wire sequence.
    pub fn apply_seq(&mut self, seq: Option<u64>, event: ActivityEvent) {
        if let Some(staged) = self.staged.as_mut() {
            staged.events.push((seq, event));
            return;
        }
        self.seq = seq;
        self.handle_activity(event);
        self.seq = None;
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
        self.commit_staged(None);
    }

    /// EXP-783 — the span-aware commit. `first_seq` is the OLDEST sequence
    /// the replay carried: everything the feed already holds BELOW it is a
    /// prefix the replay does not restate, so it is KEPT and the replay is
    /// spliced on top. Without a `first_seq` (a publisher or relay older than
    /// EXP-783) this is the full swap it has always been.
    pub fn apply_synced_from(&mut self, first_seq: Option<u64>) {
        self.commit_staged(first_seq);
    }

    /// The caller's fallback for a replay that ends without a marker: commit
    /// once the stream has been quiet for [`REPLAY_QUIET`], and
    /// unconditionally at [`REPLAY_MAX`]. Identical to
    /// [`SteerFeed::apply_synced`] — named apart so the call sites read as
    /// what they are.
    pub fn force_swap(&mut self) {
        self.commit_staged(None);
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

    /// EXP-783 — PREPEND one older page (a `history_chunk`), oldest first.
    ///
    /// The page is transcript from BELOW everything on screen, so it is folded
    /// into a scratch feed and its rows are spliced in front rather than
    /// appended: the reader's rows keep their ids, their answer state and
    /// their position, and the renderer sees exactly one front insertion. A
    /// page overlapping what is already held is trimmed against the oldest
    /// sequence in the feed — a re-asked page must never double the
    /// transcript. Returns how many rows were added.
    ///
    /// Ignored while a replay is staging: the replay is authoritative and is
    /// about to decide what the prefix even is.
    pub fn prepend_page(&mut self, page: Vec<(Option<u64>, ActivityEvent)>) -> usize {
        if self.staged.is_some() || page.is_empty() {
            return 0;
        }
        let oldest = self.items.iter().find_map(|item| item.seq);
        let mut scratch = SteerFeed::new();
        for (seq, event) in page {
            if oldest.is_some_and(|oldest| seq.is_some_and(|seq| seq >= oldest)) {
                continue;
            }
            scratch.apply_seq(seq, event);
        }
        if scratch.items.is_empty() {
            return 0;
        }
        // The prepended rows take ids BELOW every id in the feed, so the
        // renderer's `>= first` pruning and the list's key diff both still
        // see one strictly increasing sequence.
        let added = scratch.items.len();
        let base = self.items.first().map(|item| item.id).unwrap_or(added as u64);
        let base = base.saturating_sub(added as u64);
        let mut prepended = scratch.items;
        for (offset, item) in prepended.iter_mut().enumerate() {
            item.id = base + offset as u64;
        }
        self.bytes += prepended
            .iter()
            .map(|item| item_bytes(&item.kind))
            .sum::<usize>();
        prepended.append(&mut self.items);
        self.items = prepended;
        self.trim();
        added
    }

    /// EXP-783: the oldest wire sequence the feed still holds — what the next
    /// older-page request is asked relative to. `None` when nothing on screen
    /// is numbered (every publisher older than EXP-783), which is also the
    /// signal that paging is unavailable for this run.
    pub fn oldest_seq(&self) -> Option<u64> {
        self.items.iter().find_map(|item| item.seq)
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
            subagent_id: None,
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
        let seq = self.seq;
        self.bytes += item_bytes(&kind);
        self.items.push(FeedItem { id, kind, seq });
        self.trim();
        id
    }

    /// EXP-783: evict from the OLDEST end only once the run exceeds
    /// [`FEED_BYTE_CAP`] or [`FEED_ITEM_CAP`]. A transcript that fits — which
    /// is every real run — is never trimmed at all, so the renderer's window
    /// is the only thing that bounds per-frame work. The newest item always
    /// survives, however large it is: a feed that evicted everything would
    /// render blank.
    fn trim(&mut self) {
        if self.bytes <= FEED_BYTE_CAP && self.items.len() <= FEED_ITEM_CAP {
            return;
        }
        // Evicting down to the budget would then evict again on the very next
        // event, and each eviction shifts the whole `Vec`. Drop to the
        // contract's target (90%) instead, so the cost is paid once per ~10%
        // of a full transcript.
        let percent = domain::contract::STEER_FEED_TRIM_TARGET_PERCENT;
        let byte_target = FEED_BYTE_CAP * percent / 100;
        let item_target = FEED_ITEM_CAP * percent / 100;
        let mut bytes = self.bytes;
        let mut drop_to = 0usize;
        while drop_to + 1 < self.items.len()
            && (bytes > byte_target || self.items.len() - drop_to > item_target)
        {
            bytes = bytes.saturating_sub(item_bytes(&self.items[drop_to].kind));
            drop_to += 1;
        }
        if drop_to > 0 {
            self.items.drain(..drop_to);
            self.bytes = bytes;
        }
    }

    /// Re-derive [`Self::bytes`] from scratch. Used after the bulk in-place
    /// edits whose per-item deltas are not worth threading through.
    fn recount_bytes(&mut self) {
        self.bytes = self.items.iter().map(|item| item_bytes(&item.kind)).sum();
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
            .filter_map(answer_key)
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
                message_id,
                subagent_id,
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
                        let kind = FeedKind::Narration {
                            text,
                            message_id,
                            subagent_id,
                        };
                        self.bytes += item_bytes(&kind);
                        let seq = self.seq;
                        self.items.insert(at, FeedItem { id, kind, seq });
                        self.trim();
                        return;
                    }
                }
                // EXP-772: the engine flushes ONE assistant message in
                // several narration events keyed by `message_id` — a fragment
                // landing right behind its own message appends to that bubble
                // instead of shredding the paragraph into rows.
                if merge_narration_fragment(
                    &mut self.items,
                    message_id.as_deref(),
                    subagent_id.as_deref(),
                    &text,
                ) {
                    self.bytes += text.len();
                    return;
                }
                self.push_item(FeedKind::Narration {
                    text,
                    message_id,
                    subagent_id,
                });
            }
            ActivityEvent::Tool {
                name,
                detail,
                id,
                tool_kind,
                subagent_id,
                ..
            } => {
                self.push_item(FeedKind::Tool {
                    name,
                    detail: non_blank(detail),
                    subagent_id,
                    call_id: non_blank(id),
                    tool_kind,
                    settled: false,
                    failed: false,
                    diff: None,
                });
            }
            // EXP-785/786: folded INTO the tool row with that call id — the
            // newest one, since a re-run under the same id is a fresh row.
            // Never a row of its own; one for an id this feed does not hold
            // (evicted, or below the window) is dropped.
            ActivityEvent::ToolUpdate {
                id,
                status,
                diff: patch,
                ..
            } => {
                let Some(item) = self.items.iter_mut().rev().find(|item| {
                    matches!(&item.kind, FeedKind::Tool { call_id: Some(call_id), .. } if *call_id == id)
                }) else {
                    return;
                };
                let before = item_bytes(&item.kind);
                if let FeedKind::Tool {
                    settled,
                    failed,
                    diff,
                    ..
                } = &mut item.kind
                {
                    if let Some(status) = status {
                        *settled = true;
                        *failed = status == ToolUpdateStatus::Failed;
                    }
                    if let Some(patch) = non_blank(patch) {
                        *diff = Some(patch);
                    }
                }
                let after = item_bytes(&item.kind);
                self.bytes = self.bytes.saturating_sub(before) + after;
                self.trim();
            }
            ActivityEvent::UserMessage {
                text, subagent_id, ..
            } => {
                if text.trim().is_empty() {
                    return;
                }
                // A message this client just sent was already echoed locally
                // — skip its transcript-derived twin.
                if self.consume_echo(&text) {
                    return;
                }
                self.push_item(FeedKind::UserMessage { text, subagent_id });
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
                        let before = item_bytes(&existing.kind);
                        *existing.question_mut().expect("still a question") = QuestionCard {
                            resolved: previous.resolved,
                            answer: previous.answer,
                            dismissed: previous.dismissed,
                            ..card
                        };
                        let after = item_bytes(&existing.kind);
                        self.bytes = self.bytes.saturating_sub(before) + after;
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
                tool_calls,
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
                    tool_calls,
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
            ActivityEvent::Compaction { phase, trigger, .. } => match phase {
                // EXP-724: the strip is state beside the diff, never a row;
                // the end leaves a marker so "why did it forget" has an
                // answer later. A bare `ended` (codex auto-compaction) still
                // marks.
                CompactionPhase::Started => {
                    self.compacting = Some(Compaction {
                        trigger: non_blank(trigger),
                    });
                }
                CompactionPhase::Ended => {
                    self.compacting = None;
                    self.push_item(FeedKind::Compaction);
                }
            },
            ActivityEvent::ConfigState {
                options,
                current_mode,
                modes,
                commands,
                ..
            } => {
                // EXP-746: latest-wins, never a row — the publisher always
                // sends the FULL config, so the whole snapshot replaces the
                // previous one.
                self.config = Some(SessionConfig {
                    options,
                    current_mode: current_mode.filter(|mode| !mode.trim().is_empty()),
                    modes: modes.unwrap_or_default(),
                    commands: commands.unwrap_or_default(),
                });
            }
            ActivityEvent::Usage {
                context_used,
                context_size,
                cost_usd,
                ..
            } => {
                // A zero-size context is the engine saying "unknown" — clear
                // the meter rather than draw 0/0 (the EXP-688 empty-diff
                // rule, applied to the other latest-wins slot).
                self.usage = (context_size > 0).then_some(SessionUsage {
                    context_used: context_used.max(0),
                    context_size,
                    cost_usd,
                });
            }
            // EXP-784: the fourth slot. An empty/`ok` status CLEARS it (the
            // zero-size `usage` rule); anything else is the newest word.
            ActivityEvent::RateLimit {
                status,
                resets_at,
                message,
                ..
            } => {
                self.rate_limit = (!rate_limit_clears(&status)).then(|| SessionRateLimit {
                    status: status.trim().to_string(),
                    resets_at,
                    message: non_blank(message),
                });
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
        // A resolution rewrites cards anywhere in the transcript; the running
        // byte count is cheaper to re-derive here than to thread through.
        self.recount_bytes();
    }

    /// Swap a staged replay in as ONE change: the old feed and the replayed
    /// one never coexist and the feed is never momentarily empty, so no
    /// scroll observer sees the collapse that used to yank the reader.
    fn commit_staged(&mut self, first_seq: Option<u64>) {
        let Some(staged) = self.staged.take() else {
            return;
        };
        // EXP-783: everything the client already holds BELOW the replay's
        // oldest sequence is a prefix the replay does not restate — pages a
        // reader scrolled back to load, which a full swap used to throw away.
        // It is kept only when the WHOLE prefix is numbered: an unnumbered row
        // cannot be proved to be older than the replay, so one of them makes
        // this today's full swap.
        let retained: Vec<FeedItem> = match first_seq {
            Some(first) => {
                let split = self
                    .items
                    .iter()
                    .position(|item| item.seq.is_none_or(|seq| seq >= first))
                    .unwrap_or(self.items.len());
                self.items.drain(..split).collect()
            }
            None => Vec::new(),
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
        self.bytes = 0;
        let retained_next_id = retained.last().map(|item| item.id + 1);
        self.latest_diff = None;
        self.compacting = None;
        // EXP-746: the replay reinstates both slots inside the SAME staged
        // burst (the relay replays its latest-wins entries after the log), so
        // clearing them here never blanks the chips for a visible moment.
        self.config = None;
        self.usage = None;
        self.rate_limit = None;
        self.answers.clear();
        self.echoes.clear();
        if let Some(anchor) = anchor_id {
            self.next_id = anchor;
        }
        // The retained prefix keeps its rows AND its ids; the replay continues
        // numbering above them, so no row identity is reused.
        if !retained.is_empty() {
            self.items = retained;
            self.bytes = self.items.iter().map(|item| item_bytes(&item.kind)).sum();
            if let Some(next) = retained_next_id {
                self.next_id = next;
            }
        }

        for (seq, event) in staged.events {
            self.seq = seq;
            self.handle_activity(event);
            self.seq = None;
        }
        for text in staged.local_echoes {
            if self.tail_carries_echo(&text) {
                continue;
            }
            // Not in the replay: re-show it, and re-arm the dedupe so its
            // transcript-derived twin doesn't render a second copy.
            self.push_echo(&text);
            self.push_item(FeedKind::UserMessage {
                text,
                subagent_id: None,
            });
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
                FeedKind::UserMessage { text, .. } => text.trim() == needle,
                _ => false,
            })
    }

    fn carries_question(&self, key: &str) -> bool {
        self.items
            .iter()
            .any(|item| answer_key(item).is_some_and(|id| id == key))
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

    /// The same grouping as [`Self::rows`], as OWNED index specs
    /// ([`group_feed_row_specs`]) — what a renderer caches between frames.
    pub fn row_specs(&self) -> Vec<FeedRowSpec> {
        group_feed_row_specs(&self.items)
    }

    /// EXP-783: the same specs over `items[start..]` only, with ABSOLUTE
    /// indices — the transcript window. `start = 0` is [`Self::row_specs`].
    pub fn row_specs_from(&self, start: usize) -> Vec<FeedRowSpec> {
        group_feed_row_specs_from(&self.items, start)
    }

    /// [`Self::row_specs_from`] into a caller-owned buffer, so a renderer that
    /// reprojects every frame reuses one allocation.
    pub fn row_specs_from_into(&self, start: usize, out: &mut Vec<FeedRowSpec>) {
        group_feed_row_specs_into(&self.items, start, out);
    }

    /// The index of the first item at or after `id` — what a renderer turns
    /// its remembered window anchor back into. Feed ids only ever increase,
    /// so this is a binary search.
    pub fn position_of(&self, id: FeedItemId) -> usize {
        self.items.partition_point(|item| item.id < id)
    }

    /// Every subagent seen in the feed ([`collect_subagents`]).
    pub fn subagents(&self) -> Vec<SubagentSummary> {
        collect_subagents(&self.items)
    }
}

/// EXP-772 — append a narration fragment onto the feed's LAST row when both
/// carry the same `message_id`: the ACP coalescer flushes one assistant
/// message in several events, and a row per flush shredded a paragraph into
/// bubbles. `true` = the fragment was merged and must not be pushed.
///
/// Nothing merges without an id, across a row that is not narration, or across
/// a scope change (a fragment stamped with a subagent id is a different
/// bubble). Web `mergeNarrationFragment`, mirrored ×4.
fn merge_narration_fragment(
    items: &mut [FeedItem],
    message_id: Option<&str>,
    subagent_id: Option<&str>,
    fragment: &str,
) -> bool {
    let Some(message_id) = message_id.filter(|id| !id.is_empty()) else {
        return false;
    };
    let Some(last) = items.last_mut() else {
        return false;
    };
    let FeedKind::Narration {
        text,
        message_id: last_id,
        subagent_id: last_subagent,
    } = &mut last.kind
    else {
        return false;
    };
    if last_id.as_deref() != Some(message_id) || last_subagent.as_deref() != subagent_id {
        return false;
    }
    text.push_str(fragment);
    true
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

/// EXP-783: what one item weighs against [`FEED_BYTE_CAP`] — the text it
/// carries plus a flat per-item overhead standing in for the struct itself.
/// An estimate on purpose: this budget bounds memory, it does not account for
/// it, and an exact `size_of_val` walk per push would cost more than it saves.
fn item_bytes(kind: &FeedKind) -> usize {
    const OVERHEAD: usize = domain::contract::STEER_FEED_ITEM_OVERHEAD_BYTES;
    let text = match kind {
        FeedKind::Narration { text, .. } | FeedKind::UserMessage { text, .. } => text.len(),
        FeedKind::Tool {
            name, detail, diff, ..
        } => {
            name.len() + detail.as_ref().map_or(0, String::len) + diff.as_ref().map_or(0, String::len)
        }
        FeedKind::Permission { tool, detail } => {
            tool.len() + detail.as_ref().map_or(0, String::len)
        }
        FeedKind::Subagent {
            subagent_id,
            agent_type,
            detail,
            ..
        } => subagent_id.len() + agent_type.len() + detail.as_ref().map_or(0, String::len),
        FeedKind::Question(card) => {
            card.text.len()
                + card.answer.as_ref().map_or(0, String::len)
                + card.header.as_ref().map_or(0, String::len)
                + card
                    .options
                    .iter()
                    .map(|option| option.key.len() + option.label.len())
                    .sum::<usize>()
        }
        FeedKind::Compaction => 0,
    };
    OVERHEAD + text
}

// ---------------------------------------------------------------------------
// Pure projections (ports of agent-feed.ts)
// ---------------------------------------------------------------------------

/// Ids of the `question` items still answerable.
///
/// Every card carries a wire `question_id` (EXP-730: publishers stamp one on
/// every card), and those are IDENTITY-scoped — they stay answerable until an
/// explicit `question_resolved` retires them, no matter what flushes in behind
/// them. An id-less card is NOT answerable: the blind-keystroke path it used
/// (and the EXP-174 trailing-run heuristic that fed it) is gone, so it renders
/// read-only, exactly as on web and Android.
pub fn active_question_ids(items: &[FeedItem]) -> HashSet<FeedItemId> {
    items
        .iter()
        .filter(|item| {
            item.question()
                .is_some_and(|card| card.question_id.is_some() && !card.resolved)
        })
        .map(|item| item.id)
        .collect()
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

    /// The row's rhythm class ([`RowClass`]).
    pub fn class(&self) -> RowClass {
        match self {
            FeedRow::Single(item) => item.class(),
            // A collapsed run of calls and a subagent's group are machine
            // work, exactly like the single tool row they collapse.
            FeedRow::ToolRun { .. } | FeedRow::Subagent { .. } => RowClass::Tool,
            // A stepper card is a question the reader answers — prose.
            FeedRow::Ask { .. } => RowClass::Prose,
        }
    }
}

// ---------------------------------------------------------------------------
// EXP-787 — the transcript's gap ladder
// ---------------------------------------------------------------------------

/// What a transcript row is, for the purpose of the space ABOVE it.
///
/// Three classes only: a sent user TURN (the loudest seam in a run), the
/// agent's PROSE (narration, a question card, the compaction divider), and the
/// machine's TOOL chatter (tool rows and runs, subagent groups, permission
/// rows, the trailing "Working…" line). Mirrored ×4 — web
/// `lib/agent-feed.ts`, ExpCore `AgentFeed`, Android `domain/AgentFeed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowClass {
    Turn,
    Prose,
    Tool,
}

/// The space above a row, as a design-token NAME rather than a number: this
/// crate is gpui- and theme-free (the headless CLI links it), so the px live
/// in `theme::tokens::transcript` and the renderer maps a [`Gap`] onto them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// `gapTurn` — either side of a sent message.
    Turn,
    /// `gapBlock` — between two prose rows.
    Block,
    /// `gapTool` — where prose meets tool chatter.
    Tool,
    /// `gapDefault` — between two tool rows.
    Default,
}

/// EXP-787 — the ONE gap derivation, in ladder order:
///
/// 1. either side is a [`RowClass::Turn`] ⇒ [`Gap::Turn`];
/// 2. both are [`RowClass::Tool`] ⇒ [`Gap::Default`];
/// 3. exactly one is [`RowClass::Tool`] ⇒ [`Gap::Tool`];
/// 4. both prose ⇒ [`Gap::Block`].
///
/// The FIRST row of a transcript has no space above it at all — that is the
/// renderer's rule (there is no `prev` to pass), never a fifth [`Gap`].
pub fn transcript_gap(prev: RowClass, cur: RowClass) -> Gap {
    match (prev, cur) {
        (RowClass::Turn, _) | (_, RowClass::Turn) => Gap::Turn,
        (RowClass::Tool, RowClass::Tool) => Gap::Default,
        (RowClass::Tool, _) | (_, RowClass::Tool) => Gap::Tool,
        _ => Gap::Block,
    }
}

impl FeedKind {
    /// The rhythm class of a row rendering exactly this item.
    pub fn class(&self) -> RowClass {
        match self {
            FeedKind::UserMessage { .. } => RowClass::Turn,
            FeedKind::Narration { .. }
            | FeedKind::Question(_)
            | FeedKind::Compaction => RowClass::Prose,
            FeedKind::Tool { .. }
            | FeedKind::Subagent { .. }
            | FeedKind::Permission { .. } => RowClass::Tool,
        }
    }
}

impl FeedItem {
    /// [`FeedKind::class`] — a single item classifies exactly as the row that
    /// renders it alone.
    pub fn class(&self) -> RowClass {
        self.kind.class()
    }
}

/// Group the flat feed into render rows — a PURE projection: the feed (and
/// [`active_question_ids`] over it) is never restructured, so answerability is
/// unaffected. Grouped items are pulled out of their in-place position into
/// the row their group opened (web `groupFeedRows`).
pub fn group_feed_rows(items: &[FeedItem]) -> Vec<FeedRow<'_>> {
    group_feed_row_specs(items)
        .iter()
        .map(|spec| spec.resolve(items))
        .collect()
}

/// EXP-776: a [`FeedRow`] as OWNED item INDICES into the feed instead of
/// borrows of it — the shape a renderer can keep across frames (a virtualised
/// transcript resolves only the rows it paints). Same variants, same `id`
/// rule; [`Self::resolve`] turns one back into the borrowed row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedRowSpec {
    Single {
        id: FeedItemId,
        item: usize,
    },
    ToolRun {
        id: FeedItemId,
        items: Vec<usize>,
    },
    Ask {
        id: FeedItemId,
        ask_id: String,
        items: Vec<usize>,
    },
    Subagent {
        id: FeedItemId,
        subagent_id: String,
        items: Vec<usize>,
    },
}

impl FeedRowSpec {
    /// The row's stable key — its first item's id, like [`FeedRow::id`].
    pub fn id(&self) -> FeedItemId {
        match self {
            FeedRowSpec::Single { id, .. }
            | FeedRowSpec::ToolRun { id, .. }
            | FeedRowSpec::Ask { id, .. }
            | FeedRowSpec::Subagent { id, .. } => *id,
        }
    }

    /// EXP-787 — the row's rhythm class ([`FeedRow::class`]), WITHOUT
    /// resolving the row: a renderer classifies every row in its window every
    /// frame to pick the gaps, and that must not allocate. `items` is the
    /// same slice the spec was grouped from; an index the feed no longer
    /// holds reads as prose (the neutral rung).
    pub fn class(&self, items: &[FeedItem]) -> RowClass {
        match self {
            FeedRowSpec::Single { item, .. } => {
                items.get(*item).map_or(RowClass::Prose, FeedItem::class)
            }
            FeedRowSpec::ToolRun { .. } | FeedRowSpec::Subagent { .. } => RowClass::Tool,
            FeedRowSpec::Ask { .. } => RowClass::Prose,
        }
    }

    /// The indices of the items this row renders, in feed order.
    pub fn item_indices(&self) -> &[usize] {
        match self {
            FeedRowSpec::Single { item, .. } => std::slice::from_ref(item),
            FeedRowSpec::ToolRun { items, .. }
            | FeedRowSpec::Ask { items, .. }
            | FeedRowSpec::Subagent { items, .. } => items,
        }
    }

    /// The borrowed row over `items` — the SAME slice the spec was grouped
    /// from; a spec is only as current as the feed it came off.
    pub fn resolve<'a>(&self, items: &'a [FeedItem]) -> FeedRow<'a> {
        let pick = |indices: &[usize]| indices.iter().map(|&ix| &items[ix]).collect();
        match self {
            FeedRowSpec::Single { item, .. } => FeedRow::Single(&items[*item]),
            FeedRowSpec::ToolRun { id, items: ixs } => FeedRow::ToolRun {
                id: *id,
                items: pick(ixs),
            },
            FeedRowSpec::Ask {
                id,
                ask_id,
                items: ixs,
            } => FeedRow::Ask {
                id: *id,
                ask_id: ask_id.clone(),
                items: pick(ixs),
            },
            FeedRowSpec::Subagent {
                id,
                subagent_id,
                items: ixs,
            } => FeedRow::Subagent {
                id: *id,
                subagent_id: subagent_id.clone(),
                items: pick(ixs),
            },
        }
    }
}

/// The grouping behind [`group_feed_rows`], as index specs.
pub fn group_feed_row_specs(items: &[FeedItem]) -> Vec<FeedRowSpec> {
    group_feed_row_specs_from(items, 0)
}

/// EXP-783 — the same grouping restricted to `items[start..]`, emitting
/// ABSOLUTE indices so [`FeedRowSpec::resolve`] still takes the WHOLE feed and
/// every row-rendering path is untouched.
///
/// A window that cuts through a tool run, an ask or a subagent's calls opens a
/// FRESH group at the boundary: the grouping state starts empty at `start`, so
/// the first in-window row of a cut group is keyed by the first in-window item
/// rather than by an item the reader cannot see. Extending the window upward
/// therefore re-keys that boundary row, which the list sync renders as a
/// replacement of one row alongside the front splice.
pub fn group_feed_row_specs_from(items: &[FeedItem], start: usize) -> Vec<FeedRowSpec> {
    let mut rows = Vec::new();
    group_feed_row_specs_into(items, start, &mut rows);
    rows
}

/// [`group_feed_row_specs_from`] into a caller-owned buffer (cleared first).
pub fn group_feed_row_specs_into(items: &[FeedItem], start: usize, rows: &mut Vec<FeedRowSpec>) {
    rows.clear();
    // Row index of the open group, keyed by ask / subagent id.
    let mut ask_rows: HashMap<String, usize> = HashMap::new();
    let mut subagent_rows: HashMap<String, usize> = HashMap::new();
    let mut i = start.min(items.len());
    while i < items.len() {
        let item = &items[i];
        if let Some(ask_id) = item.question().and_then(|card| card.ask_id.clone()) {
            match ask_rows.get(&ask_id) {
                Some(&row) => {
                    if let FeedRowSpec::Ask { items, .. } = &mut rows[row] {
                        items.push(i);
                    }
                }
                None => {
                    ask_rows.insert(ask_id.clone(), rows.len());
                    rows.push(FeedRowSpec::Ask {
                        id: item.id,
                        ask_id,
                        items: vec![i],
                    });
                }
            }
            i += 1;
            continue;
        }
        if let Some(subagent_id) = item.subagent_id().map(str::to_string) {
            match subagent_rows.get(&subagent_id) {
                Some(&row) => {
                    if let FeedRowSpec::Subagent { items, .. } = &mut rows[row] {
                        items.push(i);
                    }
                }
                None => {
                    subagent_rows.insert(subagent_id.clone(), rows.len());
                    rows.push(FeedRowSpec::Subagent {
                        id: item.id,
                        subagent_id,
                        items: vec![i],
                    });
                }
            }
            i += 1;
            continue;
        }
        if !item.is_tool() {
            rows.push(FeedRowSpec::Single {
                id: item.id,
                item: i,
            });
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
            rows.push(FeedRowSpec::Single {
                id: item.id,
                item: i,
            });
        } else {
            rows.push(FeedRowSpec::ToolRun {
                id: item.id,
                items: (i..=end).collect(),
            });
        }
        i = end + 1;
    }
}

/// EXP-789 — the FOCUSED-subagent projection: only the rows scoped to
/// `subagent_id` (its tool calls and, since EXP-773, the prose and user turns
/// the mapper stamped with it), as ABSOLUTE indices into `items` like
/// [`group_feed_row_specs_into`]. The lifecycle markers stay out — the strip's
/// tab and the conversation header already say what they say. Consecutive
/// tool calls still collapse into a [`FeedRowSpec::ToolRun`]; nothing else
/// groups (there is no ask and no nested subagent inside one). Web
/// `AgentConversation`'s `agentItems` filter, as a row projection.
pub fn group_subagent_row_specs_into(
    items: &[FeedItem],
    start: usize,
    subagent_id: &str,
    rows: &mut Vec<FeedRowSpec>,
) {
    rows.clear();
    let scoped = |item: &FeedItem| {
        item.subagent_id() == Some(subagent_id)
            && matches!(
                item.kind,
                FeedKind::Tool { .. } | FeedKind::Narration { .. } | FeedKind::UserMessage { .. }
            )
    };
    let mut i = start.min(items.len());
    while i < items.len() {
        let item = &items[i];
        if !scoped(item) {
            i += 1;
            continue;
        }
        if !item.is_tool() {
            rows.push(FeedRowSpec::Single {
                id: item.id,
                item: i,
            });
            i += 1;
            continue;
        }
        // A run of the subagent's OWN calls, skipping over rows that belong
        // to the main line or to another subagent (they are not in this view).
        let mut run = vec![i];
        let mut j = i + 1;
        while j < items.len() {
            let next = &items[j];
            if scoped(next) {
                if !next.is_tool() {
                    break;
                }
                run.push(j);
            }
            j += 1;
        }
        let after = run.last().copied().unwrap_or(i) + 1;
        if run.len() == 1 {
            rows.push(FeedRowSpec::Single {
                id: item.id,
                item: i,
            });
        } else {
            rows.push(FeedRowSpec::ToolRun {
                id: item.id,
                items: run,
            });
        }
        i = after;
    }
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

/// EXP-789 — the tabs the subagent strip actually shows (web
/// `visibleSubagentTabs`, EXP-387): the still-RUNNING subagents, plus the
/// focused one even once it is done — a completion never yanks the reader out
/// of a conversation they are reading; the tab goes when they click away.
/// Completed runs stay readable through their inline group row in Main.
pub fn visible_subagent_tabs(
    agents: &[SubagentSummary],
    selected: Option<&str>,
) -> Vec<SubagentSummary> {
    agents
        .iter()
        .filter(|agent| !agent.done || Some(agent.subagent_id.as_str()) == selected)
        .cloned()
        .collect()
}

/// What a subagent group row displays (EXP-350) — one place for the label /
/// status / detail selection so all clients mirror it:
/// - `agent_type`: the first marker's REAL type — a later marker carrying the
///   fallback (an old desktop's completed edge) can never degrade the label;
/// - `done`: any marker completed;
/// - `detail`: the LATEST non-empty detail (the completed edge restates the
///   freshest);
/// - `tool_count`: the tool calls attributed to the subagent — the VISIBLE
///   rows, or the highest count a marker reported when that is larger
///   (EXP-748: the journal drops a subagent's oldest calls first, so a replay
///   carries the number even once the rows are gone).
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentRowSummary {
    pub agent_type: String,
    pub done: bool,
    pub detail: Option<String>,
    pub tool_count: usize,
}

pub fn summarize_subagent_row(items: &[&FeedItem]) -> SubagentRowSummary {
    let markers: Vec<(&str, SubagentStatus, Option<&str>, Option<u32>)> = items
        .iter()
        .filter_map(|item| match &item.kind {
            FeedKind::Subagent {
                agent_type,
                status,
                detail,
                tool_calls,
                ..
            } => Some((agent_type.trim(), *status, detail.as_deref(), *tool_calls)),
            _ => None,
        })
        .collect();
    let types: Vec<&str> = markers
        .iter()
        .map(|(agent_type, _, _, _)| *agent_type)
        .filter(|agent_type| !agent_type.is_empty())
        .collect();
    // EXP-748: the rows this feed still holds, or the publisher's own count
    // when it is higher — a replay whose subagent tool calls were evicted
    // still captions "done · 240 tool calls".
    let reported = markers
        .iter()
        .filter_map(|(_, _, _, tool_calls)| *tool_calls)
        .max()
        .unwrap_or(0) as usize;
    SubagentRowSummary {
        agent_type: types
            .iter()
            .find(|agent_type| **agent_type != SUBAGENT_FALLBACK_TYPE)
            .or(types.first())
            .map(|agent_type| agent_type.to_string())
            .unwrap_or_else(|| SUBAGENT_FALLBACK_TYPE.to_string()),
        done: markers
            .iter()
            .any(|(_, status, _, _)| *status == SubagentStatus::Completed),
        detail: markers
            .iter()
            .rev()
            .find_map(|(_, _, detail, _)| detail.filter(|d| !d.trim().is_empty()))
            .map(str::to_string),
        tool_count: items
            .iter()
            .filter(|item| item.is_tool())
            .count()
            .max(reported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::ConfigValue;

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
                FeedKind::Narration { text, .. } | FeedKind::UserMessage { text, .. } => {
                    text.clone()
                }
                FeedKind::Tool { name, .. } => name.clone(),
                FeedKind::Permission { tool, .. } => tool.clone(),
                FeedKind::Subagent { subagent_id, .. } => subagent_id.clone(),
                FeedKind::Question(card) => card.text.clone(),
                FeedKind::Compaction => COMPACTED_LABEL.to_string(),
            })
            .collect()
    }

    // ── Appending, trimming, blanks ────────────────────────────────────────

    /// EXP-783: a run far past the OLD 2000-event cap keeps every event —
    /// the renderer windows, the feed does not truncate.
    #[test]
    fn applies_activity_frames_and_keeps_the_whole_run() {
        let mut feed = SteerFeed::new();
        for i in 0..5_000 {
            feed.apply(ActivityEvent::narration(format!("line {i}")));
        }
        assert_eq!(feed.len(), 5_000);
        assert_eq!(texts(&feed).first().unwrap(), "line 0");
        assert_eq!(texts(&feed).last().unwrap(), "line 4999");
    }

    /// The byte budget evicts from the oldest end, keeps the newest item
    /// however large, and leaves the running count consistent.
    #[test]
    fn the_byte_budget_evicts_the_oldest_and_always_keeps_one() {
        let mut feed = SteerFeed::new();
        let chunk = "x".repeat(64 * 1024);
        // 200 × 64 KiB = 12.8 MiB, comfortably inside the 16 MiB budget.
        for i in 0..200 {
            feed.apply(ActivityEvent::narration(format!("{i}{chunk}")));
        }
        assert_eq!(feed.len(), 200, "nothing is evicted under the budget");
        assert!(feed.bytes() <= FEED_BYTE_CAP, "{} bytes", feed.bytes());

        // Past it, the oldest go and the count lands under the budget.
        for i in 200..400 {
            feed.apply(ActivityEvent::narration(format!("{i}{chunk}")));
        }
        assert!(feed.len() < 400, "the oldest were evicted");
        assert!(feed.bytes() <= FEED_BYTE_CAP, "{} bytes", feed.bytes());
        assert!(texts(&feed).last().unwrap().starts_with("399"));

        let huge = "y".repeat(FEED_BYTE_CAP + 1);
        feed.apply(ActivityEvent::narration(huge));
        assert_eq!(feed.len(), 1, "the newest item survives on its own");
        assert!(texts(&feed)[0].starts_with('y'));
    }

    /// `row_specs_from(0)` is `row_specs()` — the window is a restriction,
    /// never a different grouping.
    #[test]
    fn a_window_from_zero_is_the_whole_projection() {
        let mut feed = SteerFeed::new();
        for i in 0..50 {
            feed.apply(ActivityEvent::narration(format!("line {i}")));
            feed.apply(ActivityEvent::tool(format!("Bash {i}"), None));
            feed.apply(ActivityEvent::tool(format!("Read {i}"), None));
        }
        assert_eq!(feed.row_specs_from(0), feed.row_specs());
        assert!(feed.row_specs().len() > 1);
    }

    /// A window that cuts a tool run re-keys the boundary row onto the first
    /// item the reader can actually see.
    #[test]
    fn a_window_cut_mid_tool_run_rekeys_the_boundary_row() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("hello"));
        for i in 0..6 {
            feed.apply(ActivityEvent::tool(format!("Bash {i}"), None));
        }
        let whole = feed.row_specs();
        // narration + one tool run.
        assert_eq!(whole.len(), 2);
        let windowed = feed.row_specs_from(4);
        assert_eq!(windowed.len(), 1);
        assert_eq!(windowed[0].item_indices(), &[4, 5, 6]);
        assert_eq!(windowed[0].id(), feed.items()[4].id);
        // The absolute indices still resolve against the WHOLE feed.
        assert_eq!(windowed[0].resolve(feed.items()).id(), feed.items()[4].id);
    }

    /// EXP-783: a replay that names its span keeps the pages the reader had
    /// already scrolled back to load, instead of swapping the run away.
    #[test]
    fn a_replay_that_names_its_span_keeps_the_prefix_below_it() {
        let mut feed = SteerFeed::new();
        for seq in 0..6u64 {
            feed.apply_seq(Some(seq), ActivityEvent::narration(format!("line {seq}")));
        }
        let first_ids: Vec<FeedItemId> = feed.items().iter().map(|item| item.id).collect();

        // The relay's log only reaches back to seq 3.
        feed.apply_reset();
        for seq in 3..8u64 {
            feed.apply_seq(Some(seq), ActivityEvent::narration(format!("line {seq}")));
        }
        feed.apply_synced_from(Some(3));

        assert_eq!(
            texts(&feed),
            vec!["line 0", "line 1", "line 2", "line 3", "line 4", "line 5", "line 6", "line 7"]
        );
        // The retained rows kept their identity, so nothing the reader had
        // open or expanded moved.
        assert_eq!(
            feed.items()[..3].iter().map(|item| item.id).collect::<Vec<_>>(),
            first_ids[..3]
        );
        // …and the ids stay strictly increasing across the seam.
        assert!(feed.items().windows(2).all(|pair| pair[0].id < pair[1].id));

        // Without a span it is still the full swap.
        feed.apply_reset();
        feed.apply_seq(Some(9), ActivityEvent::narration("only"));
        feed.apply_synced();
        assert_eq!(texts(&feed), vec!["only"]);
    }

    /// EXP-783: an older page lands in FRONT, below every id on screen, and a
    /// re-asked page cannot double the transcript.
    #[test]
    fn an_older_page_is_prepended_and_never_doubles() {
        let mut feed = SteerFeed::new();
        for seq in 5..8u64 {
            feed.apply_seq(Some(seq), ActivityEvent::narration(format!("line {seq}")));
        }
        assert_eq!(feed.oldest_seq(), Some(5));

        let page: Vec<(Option<u64>, ActivityEvent)> = (2..5u64)
            .map(|seq| (Some(seq), ActivityEvent::narration(format!("line {seq}"))))
            .collect();
        assert_eq!(feed.prepend_page(page.clone()), 3);
        assert_eq!(
            texts(&feed),
            vec!["line 2", "line 3", "line 4", "line 5", "line 6", "line 7"]
        );
        assert!(feed.items().windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(feed.oldest_seq(), Some(2));

        // The same page again is entirely at-or-above the oldest seq: nothing.
        assert_eq!(feed.prepend_page(page), 0);
        assert_eq!(feed.len(), 6);

        // A page arriving mid-replay is dropped — the replay decides the prefix.
        feed.apply_reset();
        assert_eq!(
            feed.prepend_page(vec![(Some(0), ActivityEvent::narration("nope"))]),
            0
        );
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
                call_id: None,
                tool_kind: None,
                settled: false,
                failed: false,
                diff: None,
            }
        );
    }

    // ── EXP-785/786: tool_update folds into its tool row ─────────────────

    fn tool_with_id(id: &str, kind: ToolKind) -> ActivityEvent {
        ActivityEvent::Tool {
            name: "Edit".into(),
            detail: Some("src/a.rs".into()),
            id: Some(id.into()),
            tool_kind: Some(kind),
            subagent_id: None,
            at: None,
        }
    }

    fn tool_state(feed: &SteerFeed, at: usize) -> (bool, bool, Option<String>) {
        match &feed.items()[at].kind {
            FeedKind::Tool {
                settled,
                failed,
                diff,
                ..
            } => (*settled, *failed, diff.clone()),
            other => panic!("not a tool row: {other:?}"),
        }
    }

    #[test]
    fn a_tool_update_settles_and_diffs_its_row_and_never_adds_one() {
        let mut feed = SteerFeed::new();
        feed.apply(tool_with_id("tc-1", ToolKind::Edit));
        feed.apply(ActivityEvent::narration("between"));
        feed.apply(tool_with_id("tc-2", ToolKind::Execute));
        let bytes = feed.bytes();

        feed.apply(ActivityEvent::tool_update(
            "tc-1",
            Some(ToolUpdateStatus::Completed),
            Some("--- a/src/a.rs\n+++ b/src/a.rs\n".into()),
        ));
        assert_eq!(feed.len(), 3, "an update is never a row");
        assert_eq!(
            tool_state(&feed, 0),
            (true, false, Some("--- a/src/a.rs\n+++ b/src/a.rs\n".to_string()))
        );
        assert_eq!(tool_state(&feed, 2), (false, false, None));
        // The diff weighs against the budget.
        assert_eq!(feed.bytes(), bytes + "--- a/src/a.rs\n+++ b/src/a.rs\n".len());
        assert!(matches!(
            &feed.items()[0].kind,
            FeedKind::Tool { call_id: Some(id), tool_kind: Some(ToolKind::Edit), .. } if id == "tc-1"
        ));

        // A failed settle wins over the completed one; the diff stays.
        feed.apply(ActivityEvent::tool_update("tc-1", Some(ToolUpdateStatus::Failed), None));
        assert!(tool_state(&feed, 0).0);
        assert!(tool_state(&feed, 0).1);
        assert!(tool_state(&feed, 0).2.is_some());
        // A status-less update carrying only a diff does not settle.
        feed.apply(ActivityEvent::tool_update("tc-2", None, Some("+x\n".into())));
        assert_eq!(tool_state(&feed, 2), (false, false, Some("+x\n".to_string())));
    }

    #[test]
    fn a_tool_update_for_an_unknown_id_is_dropped() {
        let mut feed = SteerFeed::new();
        feed.apply(tool_with_id("tc-1", ToolKind::Read));
        let bytes = feed.bytes();
        feed.apply(ActivityEvent::tool_update(
            "tc-evicted",
            Some(ToolUpdateStatus::Failed),
            Some("+never\n".into()),
        ));
        assert_eq!(feed.len(), 1);
        assert_eq!(feed.bytes(), bytes);
        assert_eq!(tool_state(&feed, 0), (false, false, None));
        // An id-less legacy tool row never matches either.
        feed.apply(ActivityEvent::tool("Grep", None));
        feed.apply(ActivityEvent::tool_update("", Some(ToolUpdateStatus::Completed), None));
        assert_eq!(tool_state(&feed, 1), (false, false, None));
    }

    #[test]
    fn a_tool_update_folds_into_the_newest_row_with_that_id() {
        let mut feed = SteerFeed::new();
        feed.apply(tool_with_id("tc-1", ToolKind::Edit));
        feed.apply(tool_with_id("tc-1", ToolKind::Edit));
        feed.apply(ActivityEvent::tool_update("tc-1", Some(ToolUpdateStatus::Completed), None));
        assert_eq!(tool_state(&feed, 0), (false, false, None));
        assert_eq!(tool_state(&feed, 1), (true, false, None));
    }

    // ── EXP-784: the rate-limit slot ───────────────────────────────────────

    #[test]
    fn rate_limit_is_a_slot_and_an_empty_or_ok_status_clears_it() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::rate_limit(
            "allowed_warning",
            Some(1_700_000_000_000),
            Some("  80% of the 5-hour window  ".into()),
        ));
        assert!(feed.is_empty(), "never a row");
        assert_eq!(
            feed.rate_limit(),
            Some(&SessionRateLimit {
                status: "allowed_warning".into(),
                resets_at: Some(1_700_000_000_000),
                message: Some("  80% of the 5-hour window  ".into()),
            })
        );
        feed.apply(ActivityEvent::rate_limit("rejected", None, Some("".into())));
        assert_eq!(
            feed.rate_limit(),
            Some(&SessionRateLimit { status: "rejected".into(), resets_at: None, message: None })
        );
        feed.apply(ActivityEvent::rate_limit("ok", None, None));
        assert_eq!(feed.rate_limit(), None);
        feed.apply(ActivityEvent::rate_limit("rejected", None, None));
        feed.apply(ActivityEvent::rate_limit("", None, None));
        assert_eq!(feed.rate_limit(), None);
    }

    #[test]
    fn a_replay_swap_repaints_the_rate_limit_from_the_staged_events() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::rate_limit("rejected", None, None));
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("replayed"));
        feed.apply(ActivityEvent::rate_limit("allowed_warning", None, None));
        feed.apply_synced();
        assert_eq!(feed.rate_limit().map(|r| r.status.as_str()), Some("allowed_warning"));
        // A replay that carries none leaves the slot empty, not stale.
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("again"));
        feed.apply_synced();
        assert_eq!(feed.rate_limit(), None);
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

    // ── Compaction (EXP-724) ───────────────────────────────────────────────

    #[test]
    fn a_compaction_shows_the_strip_until_ended_then_leaves_a_marker() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::compaction(CompactionPhase::Started, Some("manual")));
        assert_eq!(
            feed.compacting(),
            Some(&Compaction {
                trigger: Some("manual".into())
            })
        );
        // The strip is state, never a row.
        assert!(feed.is_empty());
        feed.apply(ActivityEvent::compaction(CompactionPhase::Ended, None));
        assert_eq!(feed.compacting(), None);
        assert_eq!(feed.items()[0].kind, FeedKind::Compaction);
        // A bare `ended` (codex auto-compaction) still marks the timeline.
        feed.apply(ActivityEvent::compaction(CompactionPhase::Ended, None));
        assert_eq!(feed.len(), 2);
        assert_eq!(COMPACTING_LABEL, "Compacting context…");
        assert_eq!(COMPACTED_LABEL, "Context compacted");
    }

    #[test]
    fn the_caller_can_drop_a_stale_compaction_and_the_swap_does_too() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::compaction(CompactionPhase::Started, None));
        feed.clear_compaction();
        assert_eq!(feed.compacting(), None);

        feed.apply(ActivityEvent::compaction(CompactionPhase::Started, None));
        feed.apply_reset();
        // Still up while staging — the reset clears nothing on the spot.
        assert!(feed.compacting().is_some());
        feed.apply(ActivityEvent::narration("replayed"));
        feed.apply_synced();
        // The replay carried no `started`, so the swap re-derived "not
        // compacting".
        assert_eq!(feed.compacting(), None);
        assert_eq!(texts(&feed), vec!["replayed".to_string()]);
    }

    // ── Live config + usage (EXP-746) ──────────────────────────────────────

    fn config_event(model: &str, mode: Option<&str>) -> ActivityEvent {
        ActivityEvent::ConfigState {
            options: vec![ConfigOption {
                value: Some(model.to_string()),
                values: Some(vec![
                    ConfigValue::new("opus", "Opus"),
                    ConfigValue::new("sonnet", "Sonnet"),
                ]),
                ..ConfigOption::new("model", "Model")
            }],
            current_mode: mode.map(str::to_string),
            modes: Some(vec![ConfigMode::new("plan", "Plan")]),
            commands: Some(vec![ConfigCommand::new("review", "Review the diff")]),
            at: None,
        }
    }

    #[test]
    fn config_state_is_a_slot_not_a_row() {
        let mut feed = SteerFeed::new();
        assert_eq!(feed.config(), None);
        feed.apply(config_event("opus", Some("plan")));
        let config = feed.config().expect("a config snapshot");
        assert_eq!(config.options.len(), 1);
        assert_eq!(config.options[0].value.as_deref(), Some("opus"));
        assert_eq!(config.current_mode.as_deref(), Some("plan"));
        assert_eq!(config.modes, vec![ConfigMode::new("plan", "Plan")]);
        assert_eq!(
            config.commands,
            vec![ConfigCommand::new("review", "Review the diff")]
        );
        // The chips are state; nothing lands in the transcript.
        assert!(feed.is_empty());
        // A blank current_mode is "no mode", not a mode named "".
        feed.apply(config_event("opus", Some("  ")));
        assert_eq!(feed.config().unwrap().current_mode, None);
    }

    #[test]
    fn a_newer_config_state_replaces_the_snapshot() {
        let mut feed = SteerFeed::new();
        feed.apply(config_event("opus", Some("plan")));
        // The publisher always re-emits the FULL config — an absent modes /
        // commands list is authoritative, not "keep the old ones".
        feed.apply(ActivityEvent::ConfigState {
            options: vec![ConfigOption::new("effort", "Effort")],
            current_mode: None,
            modes: None,
            commands: None,
            at: None,
        });
        let config = feed.config().unwrap();
        assert_eq!(config.options, vec![ConfigOption::new("effort", "Effort")]);
        assert_eq!(config.current_mode, None);
        assert!(config.modes.is_empty());
        assert!(config.commands.is_empty());
        assert!(feed.is_empty());
    }

    #[test]
    fn usage_is_a_slot_and_a_zero_size_clears_it() {
        let mut feed = SteerFeed::new();
        assert_eq!(feed.usage(), None);
        feed.apply(ActivityEvent::usage(124_000, 200_000, Some(1.25)));
        assert_eq!(
            feed.usage(),
            Some(SessionUsage {
                context_used: 124_000,
                context_size: 200_000,
                cost_usd: Some(1.25),
            })
        );
        assert!(feed.is_empty());
        feed.apply(ActivityEvent::usage(130_000, 200_000, None));
        assert_eq!(feed.usage().unwrap().context_used, 130_000);
        assert_eq!(feed.usage().unwrap().cost_usd, None);
        // A zero context size is the engine saying "unknown" — clear the
        // meter rather than draw 0/0.
        feed.apply(ActivityEvent::usage(0, 0, None));
        assert_eq!(feed.usage(), None);
    }

    #[test]
    fn a_replay_swap_repaints_config_and_usage_from_the_staged_events() {
        let mut feed = SteerFeed::new();
        feed.apply(config_event("opus", Some("plan")));
        feed.apply(ActivityEvent::usage(10, 200, None));

        feed.apply_reset();
        // Still up while staging — the reset clears nothing on the spot.
        assert!(feed.config().is_some());
        assert!(feed.usage().is_some());

        // The relay replays its latest-wins slots after the log, inside the
        // same staged burst.
        feed.apply(ActivityEvent::narration("replayed"));
        feed.apply(config_event("sonnet", None));
        feed.apply(ActivityEvent::usage(20, 200, Some(0.5)));
        feed.apply_synced();
        assert_eq!(
            feed.config().unwrap().options[0].value.as_deref(),
            Some("sonnet")
        );
        assert_eq!(feed.usage().unwrap().context_used, 20);
        assert_eq!(texts(&feed), vec!["replayed".to_string()]);

        // A replay that carried NEITHER kind (an old publisher, a PTY run)
        // leaves both slots empty rather than showing stale chips.
        feed.apply_reset();
        feed.apply(ActivityEvent::narration("second"));
        feed.apply_synced();
        assert_eq!(feed.config(), None);
        assert_eq!(feed.usage(), None);
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
            message_id: None,
            subagent_id: None,
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
            message_id: None,
            subagent_id: None,
            at: None,
        });
        assert_eq!(texts(&feed), vec!["before", "orphan"]);
    }

    // ── Answer locks ───────────────────────────────────────────────────────

    #[test]
    fn an_answer_locks_its_card_until_the_ack_or_the_timeout() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]).unwrap();
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
        let key = answer_key(&feed.items()[0]).unwrap();
        feed.note_answer_sent(&key, vec!["1".into()], vec!["Yes".into()]);
        feed.fail_answer(&key);
        assert_eq!(feed.answer_state(&key).unwrap().status, AnswerStatus::Error);
        assert!(!feed.is_answer_locked(&key));
    }

    #[test]
    fn a_resolution_drops_the_lock_so_a_stale_deadline_cannot_reopen_it() {
        let mut feed = SteerFeed::new();
        feed.apply(question(Some("q1"), "Which?"));
        let key = answer_key(&feed.items()[0]).unwrap();
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
    fn an_id_less_card_is_not_answerable() {
        // EXP-730: no wire id, no answer key and no active id — the card
        // renders read-only instead of falling back to raw keystrokes.
        let mut feed = SteerFeed::new();
        feed.apply(question(None, "Id-less"));
        let item = &feed.items()[0];
        assert_eq!(answer_key(item), None);
        assert!(feed.active_question_ids().is_empty());
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
        let key = answer_key(&feed.items()[0]).unwrap();
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
        let key = answer_key(&feed.items()[0]).unwrap();
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

    /// EXP-776: the owned index specs are the SAME grouping as the borrowed
    /// rows — a renderer caching specs paints exactly what `rows()` would.
    #[test]
    fn row_specs_resolve_to_the_same_rows_as_the_borrowed_grouping() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("prose"));
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::tool("Edit", None));
        feed.apply(ActivityEvent::Subagent {
            id: "agent_01".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            id: None,
            tool_kind: None,
            subagent_id: Some("agent_01".into()),
            at: None,
        });
        for index in 1..=2u32 {
            feed.apply(ActivityEvent::Question {
                text: format!("Step {index}"),
                options: vec![QuestionOption::new("Yes", "1")],
                multi_select: None,
                plan_mode: None,
                id: Some(format!("q{index}")),
                ask_id: Some("ask_1".into()),
                index: Some(index),
                total: Some(2),
                header: None,
                at: None,
            });
        }
        feed.apply(ActivityEvent::tool("Bash", None));
        feed.apply(ActivityEvent::narration("done"));

        let specs = feed.row_specs();
        let resolved: Vec<FeedRow<'_>> = specs
            .iter()
            .map(|spec| spec.resolve(feed.items()))
            .collect();
        assert_eq!(resolved, feed.rows());
        assert_eq!(
            specs.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            feed.rows().iter().map(FeedRow::id).collect::<Vec<_>>()
        );
        // Every item lands in exactly one row, in feed order within it.
        let mut seen: Vec<usize> = specs
            .iter()
            .flat_map(|spec| spec.item_indices().iter().copied())
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..feed.items().len()).collect::<Vec<_>>());
        assert!(matches!(specs[1], FeedRowSpec::ToolRun { ref items, .. } if items == &[1, 2]));
        assert!(matches!(specs[2], FeedRowSpec::Subagent { ref items, .. } if items == &[3, 4]));
        assert!(matches!(specs[3], FeedRowSpec::Ask { ref items, .. } if items == &[5, 6]));
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
            tool_calls: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            id: None,
            tool_kind: None,
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
            id: None,
            tool_kind: None,
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
            tool_calls: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            id: None,
            tool_kind: None,
            subagent_id: Some("a1".into()),
            at: None,
        });
        feed.apply(ActivityEvent::Subagent {
            id: "a2".into(),
            agent_type: "plan".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        });
        // EXP-350: an old desktop stamps the FALLBACK type on the completed
        // edge — it must never degrade the label, and its detail wins.
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: SUBAGENT_FALLBACK_TYPE.into(),
            status: SubagentStatus::Completed,
            detail: Some("done exploring".into()),
            at: None,
            tool_calls: None,
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

    // ── EXP-789: the subagent strip (web `visibleSubagentTabs`) ────────────

    fn run(subagent_id: &str, done: bool) -> SubagentSummary {
        SubagentSummary {
            subagent_id: subagent_id.to_string(),
            agent_type: "Explore".to_string(),
            done,
            detail: None,
            tool_count: 0,
        }
    }

    /// The web test, case for case: completed runs drop, running ones stay.
    #[test]
    fn visible_tabs_drop_completed_runs_and_keep_running_ones() {
        let agents = [run("toolu_a", true), run("toolu_b", false)];
        assert_eq!(
            visible_subagent_tabs(&agents, None),
            vec![run("toolu_b", false)]
        );
    }

    #[test]
    fn the_focused_tab_survives_its_own_completion_until_deselected() {
        let agents = [run("toolu_a", true), run("toolu_b", false)];
        assert_eq!(visible_subagent_tabs(&agents, Some("toolu_a")), agents.to_vec());
        assert_eq!(
            visible_subagent_tabs(&agents, Some("toolu_b")),
            vec![run("toolu_b", false)]
        );
    }

    #[test]
    fn all_done_and_main_selected_leaves_the_strip_empty() {
        assert!(visible_subagent_tabs(&[run("toolu_a", true), run("toolu_b", true)], None)
            .is_empty());
    }

    /// EXP-789: the focused projection keeps ONLY the subagent's own rows —
    /// tool calls, prose and turns — with absolute indices, drops its
    /// lifecycle markers and everything on the main line, and still collapses
    /// its consecutive calls (skipping over foreign rows between them).
    #[test]
    fn the_focused_projection_keeps_only_the_subagents_own_rows() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("main prose")); // 0
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        }); // 1
        let scoped_tool = |name: &str, agent: &str| ActivityEvent::Tool {
            name: name.into(),
            detail: None,
            id: None,
            tool_kind: None,
            subagent_id: Some(agent.into()),
            at: None,
        };
        feed.apply(scoped_tool("Grep", "a1")); // 2
        feed.apply(ActivityEvent::tool("Bash", None)); // 3 — main line
        feed.apply(scoped_tool("Read", "a2")); // 4 — another subagent
        feed.apply(scoped_tool("Read", "a1")); // 5
        feed.apply(ActivityEvent::Narration {
            text: "found it".into(),
            before_question_id: None,
            message_id: None,
            subagent_id: Some("a1".into()),
            at: None,
        }); // 6
        feed.apply(scoped_tool("Edit", "a1")); // 7
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Completed,
            detail: None,
            at: None,
            tool_calls: None,
        }); // 8

        let items = feed.items();
        let mut rows = Vec::new();
        group_subagent_row_specs_into(items, 0, "a1", &mut rows);
        let indices: Vec<Vec<usize>> = rows.iter().map(|row| row.item_indices().to_vec()).collect();
        assert_eq!(indices, vec![vec![2, 5], vec![6], vec![7]]);
        assert!(matches!(rows[0], FeedRowSpec::ToolRun { .. }));
        assert_eq!(rows[0].id(), items[2].id);
        // The window start restricts it exactly like the main projection.
        group_subagent_row_specs_into(items, 6, "a1", &mut rows);
        let indices: Vec<Vec<usize>> = rows.iter().map(|row| row.item_indices().to_vec()).collect();
        assert_eq!(indices, vec![vec![6], vec![7]]);
        // A subagent nobody scoped a row to projects nothing.
        group_subagent_row_specs_into(items, 0, "nobody", &mut rows);
        assert!(rows.is_empty());
    }

    /// EXP-748: the journal evicts a subagent's oldest tool calls long before
    /// the main transcript, so a rejoining viewer sees a marker whose count
    /// outruns the rows it still has — the caption must follow the count.
    #[test]
    fn a_reported_tool_call_count_wins_over_the_visible_one() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        });
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            id: None,
            tool_kind: None,
            subagent_id: Some("a1".into()),
            at: None,
        });
        feed.apply(ActivityEvent::Subagent {
            id: "a1".into(),
            agent_type: "explore".into(),
            status: SubagentStatus::Completed,
            detail: None,
            at: None,
            tool_calls: Some(240),
        });
        assert_eq!(feed.subagents()[0].tool_count, 240);

        // A count that UNDERSTATES what the feed holds never shrinks the row:
        // the rows on screen are the floor.
        feed.apply(ActivityEvent::Subagent {
            id: "a2".into(),
            agent_type: "plan".into(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
        });
        for name in ["Read", "Edit"] {
            feed.apply(ActivityEvent::Tool {
                name: name.into(),
                detail: None,
                id: None,
                tool_kind: None,
                subagent_id: Some("a2".into()),
                at: None,
            });
        }
        feed.apply(ActivityEvent::Subagent {
            id: "a2".into(),
            agent_type: "plan".into(),
            status: SubagentStatus::Completed,
            detail: None,
            at: None,
            tool_calls: Some(1),
        });
        assert_eq!(feed.subagents()[1].tool_count, 2);
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
    fn a_plan_card_survives_prose_behind_it_until_it_resolves() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::Question {
            text: "The plan".into(),
            options: vec![QuestionOption::new("Approve", "1")],
            multi_select: None,
            plan_mode: Some(true),
            id: Some("plan-1".into()),
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        });
        let plan_id = feed.items()[0].id;
        // The transcript tail lags the live grid: rows flush in BEHIND a
        // picker that is still on screen — and a newer question no longer
        // retires it either, only its own resolution does.
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::narration("prose"));
        feed.apply(question(Some("q2"), "Something else"));
        let newer_id = feed.items().last().unwrap().id;
        let active = feed.active_question_ids();
        assert!(active.contains(&plan_id));
        assert!(active.contains(&newer_id));

        feed.apply(ActivityEvent::QuestionResolved {
            id: Some("plan-1".into()),
            ask_id: None,
            answers: Some(vec!["Approve".into()]),
            dismissed: None,
            at: None,
        });
        assert!(!feed.active_question_ids().contains(&plan_id));
    }

    // ── EXP-772 / EXP-773: narration fragments and subagent scoping ───────

    fn fragment(text: &str, message_id: Option<&str>, subagent_id: Option<&str>) -> ActivityEvent {
        ActivityEvent::Narration {
            text: text.into(),
            before_question_id: None,
            message_id: message_id.map(str::to_string),
            subagent_id: subagent_id.map(str::to_string),
            at: None,
        }
    }

    /// EXP-772: the ACP coalescer flushes ONE assistant message in several
    /// narration events. Same `message_id` behind the same row appends;
    /// anything else opens a new bubble.
    #[test]
    fn narration_fragments_of_one_message_merge_into_one_row() {
        let mut feed = SteerFeed::new();
        feed.apply(fragment("Reading ", Some("msg_1"), None));
        feed.apply(fragment("the file.", Some("msg_1"), None));
        assert_eq!(texts(&feed), vec!["Reading the file."]);
        assert_eq!(feed.len(), 1);

        // A different message is a different bubble.
        feed.apply(fragment("Next thought.", Some("msg_2"), None));
        assert_eq!(texts(&feed), vec!["Reading the file.", "Next thought."]);

        // A row in between blocks the merge — the fragment lands behind it.
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(fragment(" More.", Some("msg_2"), None));
        assert_eq!(
            texts(&feed),
            vec!["Reading the file.", "Next thought.", "Read", " More."]
        );

        // An id-less fragment never merges (every legacy publisher).
        let mut plain = SteerFeed::new();
        plain.apply(ActivityEvent::narration("one"));
        plain.apply(ActivityEvent::narration("two"));
        assert_eq!(plain.len(), 2);

        // A fragment stamped for a SUBAGENT is a different bubble even under
        // the same message id.
        let mut scoped = SteerFeed::new();
        scoped.apply(fragment("main", Some("msg_1"), None));
        scoped.apply(fragment("nested", Some("msg_1"), Some("toolu_task")));
        assert_eq!(texts(&scoped), vec!["main", "nested"]);
    }

    /// EXP-773: prose and user turns the mapper stamped with a subagent id
    /// belong to that subagent's group row, interleaved with its tool calls in
    /// feed order — never to the main line.
    #[test]
    fn a_subagents_prose_and_turns_fold_into_its_group() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("main line"));
        feed.apply(ActivityEvent::Subagent {
            id: "toolu_task".into(),
            agent_type: "explorer".into(),
            status: SubagentStatus::Started,
            detail: None,
            tool_calls: None,
            at: None,
        });
        feed.apply(fragment("looking around", Some("msg_9"), Some("toolu_task")));
        feed.apply(ActivityEvent::Tool {
            name: "Grep".into(),
            detail: None,
            id: None,
            tool_kind: None,
            subagent_id: Some("toolu_task".into()),
            at: None,
        });
        feed.apply(ActivityEvent::UserMessage {
            text: "keep going".into(),
            subagent_id: Some("toolu_task".into()),
            at: None,
        });
        feed.apply(ActivityEvent::narration("back on the main line"));

        let rows = feed.rows();
        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[0], FeedRow::Single(_)));
        let FeedRow::Subagent { subagent_id, items, .. } = &rows[1] else {
            panic!("the subagent's events group into one row");
        };
        assert_eq!(subagent_id, "toolu_task");
        // The marker, then its prose, its call and its turn — in feed order.
        assert_eq!(
            items
                .iter()
                .map(|item| match &item.kind {
                    FeedKind::Subagent { .. } => "marker",
                    FeedKind::Narration { .. } => "narration",
                    FeedKind::Tool { .. } => "tool",
                    FeedKind::UserMessage { .. } => "user",
                    _ => "other",
                })
                .collect::<Vec<_>>(),
            vec!["marker", "narration", "tool", "user"]
        );
        assert!(matches!(rows[2], FeedRow::Single(_)));
    }

    /// EXP-787 — the gap ladder: the space ABOVE a row, chosen from the row
    /// before it. All nine (prev, cur) pairs plus the first-row rule. This is
    /// the ONE derivation, mirrored on web (`lib/agent-feed.ts`), iOS
    /// (`AgentFeed`) and Android (`domain/AgentFeed`) — a change here is a
    /// change in all four in the same PR.
    #[test]
    fn transcript_gap_ladder() {
        use RowClass::{Prose, Tool, Turn};

        // 1. Either side is a sent turn — the loudest seam wins outright,
        //    tool chatter on the other side included.
        assert_eq!(transcript_gap(Turn, Turn), Gap::Turn);
        assert_eq!(transcript_gap(Turn, Prose), Gap::Turn);
        assert_eq!(transcript_gap(Turn, Tool), Gap::Turn);
        assert_eq!(transcript_gap(Prose, Turn), Gap::Turn);
        assert_eq!(transcript_gap(Tool, Turn), Gap::Turn);
        // 2. Two tool rows sit tight against each other.
        assert_eq!(transcript_gap(Tool, Tool), Gap::Default);
        // 3. Exactly one side is tool chatter.
        assert_eq!(transcript_gap(Tool, Prose), Gap::Tool);
        assert_eq!(transcript_gap(Prose, Tool), Gap::Tool);
        // 4. Two prose blocks.
        assert_eq!(transcript_gap(Prose, Prose), Gap::Block);

        // The FIRST row has no row before it, so it takes no space at all.
        // That is the renderer's rule, asserted here so it never grows into a
        // fifth `Gap` variant.
        let classes = [Prose, Tool, Turn];
        let gaps: Vec<Option<Gap>> = classes
            .iter()
            .enumerate()
            .map(|(ix, cur)| (ix > 0).then(|| transcript_gap(classes[ix - 1], *cur)))
            .collect();
        assert_eq!(gaps, vec![None, Some(Gap::Tool), Some(Gap::Turn)]);
    }

    /// The ladder is only as good as the classes fed to it: every row shape a
    /// transcript can render classifies, and a spec classifies exactly like
    /// the row it resolves to.
    #[test]
    fn transcript_gap_ladder_classes_every_row_shape() {
        let mut feed = SteerFeed::new();
        feed.apply(ActivityEvent::narration("thinking"));
        feed.apply(ActivityEvent::UserMessage {
            text: "do the thing".into(),
            subagent_id: None,
            at: None,
        });
        feed.apply(ActivityEvent::tool("Read", None));
        feed.apply(ActivityEvent::tool("Grep", None));
        feed.apply(ActivityEvent::Permission {
            tool: "Bash".into(),
            detail: None,
            at: None,
        });
        feed.apply(ActivityEvent::Subagent {
            id: "toolu_task".into(),
            agent_type: "explorer".into(),
            status: SubagentStatus::Started,
            detail: None,
            tool_calls: None,
            at: None,
        });
        feed.apply(ActivityEvent::compaction(CompactionPhase::Ended, None));
        feed.apply(ActivityEvent::Question {
            text: "Which one?".into(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: Some("q1".into()),
            ask_id: Some("ask1".into()),
            index: Some(1),
            total: Some(1),
            header: None,
            at: None,
        });
        feed.apply(question(Some("q2"), "And this?"));

        let rows = feed.rows();
        assert_eq!(
            rows.iter().map(FeedRow::class).collect::<Vec<_>>(),
            vec![
                RowClass::Prose, // narration
                RowClass::Turn,  // the sent message
                RowClass::Tool,  // Read + Grep collapsed into a run
                RowClass::Tool,  // the permission row
                RowClass::Tool,  // the subagent group
                RowClass::Prose, // the compaction divider
                RowClass::Prose, // the ask stepper
                RowClass::Prose, // a lone question card
            ]
        );
        // The owned specs a virtualised renderer keeps say the same thing
        // without resolving the row.
        let items = feed.items();
        assert_eq!(
            feed.row_specs()
                .iter()
                .map(|spec| spec.class(items))
                .collect::<Vec<_>>(),
            rows.iter().map(FeedRow::class).collect::<Vec<_>>()
        );
    }
}
