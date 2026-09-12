//! The frozen wire protocol's Rust mirror (masterplan-v3 §8.1) —
//! byte-for-byte against `apps/steer-relay/src/protocol.ts`. Plain serde, no
//! gpui, no tokio: unit-testable against hand-built vectors.
//!
//! Field-name discipline (each was a live native bug or a protocol subtlety):
//! the relay JSON is **camelCase** (`deviceId`, `sessionId`, `issueId`,
//! `userId`, `deviceLabel`); the tag values are **snake_case**
//! (`online`, `hello`, `start_session`, …); the input field is **`data`**
//! (UTF-8 string, ≤ 8 KiB), never `bytes`. The relay zod-validates every text
//! frame and silently drops non-conforming ones (`parseClientFrame` returns
//! `null` ⇒ ignored) — a typo is a silent hang, not an error.
//!
//! EXP-249 removed the binary PTY mirror (no client ever joined
//! `channel:'pty'`): there is no `0x01` framing, no ring, no `resync`, and no
//! geometry on the wire anymore. Every frame here is TEXT.
//!
//! Steering v2 (EXP-249) adds, all optional/additive on the wire: question
//! identity + the multi-question stepper on [`ActivityEvent::Question`], the
//! `question_resolved` / `answer_ack` / `subagent` / `permission` kinds, the
//! publisher-only [`ClientFrame::ActivityReset`], and the semantic
//! [`ServerFrame::Answer`] that replaced blind keystroke replay — EXP-730:
//! the ONLY answer path there is now.
//!
//! ## Roles and directions (EXP-696)
//!
//! The file used to be strictly "[`ClientFrame`] serializes, [`ServerFrame`]
//! deserializes" because the desktop only ever spoke as a PUBLISHER or a
//! CONTROL socket. The VIEWER role reverses several frames:
//!
//! * a viewer SENDS `join` / `input` / `answer` / `kill` — hence
//!   [`ClientFrame::Answer`], the send-side twin of [`ServerFrame::Answer`];
//! * a viewer RECEIVES `activity` / `activity_reset` / `activity_synced` /
//!   `keepalive` / `bye` / `error` — hence [`ViewerFrame`], which is the
//!   viewer's COMPLETE inbound vocabulary and is deserialize-only.
//!
//! [`ViewerFrame`] is a separate enum rather than four more [`ServerFrame`]
//! variants on purpose: the two roles have disjoint inbound vocabularies, and
//! a shared enum would force the publisher/control pumps to carry
//! "impossible here" arms (and to grow one every time the viewer protocol
//! does). Both parse functions drop unknown `t` tags to `None` — the relay's
//! own silent-drop posture, so a future frame never kills a socket.
//!
//! [`ActivityEvent`] and [`QuestionOption`] are therefore BOTH `Serialize`
//! (publisher) and `Deserialize` (viewer). Every optional field carries
//! `#[serde(default)]` beside its `skip_serializing_if` — without it a frame
//! that legitimately omits the field fails to parse and the event is dropped.

use serde::{Deserialize, Serialize};

/// Close codes (protocol.ts). Handle each distinctly (§8.6).
pub const CLOSE_SESSION_ENDED: u16 = 4001;
pub const CLOSE_REPLACED: u16 = 4002;
pub const CLOSE_UNAUTHORIZED: u16 = 4003;
pub const CLOSE_SLOW_CONSUMER: u16 = 4008;

/// `control` | `publisher` | `viewer` — mirrors `SteerRole`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SteerRole {
    Control,
    Publisher,
    Viewer,
}

// ── Client → relay (TEXT frames, JSON `{t, …}`) ─────────────────────────────

/// Every frame this publisher may send. Serialize-only (the relay never
/// echoes client frames back).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientFrame<'a> {
    /// EXP-672: presence ONLY — the deviceId → socket map and nothing more.
    /// EXP-485 took the agent/launch-defaults advertisement off this frame
    /// and PR #584 took `deviceLabel` + the EXP-253 `caps` array: the web
    /// server reads BOTH off the persisted `devices` row written by
    /// `devices.register`, which survives relay restarts and doesn't need a
    /// re-dial to change. Never re-add a field here — `onlineFrame` in
    /// `protocol.ts` parses non-strictly precisely so shipped clients that
    /// still send the old keys keep registering.
    #[serde(rename_all = "camelCase")]
    Online { device_id: &'a str },
    #[serde(rename_all = "camelCase")]
    Hello {
        session_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        issue_id: Option<&'a str>,
        /// EXP-90: the anonymous public-activity audience is removed, so the
        /// publisher ALWAYS sends `Some(false)`. The field survives because
        /// `None` = absent means "public" to LEGACY relays (pre-EXP-90 fan
        /// activity to anonymous sockets when the key is missing) — relay
        /// deploys are manual, so the explicit `false` must stay on the wire.
        #[serde(skip_serializing_if = "Option::is_none")]
        activity_public: Option<bool>,
    },
    /// Join the ONE audience the relay has (EXP-696, the viewer role).
    ///
    /// `channel` is REQUIRED by the relay's zod (`z.literal('activity')`,
    /// deliberately not optional) and a join without it is dropped in
    /// silence — the bare `{"t":"join"}` this variant serialized before
    /// EXP-696 would never have joined anything. Build it with
    /// [`ClientFrame::join`] so the literal cannot drift.
    Join {
        channel: &'a str,
    },
    /// NOTE: the field is `data` — a UTF-8 `String`, relay-enforced ≤ 8 KiB —
    /// NOT `bytes`. (A native client shipped `bytes` and steer input silently
    /// no-op'd.)
    Input {
        data: String,
    },
    /// EXP-696 (viewer role): the semantic answer to an
    /// [`ActivityEvent::Question`] — the send-side twin of
    /// [`ServerFrame::Answer`], which is what the publisher reads out of the
    /// relay. `keys` are the option keys of THAT question (relay-capped at 10
    /// of ≤8 chars); `text` is the EXP-513 typed reply for a `freeText` row
    /// (≤4000 chars). Owned fields — an answer is built once, from a card the
    /// viewer already holds, and handed to the socket task.
    #[serde(rename_all = "camelCase")]
    Answer {
        question_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        keys: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Kill,
    /// EXP-746 (viewer role): change ONE live agent option the publisher
    /// advertised in [`ActivityEvent::ConfigState`]. Fire-and-forget — the
    /// publisher's next `config_state` IS the confirmation, so there is no
    /// ack frame and no optimistic lock (unlike [`ClientFrame::Answer`]). A
    /// BLANK `value` is the "CLI default / unset" choice, which is why the
    /// relay's zod deliberately has no `min(1)` on it. Owned fields like
    /// [`ClientFrame::Answer`]: built once from a chip the viewer holds and
    /// handed to the socket task.
    SetConfig { id: String, value: String },
    /// EXP-746 (viewer role): switch to one of `config_state.modes`.
    SetMode { id: String },
    Bye {
        #[serde(skip_serializing_if = "Option::is_none")]
        outcome: Option<&'a str>,
    },
    /// One activity event (§P7 live-coding view). Serializes to
    /// `{"t":"activity","event":{...}}`; the relay fans it to authenticated
    /// activity members only (EXP-90 removed the anonymous public audience).
    /// The event text is ALREADY redacted by the emitter.
    #[serde(rename_all = "camelCase")]
    Activity {
        event: ActivityEvent,
        /// EXP-783: this event's monotonic index within the run, assigned by
        /// the publisher's `Recorder` and seeded from the journal file's line
        /// count so a resumed run keeps counting. The relay echoes it
        /// untouched; it is the ONLY monotonic anchor on the wire, and it is
        /// what lets a viewer splice a join replay onto a transcript prefix it
        /// already holds. `None` on a replayed journal line older than
        /// EXP-783.
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    /// EXP-783 (viewer role): ask for the page of transcript BELOW `before_seq`.
    /// The relay routes it to the room's LIVE publisher — or, in a history
    /// room the device already replayed and left (EXP-796, the room lingers
    /// five minutes), down that device's CONTROL socket — under an id of its
    /// own, and translates the answering [`ClientFrame::HistoryChunk`]s back
    /// to this `request_id`.
    #[serde(rename_all = "camelCase")]
    HistoryPage {
        request_id: String,
        before_seq: u64,
        limit: u32,
    },
    /// EXP-783 (publisher/device role): one page of older transcript, answering
    /// a [`ServerFrame::HistoryPage`]. The relay delivers it to the ONE viewer
    /// that asked and never adds it to the room's replay log.
    ///
    /// EXP-796: `session_id` is REQUIRED when the answer goes down the
    /// device's CONTROL socket (one socket serves every session the machine
    /// ran, so the chunk has to say which room it answers) and stays `None`
    /// from a publisher socket, whose wire form is unchanged.
    #[serde(rename_all = "camelCase")]
    HistoryChunk {
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        request_id: String,
        events: Vec<ActivityEvent>,
        seqs: Vec<u64>,
        done: bool,
    },
    /// PUBLISHER-only (EXP-249): drop the room's replay log + last diff and
    /// tell the activity audience to clear its feed. Sent right before a
    /// full-history re-publish, so a reconnect never doubles the feed.
    ActivityReset,
}

/// A single public activity event (masterplan §P7) — the desktop emits these
/// from the Claude hooks sidecar ([`crate::hooks`]) + worktree diffs, already
/// redacted. Wire mirror of `apps/steer-relay/src/protocol.ts`
/// `activityEventSchema` (discriminated on `kind`). Serialize-only.
///
/// `at` is optional epoch-millis on EVERY kind: live events may omit it, a
/// full-history re-publish after a reconnect carries the ORIGINAL stamps so
/// the replayed feed keeps its timeline.
///
/// EXP-696: also `Deserialize` — the viewer role reads these events back off
/// the wire. Every optional field carries `#[serde(default)]`, so an event
/// that omits it parses instead of failing (an internally-tagged enum has no
/// per-field fallback). An event whose `kind` this build does not know fails
/// the parse and is dropped by the caller, matching every other client's
/// "ignore future kinds, never kill the socket" rule.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityEvent {
    /// Assistant prose (a `text` content block).
    Narration {
        text: String,
        /// EXP-483: claude withholds the transcript entry that carries an
        /// `AskUserQuestion`/`ExitPlanMode` tool_use — and any prose in that
        /// SAME entry — until the picker resolves, so the prose reaches the
        /// wire AFTER the already-published card. When set, this is the
        /// claude `tool_use_id` of that ask/plan: clients splice the
        /// narration immediately BEFORE the first feed question whose
        /// `askId` or `id` equals it (no match → append as ever).
        #[serde(
            rename = "beforeQuestionId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        before_question_id: Option<String>,
        /// EXP-772: the agent's own message id (the mapper's coalescer key).
        /// One message flushes in several pieces on a long turn, so clients
        /// MERGE a narration row into the previous one when both carry the
        /// same id instead of painting a new bubble per flush.
        #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        /// EXP-773: the subagent this prose belongs to (same identity as
        /// [`ActivityEvent::Tool::subagent_id`]). Set → the row belongs INSIDE
        /// that subagent's card, never the main feed.
        #[serde(rename = "subagentId", default, skip_serializing_if = "Option::is_none")]
        subagent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A tool-call headline: the tool name + a single primary argument
    /// (file path / pattern / Bash description — NEVER a command string or a
    /// tool result). `subagentId` attributes the call to a running
    /// [`ActivityEvent::Subagent`] so clients can nest it under that agent.
    ///
    /// EXP-785: `id` is the ACP tool-call id (the key a later
    /// [`ActivityEvent::ToolUpdate`] folds into this row by) and `tool_kind`
    /// is ACP's kind bucket, so a client can tell an edit from a command
    /// without parsing the name. Both absent from pre-EXP-785 publishers. The
    /// wire key is `toolKind`, never `kind`: `kind` is this enum's tag.
    #[serde(rename_all = "camelCase")]
    Tool {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_kind: Option<ToolKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-785/786: a tool call SETTLED (`status`) and/or an `edit` call's
    /// per-file unified diff (`diff`, already redacted and cut to the
    /// contract's `toolDiffMaxLines`/`toolDiffMaxBytes` on line boundaries; a
    /// cut patch ends in a `\ N more lines truncated` marker line). A LOG row
    /// on the wire and in every journal, but never a row on screen: clients
    /// fold it INTO the [`ActivityEvent::Tool`] row whose `id` matches and
    /// DROP one for an id they do not hold (evicted, or before their window).
    #[serde(rename_all = "camelCase")]
    ToolUpdate {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<ToolUpdateStatus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
        /// EXP-846: the SUBJECT an Exponential MCP call settled on, pulled out
        /// of the tool's JSON result — the issue it created, the PR it opened,
        /// how many rows a list answered with. Set ONLY for our own
        /// `exponential_*` tools (nothing else has a result shape we know), and
        /// every field optional: a tool that named none sends no preview at
        /// all. Clients store it on the tool row; what they DRAW from it is a
        /// later phase.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview: Option<ToolPreview>,
    },
    /// A worktree unified diff snapshot (latest replaces prior, viewer-side).
    Diff {
        diff: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A human turn from the transcript: the initial prompt or a (locally- or
    /// remotely-)steered message (EXP-78). MEMBER-ONLY on the relay — never
    /// fanned to anonymous public viewers ("never steering input").
    UserMessage {
        text: String,
        /// EXP-773: the subagent whose turn this is; set → the row renders
        /// inside that subagent's card, never the main feed.
        #[serde(rename = "subagentId", default, skip_serializing_if = "Option::is_none")]
        subagent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// An interactive question the session is blocked on (`AskUserQuestion`
    /// question, or the `ExitPlanMode` plan-approval picker). MEMBER-ONLY.
    /// `options[].key` is the raw keystroke a steering client sends to pick
    /// that option — the desktop owns the TUI key mapping, clients stay dumb.
    ///
    /// `id` is the stable question identity derived from claude's
    /// `tool_use_id` (plan = the id itself; ask question `i` (0-based) =
    /// `<id>#<i>`; the review/submit step = `<askId>#submit`). Re-emitting the
    /// SAME id REPLACES that card in place — the options may grow later (a
    /// "Type something" choice only the TUI grid reveals). `id` is REQUIRED:
    /// EXP-730 retired the blind-keystroke answer path an id-less card had,
    /// and the relay rejects such frames, so a card without an identity has
    /// no reader left.
    #[serde(rename_all = "camelCase")]
    Question {
        text: String,
        options: Vec<QuestionOption>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        multi_select: Option<bool>,
        /// `Some(true)` when this question is an `ExitPlanMode` plan-approval
        /// picker (EXP-97) — clients render a dedicated "Plan ready" card.
        /// Presentation-only: the options remain the source of the keystrokes.
        /// `text` is then the full plan markdown, and `askId`/`index`/`total`
        /// are absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_mode: Option<bool>,
        id: String,
        /// Groups the steps of ONE multi-question `AskUserQuestion`. A step
        /// carries `index`/`total`; the FINAL review/submit step carries
        /// `askId` with neither.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        header: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A question stopped being answerable: answered here or elsewhere,
    /// dismissed, or the whole ask was submitted. Retires the card with `id`
    /// when present, otherwise EVERY card of `askId`.
    #[serde(rename_all = "camelCase")]
    QuestionResolved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answers: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dismissed: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// The desktop injected a steerer's answer into the TUI — clients keep
    /// the card LOCKED from here until the matching
    /// [`ActivityEvent::QuestionResolved`].
    #[serde(rename_all = "camelCase")]
    AnswerAck {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A `Task` subagent's lifecycle edge — `id` keys the
    /// [`ActivityEvent::Tool`] events attributed to it.
    ///
    /// EXP-748: `tool_calls` is the publisher's count of tool events this
    /// subagent made, stamped on the `completed` edge. Replay buffers evict
    /// subagent tool events first (they carry little for a viewer), so the
    /// count survives where the rows do not; clients render
    /// `max(visible tool rows, toolCalls)`.
    ///
    /// EXP-847: `title` is the spawning `Agent` tool call's own `description`
    /// input (its `name` as a fallback) — what the model said this subagent is
    /// FOR, which is what a reader wants on the chip. Clients show it and keep
    /// `agentType` as a secondary caption; absent for codex, an external agent
    /// and every pre-847 publisher, where `agentType` is all there is.
    #[serde(rename_all = "camelCase")]
    Subagent {
        id: String,
        agent_type: String,
        status: SubagentStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_calls: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// EXP-850 §4: the [`crate::workflow::WorkflowState::id`] this agent
        /// belongs to, set on EVERY edge whose task id equals a workflow
        /// agent's `agentId`. Clients nest such an edge under the workflow
        /// card and never count it as a loose subagent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow_id: Option<String>,
    },
    /// The session is sitting on a permission prompt. INFORMATIONAL — it
    /// carries no options and is never answerable remotely (the local TUI
    /// owns permission decisions).
    Permission {
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-724: the agent is compacting its context. `started` opens an
    /// indeterminate "Compacting context…" strip on every viewer, `ended`
    /// closes it and leaves a "Context compacted" marker in the feed. A
    /// publisher that only observes the END (codex auto-compaction) sends a
    /// bare `ended`; viewers time a lone `started` out. `trigger` is
    /// claude's PreCompact trigger (`manual` | `auto`), absent on codex —
    /// kept a plain string so a
    /// future value never fails the parse.
    Compaction {
        phase: CompactionPhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trigger: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-746: the agent's LIVE configuration — the chip vocabulary AND the
    /// values in force, re-emitted in FULL on every change (a `set_config` /
    /// `set_mode` that landed, a model the agent switched itself, an ACP
    /// `current_mode_update` / `available_commands_update`). LATEST-WINS
    /// state on every client (a snapshot slot beside the diff, never a feed
    /// row): the relay keeps only the newest per room and the journal drops
    /// its predecessor, so a joining viewer paints its chips from one frame.
    ///
    /// Field order here IS serialization order and the relay's zod is
    /// declared in the same one — never reorder (`config_state` test
    /// vectors are byte-exact).
    #[serde(rename_all = "camelCase")]
    ConfigState {
        options: Vec<ConfigOption>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modes: Option<Vec<ConfigMode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commands: Option<Vec<ConfigCommand>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-746: the run's context window + spend as the engine last measured
    /// it. LATEST-WINS state like [`ActivityEvent::ConfigState`]. Deliberately
    /// TOKENS, not a percent — the device-reported `DeviceUsageWindow`
    /// already owns the 0-100 rate-limit vocabulary and this is a different
    /// quantity.
    #[serde(rename_all = "camelCase")]
    Usage {
        context_used: i64,
        context_size: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-784: the agent is rate-limited (or was, and is not any more).
    /// LATEST-WINS state like [`ActivityEvent::Usage`], the fourth slot in
    /// every registry (`journal.rs`, `history.rs`, the engine's `FeedState`,
    /// `feed.rs`, the relay's `LATEST_WINS_KINDS`). `status` is the agent's
    /// own word for the window (`allowed_warning`, `rejected`, …); an EMPTY
    /// status or `"ok"` CLEARS the slot, the way a zero-size `usage` clears
    /// the meter. `resets_at` is a unix-ms instant when the agent names one.
    #[serde(rename_all = "camelCase")]
    RateLimit {
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-848: the END-OF-TURN signal — `started` while the agent is
    /// executing a turn, `ended` the moment it is over (`end_turn`, a cancel,
    /// a failed prompt request). LATEST-WINS state like
    /// [`ActivityEvent::RateLimit`], the FIFTH slot in every registry
    /// (`journal.rs`, `history.rs`, the engine's `FeedState`, `feed.rs`, the
    /// relay's `LATEST_WINS_KINDS`) — never a feed row. Before the first one
    /// arrives every client assumes `ended`, so a viewer never pulses
    /// "Working…" at a run that is merely connected.
    #[serde(rename_all = "camelCase")]
    Turn {
        state: TurnState,
        /// EXP-850 §5: when this turn STARTED (unix ms), carried on BOTH
        /// states so a viewer that joins mid-turn can run the working
        /// caption's duration clock. Absent from every pre-850 publisher.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<i64>,
        /// EXP-850 §5: output tokens produced in this turn so far (claude's
        /// `thinking_tokens` plus the assistant messages' `output_tokens`),
        /// monotone within the turn and republished at most every
        /// `steerWorking.tokenTickMs` — the ONE exception to this slot's
        /// identical-edge dedupe.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-850 §2: the agent's background tasks, as the CLI last listed them
    /// in FULL (an empty array = nothing running). LATEST-WINS state like
    /// [`ActivityEvent::Turn`] — never a transcript row. Clients render one
    /// line per task in the strip above the composer, beside one
    /// `Waiting on {detail}` line per unsettled [`ToolKind::Wait`] row.
    #[serde(rename_all = "camelCase")]
    BackgroundTasks {
        tasks: Vec<BackgroundTask>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// EXP-850 §3: a claude `Workflow` run's live card. LATEST-WINS PER ID
    /// (the relay keys its slot `workflow:{id}`, the journal and the on-disk
    /// history fold by id, the feed keeps a side map): the publisher always
    /// sends the WHOLE state, so the newest frame for an id replaces its
    /// predecessor. `id` is the `Workflow` tool call's own id, so clients
    /// patch the card onto that tool row instead of appending a second one.
    Workflow(crate::workflow::WorkflowState),
}

/// EXP-850 §2: one entry of the [`ActivityEvent::BackgroundTasks`] list.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    /// The CLI's own `task_id` — what a `TaskOutput` call names.
    pub id: String,
    pub kind: BackgroundTaskKind,
    /// Cut to the contract's `steerWorking.previewMax` by the publisher.
    pub description: String,
    /// The `tool_use_id` of the call that launched it, when the CLI named one
    /// — the tool row a client may link the line to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
}

/// Contract `backgroundTaskKind` — claude's `task_type` folded onto four
/// values (`local_bash` = `shell`, `local_workflow` = `workflow`,
/// `local_agent` = `agent`, anything else = `other`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskKind {
    Shell,
    Workflow,
    Agent,
    #[default]
    Other,
}

impl BackgroundTaskKind {
    /// Every value, in contract order.
    pub const ALL: [BackgroundTaskKind; 4] = [
        BackgroundTaskKind::Shell,
        BackgroundTaskKind::Workflow,
        BackgroundTaskKind::Agent,
        BackgroundTaskKind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BackgroundTaskKind::Shell => "shell",
            BackgroundTaskKind::Workflow => "workflow",
            BackgroundTaskKind::Agent => "agent",
            BackgroundTaskKind::Other => "other",
        }
    }

    /// The CLI's `task_type` word.
    pub fn from_task_type(task_type: &str) -> BackgroundTaskKind {
        match task_type {
            "local_bash" => BackgroundTaskKind::Shell,
            "local_workflow" => BackgroundTaskKind::Workflow,
            "local_agent" => BackgroundTaskKind::Agent,
            _ => BackgroundTaskKind::Other,
        }
    }
}

/// EXP-850 §2: how many background tasks one frame may carry (the relay's
/// zod cap). A machine running more than this has other problems.
pub const BACKGROUND_TASKS_MAX: usize = 32;

/// EXP-846: what an Exponential MCP call settled on, as the tool itself
/// reported it. Every field optional and independently meaningful — a
/// `pr_open` names `identifier`/`url`, an `issues_list` only `count` — and
/// every string capped at [`TOOL_PREVIEW_TEXT_MAX`] so one runaway answer
/// cannot widen a frame. The result KIND (issue, pr, list, …) is not on the
/// wire: it is the contract's `expToolResults` entry for the tool's own name,
/// which every client already has.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreview {
    /// The row's uuid, when the tool answered with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A human identifier (`EXP-42`) — what a pill renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A PR/issue url the answer carried (`prUrl`/`url`/`htmlUrl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// How many rows a LIST answered with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// The subject's status word, when the answer named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// EXP-846: the cap on every [`ToolPreview`] string — generous for an issue
/// title, far below the narration cap, and mirrored by the relay's zod.
pub const TOOL_PREVIEW_TEXT_MAX: usize = 200;

impl ToolPreview {
    /// Nothing to show: a preview with no field set is never published.
    pub fn is_empty(&self) -> bool {
        self.id.is_none()
            && self.identifier.is_none()
            && self.title.is_none()
            && self.url.is_none()
            && self.count.is_none()
            && self.status.is_none()
    }

    /// Every free-text field, mutably — the redactor walks these like any
    /// other published string.
    pub fn text_fields_mut(&mut self) -> Vec<&mut String> {
        [
            self.id.as_mut(),
            self.identifier.as_mut(),
            self.title.as_mut(),
            self.url.as_mut(),
            self.status.as_mut(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Cap every string at [`TOOL_PREVIEW_TEXT_MAX`] (the relay drops a frame
    /// whose preview is wider, so the producer cuts first).
    pub fn clamp(mut self) -> Self {
        for field in self.text_fields_mut() {
            if field.chars().count() > TOOL_PREVIEW_TEXT_MAX {
                *field = crate::activity::truncate(field, TOOL_PREVIEW_TEXT_MAX);
            }
        }
        self
    }
}

/// EXP-785: ACP's tool-call kind on the wire — the contract's `toolKind`
/// values, byte-locked by `tool_kind_matches_the_contract`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    /// EXP-850 §1: the call is WAITING on something else to finish — claude's
    /// `TaskOutput` (block on a background task) and `Monitor` (watch a
    /// stream). Rendered at the bottom strip as `Waiting on {detail}` while it
    /// is unsettled, and an ordinary tool row in the transcript;
    /// `tool_group_summary` counts it exactly like `other`.
    Wait,
    Other,
}

impl ToolKind {
    /// Every kind, in contract order.
    pub const ALL: [ToolKind; 11] = [
        ToolKind::Read,
        ToolKind::Edit,
        ToolKind::Delete,
        ToolKind::Move,
        ToolKind::Search,
        ToolKind::Execute,
        ToolKind::Think,
        ToolKind::Fetch,
        ToolKind::SwitchMode,
        ToolKind::Wait,
        ToolKind::Other,
    ];

    /// The wire / contract value.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolKind::Read => "read",
            ToolKind::Edit => "edit",
            ToolKind::Delete => "delete",
            ToolKind::Move => "move",
            ToolKind::Search => "search",
            ToolKind::Execute => "execute",
            ToolKind::Think => "think",
            ToolKind::Fetch => "fetch",
            ToolKind::SwitchMode => "switch_mode",
            ToolKind::Wait => "wait",
            ToolKind::Other => "other",
        }
    }

    /// `None` for a value this build does not know (a future contract).
    pub fn parse(value: &str) -> Option<ToolKind> {
        ToolKind::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// `completed` | `failed` — how an [`ActivityEvent::ToolUpdate`] settled its
/// call. A `failed` after a `completed` wins (the last word is the agent's).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolUpdateStatus {
    Completed,
    Failed,
}

/// EXP-784: the two `rate_limit.status` values that CLEAR the slot.
pub fn rate_limit_clears(status: &str) -> bool {
    let status = status.trim();
    status.is_empty() || status.eq_ignore_ascii_case("ok")
}

/// FEED-35: whether a `rate_limit` report is a WALL — the agent refused a
/// call — as opposed to the informational report claude files on every turn
/// past ~75% of a window (`allowed_warning`) while it keeps answering. The
/// ONE rule for the viewer's banner (desktop `steer_viewer`, web
/// `rateLimitIsWall`) and, since FEED-35, for the row's durable `blocked`
/// (`engine::mapper::emit_rate_limit`): a warning must never read as a
/// blocked run to a teammate's list or a parent agent. `rejected` is the
/// CLI's own word for the refusal; a message is the synthetic "You've hit
/// your…" notice, which only ever accompanies one.
pub fn rate_limit_is_wall(status: &str, message: Option<&str>) -> bool {
    status.trim() == "rejected" || message.is_some_and(|text| !text.trim().is_empty())
}

/// EXP-831: how long past its `resets_at` a wall still renders (ms). The
/// engine clears the slot on the run's next activity or its next rate-limit
/// event; until one arrives (and on a journal replayed after the fact) the
/// clock is the only thing that can drop a banner whose reset has come and
/// gone. One minute covers clock skew between the agent's stamp and ours.
pub const RATE_LIMIT_EXPIRY_GRACE_MS: i64 = 60_000;

/// EXP-831: whether a wall's reset time (unix ms) has passed by more than
/// [`RATE_LIMIT_EXPIRY_GRACE_MS`] at `now_ms`. Mirrored ×4 (web
/// `rateLimitExpired`, iOS `AgentFeed.rateLimitExpired`, Android
/// `rateLimitExpired`). A wall with no reset time never expires by the clock.
pub fn rate_limit_expired(resets_at: Option<i64>, now_ms: i64) -> bool {
    resets_at.is_some_and(|at| now_ms - at > RATE_LIMIT_EXPIRY_GRACE_MS)
}

/// `started` | `ended` — the two [`ActivityEvent::Compaction`] edges.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompactionPhase {
    Started,
    Ended,
}

/// EXP-848: `started` | `ended` — the two [`ActivityEvent::Turn`] edges
/// (contract `turnState`). Clients DEFAULT to `Ended`: before any turn event
/// arrives a run is assumed idle, so nothing ever pulses by default.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnState {
    Started,
    #[default]
    Ended,
}

impl TurnState {
    /// The contract `turnState` id.
    pub fn id(self) -> &'static str {
        match self {
            TurnState::Started => "started",
            TurnState::Ended => "ended",
        }
    }

    /// Whether the agent is executing a turn right now.
    pub fn is_working(self) -> bool {
        matches!(self, TurnState::Started)
    }
}

/// `started` | `completed` | `duplicate` — the [`ActivityEvent::Subagent`]
/// edges (contract `subagentStatus`). EXP-856 added `duplicate`: a
/// `task_started` arrived for an id that is ALREADY live (a `SendMessage` to a
/// running agent resumes it as a second copy), which is a warning row, not a
/// lifecycle edge — the duplicate's own `started`/`completed` still follow
/// under the same id. Old clients ignore the unknown value.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubagentStatus {
    Started,
    Completed,
    Duplicate,
}

impl SubagentStatus {
    /// Every value, in contract order.
    pub const ALL: [SubagentStatus; 3] = [
        SubagentStatus::Started,
        SubagentStatus::Completed,
        SubagentStatus::Duplicate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SubagentStatus::Started => "started",
            SubagentStatus::Completed => "completed",
            SubagentStatus::Duplicate => "duplicate",
        }
    }
}

impl ActivityEvent {
    /// The legacy shorthands — the shapes with no v2 fields at all.
    pub fn narration(text: impl Into<String>) -> Self {
        ActivityEvent::Narration {
            text: text.into(),
            before_question_id: None,
            message_id: None,
            subagent_id: None,
            at: None,
        }
    }

    pub fn diff(diff: impl Into<String>) -> Self {
        ActivityEvent::Diff { diff: diff.into(), at: None }
    }

    pub fn user_message(text: impl Into<String>) -> Self {
        ActivityEvent::UserMessage { text: text.into(), subagent_id: None, at: None }
    }

    pub fn tool(name: impl Into<String>, detail: Option<String>) -> Self {
        ActivityEvent::Tool {
            name: name.into(),
            detail,
            id: None,
            tool_kind: None,
            subagent_id: None,
            at: None,
        }
    }

    /// EXP-785/786: a settle and/or a per-call diff for the tool row `id`.
    pub fn tool_update(
        id: impl Into<String>,
        status: Option<ToolUpdateStatus>,
        diff: Option<String>,
    ) -> Self {
        ActivityEvent::ToolUpdate {
            id: id.into(),
            status,
            diff,
            at: None,
            preview: None,
        }
    }

    /// EXP-784: the rate-limit slot; an empty/`ok` status clears it.
    pub fn rate_limit(
        status: impl Into<String>,
        resets_at: Option<i64>,
        message: Option<String>,
    ) -> Self {
        ActivityEvent::RateLimit {
            status: status.into(),
            resets_at,
            message,
            at: None,
        }
    }

    /// EXP-848: the turn edge — the spinner's one source of truth. EXP-850
    /// §5 added the working caption's two inputs; [`ActivityEvent::turn_at`]
    /// is the shorthand that carries them.
    pub fn turn(state: TurnState) -> Self {
        ActivityEvent::Turn {
            state,
            started_at: None,
            tokens: None,
            at: None,
        }
    }

    /// EXP-850 §5: the turn edge WITH the working caption's inputs — the
    /// turn's start (unix ms) and the output tokens it has produced so far
    /// (`None`/zero = not known yet, and the caption omits the group).
    pub fn turn_at(state: TurnState, started_at: Option<i64>, tokens: Option<u64>) -> Self {
        ActivityEvent::Turn {
            state,
            started_at,
            tokens: tokens.filter(|tokens| *tokens > 0),
            at: None,
        }
    }

    /// EXP-850 §2: the background-task slot; an EMPTY list is the publisher
    /// saying nothing runs any more (the strip closes), never silence.
    pub fn background_tasks(tasks: Vec<BackgroundTask>) -> Self {
        ActivityEvent::BackgroundTasks { tasks, at: None }
    }

    /// EXP-850 §3: one workflow card's whole state.
    pub fn workflow(state: crate::workflow::WorkflowState) -> Self {
        ActivityEvent::Workflow(state)
    }

    /// EXP-724 compaction edge; `trigger` is `manual`/`auto` when known.
    pub fn compaction(phase: CompactionPhase, trigger: Option<&str>) -> Self {
        ActivityEvent::Compaction {
            phase,
            trigger: trigger.map(str::to_string),
            at: None,
        }
    }

    /// EXP-746 context/spend meter. No `ConfigState` twin: the engine builds
    /// that snapshot whole (options, mode, commands) and there is nothing a
    /// shorthand could leave out.
    pub fn usage(context_used: i64, context_size: i64, cost_usd: Option<f64>) -> Self {
        ActivityEvent::Usage {
            context_used,
            context_size,
            cost_usd,
            at: None,
        }
    }

    /// Every FREE-TEXT field of the event, mutably (EXP-511: the publisher
    /// walks them to put a localized image path back to the embed token the
    /// steerer sent — a local path must never reach the published feed,
    /// whichever kind ends up quoting it). Deliberately excludes the machine
    /// fields — ids, `QuestionOption::key` (raw keystrokes) — which no rewrite
    /// may touch.
    pub fn text_fields_mut(&mut self) -> Vec<&mut String> {
        match self {
            ActivityEvent::Narration { text, .. } | ActivityEvent::UserMessage { text, .. } => {
                vec![text]
            }
            ActivityEvent::Tool { name, detail, .. } => {
                let mut fields = vec![name];
                fields.extend(detail.as_mut());
                fields
            }
            ActivityEvent::Diff { diff, .. } => vec![diff],
            // EXP-846: the preview's strings are the tool's OWN answer (an
            // issue title, a PR url) and pass through the redactor like any
            // other free text.
            ActivityEvent::ToolUpdate { diff, preview, .. } => {
                let mut fields: Vec<&mut String> = diff.as_mut().into_iter().collect();
                if let Some(preview) = preview {
                    fields.extend(preview.text_fields_mut());
                }
                fields
            }
            ActivityEvent::RateLimit { message, .. } => message.as_mut().into_iter().collect(),
            ActivityEvent::Question {
                text,
                options,
                header,
                ..
            } => {
                let mut fields = vec![text];
                for option in options {
                    fields.push(&mut option.label);
                    fields.extend(option.description.as_mut());
                }
                fields.extend(header.as_mut());
                fields
            }
            ActivityEvent::QuestionResolved { answers, .. } => {
                answers.iter_mut().flatten().collect()
            }
            ActivityEvent::AnswerAck { .. } => Vec::new(),
            ActivityEvent::Subagent {
                agent_type,
                detail,
                title,
                ..
            } => {
                let mut fields = vec![agent_type];
                fields.extend(detail.as_mut());
                fields.extend(title.as_mut());
                fields
            }
            ActivityEvent::Permission { tool, detail, .. } => {
                let mut fields = vec![tool];
                fields.extend(detail.as_mut());
                fields
            }
            ActivityEvent::Compaction { .. } => Vec::new(),
            // EXP-746: labels and descriptions only. Option/value/mode ids,
            // `option.value`, `option.category` and `command.name` are
            // MACHINE fields — an id the rewrite touched would no longer
            // name anything the engine can set (same reason
            // `QuestionOption::key` is excluded).
            ActivityEvent::ConfigState {
                options,
                modes,
                commands,
                ..
            } => {
                let mut fields = Vec::new();
                for option in options {
                    fields.push(&mut option.label);
                    for value in option.values.iter_mut().flatten() {
                        fields.push(&mut value.label);
                    }
                }
                for mode in modes.iter_mut().flatten() {
                    fields.push(&mut mode.label);
                    fields.extend(mode.description.as_mut());
                }
                for command in commands.iter_mut().flatten() {
                    fields.push(&mut command.description);
                }
                fields
            }
            ActivityEvent::Usage { .. } | ActivityEvent::Turn { .. } => Vec::new(),
            // EXP-850: descriptions and previews are the AGENT's free text
            // and pass through the redactor; ids, `toolId` and the enum
            // values are machine fields a rewrite must never touch.
            ActivityEvent::BackgroundTasks { tasks, .. } => {
                tasks.iter_mut().map(|task| &mut task.description).collect()
            }
            ActivityEvent::Workflow(workflow) => {
                let mut fields = vec![&mut workflow.name];
                fields.extend(workflow.description.as_mut());
                for phase in workflow.phases.iter_mut() {
                    fields.push(&mut phase.title);
                }
                for agent in workflow.agents.iter_mut() {
                    fields.push(&mut agent.label);
                    fields.extend(agent.last_tool.as_mut());
                    fields.extend(agent.last_tool_summary.as_mut());
                    fields.extend(agent.result_preview.as_mut());
                    fields.extend(agent.error.as_mut());
                }
                fields.extend(workflow.summary.as_mut());
                fields
            }
        }
    }

    /// The event's `at` slot — the history buffer stamps events here so a
    /// re-publish keeps the original timeline.
    pub fn at_mut(&mut self) -> &mut Option<i64> {
        match self {
            ActivityEvent::Narration { at, .. }
            | ActivityEvent::Tool { at, .. }
            | ActivityEvent::Diff { at, .. }
            | ActivityEvent::UserMessage { at, .. }
            | ActivityEvent::Question { at, .. }
            | ActivityEvent::QuestionResolved { at, .. }
            | ActivityEvent::AnswerAck { at, .. }
            | ActivityEvent::Subagent { at, .. }
            | ActivityEvent::Permission { at, .. }
            | ActivityEvent::Compaction { at, .. }
            | ActivityEvent::ConfigState { at, .. }
            | ActivityEvent::Usage { at, .. }
            | ActivityEvent::ToolUpdate { at, .. }
            | ActivityEvent::RateLimit { at, .. }
            | ActivityEvent::BackgroundTasks { at, .. }
            | ActivityEvent::Turn { at, .. } => at,
            ActivityEvent::Workflow(workflow) => &mut workflow.at,
        }
    }
}

/// One answer choice of an [`ActivityEvent::Question`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    /// Raw keystroke(s) that select this option in the `claude` TUI picker.
    pub key: String,
    /// The option's secondary line (claude's `AskUserQuestion` options carry
    /// one); omitted when the picker offers none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// EXP-513: the option is claude's synthetic free-text row ("Type
    /// something."). A client renders it as an inline text input and sends
    /// the typed text on the answer frame; the desktop types it into the
    /// TUI's inline editor. Omitted when false so pre-EXP-513 consumers see
    /// byte-identical frames.
    #[serde(
        rename = "freeText",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub free_text: bool,
}

impl QuestionOption {
    pub fn new(label: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            key: key.into(),
            description: None,
            free_text: false,
        }
    }
}

/// EXP-746: one live agent option of an [`ActivityEvent::ConfigState`] — a
/// composer chip. `values` ABSENT means read-only on this run (render the
/// value, offer no menu); a blank `value` means the CLI's own default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ConfigOption {
    /// The id a [`ClientFrame::SetConfig`] names — machine field, never
    /// rewritten or localized.
    pub id: String,
    pub label: String,
    /// Grouping hint for the chip row (`model`, `effort`, …); a client that
    /// does not know it renders one chip per option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<ConfigValue>>,
}

/// One selectable value of a [`ConfigOption`].
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ConfigValue {
    pub id: String,
    pub label: String,
}

/// EXP-746: one session mode (`plan`, `default`, …) a
/// [`ClientFrame::SetMode`] may switch to.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ConfigMode {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// EXP-746: one slash command the AGENT advertises (ACP
/// `available_commands_update`). The `/` menu shows the contract catalog
/// UNION these, contract first — so `name` is a machine field the clients
/// match on, never rewritten.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ConfigCommand {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ConfigOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category: None,
            value: None,
            values: None,
        }
    }
}

impl ConfigValue {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl ConfigMode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
        }
    }
}

impl ConfigCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            hint: None,
        }
    }
}

/// The relay's only audience (`joinFrame.channel`).
pub const ACTIVITY_CHANNEL: &str = "activity";

impl ClientFrame<'_> {
    /// The scrubbed member activity stream — the ONE channel a viewer may
    /// join.
    pub fn join() -> Self {
        ClientFrame::Join {
            channel: ACTIVITY_CHANNEL,
        }
    }

    /// The JSON text-frame body. Serialization of this enum cannot fail
    /// (no non-string map keys, no non-finite floats).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("ClientFrame serialization cannot fail")
    }
}

// ── Relay → client (TEXT frames) ────────────────────────────────────────────

/// protocol.ts `StartRepoGroup` — a batch start's server-resolved repo (the
/// desktop syncs no repositories collection, so fullName/defaultBranch ride
/// the frame).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartRepoGroup {
    pub repository_id: String,
    pub full_name: String,
    pub default_branch: String,
}

/// protocol.ts `StartInput` (EXP-257) — one SERVER-RESOLVED action input
/// value riding an action `start_session` frame: `display` = repo fullName /
/// board name / the text itself, so the desktop injects a readable
/// `## Inputs` block with zero lookups. `label`/`type` are optional on the
/// wire (a future relay may thin them) — consumers fall back to the key and
/// `text`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartInput {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(rename = "type", default)]
    pub input_type: Option<String>,
    pub value: String,
    #[serde(default)]
    pub display: Option<String>,
}

/// Every frame the relay may send. Deserialize-only.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerFrame {
    #[serde(rename_all = "camelCase")]
    StartSession {
        /// Exactly one of `issue_id` / `issue_ids` / `action_id` is set on
        /// a conforming frame (guarded in
        /// `control_channel::remote_start_from_frame`): a single-issue start
        /// carries `issue_id`; a batch start (EXP-106) carries `issue_ids` +
        /// `team_id` + `repo`; an action start (EXP-253) carries `action_id`
        /// + `action_name` + `team_id` (+ `repo` when repo-backed).
        #[serde(default)]
        issue_id: Option<String>,
        #[serde(default)]
        issue_ids: Option<Vec<String>>,
        #[serde(default)]
        action_id: Option<String>,
        #[serde(default)]
        action_name: Option<String>,
        #[serde(default)]
        team_id: Option<String>,
        #[serde(default)]
        repo: Option<StartRepoGroup>,
        /// EXP-257: an action start's server-resolved input values (absent
        /// on issue/batch frames and on input-less action runs).
        #[serde(default)]
        inputs: Option<Vec<StartInput>>,
        /// EXP-432: the requesting teammate's userId on a start targeting a
        /// SHARED server device — echoed into `codingSessions.start` as
        /// `startedById` so the session row is requester-owned. Absent on
        /// every own-device start.
        #[serde(default)]
        started_by: Option<String>,
        /// EXP-679: `"agent"` when ANOTHER coding session started this run
        /// (the web server's MCP `exponential_sessions_start`). Echoed into
        /// `codingSessions.start`, which makes the run unattended: it gets
        /// the `exponential_sessions_end` tool, and that close-out ENDS it.
        /// Absent = a person asked for the start, and the run stays open.
        #[serde(default)]
        started_reason: Option<String>,
        /// Launch options (EXP-149) — absent on frames from clients that
        /// don't send them yet; absent = desktop settings default.
        /// `agent` is the EXP-201 addition (absent agent = claude, the
        /// exact pre-EXP-201 behavior). EXP-690 retired `skipPermissions`:
        /// old clients still send it and it parses away into nothing.
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        effort: Option<String>,
        #[serde(default)]
        ultracode: Option<bool>,
        #[serde(default)]
        plan_mode: Option<bool>,
        /// EXP-481: resume the issue's existing worktree/agent session
        /// instead of starting fresh. Single-issue frames only (the web
        /// server rejects it elsewhere); the launcher's marker gate degrades
        /// a mismatched resume to a fresh seeded session. EXP-542: absent
        /// (every pre-481 sender) is simply `false` — "resume, unstated" was
        /// never a third state, so the Option only made every consumer
        /// `unwrap_or(false)`.
        #[serde(default)]
        resume: bool,
        /// EXP-637: RESUME an ended action/chat run. The only subject key on
        /// the frame when set — `teamId` rides along, and every launch
        /// option is forbidden beside it (a resumed run keeps its recorded
        /// agent and options). Absent on every pre-EXP-637 sender.
        #[serde(default)]
        resume_session_id: Option<String>,
        /// EXP-792: the team MCP servers (`mcp_servers` row ids) the run
        /// connects to beside `exponential`. Absent = none.
        #[serde(default)]
        mcp_server_ids: Option<Vec<String>>,
        /// EXP-792 (EXP-747 B7): the agent account profile to run on.
        /// Absent/`system` = the ambient login.
        #[serde(default)]
        account: Option<String>,
        /// EXP-825: the composer's free text — the chat prompt / creator
        /// request for the two builtins, additional instructions for every
        /// other subject (image embeds in the steer message shape). Never
        /// on a resume frame; absent on every pre-EXP-825 sender.
        #[serde(default)]
        prompt: Option<String>,
    },
    /// EXP-773: a viewer asked for the transcript of a session that is no
    /// longer live, and the relay routed the ask to THIS device (the ticket
    /// named it). The host reads `{data_dir}/journal/<sessionId>.jsonl` back
    /// and republishes it with [`crate::history::publish_history`]; there is
    /// no reply frame, and a device with no such file simply says nothing
    /// (the relay's own 20s timer answers the viewer).
    #[serde(rename_all = "camelCase")]
    HistoryRequest { session_id: String },
    /// EXP-783: a viewer scrolled past the top of what it holds and asked for
    /// the page of transcript BELOW `before_seq`. Reaches the room's live
    /// publisher, which reads the page off its journal file and answers with
    /// [`ClientFrame::HistoryChunk`]s under the same (relay-issued)
    /// `request_id` — or (EXP-796) this device's CONTROL socket, once the
    /// device replayed the finished run and the room lingers without a
    /// publisher; the control-socket answer names `session_id` back. Silence
    /// is a legal answer (no journal); the relay frees the ask after its own
    /// timeout.
    #[serde(rename_all = "camelCase")]
    HistoryPage {
        session_id: String,
        request_id: String,
        before_seq: u64,
        limit: u32,
    },
    /// EXP-481: fire-and-forget check-in nudge — the web server persisted
    /// new work for this device (a queued command, edited launch defaults);
    /// heartbeat NOW instead of on the next cadence. No reply frame exists;
    /// the heartbeat pickup is the durable path.
    CheckIn,
    /// Viewer keystrokes, relay → publisher.
    Input {
        data: String,
    },
    /// A SEMANTIC answer to an [`ActivityEvent::Question`] (EXP-249), relay →
    /// publisher, forwarded verbatim from a joined viewer (same gating as
    /// `input`). `keys` are the option keys of THAT question — the
    /// publisher resolves them against its own live picker state instead of
    /// replaying blind keystrokes.
    #[serde(rename_all = "camelCase")]
    Answer {
        question_id: String,
        #[serde(default)]
        ask_id: Option<String>,
        keys: Vec<String>,
        /// EXP-513: the typed reply for a `freeText` option — the desktop
        /// selects the row with `keys`, types this into the inline editor,
        /// and submits. Absent on ordinary answers (and from pre-EXP-513
        /// clients).
        #[serde(default)]
        text: Option<String>,
    },
    /// EXP-746: live-config steering, relay → publisher, forwarded verbatim
    /// from a joined viewer (same gating as `input`/`answer`). Never touches
    /// the PTY — it crosses to the engine through
    /// [`crate::activity::ConfigLink`], which owns the ACP session, and the
    /// re-emitted `config_state` is the only confirmation the wire has.
    SetConfig { id: String, value: String },
    /// EXP-746: switch to one of the modes the publisher advertised.
    SetMode { id: String },
    Kill,
    Bye {
        #[serde(default)]
        outcome: Option<String>,
    },
    Error {
        code: String,
        #[serde(default)]
        message: Option<String>,
    },
}

impl ServerFrame {
    /// Parse a relay text frame; `None` for anything non-conforming (mirror
    /// of the relay's own silent-drop posture — an unknown future frame must
    /// not kill the socket).
    pub fn parse(raw: &str) -> Option<ServerFrame> {
        serde_json::from_str(raw).ok()
    }
}

// ── Relay → VIEWER (EXP-696) ────────────────────────────────────────────────

/// Every frame the relay sends to a socket that joined `channel:'activity'`.
/// Deserialize-only, and deliberately separate from [`ServerFrame`]: the
/// publisher/control inbound vocabulary and the viewer's are disjoint, and
/// the two pumps stay free of each other's "impossible here" arms.
///
/// A viewer connection's whole life is readable off this enum:
/// `activity_reset` (+ the replay, + [`ViewerFrame::ActivitySynced`]) answers
/// the join; [`ViewerFrame::Activity`] carries the live tail;
/// [`ViewerFrame::Keepalive`] is the 15s liveness beat (EXP-648) that lets a
/// quiet socket be told from a dead one; `bye`/`error` end it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ViewerFrame {
    /// One already-scrubbed activity event, fanned out from the publisher.
    /// EXP-783: `seq` is the publisher's own monotonic index, echoed by the
    /// relay; absent from a publisher older than EXP-783.
    #[serde(rename_all = "camelCase")]
    Activity {
        event: ActivityEvent,
        #[serde(default)]
        seq: Option<u64>,
    },
    /// "Drop everything rendered so far" — sent immediately BEFORE the join
    /// replay and before any publisher-driven full re-publish. EXP-656: a
    /// client stages what follows rather than blanking the feed on the spot.
    ActivityReset,
    /// EXP-656: end-of-replay marker, sent to the JOINING viewer right after
    /// the replay — "the picture is complete, commit it". Absent on a
    /// publisher-driven republish (old desktops give the relay no
    /// end-of-republish signal), which is why clients also keep a quiet-timer
    /// fallback.
    ///
    /// EXP-783: it now names the SPAN the replay covered. A client that
    /// already holds this run's transcript keeps everything BELOW `first_seq`
    /// and splices the replay on top; `truncated` says the relay's log is a
    /// TAIL, so the pages below it must be asked for from the device
    /// ([`ClientFrame::HistoryPage`]) rather than assumed gone.
    #[serde(rename_all = "camelCase")]
    ActivitySynced {
        #[serde(default)]
        first_seq: Option<u64>,
        #[serde(default)]
        last_seq: Option<u64>,
        #[serde(default)]
        truncated: Option<bool>,
    },
    /// EXP-783: one page of older transcript, answering this viewer's
    /// [`ClientFrame::HistoryPage`]. Never part of the live feed — the client
    /// PREPENDS these events instead of appending them.
    #[serde(rename_all = "camelCase")]
    HistoryChunk {
        request_id: String,
        events: Vec<ActivityEvent>,
        #[serde(default)]
        seqs: Vec<u64>,
        done: bool,
    },
    /// EXP-648: the relay's 15s beat to joined viewers. Carries nothing and
    /// never changes a phase — its only job is to prove the socket is alive,
    /// because an agent parked on a question sends nothing for minutes.
    Keepalive,
    /// The room is finished. `outcome: "publisher_lost"` is the one RETRYABLE
    /// value: the desktop's socket dropped but the session may still be
    /// running.
    Bye {
        #[serde(default)]
        outcome: Option<String>,
    },
    /// A relay-side refusal. `code: "no_such_session"` means the room is not
    /// up (yet) — the desktop may still be dialing its publisher socket.
    Error {
        code: String,
        #[serde(default)]
        message: Option<String>,
    },
}

impl ViewerFrame {
    /// Parse a relay text frame; `None` for anything non-conforming —
    /// unknown `t` tags, malformed JSON, and (deliberately) an `activity`
    /// frame whose `kind` this build does not know. Every one of those is an
    /// IGNORE, never a socket teardown: the relay adds frames and event kinds
    /// independently of desktop releases.
    pub fn parse(raw: &str) -> Option<ViewerFrame> {
        serde_json::from_str(raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ClientFrame vectors — authored from protocol.ts zod schemas + the
    // hub tests' literal frames (`hub.test.ts` sends exactly these shapes).

    #[test]
    fn online_serializes_the_device_id_alone() {
        // EXP-672 (PR #584): the relay stopped reading `deviceLabel` and the
        // EXP-253 `caps` array off this frame — presence is the deviceId →
        // socket map, and every start gate reads the persisted `devices` row
        // `devices.register` writes. The frame carries exactly one field.
        assert_eq!(
            ClientFrame::Online { device_id: "dev-1" }.to_json(),
            r#"{"t":"online","deviceId":"dev-1"}"#
        );
    }

    #[test]
    fn hello_serializes_session_without_geometry() {
        // EXP-249: cols/rows belonged to the removed PTY mirror. The relay's
        // helloFrame keeps them optional, so omitting them parses on every
        // relay generation (old relays included).
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: Some("issue-1"),
                activity_public: None,
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1","issueId":"issue-1"}"#
        );
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: None,
                activity_public: None,
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1"}"#
        );
    }

    #[test]
    fn hello_activity_public_false_serializes_byte_exact() {
        // EXP-90: every real hello carries the explicit camelCase
        // `activityPublic:false` — absent means "public" to legacy relays.
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: Some("issue-1"),
                activity_public: Some(false),
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1","issueId":"issue-1","activityPublic":false}"#
        );
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: None,
                activity_public: Some(false),
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1","activityPublic":false}"#
        );
    }

    #[test]
    fn input_field_is_data_not_bytes() {
        // §8.1: the exact regression vector from the spec.
        assert_eq!(
            ClientFrame::Input { data: "x".into() }.to_json(),
            r#"{"t":"input","data":"x"}"#
        );
    }

    #[test]
    fn bare_frames_serialize_tag_only() {
        // EXP-696: the relay's joinFrame REQUIRES the channel literal — a
        // bare `{"t":"join"}` fails its zod parse and is dropped in silence.
        assert_eq!(
            ClientFrame::join().to_json(),
            r#"{"t":"join","channel":"activity"}"#
        );
        assert_eq!(ClientFrame::Kill.to_json(), r#"{"t":"kill"}"#);
        assert_eq!(ClientFrame::Bye { outcome: None }.to_json(), r#"{"t":"bye"}"#);
        assert_eq!(
            ClientFrame::Bye {
                outcome: Some("exit:0")
            }
            .to_json(),
            r#"{"t":"bye","outcome":"exit:0"}"#
        );
    }

    #[test]
    fn activity_frame_serializes_to_the_relay_schema() {
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::narration("Reading the file"),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"narration","text":"Reading the file"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::tool("Edit", Some("src/main.rs".into())),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"Edit","detail":"src/main.rs"}}"#
        );
        // detail is omitted when absent.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::tool("TodoWrite", None),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"TodoWrite"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::diff("--- a\n+++ b\n"),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"diff","diff":"--- a\n+++ b\n"}}"#
        );
    }

    #[test]
    fn user_message_and_question_serialize_to_the_relay_schema() {
        // EXP-78 kinds — tag snake_case, fields camelCase (`multiSelect`).
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::user_message("fix the login bug"),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"user_message","text":"fix the login bug"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "Which color?".into(),
                    options: vec![
                        QuestionOption::new("Red", "1"),
                        QuestionOption::new("Blue", "2"),
                    ],
                    multi_select: Some(true),
                    plan_mode: None,
                    id: "toolu_01#0".into(),
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Which color?","options":[{"label":"Red","key":"1"},{"label":"Blue","key":"2"}],"multiSelect":true,"id":"toolu_01#0"}}"#
        );
        // multiSelect and planMode are omitted when absent.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "Approve?".into(),
                    options: vec![QuestionOption::new("Approve", "1")],
                    multi_select: None,
                    plan_mode: None,
                    id: "toolu_02".into(),
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Approve?","options":[{"label":"Approve","key":"1"}],"id":"toolu_02"}}"#
        );
        // A plan-approval question carries the planMode marker (EXP-97).
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "The plan".into(),
                    options: vec![QuestionOption::new("Approve — auto-accept edits", "1")],
                    multi_select: None,
                    plan_mode: Some(true),
                    id: "toolu_03".into(),
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"The plan","options":[{"label":"Approve — auto-accept edits","key":"1"}],"planMode":true,"id":"toolu_03"}}"#
        );
    }

    // ── EXP-249 (steer protocol v2) vectors ─────────────────────────────────

    #[test]
    fn question_carries_the_v2_identity_fields() {
        // Step 2 of a 3-question ask: id = `<tool_use_id>#<i>`, askId groups
        // the steps, header/description ride along.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "Which color?".into(),
                    options: vec![
                        QuestionOption {
                            label: "Red".into(),
                            key: "1".into(),
                            description: Some("warm".into()),
                            free_text: false,
                        },
                        QuestionOption::new("Blue", "2"),
                    ],
                    multi_select: Some(false),
                    plan_mode: None,
                    id: "toolu_01#1".into(),
                    ask_id: Some("toolu_01".into()),
                    index: Some(2),
                    total: Some(3),
                    header: Some("Color".into()),
                    at: Some(1_751_500_000_000),
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Which color?","options":[{"label":"Red","key":"1","description":"warm"},{"label":"Blue","key":"2"}],"multiSelect":false,"id":"toolu_01#1","askId":"toolu_01","index":2,"total":3,"header":"Color","at":1751500000000}}"#
        );
        // The final review/submit step: askId, no index/total.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "Submit answers?".into(),
                    options: vec![QuestionOption::new("Submit", "\r")],
                    multi_select: None,
                    plan_mode: None,
                    id: "toolu_01#submit".into(),
                    ask_id: Some("toolu_01".into()),
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Submit answers?","options":[{"label":"Submit","key":"\r"}],"id":"toolu_01#submit","askId":"toolu_01"}}"#
        );
    }

    #[test]
    fn resolution_and_ack_events_serialize() {
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::QuestionResolved {
                    id: Some("toolu_01#0".into()),
                    ask_id: Some("toolu_01".into()),
                    answers: Some(vec!["Red".into()]),
                    dismissed: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question_resolved","id":"toolu_01#0","askId":"toolu_01","answers":["Red"]}}"#
        );
        // Dismissing retires EVERY card of the ask (id absent).
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::QuestionResolved {
                    id: None,
                    ask_id: Some("toolu_01".into()),
                    answers: None,
                    dismissed: Some(true),
                    at: Some(1_751_500_000_000),
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question_resolved","askId":"toolu_01","dismissed":true,"at":1751500000000}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::AnswerAck {
                    id: "toolu_01#0".into(),
                    ask_id: Some("toolu_01".into()),
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"answer_ack","id":"toolu_01#0","askId":"toolu_01"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::AnswerAck {
                    id: "plan-1".into(),
                    ask_id: None,
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"answer_ack","id":"plan-1"}}"#
        );
    }

    #[test]
    fn subagent_permission_and_attributed_tool_serialize() {
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Subagent {
                    id: "agent_01".into(),
                    agent_type: "explore".into(),
                    status: SubagentStatus::Started,
                    detail: Some("Map the steer crate".into()),
                    at: None,
                    tool_calls: None,
                    // EXP-847: the spawning Agent call's `description`.
                    title: Some("Audit the shape proxies".into()),
                    workflow_id: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"subagent","id":"agent_01","agentType":"explore","status":"started","detail":"Map the steer crate","title":"Audit the shape proxies"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Subagent {
                    id: "agent_01".into(),
                    agent_type: "explore".into(),
                    status: SubagentStatus::Completed,
                    detail: None,
                    at: None,
                    tool_calls: None,
                    title: None,
                    workflow_id: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"subagent","id":"agent_01","agentType":"explore","status":"completed"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Tool {
                    name: "Grep".into(),
                    detail: Some("fn main".into()),
                    id: None,
                    tool_kind: None,
                    subagent_id: Some("agent_01".into()),
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"Grep","detail":"fn main","subagentId":"agent_01"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Permission {
                    tool: "Bash".into(),
                    detail: Some("needs your permission".into()),
                    at: None,
                },
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"permission","tool":"Bash","detail":"needs your permission"}}"#
        );
    }

    #[test]
    fn compaction_serializes_to_the_relay_schema_and_parses_back() {
        // EXP-724: `{kind, phase, trigger?, at?}` — trigger omitted when
        // unknown (codex), present verbatim when claude reports it.
        let started = ActivityEvent::compaction(CompactionPhase::Started, Some("manual"));
        assert_eq!(
            ClientFrame::Activity { event: started.clone(), seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"compaction","phase":"started","trigger":"manual"}}"#
        );
        let ended = ActivityEvent::compaction(CompactionPhase::Ended, None);
        assert_eq!(
            ClientFrame::Activity { event: ended.clone(), seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"compaction","phase":"ended"}}"#
        );
        // The viewer role reads them back (EXP-696) — a future trigger value
        // parses, an unknown phase does not.
        let parsed: ActivityEvent = serde_json::from_str(
            r#"{"kind":"compaction","phase":"started","trigger":"overflow","at":3}"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            ActivityEvent::Compaction {
                phase: CompactionPhase::Started,
                trigger: Some("overflow".into()),
                at: Some(3),
            }
        );
        assert!(serde_json::from_str::<ActivityEvent>(
            r#"{"kind":"compaction","phase":"paused"}"#
        )
        .is_err());
    }

    /// EXP-846: `tool_update`'s preview — `{kind, id, status?, diff?, at?,
    /// preview?}`, the preview's own keys in the relay's zod order, and a
    /// preview that named nothing is never published.
    #[test]
    fn a_tool_update_preview_serializes_to_the_relay_schema_and_parses_back() {
        let event = ActivityEvent::ToolUpdate {
            id: "tc-1".into(),
            status: Some(ToolUpdateStatus::Completed),
            diff: None,
            at: None,
            preview: Some(ToolPreview {
                id: Some("c0ffee".into()),
                identifier: Some("EXP-42".into()),
                title: Some("Fix the flicker".into()),
                url: Some("https://github.com/a/b/pull/7".into()),
                count: Some(3),
                status: Some("in_progress".into()),
            }),
        };
        assert_eq!(
            ClientFrame::Activity { event: event.clone(), seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"tool_update","id":"tc-1","status":"completed","preview":{"id":"c0ffee","identifier":"EXP-42","title":"Fix the flicker","url":"https://github.com/a/b/pull/7","count":3,"status":"in_progress"}}}"#
        );
        assert_eq!(
            serde_json::from_str::<ActivityEvent>(
                r#"{"kind":"tool_update","id":"tc-1","status":"completed","preview":{"identifier":"EXP-42"}}"#
            )
            .unwrap(),
            ActivityEvent::ToolUpdate {
                id: "tc-1".into(),
                status: Some(ToolUpdateStatus::Completed),
                diff: None,
                at: None,
                preview: Some(ToolPreview {
                    identifier: Some("EXP-42".into()),
                    ..ToolPreview::default()
                }),
            }
        );
        // A pre-846 publisher sends no preview at all, and the shorthand
        // constructor keeps that byte-identical.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::tool_update("tc-1", Some(ToolUpdateStatus::Failed), None),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool_update","id":"tc-1","status":"failed"}}"#
        );
        assert!(ToolPreview::default().is_empty());
        assert!(!ToolPreview { count: Some(0), ..ToolPreview::default() }.is_empty());
        // Every string is clamped to the relay's own cap.
        let clamped = ToolPreview {
            title: Some("x".repeat(TOOL_PREVIEW_TEXT_MAX + 10)),
            ..ToolPreview::default()
        }
        .clamp();
        assert_eq!(
            clamped.title.as_deref().map(|title| title.chars().count()),
            Some(TOOL_PREVIEW_TEXT_MAX)
        );
    }

    /// EXP-848: the turn slot's wire shape — `{kind, state, at?}` — and the
    /// contract's `turnState` vocabulary, byte for byte.
    #[test]
    fn turn_serializes_to_the_relay_schema_and_matches_the_contract() {
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::turn(TurnState::Started),
                seq: None,
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"turn","state":"started"}}"#
        );
        let mut ended = ActivityEvent::turn(TurnState::Ended);
        *ended.at_mut() = Some(7);
        assert_eq!(
            ClientFrame::Activity { event: ended, seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"turn","state":"ended","at":7}}"#
        );
        // The viewer role reads them back (EXP-696); an unknown state does not
        // parse.
        assert_eq!(
            serde_json::from_str::<ActivityEvent>(r#"{"kind":"turn","state":"started"}"#).unwrap(),
            ActivityEvent::Turn {
                state: TurnState::Started,
                started_at: None,
                tokens: None,
                at: None
            }
        );
        assert!(
            serde_json::from_str::<ActivityEvent>(r#"{"kind":"turn","state":"thinking"}"#).is_err()
        );
        // The contract owns the vocabulary, and `Ended` is the DEFAULT: a
        // client that has seen no turn event never pulses.
        assert_eq!(
            [TurnState::Started.id(), TurnState::Ended.id()].as_slice(),
            domain::contract::TURN_STATE_VALUES
        );
        assert_eq!(TurnState::default(), TurnState::Ended);
        assert!(TurnState::Started.is_working());
        assert!(!TurnState::Ended.is_working());
    }

    /// EXP-746: the maximal snapshot, byte-for-byte in the zod's field order
    /// (`options[{id,label,category?,value?,values?[{id,label}]}]`,
    /// `currentMode?`, `modes?`, `commands?`, `at?`). Reordering a field here
    /// reorders the wire.
    #[test]
    fn config_state_serializes_to_the_relay_schema_and_parses_back() {
        let event = ActivityEvent::ConfigState {
            options: vec![ConfigOption {
                category: Some("model".into()),
                value: Some("opus".into()),
                values: Some(vec![
                    ConfigValue::new("opus", "Opus"),
                    ConfigValue::new("sonnet", "Sonnet"),
                ]),
                ..ConfigOption::new("model", "Model")
            }],
            current_mode: Some("plan".into()),
            modes: vec![
                ConfigMode {
                    description: Some("Read-only until approved".into()),
                    ..ConfigMode::new("plan", "Plan")
                },
                ConfigMode::new("default", "Default"),
            ]
            .into(),
            commands: vec![
                ConfigCommand {
                    hint: Some("instructions".into()),
                    ..ConfigCommand::new("compact", "Compact the context")
                },
                ConfigCommand::new("new", "Start a fresh context"),
            ]
            .into(),
            at: Some(9),
        };
        assert_eq!(
            ClientFrame::Activity { event: event.clone(), seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"config_state","options":[{"id":"model","label":"Model","category":"model","value":"opus","values":[{"id":"opus","label":"Opus"},{"id":"sonnet","label":"Sonnet"}]}],"currentMode":"plan","modes":[{"id":"plan","label":"Plan","description":"Read-only until approved"},{"id":"default","label":"Default"}],"commands":[{"name":"compact","description":"Compact the context","hint":"instructions"},{"name":"new","description":"Start a fresh context"}],"at":9}}"#
        );
        assert_eq!(
            ViewerFrame::parse(&ClientFrame::Activity { event: event.clone(), seq: None }.to_json()).unwrap(),
            ViewerFrame::Activity { event, seq: None }
        );
    }

    #[test]
    fn config_state_omits_every_absent_optional() {
        // A read-only run with no modes and no agent commands is the whole
        // frame minus five keys — nothing may serialize as `null`.
        let event = ActivityEvent::ConfigState {
            options: Vec::new(),
            current_mode: None,
            modes: None,
            commands: None,
            at: None,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"kind":"config_state","options":[]}"#
        );
        // An option with no category/value/values is equally bare.
        let bare = ActivityEvent::ConfigState {
            options: vec![ConfigOption::new("effort", "Effort")],
            current_mode: None,
            modes: None,
            commands: None,
            at: None,
        };
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            r#"{"kind":"config_state","options":[{"id":"effort","label":"Effort"}]}"#
        );
    }

    #[test]
    fn config_state_parses_a_frame_that_omits_modes_and_commands() {
        // Proves the `#[serde(default)]` on every optional: an internally
        // tagged enum has no per-field fallback, so a publisher that sends a
        // model chip and nothing else would otherwise fail the parse and the
        // viewer would render no chips at all.
        let parsed: ActivityEvent = serde_json::from_str(
            r#"{"kind":"config_state","options":[{"id":"model","label":"Model","value":""}]}"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            ActivityEvent::ConfigState {
                options: vec![ConfigOption {
                    value: Some(String::new()),
                    ..ConfigOption::new("model", "Model")
                }],
                current_mode: None,
                modes: None,
                commands: None,
                at: None,
            }
        );
        // Unknown future fields inside a nested struct are ignored too.
        let forward: ActivityEvent = serde_json::from_str(
            r#"{"kind":"config_state","options":[{"id":"a","label":"b","hologram":true}],"modes":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            forward,
            ActivityEvent::ConfigState { ref modes, .. } if modes.as_deref() == Some(&[][..])
        ));
    }

    #[test]
    fn usage_serializes_to_the_relay_schema_and_parses_back() {
        let metered = ActivityEvent::Usage {
            context_used: 124_000,
            context_size: 200_000,
            cost_usd: Some(1.25),
            at: Some(5),
        };
        assert_eq!(
            ClientFrame::Activity { event: metered.clone(), seq: None }.to_json(),
            r#"{"t":"activity","event":{"kind":"usage","contextUsed":124000,"contextSize":200000,"costUsd":1.25,"at":5}}"#
        );
        // A plan run reports no spend: the key is absent, never `null`.
        assert_eq!(
            serde_json::to_string(&ActivityEvent::usage(0, 200_000, None)).unwrap(),
            r#"{"kind":"usage","contextUsed":0,"contextSize":200000}"#
        );
        assert_eq!(
            ViewerFrame::parse(&ClientFrame::Activity { event: metered.clone(), seq: None }.to_json()).unwrap(),
            ViewerFrame::Activity { event: metered, seq: None }
        );
    }

    // EXP-785/786/784: the three wire additions, byte-exact against
    // protocol.ts and parsed back through the viewer path.
    #[test]
    fn tool_carries_its_call_id_and_kind_under_tool_kind_never_kind() {
        let event = ActivityEvent::Tool {
            name: "Edit".into(),
            detail: Some("src/main.rs".into()),
            id: Some("tc-1".into()),
            tool_kind: Some(ToolKind::Edit),
            subagent_id: None,
            at: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"tool","name":"Edit","detail":"src/main.rs","id":"tc-1","toolKind":"edit"}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), event);
        // A pre-EXP-785 frame still parses, with both absent.
        assert_eq!(
            serde_json::from_str::<ActivityEvent>(r#"{"kind":"tool","name":"Edit"}"#).unwrap(),
            ActivityEvent::tool("Edit", None)
        );
        assert_eq!(
            serde_json::to_string(&ToolKind::SwitchMode).unwrap(),
            r#""switch_mode""#
        );
    }

    #[test]
    fn tool_kind_matches_the_contract() {
        let wire: Vec<&str> = ToolKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(wire, domain::contract::TOOL_KIND_VALUES);
        for kind in ToolKind::ALL {
            assert_eq!(ToolKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        assert_eq!(ToolKind::parse("teleport"), None);
    }

    #[test]
    fn tool_update_serializes_to_the_relay_schema_and_parses_back() {
        let settled = ActivityEvent::tool_update("tc-1", Some(ToolUpdateStatus::Failed), None);
        assert_eq!(
            serde_json::to_string(&settled).unwrap(),
            r#"{"kind":"tool_update","id":"tc-1","status":"failed"}"#
        );
        let diffed = ActivityEvent::tool_update(
            "tc-2",
            Some(ToolUpdateStatus::Completed),
            Some("--- a/x\n+++ b/x\n".into()),
        );
        let json = serde_json::to_string(&diffed).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"tool_update","id":"tc-2","status":"completed","diff":"--- a/x\n+++ b/x\n"}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), diffed);
        // A bare update (no status, no diff) is legal and says nothing.
        assert_eq!(
            serde_json::from_str::<ActivityEvent>(r#"{"kind":"tool_update","id":"tc-3"}"#).unwrap(),
            ActivityEvent::tool_update("tc-3", None, None)
        );
    }

    #[test]
    fn rate_limit_serializes_to_the_relay_schema_and_clears_on_empty_or_ok() {
        let limited = ActivityEvent::rate_limit(
            "rejected",
            Some(1_700_000_000_000),
            Some("5-hour limit reached".into()),
        );
        let json = serde_json::to_string(&limited).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"rate_limit","status":"rejected","resetsAt":1700000000000,"message":"5-hour limit reached"}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), limited);
        assert_eq!(
            serde_json::to_string(&ActivityEvent::rate_limit("", None, None)).unwrap(),
            r#"{"kind":"rate_limit","status":""}"#
        );
        assert!(rate_limit_clears(""));
        assert!(rate_limit_clears("ok"));
        assert!(rate_limit_clears(" OK "));
        assert!(!rate_limit_clears("allowed_warning"));
        assert!(!rate_limit_clears("rejected"));
    }

    /// FEED-35: the wall rule — locked ×2 with web `rateLimitIsWall`.
    #[test]
    fn a_wall_is_a_rejection_or_a_notice_never_a_warning() {
        assert!(rate_limit_is_wall("rejected", None));
        assert!(rate_limit_is_wall(" rejected ", None));
        assert!(rate_limit_is_wall("allowed_warning", Some("You've hit your limit")));
        assert!(!rate_limit_is_wall("allowed_warning", None));
        assert!(!rate_limit_is_wall("allowed_warning", Some("   ")));
        assert!(!rate_limit_is_wall("", None));
        assert!(!rate_limit_is_wall("ok", None));
    }

    /// EXP-831: the clock rule — past the reset plus a minute the wall is
    /// gone; inside it, and with no reset at all, it stands.
    #[test]
    fn a_wall_expires_a_minute_past_its_reset() {
        let now = 1_700_000_000_000;
        assert!(rate_limit_expired(Some(now - 61_000), now));
        assert!(!rate_limit_expired(Some(now - 60_000), now));
        assert!(!rate_limit_expired(Some(now - 1_000), now));
        assert!(!rate_limit_expired(Some(now + 3_600_000), now));
        assert!(!rate_limit_expired(None, now));
    }

    #[test]
    fn text_fields_mut_skips_config_ids_values_and_command_names() {
        // EXP-511's reverse rewrite walks free text ONLY. Ids, the value in
        // force, the category and a command's name are machine fields: a
        // rewritten id would name nothing the engine can set.
        let mut event = ActivityEvent::ConfigState {
            options: vec![ConfigOption {
                category: Some("model".into()),
                value: Some("opus".into()),
                values: Some(vec![ConfigValue::new("opus", "Opus")]),
                ..ConfigOption::new("model", "Model")
            }],
            current_mode: Some("plan".into()),
            modes: Some(vec![ConfigMode {
                description: Some("Read-only".into()),
                ..ConfigMode::new("plan", "Plan")
            }]),
            commands: Some(vec![ConfigCommand::new("compact", "Compact the context")]),
            at: None,
        };
        let seen: Vec<String> = event
            .text_fields_mut()
            .into_iter()
            .map(|field| field.clone())
            .collect();
        assert_eq!(
            seen,
            vec![
                "Model".to_string(),
                "Opus".to_string(),
                "Plan".to_string(),
                "Read-only".to_string(),
                "Compact the context".to_string(),
            ]
        );
        // `usage` carries no free text at all.
        assert!(ActivityEvent::usage(1, 2, None).text_fields_mut().is_empty());
    }

    #[test]
    fn activity_reset_is_a_bare_tag() {
        assert_eq!(ClientFrame::ActivityReset.to_json(), r#"{"t":"activity_reset"}"#);
    }

    #[test]
    fn at_mut_reaches_every_kind() {
        let mut events = vec![
            ActivityEvent::narration("n"),
            ActivityEvent::tool("Edit", None),
            ActivityEvent::diff("d"),
            ActivityEvent::user_message("u"),
            ActivityEvent::Question {
                text: "q".into(),
                options: vec![QuestionOption::new("a", "1")],
                multi_select: None,
                plan_mode: None,
                id: "q1".into(),
                ask_id: None,
                index: None,
                total: None,
                header: None,
                at: None,
            },
            ActivityEvent::QuestionResolved {
                id: None,
                ask_id: None,
                answers: None,
                dismissed: None,
                at: None,
            },
            ActivityEvent::AnswerAck { id: "i".into(), ask_id: None, at: None },
            ActivityEvent::Subagent {
                id: "a".into(),
                agent_type: "t".into(),
                status: SubagentStatus::Started,
                detail: None,
                at: None,
                tool_calls: None,
                title: None,
                workflow_id: None,
            },
            ActivityEvent::Permission { tool: "Bash".into(), detail: None, at: None },
            ActivityEvent::compaction(CompactionPhase::Started, None),
            ActivityEvent::ConfigState {
                options: vec![ConfigOption::new("model", "Model")],
                current_mode: None,
                modes: None,
                commands: None,
                at: None,
            },
            ActivityEvent::usage(1, 2, None),
            ActivityEvent::tool_update("t", None, None),
            ActivityEvent::rate_limit("rejected", None, None),
        ];
        for event in &mut events {
            *event.at_mut() = Some(7);
            let json = serde_json::to_string(&event).unwrap();
            assert!(json.contains(r#""at":7"#), "{json}");
        }
    }

    #[test]
    fn narration_anchor_serializes_camel_case_and_omits_none() {
        // EXP-483: the splice anchor rides the wire as `beforeQuestionId`
        // and stays entirely absent on ordinary narration.
        let plain = ActivityEvent::narration("hi");
        assert!(!serde_json::to_string(&plain).unwrap().contains("beforeQuestionId"));
        let anchored = ActivityEvent::Narration {
            text: "summary".into(),
            before_question_id: Some("toolu_01".into()),
            message_id: None,
            subagent_id: None,
            at: None,
        };
        assert!(serde_json::to_string(&anchored)
            .unwrap()
            .contains(r#""beforeQuestionId":"toolu_01""#));
    }

    #[test]
    fn narration_identity_fields_serialize_camel_case_and_omit_none() {
        // EXP-772 / EXP-773: the merge key and the subagent scope ride the
        // wire as `messageId` / `subagentId`, absent on plain narration.
        let plain = serde_json::to_string(&ActivityEvent::narration("hi")).unwrap();
        assert!(!plain.contains("messageId"), "{plain}");
        assert!(!plain.contains("subagentId"), "{plain}");
        let scoped = ActivityEvent::Narration {
            text: "inside".into(),
            before_question_id: None,
            message_id: Some("msg_01".into()),
            subagent_id: Some("toolu_task".into()),
            at: None,
        };
        let json = serde_json::to_string(&scoped).unwrap();
        assert!(json.contains(r#""messageId":"msg_01""#), "{json}");
        assert!(json.contains(r#""subagentId":"toolu_task""#), "{json}");
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), scoped);
        let user = ActivityEvent::UserMessage {
            text: "go".into(),
            subagent_id: Some("toolu_task".into()),
            at: None,
        };
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains(r#""subagentId":"toolu_task""#), "{json}");
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), user);
    }

    #[test]
    fn answer_frame_deserializes_camel_case() {
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"answer","questionId":"toolu_01#0","askId":"toolu_01","keys":["1","3"]}"#
            )
            .unwrap(),
            ServerFrame::Answer {
                question_id: "toolu_01#0".into(),
                ask_id: Some("toolu_01".into()),
                keys: vec!["1".into(), "3".into()],
                text: None,
            }
        );
        // A plan answer carries no askId.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"answer","questionId":"plan-1","keys":["1"]}"#).unwrap(),
            ServerFrame::Answer {
                question_id: "plan-1".into(),
                ask_id: None,
                keys: vec!["1".into()],
                text: None,
            }
        );
        // EXP-513: a free-text answer rides its typed reply.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"answer","questionId":"toolu_01#0","askId":"toolu_01","keys":["4"],"text":"purple"}"#
            )
            .unwrap(),
            ServerFrame::Answer {
                question_id: "toolu_01#0".into(),
                ask_id: Some("toolu_01".into()),
                keys: vec!["4".into()],
                text: Some("purple".into()),
            }
        );
    }

    #[test]
    fn question_options_omit_free_text_unless_set() {
        // Pre-EXP-513 consumers must see byte-identical frames for ordinary
        // options; a free-text row carries the flag.
        let plain = serde_json::to_string(&QuestionOption::new("Red", "1")).unwrap();
        assert_eq!(plain, r#"{"label":"Red","key":"1"}"#);
        let free = QuestionOption {
            free_text: true,
            ..QuestionOption::new("Type something.", "4")
        };
        assert_eq!(
            serde_json::to_string(&free).unwrap(),
            r#"{"label":"Type something.","key":"4","freeText":true}"#
        );
    }

    #[test]
    fn client_frames_satisfy_relay_zod_constraints() {
        // Round-trip our own serialization through a permissive parse to
        // assert the tag names the relay's discriminated union expects.
        for (frame, tag) in [
            (
                ClientFrame::Online { device_id: "d" },
                "online",
            ),
            (
                ClientFrame::Hello {
                    session_id: "s",
                    issue_id: None,
                    activity_public: None,
                },
                "hello",
            ),
            (ClientFrame::join(), "join"),
            (ClientFrame::Input { data: String::new() }, "input"),
            (ClientFrame::Kill, "kill"),
            (
                ClientFrame::SetConfig {
                    id: "model".into(),
                    value: "opus".into(),
                },
                "set_config",
            ),
            (ClientFrame::SetMode { id: "plan".into() }, "set_mode"),
            (ClientFrame::Bye { outcome: None }, "bye"),
            (ClientFrame::ActivityReset, "activity_reset"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&frame.to_json()).unwrap();
            assert_eq!(value["t"], tag, "tag mismatch for {frame:?}");
        }
    }

    // ── ServerFrame vectors — captured relay strings (hub.ts `frame(...)`
    // emits `JSON.stringify` of exactly these objects).

    #[test]
    fn start_session_deserializes_camel_issue_id() {
        // hub.ts startSession: frame({ t: `start_session`, issueId }) — the
        // option-less frame older relays/clients send (EXP-149 fields absent).
        // The compat lock: a legacy option-less frame keeps `issue_id: Some`
        // with `issue_ids: None`.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9"}"#).unwrap(),
            ServerFrame::StartSession {
                issue_id: Some("issue-9".into()),
                issue_ids: None,
                action_id: None,
                action_name: None,
                team_id: None,
                repo: None,
                inputs: None,
                started_by: None,
                started_reason: None,
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_resume_and_check_in() {
        // EXP-481: `resume: true` rides a single-issue frame; absent = false
        // (fresh start — the pre-481 wire, byte-identical).
        match ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9","resume":true}"#)
            .unwrap()
        {
            ServerFrame::StartSession { issue_id, resume, .. } => {
                assert_eq!(issue_id.as_deref(), Some("issue-9"));
                assert!(resume);
            }
            other => panic!("expected StartSession, got {other:?}"),
        }
        // EXP-542: an absent flag deserializes to plain `false`.
        match ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9"}"#).unwrap() {
            ServerFrame::StartSession { resume, .. } => assert!(!resume),
            other => panic!("expected StartSession, got {other:?}"),
        }
        // The check-in nudge is a bare tag frame.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"check_in"}"#).unwrap(),
            ServerFrame::CheckIn
        );
        // EXP-773: the history ask names the session it wants back.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"history_request","sessionId":"sess-1"}"#).unwrap(),
            ServerFrame::HistoryRequest {
                session_id: "sess-1".to_string()
            }
        );
        assert_eq!(ServerFrame::parse(r#"{"t":"history_request"}"#), None);
        // EXP-783/796: a page ask, on a publisher OR a control socket.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"history_page","sessionId":"sess-1","requestId":"h7","beforeSeq":40,"limit":20}"#
            )
            .unwrap(),
            ServerFrame::HistoryPage {
                session_id: "sess-1".to_string(),
                request_id: "h7".to_string(),
                before_seq: 40,
                limit: 20,
            }
        );
        // Unknown future frames still drop silently, never kill the socket.
        assert_eq!(ServerFrame::parse(r#"{"t":"telepathy"}"#), None);
    }

    /// EXP-796: the chunk's wire form is unchanged for a publisher (no
    /// `sessionId` key at all), and names the session from a control socket.
    #[test]
    fn history_chunk_serializes_session_id_only_when_set() {
        let chunk = ClientFrame::HistoryChunk {
            session_id: None,
            request_id: "h7".to_string(),
            events: vec![ActivityEvent::narration("older")],
            seqs: vec![39],
            done: true,
        };
        assert_eq!(
            chunk.to_json(),
            r#"{"t":"history_chunk","requestId":"h7","events":[{"kind":"narration","text":"older"}],"seqs":[39],"done":true}"#
        );
        let chunk = ClientFrame::HistoryChunk {
            session_id: Some("sess-1".to_string()),
            request_id: "h7".to_string(),
            events: Vec::new(),
            seqs: Vec::new(),
            done: true,
        };
        assert_eq!(
            chunk.to_json(),
            r#"{"t":"history_chunk","sessionId":"sess-1","requestId":"h7","events":[],"seqs":[],"done":true}"#
        );
    }

    /// EXP-637: the resume frame — camelCase on the wire like every other
    /// field, absent on every pre-EXP-637 sender.
    #[test]
    fn start_session_deserializes_resume_session_id() {
        match ServerFrame::parse(
            r#"{"t":"start_session","resumeSessionId":"sess-old","teamId":"ws-1"}"#,
        )
        .unwrap()
        {
            ServerFrame::StartSession {
                resume_session_id,
                team_id,
                issue_id,
                action_id,
                ..
            } => {
                assert_eq!(resume_session_id.as_deref(), Some("sess-old"));
                assert_eq!(team_id.as_deref(), Some("ws-1"));
                assert_eq!(issue_id, None);
                assert_eq!(action_id, None);
            }
            other => panic!("expected StartSession, got {other:?}"),
        }
        // Absent = None (an ordinary start).
        match ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9"}"#).unwrap() {
            ServerFrame::StartSession { resume_session_id, .. } => {
                assert_eq!(resume_session_id, None)
            }
            other => panic!("expected StartSession, got {other:?}"),
        }
        // A malformed (non-string) value must not kill the socket.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"start_session","resumeSessionId":42}"#),
            None
        );
    }

    #[test]
    fn start_session_deserializes_started_by() {
        // EXP-432: a shared-device start carries the requesting teammate's
        // userId — pure attribution, alongside the normal subject/options.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","issueId":"issue-9","startedBy":"user-2"}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: Some("issue-9".into()),
                issue_ids: None,
                action_id: None,
                action_name: None,
                team_id: None,
                repo: None,
                inputs: None,
                started_by: Some("user-2".into()),
                started_reason: None,
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_started_reason() {
        // EXP-679: `startedReason: "agent"` — another coding session started
        // this run, so it is UNATTENDED (it gets the close-out tool, and
        // that call ends it). Snake_case on the wire, beside `startedBy`.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","issueId":"issue-9","startedBy":"user-2","startedReason":"agent"}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: Some("issue-9".into()),
                issue_ids: None,
                action_id: None,
                action_name: None,
                team_id: None,
                repo: None,
                inputs: None,
                started_by: Some("user-2".into()),
                started_reason: Some("agent".into()),
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
        // Absent (every person-started frame, and every pre-EXP-679 sender)
        // is simply None — the run stays attended.
        match ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9"}"#).unwrap() {
            ServerFrame::StartSession { started_reason, .. } => assert_eq!(started_reason, None),
            other => panic!("expected StartSession, got {other:?}"),
        }
    }

    #[test]
    fn start_session_deserializes_launch_options() {
        // hub.ts startSession with EXP-149 options spread into the frame.
        // `effort: ""` is a real value (explicit "CLI default"), not absent.
        // EXP-690: `skipPermissions` is retired but old clients keep sending
        // it — it must parse away silently, never fail the frame.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","issueId":"issue-9","agent":"codex","model":"opus","effort":"","ultracode":true,"planMode":false,"skipPermissions":true}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: Some("issue-9".into()),
                issue_ids: None,
                action_id: None,
                action_name: None,
                team_id: None,
                repo: None,
                inputs: None,
                started_by: None,
                started_reason: None,
                agent: Some("codex".into()),
                model: Some("opus".into()),
                effort: Some(String::new()),
                ultracode: Some(true),
                plan_mode: Some(false),
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_batch_frame_with_options() {
        // EXP-106 batch start: issueIds + teamId + repo, options spread.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","issueIds":["issue-1","issue-2"],"teamId":"ws-7","repo":{"repositoryId":"repo-1","fullName":"acme/api","defaultBranch":"main"},"model":"opus","effort":"high","ultracode":true,"planMode":false}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: None,
                issue_ids: Some(vec!["issue-1".into(), "issue-2".into()]),
                action_id: None,
                action_name: None,
                team_id: Some("ws-7".into()),
                repo: Some(StartRepoGroup {
                    repository_id: "repo-1".into(),
                    full_name: "acme/api".into(),
                    default_branch: "main".into(),
                }),
                inputs: None,
                started_by: None,
                started_reason: None,
                agent: None,
                model: Some("opus".into()),
                effort: Some("high".into()),
                ultracode: Some(true),
                plan_mode: Some(false),
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_action_frame() {
        // EXP-253 action start: actionId + actionName + teamId (+ repo when
        // repo-backed), model/effort options.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","actionId":"act-1","actionName":"Code review","teamId":"ws-7","repo":{"repositoryId":"repo-1","fullName":"acme/api","defaultBranch":"main"},"model":"opus","effort":"high"}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: None,
                issue_ids: None,
                action_id: Some("act-1".into()),
                action_name: Some("Code review".into()),
                team_id: Some("ws-7".into()),
                repo: Some(StartRepoGroup {
                    repository_id: "repo-1".into(),
                    full_name: "acme/api".into(),
                    default_branch: "main".into(),
                }),
                inputs: None,
                started_by: None,
                started_reason: None,
                agent: None,
                model: Some("opus".into()),
                effort: Some("high".into()),
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_action_frame_with_inputs() {
        // EXP-257: an inputs-carrying action start — server-resolved values
        // ride `inputs` (camelCase fields, `type` literal), full options.
        // The retired EXP-690 `skipPermissions` key parses away here too.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","actionId":"act-1","actionName":"Groom","teamId":"ws-7","inputs":[{"key":"scope","label":"Scope","type":"text","value":"urgent only","display":"urgent only"},{"key":"repo","type":"repo","value":"repo-1","display":"acme/api"}],"agent":"codex","model":"gpt-5.6-sol","skipPermissions":true}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: None,
                issue_ids: None,
                action_id: Some("act-1".into()),
                action_name: Some("Groom".into()),
                team_id: Some("ws-7".into()),
                repo: None,
                inputs: Some(vec![
                    StartInput {
                        key: "scope".into(),
                        label: Some("Scope".into()),
                        input_type: Some("text".into()),
                        value: "urgent only".into(),
                        display: Some("urgent only".into()),
                    },
                    StartInput {
                        key: "repo".into(),
                        // A thinned entry: label absent, type present —
                        // consumers fall back per-field.
                        label: None,
                        input_type: Some("repo".into()),
                        value: "repo-1".into(),
                        display: Some("acme/api".into()),
                    },
                ]),
                started_by: None,
                started_reason: None,
                agent: Some("codex".into()),
                model: Some("gpt-5.6-sol".into()),
                effort: None,
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_batch_frame_without_options() {
        // A batch start from a client that sends no EXP-149 options — every
        // option arrives None, the subject fields stay populated.
        assert_eq!(
            ServerFrame::parse(
                r#"{"t":"start_session","issueIds":["issue-1","issue-2"],"teamId":"ws-7","repo":{"repositoryId":"repo-1","fullName":"acme/api","defaultBranch":"main"}}"#
            )
            .unwrap(),
            ServerFrame::StartSession {
                issue_id: None,
                issue_ids: Some(vec!["issue-1".into(), "issue-2".into()]),
                action_id: None,
                action_name: None,
                team_id: Some("ws-7".into()),
                repo: Some(StartRepoGroup {
                    repository_id: "repo-1".into(),
                    full_name: "acme/api".into(),
                    default_branch: "main".into(),
                }),
                inputs: None,
                started_by: None,
                started_reason: None,
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                mcp_server_ids: None,
                account: None,
                resume: false,
                resume_session_id: None,
                prompt: None,
            }
        );
    }

    #[test]
    fn remaining_server_frames_deserialize() {
        assert_eq!(
            ServerFrame::parse(r#"{"t":"input","data":"ls\r"}"#).unwrap(),
            ServerFrame::Input { data: "ls\r".into() }
        );
        assert_eq!(ServerFrame::parse(r#"{"t":"kill"}"#).unwrap(), ServerFrame::Kill);
        // EXP-746: the relay forwards a viewer's chip change verbatim; the
        // BLANK value ("CLI default") is a legitimate payload, not a
        // malformed one.
        assert_eq!(
            ServerFrame::parse(r#"{"t":"set_config","id":"model","value":"opus"}"#).unwrap(),
            ServerFrame::SetConfig {
                id: "model".into(),
                value: "opus".into(),
            }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"set_config","id":"model","value":""}"#).unwrap(),
            ServerFrame::SetConfig {
                id: "model".into(),
                value: String::new(),
            }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"set_mode","id":"plan"}"#).unwrap(),
            ServerFrame::SetMode { id: "plan".into() }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"bye","outcome":"publisher_lost"}"#).unwrap(),
            ServerFrame::Bye {
                outcome: Some("publisher_lost".into())
            }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"bye"}"#).unwrap(),
            ServerFrame::Bye { outcome: None }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"error","code":"no_such_session"}"#).unwrap(),
            ServerFrame::Error {
                code: "no_such_session".into(),
                message: None,
            }
        );
    }

    #[test]
    fn unknown_or_malformed_frames_parse_to_none() {
        // Mirror of the relay's silent-drop: never kill the socket on a
        // future frame type or junk.
        assert_eq!(ServerFrame::parse(r#"{"t":"future_frame"}"#), None);
        assert_eq!(ServerFrame::parse("not json"), None);
        assert_eq!(ServerFrame::parse(r#"{"cols":1}"#), None);
        // The retired PTY-mirror frames take the same path as any unknown
        // frame — an old relay's `resize`/`resync` must not kill the socket.
        assert_eq!(ServerFrame::parse(r#"{"t":"resize","cols":120,"rows":40}"#), None);
        assert_eq!(ServerFrame::parse(r#"{"t":"resync"}"#), None);
    }

    // ── EXP-696 viewer role: the send-side answer + the inbound vocabulary ──

    #[test]
    fn client_answer_serializes_camel_case_and_omits_none() {
        // Byte-for-byte the shape `answerFrame` validates and the shape the
        // web viewer sends (`JSON.stringify({t:"answer",questionId,askId,
        // keys,text})`, minus the keys it leaves undefined).
        assert_eq!(
            ClientFrame::Answer {
                question_id: "toolu_01#0".into(),
                ask_id: Some("toolu_01".into()),
                keys: vec!["1".into(), "3".into()],
                text: None,
            }
            .to_json(),
            r#"{"t":"answer","questionId":"toolu_01#0","askId":"toolu_01","keys":["1","3"]}"#
        );
        // A plan-approval answer has no ask to belong to.
        assert_eq!(
            ClientFrame::Answer {
                question_id: "plan-1".into(),
                ask_id: None,
                keys: vec!["1".into()],
                text: None,
            }
            .to_json(),
            r#"{"t":"answer","questionId":"plan-1","keys":["1"]}"#
        );
        // EXP-513: the typed reply for a freeText row rides `text`.
        assert_eq!(
            ClientFrame::Answer {
                question_id: "toolu_01#0".into(),
                ask_id: Some("toolu_01".into()),
                keys: vec!["4".into()],
                text: Some("purple".into()),
            }
            .to_json(),
            r#"{"t":"answer","questionId":"toolu_01#0","askId":"toolu_01","keys":["4"],"text":"purple"}"#
        );
    }

    #[test]
    fn client_answer_round_trips_through_the_publisher_side_frame() {
        // The two halves of the same wire frame: what a viewer sends must be
        // exactly what a publisher parses (they are separate enums because
        // the DIRECTIONS differ, not the bytes).
        let sent = ClientFrame::Answer {
            question_id: "toolu_01#1".into(),
            ask_id: Some("toolu_01".into()),
            keys: vec!["2".into()],
            text: Some("purple".into()),
        }
        .to_json();
        assert_eq!(
            ServerFrame::parse(&sent).unwrap(),
            ServerFrame::Answer {
                question_id: "toolu_01#1".into(),
                ask_id: Some("toolu_01".into()),
                keys: vec!["2".into()],
                text: Some("purple".into()),
            }
        );
    }

    #[test]
    fn client_set_config_and_set_mode_serialize() {
        // EXP-746: byte-for-byte the shapes `setConfigFrame`/`setModeFrame`
        // validate. Single-word fields — no camelCase rename anywhere.
        assert_eq!(
            ClientFrame::SetConfig {
                id: "model".into(),
                value: "opus".into(),
            }
            .to_json(),
            r#"{"t":"set_config","id":"model","value":"opus"}"#
        );
        // A BLANK value is the "CLI default / omit the flag" choice and must
        // stay ON the wire — the relay's zod deliberately has no `min(1)`
        // there, and dropping the key would parse as a malformed frame.
        assert_eq!(
            ClientFrame::SetConfig {
                id: "model".into(),
                value: String::new(),
            }
            .to_json(),
            r#"{"t":"set_config","id":"model","value":""}"#
        );
        assert_eq!(
            ClientFrame::SetMode { id: "plan".into() }.to_json(),
            r#"{"t":"set_mode","id":"plan"}"#
        );
        // Both halves of the same frame: what a viewer sends is exactly what
        // the publisher parses back off the relay.
        assert_eq!(
            ServerFrame::parse(
                &ClientFrame::SetConfig {
                    id: "effort".into(),
                    value: "high".into(),
                }
                .to_json()
            )
            .unwrap(),
            ServerFrame::SetConfig {
                id: "effort".into(),
                value: "high".into(),
            }
        );
    }

    #[test]
    fn viewer_frames_deserialize_the_relay_vocabulary() {
        // Captured relay strings (hub.ts fans `frame(...)` = JSON.stringify of
        // exactly these objects to the activity audience).
        assert_eq!(
            ViewerFrame::parse(
                r#"{"t":"activity","event":{"kind":"narration","text":"Reading the file"}}"#
            )
            .unwrap(),
            ViewerFrame::Activity {
                event: ActivityEvent::narration("Reading the file"),
                seq: None,
            }
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"activity_reset"}"#).unwrap(),
            ViewerFrame::ActivityReset
        );
        // EXP-656 / EXP-648: the two bare markers.
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"activity_synced"}"#).unwrap(),
            ViewerFrame::ActivitySynced {
                first_seq: None,
                last_seq: None,
                truncated: None,
            }
        );
        // EXP-783: the span-carrying form.
        assert_eq!(
            ViewerFrame::parse(
                r#"{"t":"activity_synced","firstSeq":12,"lastSeq":40,"truncated":true}"#
            )
            .unwrap(),
            ViewerFrame::ActivitySynced {
                first_seq: Some(12),
                last_seq: Some(40),
                truncated: Some(true),
            }
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"keepalive"}"#).unwrap(),
            ViewerFrame::Keepalive
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"bye","outcome":"publisher_lost"}"#).unwrap(),
            ViewerFrame::Bye {
                outcome: Some("publisher_lost".into()),
            }
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"bye"}"#).unwrap(),
            ViewerFrame::Bye { outcome: None }
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"error","code":"no_such_session"}"#).unwrap(),
            ViewerFrame::Error {
                code: "no_such_session".into(),
                message: None,
            }
        );
    }

    #[test]
    fn viewer_ignores_publisher_bound_and_unknown_frames() {
        // The control/publisher inbound frames are not this role's business —
        // and, like any future frame, they must parse to None rather than
        // erroring the pump.
        assert_eq!(ViewerFrame::parse(r#"{"t":"start_session","issueId":"i"}"#), None);
        assert_eq!(ViewerFrame::parse(r#"{"t":"input","data":"x"}"#), None);
        assert_eq!(ViewerFrame::parse(r#"{"t":"kill"}"#), None);
        assert_eq!(ViewerFrame::parse(r#"{"t":"telepathy"}"#), None);
        assert_eq!(ViewerFrame::parse("not json"), None);
        // An event kind from a newer desktop: dropped, socket untouched.
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"activity","event":{"kind":"hologram"}}"#),
            None
        );
    }

    #[test]
    fn activity_events_round_trip_through_the_wire() {
        // The publisher serializes, the viewer deserializes: every kind must
        // survive the round trip with every optional field intact.
        let events = vec![
            ActivityEvent::Narration {
                text: "summary".into(),
                before_question_id: Some("toolu_01".into()),
                message_id: Some("msg_01".into()),
                subagent_id: Some("toolu_task".into()),
                at: Some(1_751_500_000_000),
            },
            ActivityEvent::narration("plain prose"),
            ActivityEvent::Tool {
                name: "Grep".into(),
                detail: Some("fn main".into()),
                id: None,
                tool_kind: None,
                subagent_id: Some("agent_01".into()),
                at: None,
            },
            ActivityEvent::tool("TodoWrite", None),
            ActivityEvent::diff("--- a\n+++ b\n"),
            ActivityEvent::user_message("fix the login bug"),
            ActivityEvent::Question {
                text: "Which color?".into(),
                options: vec![
                    QuestionOption {
                        label: "Red".into(),
                        key: "1".into(),
                        description: Some("warm".into()),
                        free_text: false,
                    },
                    QuestionOption {
                        free_text: true,
                        ..QuestionOption::new("Type something.", "4")
                    },
                ],
                multi_select: Some(true),
                plan_mode: Some(true),
                id: "toolu_01#1".into(),
                ask_id: Some("toolu_01".into()),
                index: Some(2),
                total: Some(3),
                header: Some("Color".into()),
                at: Some(7),
            },
            ActivityEvent::QuestionResolved {
                id: Some("toolu_01#0".into()),
                ask_id: Some("toolu_01".into()),
                answers: Some(vec!["Red".into()]),
                dismissed: Some(true),
                at: None,
            },
            ActivityEvent::AnswerAck {
                id: "toolu_01#0".into(),
                ask_id: None,
                at: None,
            },
            ActivityEvent::Subagent {
                id: "agent_01".into(),
                agent_type: "explore".into(),
                status: SubagentStatus::Completed,
                detail: Some("Map the steer crate".into()),
                at: None,
                tool_calls: None,
                title: None,
                workflow_id: None,
            },
            ActivityEvent::Permission {
                tool: "Bash".into(),
                detail: None,
                at: None,
            },
            ActivityEvent::ConfigState {
                options: vec![
                    ConfigOption {
                        category: Some("model".into()),
                        value: Some("opus".into()),
                        values: Some(vec![ConfigValue::new("opus", "Opus")]),
                        ..ConfigOption::new("model", "Model")
                    },
                    ConfigOption::new("effort", "Effort"),
                ],
                current_mode: Some("plan".into()),
                modes: Some(vec![ConfigMode {
                    description: Some("Read-only until approved".into()),
                    ..ConfigMode::new("plan", "Plan")
                }]),
                commands: Some(vec![ConfigCommand::new("compact", "Compact the context")]),
                at: Some(11),
            },
            ActivityEvent::Usage {
                context_used: 124_000,
                context_size: 200_000,
                cost_usd: Some(1.25),
                at: None,
            },
            ActivityEvent::usage(0, 0, None),
        ];
        for event in events {
            let frame = ClientFrame::Activity {
                event: event.clone(),
                seq: None,
            }
            .to_json();
            assert_eq!(
                ViewerFrame::parse(&frame).unwrap(),
                ViewerFrame::Activity { event: event.clone(), seq: None },
                "round trip {event:?}"
            );
        }
    }

    #[test]
    fn question_option_defaults_fill_in_for_absent_fields() {
        // Pre-EXP-513 publishers omit `freeText` and options often carry no
        // description — both must parse, not fail (an internally-tagged enum
        // has no per-field fallback without `#[serde(default)]`).
        let parsed: QuestionOption =
            serde_json::from_str(r#"{"label":"Red","key":"1"}"#).unwrap();
        assert_eq!(parsed, QuestionOption::new("Red", "1"));
        assert!(!parsed.free_text);
        let free: QuestionOption =
            serde_json::from_str(r#"{"label":"Type something.","key":"4","freeText":true}"#)
                .unwrap();
        assert!(free.free_text);
        // Unknown future fields are ignored, never a parse failure.
        let forward: QuestionOption =
            serde_json::from_str(r#"{"label":"Red","key":"1","hologram":true}"#).unwrap();
        assert_eq!(forward.label, "Red");
    }
}

#[cfg(test)]
mod exp850_tests {
    use super::*;
    use crate::workflow::{
        WorkflowAgent, WorkflowAgentState, WorkflowPhase, WorkflowState, WorkflowStatus,
    };

    /// EXP-850 §1: `wait` joined the contract's `toolKind` values in place
    /// (before `other`), and the ORDER is the lock — reordering it would make
    /// every client's kind bucket disagree with the next.
    #[test]
    fn the_wait_tool_kind_is_in_contract_order() {
        let wire: Vec<&str> = ToolKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(wire, domain::contract::TOOL_KIND_VALUES);
        assert_eq!(ToolKind::parse("wait"), Some(ToolKind::Wait));
        assert_eq!(serde_json::to_string(&ToolKind::Wait).unwrap(), r#""wait""#);
        // A `wait` row is an ordinary tool row on the wire.
        let row = ActivityEvent::Tool {
            name: "TaskOutput".to_string(),
            detail: Some("Sleep in the background".to_string()),
            id: Some("toolu_1".to_string()),
            tool_kind: Some(ToolKind::Wait),
            subagent_id: None,
            at: None,
        };
        let json = serde_json::to_string(&row).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"tool","name":"TaskOutput","detail":"Sleep in the background","id":"toolu_1","toolKind":"wait"}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), row);
    }

    /// EXP-856 / EXP-850 §4: the two additive fields on `subagent`. A pre-850
    /// edge is byte-identical (both are `skip_serializing_if`), and the new
    /// status parses back.
    #[test]
    fn a_duplicate_subagent_edge_serializes_to_the_relay_schema() {
        let plain = ActivityEvent::Subagent {
            id: "toolu_1".to_string(),
            agent_type: "explore".to_string(),
            status: SubagentStatus::Started,
            detail: None,
            at: None,
            tool_calls: None,
            title: None,
            workflow_id: None,
        };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"kind":"subagent","id":"toolu_1","agentType":"explore","status":"started"}"#
        );
        let duplicate = ActivityEvent::Subagent {
            id: "a55b7012793deae02".to_string(),
            agent_type: "general-purpose".to_string(),
            status: SubagentStatus::Duplicate,
            detail: Some(
                "Second copy of slowpoke started while the first is still running (resumed by SendMessage)"
                    .to_string(),
            ),
            at: None,
            tool_calls: None,
            title: Some("slowpoke".to_string()),
            workflow_id: Some("toolu_017Lh63mYhRJ3MrA4A1PXytt".to_string()),
        };
        let json = serde_json::to_string(&duplicate).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"subagent","id":"a55b7012793deae02","agentType":"general-purpose","status":"duplicate","detail":"Second copy of slowpoke started while the first is still running (resumed by SendMessage)","title":"slowpoke","workflowId":"toolu_017Lh63mYhRJ3MrA4A1PXytt"}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), duplicate);
        // The contract owns the vocabulary.
        let wire: Vec<&str> = SubagentStatus::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(wire, domain::contract::SUBAGENT_STATUS_VALUES);
    }

    /// EXP-850 §5: the turn slot's two new fields. A pre-850 edge is
    /// byte-identical, and `turn_at` omits a zero token count (the caption
    /// draws no token group while it is unknown).
    #[test]
    fn the_turn_slot_carries_the_working_caption_inputs() {
        assert_eq!(
            serde_json::to_string(&ActivityEvent::turn(TurnState::Started)).unwrap(),
            r#"{"kind":"turn","state":"started"}"#
        );
        assert_eq!(
            serde_json::to_string(&ActivityEvent::turn_at(
                TurnState::Started,
                Some(1_789_204_409_163),
                Some(0)
            ))
            .unwrap(),
            r#"{"kind":"turn","state":"started","startedAt":1789204409163}"#
        );
        let working =
            ActivityEvent::turn_at(TurnState::Started, Some(1_789_204_409_163), Some(1432));
        let json = serde_json::to_string(&working).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"turn","state":"started","startedAt":1789204409163,"tokens":1432}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), working);
    }

    /// EXP-850 §2: the background-task slot, including the empty list that
    /// closes the strip.
    #[test]
    fn background_tasks_serialize_to_the_relay_schema() {
        assert_eq!(
            serde_json::to_string(&ActivityEvent::background_tasks(Vec::new())).unwrap(),
            r#"{"kind":"background_tasks","tasks":[]}"#
        );
        let event = ActivityEvent::background_tasks(vec![
            BackgroundTask {
                id: "b4mwz6csc".to_string(),
                kind: BackgroundTaskKind::Shell,
                description: "Sleep in the background".to_string(),
                tool_id: Some("toolu_01MCRoRaXN1cvEsHJzDEg2B3".to_string()),
            },
            BackgroundTask {
                id: "w5zr2977l".to_string(),
                kind: BackgroundTaskKind::Workflow,
                description: "Probe the workflow progress wire".to_string(),
                tool_id: None,
            },
        ]);
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"background_tasks","tasks":[{"id":"b4mwz6csc","kind":"shell","description":"Sleep in the background","toolId":"toolu_01MCRoRaXN1cvEsHJzDEg2B3"},{"id":"w5zr2977l","kind":"workflow","description":"Probe the workflow progress wire"}]}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), event);
        // The contract owns the vocabulary, and the CLI's task types fold onto
        // it (anything unknown is `other`, never a dropped frame).
        let wire: Vec<&str> = BackgroundTaskKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(wire, domain::contract::BACKGROUND_TASK_KIND_VALUES);
        assert_eq!(
            BackgroundTaskKind::from_task_type("local_agent"),
            BackgroundTaskKind::Agent
        );
        assert_eq!(
            BackgroundTaskKind::from_task_type("local_teleport"),
            BackgroundTaskKind::Other
        );
    }

    /// EXP-850 §3: the workflow card. Field order IS serialization order and
    /// the relay's zod is declared in the same one.
    #[test]
    fn a_workflow_card_serializes_to_the_relay_schema() {
        let event = ActivityEvent::workflow(WorkflowState {
            id: "toolu_017aGvi2moAfSykrRA4LmyT4".to_string(),
            name: "wire-probe".to_string(),
            description: Some("Probe the workflow progress wire".to_string()),
            status: WorkflowStatus::Running,
            phases: vec![
                WorkflowPhase { index: 1, title: "Alpha".to_string() },
                WorkflowPhase { index: 2, title: "Beta".to_string() },
            ],
            agents: vec![
                WorkflowAgent {
                    index: 1,
                    label: "alpha:one".to_string(),
                    phase_index: Some(1),
                    agent_id: Some("a0ce244c651aaa623".to_string()),
                    model: Some("claude-haiku-4-5-20251001".to_string()),
                    state: WorkflowAgentState::Done,
                    tokens: Some(9629),
                    tool_calls: Some(0),
                    duration_ms: Some(1075),
                    last_tool: None,
                    last_tool_summary: None,
                    result_preview: Some("ok".to_string()),
                    error: None,
                },
                WorkflowAgent {
                    index: 2,
                    label: "alpha:two".to_string(),
                    phase_index: Some(1),
                    state: WorkflowAgentState::Queued,
                    ..WorkflowAgent::default()
                },
            ],
            summary: None,
            at: None,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"workflow","id":"toolu_017aGvi2moAfSykrRA4LmyT4","name":"wire-probe","description":"Probe the workflow progress wire","status":"running","phases":[{"index":1,"title":"Alpha"},{"index":2,"title":"Beta"}],"agents":[{"index":1,"label":"alpha:one","phaseIndex":1,"agentId":"a0ce244c651aaa623","model":"claude-haiku-4-5-20251001","state":"done","tokens":9629,"toolCalls":0,"durationMs":1075,"resultPreview":"ok"},{"index":2,"label":"alpha:two","phaseIndex":1,"state":"queued"}]}"#
        );
        assert_eq!(serde_json::from_str::<ActivityEvent>(&json).unwrap(), event);
        // An unknown status fails the parse, so a future value is DROPPED (the
        // "ignore what you do not know, never kill the socket" rule) rather
        // than rendered as running.
        assert!(serde_json::from_str::<ActivityEvent>(
            r#"{"kind":"workflow","id":"a","name":"w","status":"melting","phases":[],"agents":[]}"#
        )
        .is_err());
    }

    /// The redactor walks every free-text field of the two new kinds, and
    /// NONE of their machine fields (ids, enum values, the launching tool id).
    #[test]
    fn the_new_kinds_expose_only_their_free_text_to_the_redactor() {
        let mut tasks = ActivityEvent::background_tasks(vec![BackgroundTask {
            id: "b1".to_string(),
            kind: BackgroundTaskKind::Shell,
            description: "sleep".to_string(),
            tool_id: Some("toolu_1".to_string()),
        }]);
        assert_eq!(
            tasks.text_fields_mut().iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            vec!["sleep"]
        );
        let mut workflow = ActivityEvent::workflow(WorkflowState {
            id: "toolu_1".to_string(),
            name: "wire-probe".to_string(),
            description: Some("probe".to_string()),
            status: WorkflowStatus::Completed,
            phases: vec![WorkflowPhase { index: 1, title: "Alpha".to_string() }],
            agents: vec![WorkflowAgent {
                index: 1,
                label: "alpha:one".to_string(),
                agent_id: Some("a0ce".to_string()),
                state: WorkflowAgentState::Error,
                last_tool: Some("Bash".to_string()),
                last_tool_summary: Some("sleep 60".to_string()),
                result_preview: Some("ok".to_string()),
                error: Some("boom".to_string()),
                ..WorkflowAgent::default()
            }],
            summary: Some("done".to_string()),
            at: None,
        });
        assert_eq!(
            workflow.text_fields_mut().iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            vec![
                "wire-probe", "probe", "Alpha", "alpha:one", "Bash", "sleep 60", "ok", "boom",
                "done"
            ]
        );
        // `at` is stampable on both, like every other kind.
        *tasks.at_mut() = Some(7);
        *workflow.at_mut() = Some(9);
        assert_eq!(*tasks.at_mut(), Some(7));
        assert_eq!(*workflow.at_mut(), Some(9));
    }
}
