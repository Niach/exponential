//! The codex activity emitter (EXP-383): tails the rollout JSONL an
//! interactive `codex` TUI session writes and maps it onto the same scrubbed
//! [`ActivityEvent`] stream the claude emitter publishes — narration, tool
//! headlines, the user's prompts, `request_user_input` questions, and the
//! synced needs-input flag — plus the shared debounced worktree diff.
//!
//! Ground truth (verified against codex-cli 0.144.5 source + real rollouts):
//!
//! - Codex records every session as
//!   `$CODEX_HOME|~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`,
//!   flushed PER LINE (`codex-rs/rollout/src/recorder.rs`), so a same-machine
//!   tail is live. The first line is `session_meta` with the canonical spawn
//!   `cwd` — the same discovery contract `coding::codex_sessions` uses for
//!   resume (duplicated here: `steer` cannot depend on `coding`, §3.1).
//! - Envelope: `{"timestamp": …, "type": <variant>, "payload": {…}}`. Unknown
//!   `type`/`payload.type` values MUST be ignored — the vocabulary grows
//!   between codex versions (`world_state` appeared inside one minor line).
//! - In the default `history_mode: "legacy"`, human prompts arrive as
//!   `event_msg`/`user_message` (human text ONLY — the `response_item`
//!   `role:"user"` stream also carries injected `<environment_context>`
//!   blocks and must never be published), narration as `event_msg`/
//!   `agent_message` + `agent_reasoning`, and turn edges as `task_started`/
//!   `task_complete`/`turn_aborted`. In the experimental `paginated` mode
//!   those event_msgs are replaced by `item_completed` turn items; we then
//!   degrade to the always-persisted `response_item` stream (narration +
//!   tools keep working, questions too) and say so once.
//! - Exec/patch APPROVAL prompts are never persisted to the rollout — the
//!   emitter watches the terminal GRID for them instead
//!   ([`crate::codex_approval_picker`], EXP-455) and publishes each modal as
//!   an answerable question, executing remote answers against the TUI by
//!   keystroke exactly like the claude emitter does.
//! - `codex resume` appends to the SAME rollout file — a full-history replay
//!   on attach, deliberately matching the claude `--continue` posture.
//!
//! Redaction stance (stricter than claude's, because codex has no Bash
//! `description` field): an `exec_command` headline is the DERIVED first
//! token of the command, never the raw command string; `apply_patch`
//! publishes only the touched file paths, never the patch body; tool OUTPUTS
//! are never read beyond `request_user_input` answer extraction. Everything
//! published passes the [`Redactor`] + the shared truncation caps.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use serde_json::Value;
use terminal::{display_offset, screen_lines, TermHandle};

use crate::activity::{
    secrets_from_worktree, DiffSnapshots, EmitterConfig, NeedsInputForwarder, Redactor,
    ANSWERS_MAX, ANSWER_MAX, NARRATION_MAX, OPTION_DESCRIPTION_MAX, OPTION_LABEL_MAX,
    POLL_INTERVAL, QUESTION_HEADER_MAX, QUESTION_OPTIONS_MAX, QUESTION_TEXT_MAX, TOOL_DETAIL_MAX,
    TOOL_NAME_MAX,
};
use crate::activity::{
    settle, settle_for, tail_transcript, truncate, AnswerAttempt, RemoteAnswer, ANSWER_RETRY_TTL,
    PLAN_SUBMIT_PROBE,
};
use crate::codex_approval_picker::{self, ApprovalSnapshot, CodexApprovalWatcher};
use crate::frames::{ActivityEvent, QuestionOption};
use crate::publisher::{ActivitySender, InputHook};

/// How long to wait for the rollout file before logging (mirrors the claude
/// emitter's TRANSCRIPT_WAIT posture — diffs keep flowing either way).
const ROLLOUT_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Full-tree rediscovery cadence once a rollout is bound: the sweep walks
/// every session codex ever recorded, so it must not run every tick. A
/// missing file (fresh session still materializing) rescans every tick.
const REDISCOVER_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// Discovery slack: the rollout's mtime must be at/after spawn minus this —
/// clock granularity must not hide a file codex created just before we
/// snapshotted the spawn time.
const MTIME_SLACK: std::time::Duration = std::time::Duration::from_secs(5);

/// `$CODEX_HOME|~/.codex` + `sessions` (duplicate of
/// `coding::codex_sessions::default_codex_sessions_root` — §3.1).
fn default_codex_sessions_root() -> Option<PathBuf> {
    let home = match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            PathBuf::from(home).join(".codex")
        }
    };
    Some(home.join("sessions"))
}

/// EXP-443: what discovery is allowed to bind to, beyond the cwd match.
#[derive(Clone, Debug, Default)]
pub(crate) struct RolloutWant {
    /// The launcher-stamped per-spawn originator
    /// (`CODEX_INTERNAL_ORIGINATOR_OVERRIDE`) — rollout metas whose
    /// `originator` matches are preferred outright, so a foreign codex (a
    /// "+" shell tab, a manual `codex`, a concurrent action run) sharing the
    /// cwd can no longer hijack the feed. `None`, or no matching rollout
    /// (a codex build that ignores the override): degrade to the legacy
    /// newest-cwd-match, logged once by the caller.
    pub(crate) originator: Option<String>,
    /// A native resume's exact rollout session id — the strongest pin; an id
    /// match beats everything (an in-TUI `/new` afterwards rotates the file
    /// but keeps the originator, so the originator pass still follows).
    pub(crate) session_id: Option<String>,
}

/// The newest live rollout for this session: `rollout-*.jsonl` (a `.zst`
/// sibling is by definition not the live one), modified at/after `after`,
/// whose first-line `session_meta.cwd` matches the worktree (raw or
/// canonicalized — codex records its canonical cwd). Filenames embed the full
/// ISO timestamp, so a descending sort over basenames is newest-first.
///
/// EXP-443 pin order among cwd matches: `want.session_id` exact match, then
/// newest `want.originator` match, then the legacy newest match (the
/// explicit fallback keeping a wrong/ignored originator override exactly as
/// safe as the pre-pin behavior, never worse).
pub(crate) fn find_live_rollout(
    sessions_root: &Path,
    worktree: &Path,
    after: SystemTime,
    want: &RolloutWant,
) -> Option<PathBuf> {
    let after = after.checked_sub(MTIME_SLACK).unwrap_or(after);
    let mut rollouts: Vec<(String, PathBuf)> = Vec::new();
    collect_rollouts(sessions_root, 0, after, &mut rollouts);
    rollouts.sort_by(|a, b| b.0.cmp(&a.0));
    let canonical = std::fs::canonicalize(worktree).ok();
    let mut originator_match: Option<PathBuf> = None;
    let mut newest_cwd_match: Option<PathBuf> = None;
    for (_, path) in rollouts {
        let Some(meta) = read_session_meta(&path) else {
            continue;
        };
        let cwd = Path::new(&meta.cwd);
        if !(cwd == worktree || Some(cwd) == canonical.as_deref()) {
            continue;
        }
        if let (Some(want_id), Some(id)) = (&want.session_id, &meta.id) {
            if want_id == id {
                return Some(path);
            }
        }
        if originator_match.is_none()
            && want.originator.is_some()
            && meta.originator == want.originator
        {
            originator_match = Some(path.clone());
        }
        if newest_cwd_match.is_none() {
            newest_cwd_match = Some(path);
        }
        // Keep walking: an older file may still carry the resume id.
        if want.session_id.is_none() && (want.originator.is_none() || originator_match.is_some()) {
            break;
        }
    }
    originator_match.or(newest_cwd_match)
}

/// Recursive bounded `rollout-*.jsonl` sweep, pre-filtered by mtime (a resume
/// appends to an old file, bumping its mtime — so the filter keeps it).
fn collect_rollouts(dir: &Path, depth: usize, after: SystemTime, out: &mut Vec<(String, PathBuf)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, depth + 1, after, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
            continue;
        }
        let fresh = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified >= after)
            .unwrap_or(false);
        if fresh {
            out.push((name.to_string(), path));
        }
    }
}

/// First-line session meta. Lenient by design: the payload nesting is
/// current codex; a flat legacy shape still resolves. `id`/`originator` are
/// optional extras (EXP-443) — only `cwd` gates discovery. Sibling of
/// `coding::codex_sessions`' meta parsing (§3.1 — the crates cannot share).
struct RolloutMeta {
    cwd: String,
    id: Option<String>,
    originator: Option<String>,
}

fn read_session_meta(path: &Path) -> Option<RolloutMeta> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let meta = value.get("payload").unwrap_or(&value);
    Some(RolloutMeta {
        cwd: meta.get("cwd")?.as_str()?.to_string(),
        id: meta
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string),
        originator: meta
            .get("originator")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

// ---------------------------------------------------------------------------
// Rollout line parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HistoryMode {
    #[default]
    Legacy,
    Paginated,
}

/// One pending `request_user_input` ask, keyed by its `call_id` — resolved
/// (and its cards retired at the viewers) by the matching
/// `function_call_output`.
#[derive(Debug, Default)]
struct PendingAsk {
    questions: usize,
}

#[derive(Debug, Default)]
pub(crate) struct CodexState {
    history_mode: HistoryMode,
    paginated_notice_sent: bool,
    pending_asks: HashMap<String, PendingAsk>,
    /// Consecutive-duplicate suppression for reasoning headlines.
    last_reasoning: String,
    /// The turn-edge half of the attention flag (`task_complete` /
    /// `turn_aborted` → idle, waiting on a human).
    idle: bool,
}

impl CodexState {
    /// The combined "agent waits for a human" flag the needs-input forwarder
    /// publishes: idle between turns, or parked on a `request_user_input`.
    pub(crate) fn attention(&self) -> bool {
        self.idle || !self.pending_asks.is_empty()
    }
}

/// The remotely-answerable approval question riding the grid watcher
/// (EXP-455) — codex's analogue of the claude emitter's permission state.
#[derive(Default)]
pub(crate) struct CodexApprovals {
    seq: u32,
    /// `(question id, published options)` while a card is live.
    live: Option<(String, Vec<QuestionOption>)>,
    answered: HashSet<String>,
}

/// An approval overlay settled on screen — publish it as an ordinary
/// id-carrying question so every client's existing card UI carries the
/// decision. A changed overlay retires the previous card first.
///
/// The question text carries the overlay's own context lines — including the
/// `$ command` row — through the [`Redactor`]. That is a DELIBERATE
/// exception to this module's derived-first-token-only stance for exec
/// headlines: the command IS the decision payload here, and approving it
/// blind would be worse than showing it (same posture as the claude
/// permission dialog, whose card shows the redacted command too).
fn publish_approval(
    approvals: &mut CodexApprovals,
    snapshot: ApprovalSnapshot,
    sender: &ActivitySender,
    redactor: &Redactor,
) {
    resolve_approval(approvals, sender);
    approvals.seq += 1;
    let id = format!("approval:{}", approvals.seq);
    let mut text = snapshot.title;
    if !snapshot.context.is_empty() {
        text.push_str("\n\n");
        text.push_str(&snapshot.context.join("\n"));
    }
    let options: Vec<QuestionOption> = snapshot
        .options
        .into_iter()
        .take(QUESTION_OPTIONS_MAX)
        .map(|option| {
            QuestionOption::new(
                truncate(&redactor.redact(&option.label), OPTION_LABEL_MAX),
                option.key,
            )
        })
        .collect();
    sender.send(ActivityEvent::Question {
        text: truncate(&redactor.redact(&text), QUESTION_TEXT_MAX),
        options: options.clone(),
        multi_select: None,
        plan_mode: None,
        id: Some(id.clone()),
        ask_id: None,
        index: None,
        total: None,
        header: Some("Approval required".to_string()),
        at: None,
    });
    approvals.live = Some((id, options));
}

/// The approval overlay left the grid — answered (remotely or at the local
/// TUI) or dismissed; retire the card.
fn resolve_approval(approvals: &mut CodexApprovals, sender: &ActivitySender) {
    let Some((id, _)) = approvals.live.take() else {
        return;
    };
    sender.send(ActivityEvent::QuestionResolved {
        id: Some(id),
        ask_id: None,
        answers: None,
        dismissed: None,
        at: None,
    });
}

/// Inject a steerer's answer into the codex TUI and acknowledge it once the
/// overlay moves on — the same digit-then-probe-then-Enter choreography and
/// Retry/Settled contract as the claude emitter's `handle_answer` (EXP-334):
/// codex's approval list actuates on the digit itself (verified against
/// list_selection_view 0.144.5), so the Enter branch is the safety net.
fn handle_approval_answer(
    approvals: &mut CodexApprovals,
    answer: &RemoteAnswer,
    term: &TermHandle,
    write_input: &InputHook,
    sender: &ActivitySender,
) -> AnswerAttempt {
    if approvals.answered.contains(&answer.question_id) {
        // Already injected — never twice, but re-ack for late joiners
        // (EXP-374).
        sender.send(ActivityEvent::AnswerAck {
            id: answer.question_id.clone(),
            ask_id: None,
            at: None,
        });
        return AnswerAttempt::Settled;
    }
    let Some((id, options)) = approvals.live.clone() else {
        return AnswerAttempt::Settled; // stale/unknown id
    };
    if answer.question_id != id {
        return AnswerAttempt::Settled;
    }
    if display_offset(term) > 0 {
        return AnswerAttempt::Retry;
    }
    let Some(visible) = codex_approval_picker::detect(&screen_lines(term)) else {
        return AnswerAttempt::Retry;
    };
    let Some(key) = answer.keys.first() else {
        return AnswerAttempt::Settled;
    };
    if !options.iter().any(|option| &option.key == key)
        || !visible.options.iter().any(|option| &option.key == key)
    {
        return AnswerAttempt::Settled;
    }
    write_input(key.as_bytes());
    // "Moved" compares SNAPSHOTS, not mere presence: confirming one modal
    // can paint the queued next one in its place within the probe window,
    // and an Enter fired at that new modal would activate its highlighted
    // row.
    let moved = || match codex_approval_picker::detect(&screen_lines(term)) {
        None => true,
        Some(next) => next != visible,
    };
    if !settle_for(PLAN_SUBMIT_PROBE, moved) {
        write_input(b"\r");
        if !settle(moved) {
            return AnswerAttempt::Settled; // injected — never twice
        }
    }
    // `live` stays set — the watcher's absence transition still owes the
    // viewers the `question_resolved` retirement.
    approvals.answered.insert(id.clone());
    sender.send(ActivityEvent::AnswerAck {
        id,
        ask_id: None,
        at: None,
    });
    AnswerAttempt::Settled
}

/// Parse one rollout line into zero or more publishable events, updating
/// `state`. Unknown shapes are silently ignored — the codex vocabulary grows
/// between versions and an unparseable line must never kill the feed.
pub(crate) fn parse_rollout_line(
    line: &str,
    state: &mut CodexState,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    let payload = value.get("payload").unwrap_or(&Value::Null);
    match kind {
        "session_meta" => {
            if payload.get("history_mode").and_then(Value::as_str) == Some("paginated") {
                state.history_mode = HistoryMode::Paginated;
                if !state.paginated_notice_sent {
                    state.paginated_notice_sent = true;
                    return vec![ActivityEvent::narration(
                        "Live detail is limited for this codex session (paginated history).",
                    )];
                }
            }
            Vec::new()
        }
        "event_msg" => parse_event_msg(payload, state, redactor),
        "response_item" => parse_response_item(payload, state, redactor),
        _ => Vec::new(),
    }
}

fn parse_event_msg(payload: &Value, state: &mut CodexState, redactor: &Redactor) -> Vec<ActivityEvent> {
    let Some(kind) = payload.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match kind {
        // The human's prompt — steered messages land here too (mid-turn, no
        // new task_started). This is the ONLY user-text source in legacy mode.
        "user_message" => {
            state.idle = false;
            text_event(payload.get("message"), redactor, |text| {
                ActivityEvent::user_message(truncate(text, NARRATION_MAX))
            })
        }
        // Final prose for the turn.
        "agent_message" => text_event(payload.get("message"), redactor, |text| {
            ActivityEvent::narration(truncate(text, NARRATION_MAX))
        }),
        // The in-flight reasoning headline the TUI shows while working —
        // first line only, consecutive dupes suppressed.
        "agent_reasoning" => {
            let Some(text) = payload.get("text").and_then(Value::as_str) else {
                return Vec::new();
            };
            let headline = text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("")
                .trim_matches('*')
                .trim()
                .to_string();
            if headline.is_empty() || headline == state.last_reasoning {
                return Vec::new();
            }
            state.last_reasoning = headline.clone();
            vec![ActivityEvent::narration(truncate(
                &redactor.redact(&headline),
                NARRATION_MAX,
            ))]
        }
        "task_started" => {
            state.idle = false;
            Vec::new()
        }
        // `last_agent_message` was already published as `agent_message`.
        "task_complete" => {
            state.idle = true;
            Vec::new()
        }
        "turn_aborted" => {
            state.idle = true;
            vec![ActivityEvent::narration("Turn aborted.")]
        }
        _ => Vec::new(),
    }
}

fn parse_response_item(
    payload: &Value,
    state: &mut CodexState,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let Some(kind) = payload.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match kind {
        "function_call" => parse_function_call(payload, state, redactor),
        "function_call_output" => parse_function_call_output(payload, state, redactor),
        "custom_tool_call" => parse_custom_tool_call(payload, redactor),
        // `message` mirrors the event_msg stream in legacy mode (skip: it
        // would double-publish, and the `role:"user"` items carry injected
        // context blocks). In paginated mode the event_msgs are gone, so the
        // assistant half becomes the narration source.
        "message" if state.history_mode == HistoryMode::Paginated => {
            if payload.get("role").and_then(Value::as_str) != Some("assistant") {
                return Vec::new();
            }
            let text = content_text(payload.get("content"));
            if text.is_empty() {
                return Vec::new();
            }
            vec![ActivityEvent::narration(truncate(
                &redactor.redact(&text),
                NARRATION_MAX,
            ))]
        }
        _ => Vec::new(),
    }
}

/// Redact-and-wrap a non-empty string field into one event.
fn text_event(
    field: Option<&Value>,
    redactor: &Redactor,
    build: impl Fn(&str) -> ActivityEvent,
) -> Vec<ActivityEvent> {
    let Some(text) = field.and_then(Value::as_str) else {
        return Vec::new();
    };
    let text = redactor.redact(text.trim());
    if text.is_empty() {
        return Vec::new();
    }
    vec![build(&text)]
}

/// Join the text blocks of a `content` array (`output_text`/`input_text`).
fn content_text(content: Option<&Value>) -> String {
    let Some(items) = content.and_then(Value::as_array) else {
        return String::new();
    };
    items
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn parse_function_call(
    payload: &Value,
    state: &mut CodexState,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return Vec::new();
    };
    // `arguments` is a JSON STRING, not an object.
    let arguments: Value = payload
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    match name {
        // Codex's AskUserQuestion analogue — the only interactive prompt the
        // rollout persists. Published as id-less cards (the legacy viewer
        // path: informational, answered by replying in chat or locally).
        "request_user_input" => {
            let call_id = payload.get("call_id").and_then(Value::as_str);
            request_user_input_events(call_id, &arguments, state, redactor)
        }
        // The raw command string is never published — the derived first
        // token only (codex has no claude-style description field).
        "exec_command" => {
            let detail = exec_command_headline(&arguments)
                .map(|headline| truncate(&redactor.redact(&headline), TOOL_DETAIL_MAX));
            vec![ActivityEvent::tool("exec_command", detail)]
        }
        // Interactive stdin chunks to a running exec — noise, and their
        // payloads are input bytes we must not publish.
        "write_stdin" => Vec::new(),
        "view_image" => {
            let detail = arguments
                .get("path")
                .and_then(Value::as_str)
                .map(|path| truncate(&redactor.redact(path), TOOL_DETAIL_MAX));
            vec![ActivityEvent::tool("view_image", detail)]
        }
        "update_plan" => vec![ActivityEvent::tool("update_plan", None)],
        other => {
            // MCP tools land as `mcp__<server>__<tool>` — publish the bare
            // tool name, exactly what a claude session shows.
            let name = other
                .strip_prefix("mcp__")
                .and_then(|rest| rest.split_once("__"))
                .map(|(_, tool)| tool)
                .unwrap_or(other);
            vec![ActivityEvent::tool(truncate(name, TOOL_NAME_MAX), None)]
        }
    }
}

/// `exec_command` arguments → the derived headline: the command's first
/// token (`cmd` as string, or the historical `command` argv array).
fn exec_command_headline(arguments: &Value) -> Option<String> {
    if let Some(cmd) = arguments.get("cmd").and_then(Value::as_str) {
        return cmd.split_whitespace().next().map(str::to_string);
    }
    arguments
        .get("command")
        .and_then(Value::as_array)
        .and_then(|argv| argv.first())
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `request_user_input` → one id-less [`ActivityEvent::Question`] per
/// question. Options get digit keys (a legacy keystroke may or may not land
/// in codex's picker — harmless; the primary remote answer path is a chat
/// reply). An option-less question can't ride the wire (the relay schema
/// requires ≥1 option) — it degrades to a narration.
fn request_user_input_events(
    call_id: Option<&str>,
    arguments: &Value,
    state: &mut CodexState,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let questions = arguments
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut events = Vec::new();
    let mut published = 0usize;
    for question in &questions {
        let Some(text) = question.get("question").and_then(Value::as_str) else {
            continue;
        };
        let text = truncate(&redactor.redact(text), QUESTION_TEXT_MAX);
        let options: Vec<QuestionOption> = question
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .take(QUESTION_OPTIONS_MAX)
                    .enumerate()
                    .filter_map(|(i, option)| {
                        let label = option.get("label").and_then(Value::as_str)?;
                        let mut opt = QuestionOption::new(
                            truncate(&redactor.redact(label), OPTION_LABEL_MAX),
                            (i + 1).to_string(),
                        );
                        opt.description = option
                            .get("description")
                            .and_then(Value::as_str)
                            .map(|d| truncate(&redactor.redact(d), OPTION_DESCRIPTION_MAX));
                        Some(opt)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if options.is_empty() {
            // Free-form question: still surface it, as prose.
            events.push(ActivityEvent::narration(text));
            continue;
        }
        let header = question
            .get("header")
            .and_then(Value::as_str)
            .map(|h| truncate(&redactor.redact(h), QUESTION_HEADER_MAX));
        events.push(ActivityEvent::Question {
            text,
            options,
            multi_select: None,
            plan_mode: None,
            id: None,
            ask_id: None,
            index: None,
            total: None,
            header,
            at: None,
        });
        published += 1;
    }
    if published > 0 {
        if let Some(call_id) = call_id {
            state.pending_asks.insert(
                call_id.to_string(),
                PendingAsk {
                    questions: published,
                },
            );
        }
    }
    events
}

/// A `function_call_output` resolves a pending ask: publish one semantic
/// `question_resolved` (EXP-249) carrying the extracted answers, or the
/// dismissal when none could be read. It stays id-less AND askId-less like
/// the cards this ask published, so viewers retire every pending card and
/// land the answers positionally. Every OTHER tool output is never read or
/// published.
fn parse_function_call_output(
    payload: &Value,
    state: &mut CodexState,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(ask) = state.pending_asks.remove(call_id) else {
        return Vec::new();
    };
    let answers = extract_answers(payload.get("output"), ask.questions);
    let dismissed = answers.is_empty();
    let mut collected: Vec<String> = answers
        .into_iter()
        .map(|answer| truncate(&redactor.redact(&answer), ANSWER_MAX))
        .collect();
    collected.truncate(ANSWERS_MAX);
    vec![ActivityEvent::QuestionResolved {
        id: None,
        ask_id: None,
        answers: (!dismissed).then_some(collected),
        dismissed: dismissed.then_some(true),
        at: None,
    }]
}

/// Best-effort answer extraction from a `request_user_input` output. The
/// output is a JSON string (sometimes nested one level under `output`);
/// answers live in an `answers` object/array whose entries carry string
/// values (or `answer`/`label`/`answers` fields). A short unstructured
/// output counts as one answer; anything else means dismissal.
fn extract_answers(output: Option<&Value>, questions: usize) -> Vec<String> {
    let Some(output) = output else {
        return Vec::new();
    };
    // Unwrap string → JSON (possibly twice: `output` may itself hold an
    // `{"output": "<json>"}` envelope).
    let mut value = output.clone();
    for _ in 0..2 {
        if let Some(raw) = value.as_str() {
            match serde_json::from_str::<Value>(raw) {
                Ok(parsed) => value = parsed,
                Err(_) => break,
            }
        } else if let Some(inner) = value.get("output") {
            value = inner.clone();
        } else {
            break;
        }
    }
    let mut answers = Vec::new();
    collect_answer_strings(value.get("answers").unwrap_or(&value), &mut answers);
    if answers.is_empty() {
        if let Some(raw) = value.as_str().or_else(|| output.as_str()) {
            let raw = raw.trim();
            if !raw.is_empty() && raw.len() <= 200 {
                answers.push(raw.to_string());
            }
        }
    }
    answers.truncate(questions.max(1));
    answers
}

fn collect_answer_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, entry) in map {
                match entry {
                    Value::String(text) if !text.trim().is_empty() => {
                        // Skip obvious non-answer metadata fields.
                        if key != "id" && key != "question_id" && key != "header" {
                            out.push(text.trim().to_string());
                        }
                    }
                    Value::Array(_) | Value::Object(_) => collect_answer_strings(entry, out),
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(text) if !text.trim().is_empty() => {
                        out.push(text.trim().to_string());
                    }
                    _ => collect_answer_strings(item, out),
                }
            }
        }
        _ => {}
    }
}

/// File-edit and scripted tools ride `custom_tool_call`:
///
/// * `apply_patch` — publish only the touched file paths parsed from the
///   patch header lines, never the patch body.
/// * `exec` (the newest models' scripting tool: `input` is JavaScript
///   driving `tools.<name>(…)` calls) — an embedded apply_patch payload
///   still yields per-path `apply_patch` rows; otherwise the headline is the
///   first `tools.<name>(` invoked. The script body is never published.
fn parse_custom_tool_call(payload: &Value, redactor: &Redactor) -> Vec<ActivityEvent> {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return Vec::new();
    };
    let input = payload.get("input").and_then(Value::as_str).unwrap_or("");
    match name {
        "apply_patch" => {
            let events = patch_path_events(input, redactor);
            if events.is_empty() {
                return vec![ActivityEvent::tool("apply_patch", None)];
            }
            events
        }
        "exec" => {
            let events = patch_path_events(input, redactor);
            if !events.is_empty() {
                return events;
            }
            let detail = first_scripted_tool(input)
                .map(|tool| truncate(&redactor.redact(&tool), TOOL_DETAIL_MAX));
            vec![ActivityEvent::tool("exec", detail)]
        }
        other => vec![ActivityEvent::tool(truncate(other, TOOL_NAME_MAX), None)],
    }
}

/// The `*** Update/Add/Delete File:` paths of an apply_patch payload — the
/// only lines of a patch safe to publish. A patch embedded in a scripted
/// `exec` rides inside a JS string literal, where its newlines are the
/// two-character `\n` escape — normalize those to real newlines first.
fn patch_path_events(input: &str, redactor: &Redactor) -> Vec<ActivityEvent> {
    const PATCH_FILES_MAX: usize = 8;
    let normalized = input.replace("\\n", "\n");
    normalized
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("*** Update File: ")
                .or_else(|| line.strip_prefix("*** Add File: "))
                .or_else(|| line.strip_prefix("*** Delete File: "))
        })
        .take(PATCH_FILES_MAX)
        .map(|path| {
            ActivityEvent::tool(
                "apply_patch",
                Some(truncate(&redactor.redact(path.trim()), TOOL_DETAIL_MAX)),
            )
        })
        .collect()
}

/// The first `tools.<name>(` call of a scripted `exec` input — a safe,
/// derived headline (a bare identifier, never script content).
fn first_scripted_tool(input: &str) -> Option<String> {
    let start = input.find("tools.")? + "tools.".len();
    let rest = &input[start..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

// ---------------------------------------------------------------------------
// The emitter loop
// ---------------------------------------------------------------------------

pub(crate) fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    run_emitter_with_root(config, sender, active, default_codex_sessions_root());
}

fn run_emitter_with_root(
    config: EmitterConfig,
    sender: ActivitySender,
    active: Arc<AtomicBool>,
    sessions_root: Option<PathBuf>,
) {
    let mut exact_secrets = secrets_from_worktree(&config.worktree);
    exact_secrets.extend(config.extra_secrets.iter().cloned());
    let redactor = Redactor::new(exact_secrets);

    sender.send(ActivityEvent::narration(crate::activity::launch_narration(
        config.bypass_permissions,
        config.plan_mode,
    )));

    let spawn_time = SystemTime::now();
    let mut current: Option<PathBuf> = None;
    let mut offset: u64 = 0;
    let mut rescan_at: Option<Instant> = None;
    let mut rollout_deadline = Some(Instant::now() + ROLLOUT_WAIT);
    let mut state = CodexState::default();
    let mut diffs = DiffSnapshots::new();
    let mut needs_input = NeedsInputForwarder::new();
    // EXP-455: exec/patch approval modals render only on the grid (never in
    // the rollout) — watch for them and make them remotely answerable.
    let mut approval_watcher = CodexApprovalWatcher::new();
    let mut approvals = CodexApprovals::default();
    let mut parked_answers: Vec<(RemoteAnswer, Instant)> = Vec::new();
    // The publisher's EXP-334 free-text reroute signal, sticky while the
    // viewport is scrolled — like the claude emitter's grid memory.
    let mut grid_picker_visible = false;
    // EXP-443: the launcher-stamped per-spawn originator (and, on a native
    // resume, the exact rollout id) pin discovery to OUR codex among the
    // cwd's rollouts — an in-TUI `/new` rotates the file but keeps the
    // originator, so rotation still follows. No match falls back to the
    // legacy newest-cwd behavior (a codex build ignoring the override must
    // never be worse off than before), logged once.
    let want = RolloutWant {
        originator: config.codex_originator.clone(),
        session_id: config.codex_resume_id.clone(),
    };
    let mut fallback_logged = false;

    while active.load(Ordering::SeqCst) {
        // 1) Resolve / re-resolve the rollout. The full-tree sweep walks every
        //    recorded session, so once bound it re-runs on a slower cadence
        //    (a newer matching rollout in the same cwd supersedes — a manual
        //    `codex` restarted in the worktree, or our own /new rotation).
        if let Some(root) = &sessions_root {
            let due = current.is_none() || rescan_at.is_none_or(|at| at.elapsed() >= REDISCOVER_INTERVAL);
            if due {
                rescan_at = Some(Instant::now());
                if let Some(newest) = find_live_rollout(root, &config.worktree, spawn_time, &want) {
                    if !fallback_logged && want.originator.is_some() {
                        let matched = read_session_meta(&newest)
                            .and_then(|meta| meta.originator)
                            == want.originator;
                        if !matched {
                            fallback_logged = true;
                            log::info!(
                                "activity: no rollout carries originator {:?} — this codex \
                                 build may not honor the override; falling back to cwd-only \
                                 discovery",
                                want.originator
                            );
                        }
                    }
                    if current.as_deref() != Some(newest.as_path()) {
                        current = Some(newest);
                        offset = 0;
                        state = CodexState::default();
                    }
                    rollout_deadline = None;
                } else if let Some(deadline) = rollout_deadline {
                    if Instant::now() >= deadline {
                        log::info!(
                            "activity: no codex rollout for {} within {}s — diffs only",
                            config.worktree.display(),
                            ROLLOUT_WAIT.as_secs()
                        );
                        // EXP-389: say so on the feed too — a codex parked on
                        // an interactive startup screen (login, an onboarding
                        // prompt a future version adds) records no rollout,
                        // and without this a remote viewer just stares at an
                        // empty feed.
                        sender.send(ActivityEvent::narration(
                            "Codex hasn't produced any activity yet. It may be waiting on a prompt in the desktop terminal.",
                        ));
                        rollout_deadline = None;
                    }
                }
            }
        }

        // 2) Tail any new complete lines (`tail_transcript` resets the offset
        //    if the file shrank; regrowth after a resume just keeps reading).
        if let Some(path) = current.clone() {
            offset = tail_transcript(
                &path,
                offset,
                &mut |line| parse_rollout_line(line, &mut state, &redactor),
                &mut |event| sender.send(event),
            );
        }

        // 3) EXP-455: approval-modal watch on the live grid. A settled
        //    overlay becomes an answerable question; its disappearance
        //    retires the card. While a `request_user_input` ask is pending
        //    its picker owns the screen (the rollout already published those
        //    questions as cards) — the watcher's Show is swallowed.
        if let Some(term) = &config.term {
            let lines = screen_lines(term);
            let grid_offset = display_offset(term);
            match approval_watcher.tick(&lines, grid_offset) {
                Some(codex_approval_picker::Transition::Show(snapshot)) => {
                    if state.pending_asks.is_empty() {
                        publish_approval(&mut approvals, snapshot, &sender, &redactor);
                    }
                }
                Some(codex_approval_picker::Transition::Resolved) => {
                    resolve_approval(&mut approvals, &sender);
                }
                None => {}
            }
            if grid_offset == 0 {
                // Free text typed at the modal would be eaten and its
                // trailing Enter would confirm the highlighted row — the
                // publisher's Esc-reroute (EXP-334) lands on the modal's
                // esc-to-cancel instead, and the message arrives as the
                // "what to do differently" feedback.
                grid_picker_visible = codex_approval_picker::detect(&lines).is_some();
            }
            if let Some(steering) = &config.steering {
                steering.link.set_grid_picker_pending(grid_picker_visible);
            }
        }

        // 4) The synced needs-input flag: idle between turns, parked on a
        //    `request_user_input`, or blocked on an approval modal.
        needs_input.tick(
            state.attention() || approval_watcher.is_pending(),
            &config.on_needs_input,
        );
        // EXP-637: codex's own `task_started`/`task_complete` edges ARE the
        // turn boundary the graceful stop waits on.
        if let Some(signal) = &config.turn_signal {
            signal.set_idle(state.idle);
        }

        // 5) Debounced worktree diff snapshot (only when changed).
        diffs.tick(&config.worktree, &sender, &redactor);

        // 6) Wait out the poll interval — interrupted by a remote answer,
        //    with the same parked-retry contract as the claude emitter
        //    (EXP-334): a tap can beat the modal's paint or land mid-render.
        match &config.steering {
            Some(steering) => {
                let deadline = Instant::now() + POLL_INTERVAL;
                loop {
                    match &config.term {
                        Some(term) => parked_answers.retain_mut(|(answer, since)| {
                            match handle_approval_answer(
                                &mut approvals,
                                answer,
                                term,
                                &steering.write_input,
                                &sender,
                            ) {
                                AnswerAttempt::Settled => false,
                                AnswerAttempt::Retry => since.elapsed() < ANSWER_RETRY_TTL,
                            }
                        }),
                        None => {
                            if !parked_answers.is_empty() {
                                parked_answers.clear();
                                log::debug!("activity: answer dropped — no terminal grid");
                            }
                        }
                    }
                    let Some(wait) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    match steering.answers.recv_timeout(wait) {
                        Ok(answer) => {
                            parked_answers
                                .retain(|(parked, _)| parked.question_id != answer.question_id);
                            parked_answers.push((answer, Instant::now()));
                        }
                        Err(_) => break,
                    }
                }
            }
            None => std::thread::sleep(POLL_INTERVAL),
        }
    }

    needs_input.clear_on_teardown(&config.on_needs_input);
    if let Some(steering) = &config.steering {
        steering.link.set_grid_picker_pending(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(Vec::new())
    }

    fn parse(state: &mut CodexState, line: &str) -> Vec<ActivityEvent> {
        parse_rollout_line(line, state, &redactor())
    }

    fn narration_text(event: &ActivityEvent) -> &str {
        match event {
            ActivityEvent::Narration { text, .. } => text,
            other => panic!("expected narration, got {other:?}"),
        }
    }

    #[test]
    fn user_message_event_msg_is_published_and_response_item_user_is_not() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"user_message","message":"fix the bug","images":[]}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            ActivityEvent::UserMessage { text, .. } if text == "fix the bug"
        ));
        // The response_item user stream carries injected context — never
        // published in legacy mode.
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>secret cwd</environment_context>"}]}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn agent_message_and_reasoning_become_narration_with_dedupe() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"agent_message","message":"Done.","phase":"final_answer"}}"#,
        );
        assert_eq!(narration_text(&events[0]), "Done.");
        let first = parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"agent_reasoning","text":"**Inspecting the repo**\nmore detail"}}"#,
        );
        assert_eq!(narration_text(&first[0]), "Inspecting the repo");
        // Identical consecutive headline is suppressed.
        let dupe = parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"agent_reasoning","text":"**Inspecting the repo**"}}"#,
        );
        assert!(dupe.is_empty());
    }

    #[test]
    fn assistant_response_item_is_skipped_in_legacy_mode() {
        // It mirrors agent_message — publishing both would double the prose.
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hi"}]}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn exec_command_publishes_the_derived_headline_never_the_command() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"git log --oneline -n 25 secret-token\",\"workdir\":\"/w\"}","call_id":"c1"}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "exec_command");
                assert_eq!(detail.as_deref(), Some("git"));
            }
            other => panic!("expected tool, got {other:?}"),
        }
        // argv-array variant.
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"command\":[\"cargo\",\"test\"]}","call_id":"c2"}}"#,
        );
        match &events[0] {
            ActivityEvent::Tool { detail, .. } => assert_eq!(detail.as_deref(), Some("cargo")),
            other => panic!("expected tool, got {other:?}"),
        }
    }

    #[test]
    fn mcp_tool_names_are_trimmed_and_write_stdin_is_skipped() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call","name":"mcp__exponential__exponential_pr_open","arguments":"{}","call_id":"c1"}}"#,
        );
        match &events[0] {
            ActivityEvent::Tool { name, .. } => assert_eq!(name, "exponential_pr_open"),
            other => panic!("expected tool, got {other:?}"),
        }
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call","name":"write_stdin","arguments":"{\"chunk\":\"secret\"}","call_id":"c2"}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn apply_patch_publishes_paths_only_never_the_body() {
        let mut state = CodexState::default();
        let input = "*** Begin Patch\\n*** Update File: src/a.rs\\n@@\\n-old secret line\\n+new\\n*** Add File: src/b.rs\\n+content\\n*** End Patch";
        let line = format!(
            r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"custom_tool_call","name":"apply_patch","input":"{input}","call_id":"c1","status":"completed"}}}}"#
        );
        let events = parse(&mut state, &line);
        assert_eq!(events.len(), 2);
        let details: Vec<_> = events
            .iter()
            .map(|event| match event {
                ActivityEvent::Tool { name, detail, .. } => {
                    assert_eq!(name, "apply_patch");
                    detail.clone().unwrap()
                }
                other => panic!("expected tool, got {other:?}"),
            })
            .collect();
        assert_eq!(details, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn scripted_exec_derives_patch_paths_or_tool_names_never_the_script() {
        // The newest codex models route file edits through a custom_tool_call
        // named `exec` whose input is JavaScript driving tools.apply_patch —
        // observed live on codex-cli 0.144.5 with gpt-5.6-terra (EXP-383
        // harness run). The embedded patch yields per-path apply_patch rows.
        let mut state = CodexState::default();
        let input = "const r = await tools.apply_patch(\\\"*** Begin Patch\\\\n*** Add File: hello.txt\\\\n+secret content\\\\n*** End Patch\\\");\\ntext(r)";
        let line = format!(
            r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"custom_tool_call","name":"exec","input":"{input}","call_id":"c1","status":"completed"}}}}"#
        );
        let events = parse(&mut state, &line);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "apply_patch");
                assert_eq!(detail.as_deref(), Some("hello.txt"));
            }
            other => panic!("expected tool, got {other:?}"),
        }

        // No embedded patch: the first tools.<name>( call is the headline.
        let line = r#"{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"const files = await tools.list_dir({ path: \"src\" });\ntext(files)","call_id":"c2","status":"completed"}}"#;
        let events = parse(&mut state, line);
        match &events[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "exec");
                assert_eq!(detail.as_deref(), Some("list_dir"));
            }
            other => panic!("expected tool, got {other:?}"),
        }

        // Script with no tools call at all: bare exec row, script never leaks.
        let line = r#"{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"text('secret literal')","call_id":"c3"}}"#;
        let events = parse(&mut state, line);
        match &events[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "exec");
                assert_eq!(detail, &None);
            }
            other => panic!("expected tool, got {other:?}"),
        }
    }

    #[test]
    fn request_user_input_publishes_idless_question_cards_and_flags_attention() {
        let mut state = CodexState::default();
        let args = r#"{\"questions\":[{\"header\":\"Risk\",\"id\":\"q1\",\"question\":\"Ship it?\",\"options\":[{\"label\":\"Yes\",\"description\":\"do it\"},{\"label\":\"No\"}]}]}"#;
        let line = format!(
            r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"function_call","name":"request_user_input","arguments":"{args}","call_id":"ask1"}}}}"#
        );
        let events = parse(&mut state, &line);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ActivityEvent::Question {
                text,
                options,
                id,
                header,
                ..
            } => {
                assert_eq!(text, "Ship it?");
                assert_eq!(id, &None, "codex questions ride the legacy id-less path");
                assert_eq!(header.as_deref(), Some("Risk"));
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].label, "Yes");
                assert_eq!(options[0].key, "1");
                assert_eq!(options[0].description.as_deref(), Some("do it"));
                assert_eq!(options[1].key, "2");
            }
            other => panic!("expected question, got {other:?}"),
        }
        assert!(state.attention(), "pending ask flags needs-input");

        // The matching output resolves the id-less cards with their answers.
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call_output","call_id":"ask1","output":"{\"answers\":{\"q1\":\"Yes\"}}"}}"#,
        );
        assert_eq!(
            events,
            vec![ActivityEvent::QuestionResolved {
                id: None,
                ask_id: None,
                answers: Some(vec!["Yes".into()]),
                dismissed: None,
                at: None,
            }]
        );
        assert!(!state.attention());
    }

    #[test]
    fn unparseable_ask_output_resolves_as_dismissed() {
        let mut state = CodexState::default();
        let args = r#"{\"questions\":[{\"question\":\"Pick\",\"options\":[{\"label\":\"A\"}]}]}"#;
        let line = format!(
            r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"function_call","name":"request_user_input","arguments":"{args}","call_id":"ask2"}}}}"#
        );
        parse(&mut state, &line);
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call_output","call_id":"ask2","output":""}}"#,
        );
        assert_eq!(
            events,
            vec![ActivityEvent::QuestionResolved {
                id: None,
                ask_id: None,
                answers: None,
                dismissed: Some(true),
                at: None,
            }]
        );
        assert!(!state.attention());
    }

    #[test]
    fn option_less_question_degrades_to_narration() {
        // The relay schema requires ≥1 option — a free-form question rides
        // as prose instead of an unsendable card.
        let mut state = CodexState::default();
        let args = r#"{\"questions\":[{\"question\":\"Describe the approach\",\"options\":[]}]}"#;
        let line = format!(
            r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"function_call","name":"request_user_input","arguments":"{args}","call_id":"ask3"}}}}"#
        );
        let events = parse(&mut state, &line);
        assert_eq!(events.len(), 1);
        assert_eq!(narration_text(&events[0]), "Describe the approach");
        assert!(!state.attention(), "nothing answerable was published");
    }

    #[test]
    fn other_tool_outputs_are_never_published() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"function_call_output","call_id":"c9","output":"secret command output"}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn turn_edges_drive_the_attention_flag() {
        let mut state = CodexState::default();
        assert!(!state.attention());
        parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","last_agent_message":"Done."}}"#,
        );
        assert!(state.attention(), "idle after task_complete");
        parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_started","turn_id":"t2"}}"#,
        );
        assert!(!state.attention());
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#,
        );
        assert_eq!(narration_text(&events[0]), "Turn aborted.");
        assert!(state.attention());
    }

    #[test]
    fn unknown_types_and_malformed_lines_are_ignored() {
        let mut state = CodexState::default();
        assert!(parse(&mut state, "not json at all").is_empty());
        assert!(parse(
            &mut state,
            r#"{"timestamp":"t","type":"world_state","payload":{"state":"opaque"}}"#
        )
        .is_empty());
        assert!(parse(
            &mut state,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"brand_new_kind","x":1}}"#
        )
        .is_empty());
        assert!(parse(
            &mut state,
            r#"{"timestamp":"t","type":"turn_context","payload":{"model":"gpt-5.6"}}"#
        )
        .is_empty());
    }

    #[test]
    fn paginated_history_mode_notes_once_and_uses_response_items() {
        let mut state = CodexState::default();
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"session_meta","payload":{"id":"s1","cwd":"/w","history_mode":"paginated"}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(narration_text(&events[0]).contains("paginated"));
        // The assistant response_item becomes the narration source.
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Working on it."}]}}"#,
        );
        assert_eq!(narration_text(&events[0]), "Working on it.");
        // The meta re-read after a tail reset does not re-announce.
        let events = parse(
            &mut state,
            r#"{"timestamp":"t","type":"session_meta","payload":{"id":"s1","cwd":"/w","history_mode":"paginated"}}"#,
        );
        assert!(events.is_empty());
    }

    // -- approvals (EXP-455) -----------------------------------------------

    use crate::publisher::PublisherCmd;
    use std::sync::{Arc as StdArc, Mutex};

    fn drained(rx: &flume::Receiver<PublisherCmd>) -> Vec<ActivityEvent> {
        rx.drain()
            .map(|cmd| match cmd {
                PublisherCmd::Activity(event) => event,
                other => panic!("the emitter only ever sends activity: {other:?}"),
            })
            .collect()
    }

    fn paint(term: &TermHandle, rows: &[&str]) {
        let mut bytes = b"\x1b[2J\x1b[H".to_vec();
        for row in rows {
            bytes.extend_from_slice(row.as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        terminal::advance_bytes(term, &bytes);
    }

    /// The exec approval overlay (codex-cli 0.144.5 render snapshot).
    const EXEC_APPROVAL_ROWS: &[&str] = &[
        "  Would you like to run the following command?",
        "",
        "  Reason: need filesystem access",
        "",
        "  $ cat /tmp/readme.txt",
        "",
        "› 1. Yes, proceed (y)",
        "  2. No, and tell Codex what to do differently (esc)",
        "",
        "  Press enter to confirm or esc to cancel",
    ];

    fn exec_snapshot() -> ApprovalSnapshot {
        let rows: Vec<String> = EXEC_APPROVAL_ROWS.iter().map(|r| r.to_string()).collect();
        codex_approval_picker::detect(&rows).expect("overlay detected")
    }

    #[test]
    fn an_approval_overlay_becomes_an_answerable_question_and_resolves() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = redactor();
        let mut approvals = CodexApprovals::default();
        publish_approval(&mut approvals, exec_snapshot(), &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id,
                header,
                text,
                options,
                ..
            }] => {
                assert_eq!(id.as_deref(), Some("approval:1"));
                assert_eq!(header.as_deref(), Some("Approval required"));
                assert!(text.starts_with("Would you like to run the following command?"));
                assert!(text.contains("$ cat /tmp/readme.txt"));
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].key, "1");
            }
            other => panic!("expected an approval question, got {other:?}"),
        }

        resolve_approval(&mut approvals, &sender);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, .. }] => {
                assert_eq!(id.as_deref(), Some("approval:1"));
            }
            other => panic!("expected a resolution, got {other:?}"),
        }
        // Idempotent once retired.
        resolve_approval(&mut approvals, &sender);
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn a_changed_overlay_retires_the_previous_card_first() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = redactor();
        let mut approvals = CodexApprovals::default();
        publish_approval(&mut approvals, exec_snapshot(), &sender, &redactor);
        drained(&rx);
        publish_approval(&mut approvals, exec_snapshot(), &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, .. }, ActivityEvent::Question { id: next, .. }] =>
            {
                assert_eq!(id.as_deref(), Some("approval:1"));
                assert_eq!(next.as_deref(), Some("approval:2"));
            }
            other => panic!("expected retire-then-publish, got {other:?}"),
        }
    }

    #[test]
    fn a_remote_approval_answer_is_injected_and_acked() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, EXEC_APPROVAL_ROWS);

        let (sender, rx) = ActivitySender::test_pair();
        let mut approvals = CodexApprovals::default();
        publish_approval(&mut approvals, exec_snapshot(), &sender, &redactor());
        drained(&rx);

        // The digit actuates the row (list_selection_view semantics); the
        // overlay leaves the grid, the answer acks — no trailing Enter.
        let keys = StdArc::new(Mutex::new(Vec::<String>::new()));
        let recorded = keys.clone();
        let repaint_term = term.clone();
        let write_input: InputHook = StdArc::new(move |bytes: &[u8]| {
            let key = String::from_utf8_lossy(bytes).to_string();
            let actuated = key == "1";
            recorded.lock().unwrap().push(key);
            if actuated {
                paint(&repaint_term, &["  Working…"]);
            }
        });
        let answer = RemoteAnswer {
            question_id: "approval:1".to_string(),
            ask_id: None,
            keys: vec!["1".to_string()],
            text: None,
        };
        let outcome = handle_approval_answer(&mut approvals, &answer, &term, &write_input, &sender);
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "approval:1"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }

        // A duplicate re-acks without injecting again (EXP-374).
        let outcome = handle_approval_answer(&mut approvals, &answer, &term, &write_input, &sender);
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(keys.lock().unwrap().len(), 1, "no second injection");
        assert!(matches!(
            drained(&rx)[..],
            [ActivityEvent::AnswerAck { .. }]
        ));
    }

    #[test]
    fn an_approval_answer_without_the_overlay_on_screen_is_retried() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &["  Working…"]);

        let (sender, rx) = ActivitySender::test_pair();
        let mut approvals = CodexApprovals::default();
        publish_approval(&mut approvals, exec_snapshot(), &sender, &redactor());
        drained(&rx);

        let keys = StdArc::new(Mutex::new(Vec::<String>::new()));
        let recorded = keys.clone();
        let write_input: InputHook = StdArc::new(move |bytes: &[u8]| {
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(bytes).to_string());
        });
        let outcome = handle_approval_answer(
            &mut approvals,
            &RemoteAnswer {
                question_id: "approval:1".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Retry);
        assert!(keys.lock().unwrap().is_empty(), "nothing may be injected");
        assert!(drained(&rx).is_empty(), "and nothing is acked");

        // A stale id can never become answerable — settled, not retried.
        let outcome = handle_approval_answer(
            &mut approvals,
            &RemoteAnswer {
                question_id: "approval:99".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert!(keys.lock().unwrap().is_empty());
    }

    // -- discovery ---------------------------------------------------------

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-codex-activity-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_rollout(root: &Path, day: &str, stamp: &str, id: &str, cwd: &Path) -> PathBuf {
        write_rollout_with(root, day, stamp, id, cwd, "codex-tui")
    }

    fn write_rollout_with(
        root: &Path,
        day: &str,
        stamp: &str,
        id: &str,
        cwd: &Path,
        originator: &str,
    ) -> PathBuf {
        let dir = root.join(day);
        std::fs::create_dir_all(&dir).unwrap();
        let meta = serde_json::json!({
            "timestamp": stamp,
            "type": "session_meta",
            "payload": { "id": id, "cwd": cwd.to_string_lossy(), "originator": originator },
        });
        let path = dir.join(format!("rollout-{stamp}-{id}.jsonl"));
        std::fs::write(&path, format!("{meta}\n")).unwrap();
        path
    }

    #[test]
    fn discovery_matches_cwd_and_prefers_the_newest() {
        let dir = temp_dir("match");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let elsewhere = dir.join("other");
        std::fs::create_dir_all(&elsewhere).unwrap();
        write_rollout(&root, "2026/07/30", "2026-07-30T10-00-00", "old", &worktree);
        write_rollout(&root, "2026/07/31", "2026-07-31T09-00-00", "wrong", &elsewhere);
        let newest = write_rollout(&root, "2026/07/31", "2026-07-31T08-00-00", "new", &worktree);
        let spawn = SystemTime::now();
        assert_eq!(
            find_live_rollout(&root, &worktree, spawn, &RolloutWant::default()),
            Some(newest),
            "newest matching cwd wins (all files fresh under the mtime slack)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discovery_skips_stale_files_and_compressed_siblings() {
        let dir = temp_dir("stale");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let path = write_rollout(&root, "2026/07/20", "2026-07-20T10-00-00", "old", &worktree);
        // Back-date the file well past the slack window.
        let old = SystemTime::now() - std::time::Duration::from_secs(3600);
        let file = std::fs::File::options().append(true).open(&path).unwrap();
        file.set_modified(old).unwrap();
        drop(file);
        // A compressed sibling is never the live rollout.
        std::fs::write(
            root.join("2026/07/20/rollout-2026-07-20T09-00-00-z.jsonl.zst"),
            b"zstd",
        )
        .unwrap();
        assert_eq!(find_live_rollout(&root, &worktree, SystemTime::now(), &RolloutWant::default()), None);
        // A resume APPENDS to the old file, bumping its mtime — it becomes
        // discoverable again.
        std::fs::File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now())
            .unwrap();
        assert_eq!(
            find_live_rollout(&root, &worktree, SystemTime::now(), &RolloutWant::default()),
            Some(path)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-443: among same-cwd rollouts, the expected originator wins even
    /// against a NEWER foreign one — the hijack this pin exists for.
    #[test]
    fn discovery_prefers_the_expected_originator() {
        let dir = temp_dir("orig");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let ours = write_rollout_with(
            &root,
            "2026/08/07",
            "2026-08-07T10-00-00",
            "ours",
            &worktree,
            "exponential-abc12345",
        );
        write_rollout_with(
            &root,
            "2026/08/07",
            "2026-08-07T11-00-00",
            "foreign",
            &worktree,
            "codex-tui",
        );
        let want = RolloutWant {
            originator: Some("exponential-abc12345".to_string()),
            session_id: None,
        };
        assert_eq!(
            find_live_rollout(&root, &worktree, SystemTime::now(), &want),
            Some(ours)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-443: a codex that ignores the originator override must degrade to
    /// exactly the legacy newest-cwd match, never to a dead feed.
    #[test]
    fn discovery_falls_back_to_cwd_when_no_originator_matches() {
        let dir = temp_dir("orig-fallback");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        write_rollout(&root, "2026/08/07", "2026-08-07T10-00-00", "older", &worktree);
        let newest = write_rollout(&root, "2026/08/07", "2026-08-07T11-00-00", "newer", &worktree);
        let want = RolloutWant {
            originator: Some("exponential-never".to_string()),
            session_id: None,
        };
        assert_eq!(
            find_live_rollout(&root, &worktree, SystemTime::now(), &want),
            Some(newest)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-443: a native resume's exact session id beats newer files AND the
    /// originator pass.
    #[test]
    fn discovery_pins_the_resumed_session_id() {
        let dir = temp_dir("resume-pin");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let resumed = write_rollout(&root, "2026/08/06", "2026-08-06T10-00-00", "resumed", &worktree);
        write_rollout_with(
            &root,
            "2026/08/07",
            "2026-08-07T11-00-00",
            "fresh",
            &worktree,
            "exponential-abc12345",
        );
        let want = RolloutWant {
            originator: Some("exponential-abc12345".to_string()),
            session_id: Some("resumed".to_string()),
        };
        assert_eq!(
            find_live_rollout(&root, &worktree, SystemTime::now(), &want),
            Some(resumed)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discovery_matches_the_canonicalized_worktree() {
        let dir = temp_dir("canon");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let canonical = std::fs::canonicalize(&worktree).unwrap();
        let path = write_rollout(&root, "2026/07/31", "2026-07-31T10-00-00", "c", &canonical);
        assert_eq!(
            find_live_rollout(&root, &worktree, SystemTime::now(), &RolloutWant::default()),
            Some(path)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
