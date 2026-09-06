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
    Activity {
        event: ActivityEvent,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A tool-call headline: the tool name + a single primary argument
    /// (file path / pattern / Bash description — NEVER a command string or a
    /// tool result). `subagentId` attributes the call to a running
    /// [`ActivityEvent::Subagent`] so clients can nest it under that agent.
    #[serde(rename_all = "camelCase")]
    Tool {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
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
    /// "Type something" choice only the TUI grid reveals). `id` stays
    /// `Option` so decoding an old publisher's frame never fails, but an
    /// id-less question is READ-ONLY on every client (EXP-730 retired the
    /// blind-keystroke answer path the desktop kept for it).
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
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
    /// claude's PreCompact trigger (`manual` | `auto`; pi's threshold/
    /// overflow map to `auto`), absent on codex — kept a plain string so a
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
}

/// `started` | `ended` — the two [`ActivityEvent::Compaction`] edges.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompactionPhase {
    Started,
    Ended,
}

/// `started` | `completed` — the two [`ActivityEvent::Subagent`] edges.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubagentStatus {
    Started,
    Completed,
}

impl ActivityEvent {
    /// The legacy shorthands — the shapes with no v2 fields at all.
    pub fn narration(text: impl Into<String>) -> Self {
        ActivityEvent::Narration {
            text: text.into(),
            before_question_id: None,
            at: None,
        }
    }

    pub fn diff(diff: impl Into<String>) -> Self {
        ActivityEvent::Diff { diff: diff.into(), at: None }
    }

    pub fn user_message(text: impl Into<String>) -> Self {
        ActivityEvent::UserMessage { text: text.into(), at: None }
    }

    pub fn tool(name: impl Into<String>, detail: Option<String>) -> Self {
        ActivityEvent::Tool {
            name: name.into(),
            detail,
            subagent_id: None,
            at: None,
        }
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
                agent_type, detail, ..
            } => {
                let mut fields = vec![agent_type];
                fields.extend(detail.as_mut());
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
            ActivityEvent::Usage { .. } => Vec::new(),
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
            | ActivityEvent::Usage { at, .. } => at,
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
    Activity { event: ActivityEvent },
    /// "Drop everything rendered so far" — sent immediately BEFORE the join
    /// replay and before any publisher-driven full re-publish. EXP-656: a
    /// client stages what follows rather than blanking the feed on the spot.
    ActivityReset,
    /// EXP-656: end-of-replay marker, sent to the JOINING viewer right after
    /// the replay — "the picture is complete, commit it". Absent on a
    /// publisher-driven republish (old desktops give the relay no
    /// end-of-republish signal), which is why clients also keep a quiet-timer
    /// fallback.
    ActivitySynced,
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
                event: ActivityEvent::narration("Reading the file")
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"narration","text":"Reading the file"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::tool("Edit", Some("src/main.rs".into()))
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"Edit","detail":"src/main.rs"}}"#
        );
        // detail is omitted when absent.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::tool("TodoWrite", None)
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"TodoWrite"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::diff("--- a\n+++ b\n")
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
                event: ActivityEvent::user_message("fix the login bug")
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
                    id: None,
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Which color?","options":[{"label":"Red","key":"1"},{"label":"Blue","key":"2"}],"multiSelect":true}}"#
        );
        // multiSelect and planMode are omitted when absent.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "Approve?".into(),
                    options: vec![QuestionOption::new("Approve", "1")],
                    multi_select: None,
                    plan_mode: None,
                    id: None,
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"Approve?","options":[{"label":"Approve","key":"1"}]}}"#
        );
        // A plan-approval question carries the planMode marker (EXP-97).
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Question {
                    text: "The plan".into(),
                    options: vec![QuestionOption::new("Approve — auto-accept edits", "1")],
                    multi_select: None,
                    plan_mode: Some(true),
                    id: None,
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"question","text":"The plan","options":[{"label":"Approve — auto-accept edits","key":"1"}],"planMode":true}}"#
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
                    id: Some("toolu_01#1".into()),
                    ask_id: Some("toolu_01".into()),
                    index: Some(2),
                    total: Some(3),
                    header: Some("Color".into()),
                    at: Some(1_751_500_000_000),
                }
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
                    id: Some("toolu_01#submit".into()),
                    ask_id: Some("toolu_01".into()),
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                }
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
                }
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
                }
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
                }
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
                }
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
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"subagent","id":"agent_01","agentType":"explore","status":"started","detail":"Map the steer crate"}}"#
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
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"subagent","id":"agent_01","agentType":"explore","status":"completed"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Tool {
                    name: "Grep".into(),
                    detail: Some("fn main".into()),
                    subagent_id: Some("agent_01".into()),
                    at: None,
                }
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
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"permission","tool":"Bash","detail":"needs your permission"}}"#
        );
    }

    #[test]
    fn compaction_serializes_to_the_relay_schema_and_parses_back() {
        // EXP-724: `{kind, phase, trigger?, at?}` — trigger omitted when
        // unknown (codex), present verbatim when claude/pi report it.
        let started = ActivityEvent::compaction(CompactionPhase::Started, Some("manual"));
        assert_eq!(
            ClientFrame::Activity { event: started.clone() }.to_json(),
            r#"{"t":"activity","event":{"kind":"compaction","phase":"started","trigger":"manual"}}"#
        );
        let ended = ActivityEvent::compaction(CompactionPhase::Ended, None);
        assert_eq!(
            ClientFrame::Activity { event: ended.clone() }.to_json(),
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
            ClientFrame::Activity { event: event.clone() }.to_json(),
            r#"{"t":"activity","event":{"kind":"config_state","options":[{"id":"model","label":"Model","category":"model","value":"opus","values":[{"id":"opus","label":"Opus"},{"id":"sonnet","label":"Sonnet"}]}],"currentMode":"plan","modes":[{"id":"plan","label":"Plan","description":"Read-only until approved"},{"id":"default","label":"Default"}],"commands":[{"name":"compact","description":"Compact the context","hint":"instructions"},{"name":"new","description":"Start a fresh context"}],"at":9}}"#
        );
        assert_eq!(
            ViewerFrame::parse(&ClientFrame::Activity { event: event.clone() }.to_json()).unwrap(),
            ViewerFrame::Activity { event }
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
            ClientFrame::Activity { event: metered.clone() }.to_json(),
            r#"{"t":"activity","event":{"kind":"usage","contextUsed":124000,"contextSize":200000,"costUsd":1.25,"at":5}}"#
        );
        // A plan run reports no spend: the key is absent, never `null`.
        assert_eq!(
            serde_json::to_string(&ActivityEvent::usage(0, 200_000, None)).unwrap(),
            r#"{"kind":"usage","contextUsed":0,"contextSize":200000}"#
        );
        assert_eq!(
            ViewerFrame::parse(&ClientFrame::Activity { event: metered.clone() }.to_json()).unwrap(),
            ViewerFrame::Activity { event: metered }
        );
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
                id: None,
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
            at: None,
        };
        assert!(serde_json::to_string(&anchored)
            .unwrap()
            .contains(r#""beforeQuestionId":"toolu_01""#));
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
                resume: false,
                resume_session_id: None,
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
        // Unknown future frames still drop silently, never kill the socket.
        assert_eq!(ServerFrame::parse(r#"{"t":"telepathy"}"#), None);
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
                resume: false,
                resume_session_id: None,
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
            }
        );
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"activity_reset"}"#).unwrap(),
            ViewerFrame::ActivityReset
        );
        // EXP-656 / EXP-648: the two bare markers.
        assert_eq!(
            ViewerFrame::parse(r#"{"t":"activity_synced"}"#).unwrap(),
            ViewerFrame::ActivitySynced
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
                at: Some(1_751_500_000_000),
            },
            ActivityEvent::narration("plain prose"),
            ActivityEvent::Tool {
                name: "Grep".into(),
                detail: Some("fn main".into()),
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
                id: Some("toolu_01#1".into()),
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
            }
            .to_json();
            assert_eq!(
                ViewerFrame::parse(&frame).unwrap(),
                ViewerFrame::Activity { event: event.clone() },
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
