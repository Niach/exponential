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
//! [`ServerFrame::Answer`] that replaces blind keystroke replay.

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
    /// EXP-485: presence ONLY. The agent/launch-defaults advertisement left
    /// this frame — the web server reads it off the persisted `devices` row
    /// written by `devices.register`, which survives relay restarts and
    /// doesn't need a re-dial to change. `caps` stays: the relay itself
    /// routes on it.
    #[serde(rename_all = "camelCase")]
    Online {
        device_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_label: Option<&'a str>,
        /// EXP-253/EXP-257: feature capabilities (`actions`,
        /// `action-inputs`) — remote Run-action pickers strictly gate on
        /// them (absent = no action launch path).
        #[serde(skip_serializing_if = "Option::is_none")]
        caps: Option<&'a [String]>,
    },
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
    Join,
    /// NOTE: the field is `data` — a UTF-8 `String`, relay-enforced ≤ 8 KiB —
    /// NOT `bytes`. (A native client shipped `bytes` and steer input silently
    /// no-op'd.)
    Input {
        data: String,
    },
    Kill,
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
#[derive(Clone, Debug, Serialize, PartialEq)]
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
            skip_serializing_if = "Option::is_none"
        )]
        before_question_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A tool-call headline: the tool name + a single primary argument
    /// (file path / pattern / Bash description — NEVER a command string or a
    /// tool result). `subagentId` attributes the call to a running
    /// [`ActivityEvent::Subagent`] so clients can nest it under that agent.
    #[serde(rename_all = "camelCase")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subagent_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A worktree unified diff snapshot (latest replaces prior, viewer-side).
    Diff {
        diff: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A human turn from the transcript: the initial prompt or a (locally- or
    /// remotely-)steered message (EXP-78). MEMBER-ONLY on the relay — never
    /// fanned to anonymous public viewers ("never steering input").
    UserMessage {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
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
    /// "Type something" choice only the TUI grid reveals). An id-less
    /// question is the legacy keystroke-only path (old desktop).
    #[serde(rename_all = "camelCase")]
    Question {
        text: String,
        options: Vec<QuestionOption>,
        #[serde(skip_serializing_if = "Option::is_none")]
        multi_select: Option<bool>,
        /// `Some(true)` when this question is an `ExitPlanMode` plan-approval
        /// picker (EXP-97) — clients render a dedicated "Plan ready" card.
        /// Presentation-only: the options remain the source of the keystrokes.
        /// `text` is then the full plan markdown, and `askId`/`index`/`total`
        /// are absent.
        #[serde(skip_serializing_if = "Option::is_none")]
        plan_mode: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Groups the steps of ONE multi-question `AskUserQuestion`. A step
        /// carries `index`/`total`; the FINAL review/submit step carries
        /// `askId` with neither.
        #[serde(skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        header: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A question stopped being answerable: answered here or elsewhere,
    /// dismissed, or the whole ask was submitted. Retires the card with `id`
    /// when present, otherwise EVERY card of `askId`.
    #[serde(rename_all = "camelCase")]
    QuestionResolved {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        answers: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        dismissed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// The desktop injected a steerer's answer into the TUI — clients keep
    /// the card LOCKED from here until the matching
    /// [`ActivityEvent::QuestionResolved`].
    #[serde(rename_all = "camelCase")]
    AnswerAck {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ask_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A `Task` subagent's lifecycle edge — `id` keys the
    /// [`ActivityEvent::Tool`] events attributed to it.
    #[serde(rename_all = "camelCase")]
    Subagent {
        id: String,
        agent_type: String,
        status: SubagentStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// The session is sitting on a permission prompt. INFORMATIONAL — it
    /// carries no options and is never answerable remotely (the local TUI
    /// owns permission decisions).
    Permission {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
}

/// `started` | `completed` — the two [`ActivityEvent::Subagent`] edges.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
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
            | ActivityEvent::Permission { at, .. } => at,
        }
    }
}

/// One answer choice of an [`ActivityEvent::Question`].
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    /// Raw keystroke(s) that select this option in the `claude` TUI picker.
    pub key: String,
    /// The option's secondary line (claude's `AskUserQuestion` options carry
    /// one); omitted when the picker offers none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// EXP-513: the option is claude's synthetic free-text row ("Type
    /// something."). A client renders it as an inline text input and sends
    /// the typed text on the answer frame; the desktop types it into the
    /// TUI's inline editor. Omitted when false so pre-EXP-513 consumers see
    /// byte-identical frames.
    #[serde(rename = "freeText", skip_serializing_if = "std::ops::Not::not")]
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

impl ClientFrame<'_> {
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
        /// Launch options (EXP-149) — absent on frames from clients that
        /// don't send them yet; absent = desktop settings default.
        /// `agent`/`skip_permissions` are the EXP-201 additions (absent
        /// agent = claude, the exact pre-EXP-201 behavior).
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
        #[serde(default)]
        skip_permissions: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── ClientFrame vectors — authored from protocol.ts zod schemas + the
    // hub tests' literal frames (`hub.test.ts` sends exactly these shapes).

    #[test]
    fn online_serializes_camel_case_device_fields() {
        assert_eq!(
            ClientFrame::Online {
                device_id: "dev-1",
                device_label: Some("MacBook"),
                caps: None,
            }
            .to_json(),
            r#"{"t":"online","deviceId":"dev-1","deviceLabel":"MacBook"}"#
        );
        assert_eq!(
            ClientFrame::Online {
                device_id: "dev-1",
                device_label: None,
                caps: None,
            }
            .to_json(),
            r#"{"t":"online","deviceId":"dev-1"}"#
        );
        // EXP-253: the capability advertisement rides `caps` — the ONE
        // advertisement left on this frame. EXP-485 took `agents`,
        // `unauthedAgents` and `launchDefaults` off it: the web server reads
        // them off the persisted devices row instead.
        let caps = vec!["actions".to_string()];
        assert_eq!(
            ClientFrame::Online {
                device_id: "dev-1",
                device_label: Some("MacBook"),
                caps: Some(&caps),
            }
            .to_json(),
            r#"{"t":"online","deviceId":"dev-1","deviceLabel":"MacBook","caps":["actions"]}"#
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
        assert_eq!(ClientFrame::Join.to_json(), r#"{"t":"join"}"#);
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
            },
            ActivityEvent::Permission { tool: "Bash".into(), detail: None, at: None },
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
                ClientFrame::Online {
                    device_id: "d",
                    device_label: None,
                    caps: None,
                },
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
            (ClientFrame::Join, "join"),
            (ClientFrame::Input { data: String::new() }, "input"),
            (ClientFrame::Kill, "kill"),
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
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                skip_permissions: None,
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
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                skip_permissions: None,
                resume: false,
                resume_session_id: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_launch_options() {
        // hub.ts startSession with EXP-149 options spread into the frame.
        // `effort: ""` is a real value (explicit "CLI default"), not absent.
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
                agent: Some("codex".into()),
                model: Some("opus".into()),
                effort: Some(String::new()),
                ultracode: Some(true),
                plan_mode: Some(false),
                skip_permissions: Some(true),
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
                agent: None,
                model: Some("opus".into()),
                effort: Some("high".into()),
                ultracode: Some(true),
                plan_mode: Some(false),
                skip_permissions: None,
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
                agent: None,
                model: Some("opus".into()),
                effort: Some("high".into()),
                ultracode: None,
                plan_mode: None,
                skip_permissions: None,
                resume: false,
                resume_session_id: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_action_frame_with_inputs() {
        // EXP-257: an inputs-carrying action start — server-resolved values
        // ride `inputs` (camelCase fields, `type` literal), full options.
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
                agent: Some("codex".into()),
                model: Some("gpt-5.6-sol".into()),
                effort: None,
                ultracode: None,
                plan_mode: None,
                skip_permissions: Some(true),
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
                agent: None,
                model: None,
                effort: None,
                ultracode: None,
                plan_mode: None,
                skip_permissions: None,
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
}
