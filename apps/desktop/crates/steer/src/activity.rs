//! The live-coding activity emitter (masterplan §P7 + EXP-78).
//!
//! The app publishes a **stripped, redacted** activity stream over the
//! EXISTING steer publisher socket — never the raw PTY. Event kinds (relay
//! `activityEventSchema`):
//!
//! * **narration** — assistant prose (`text` content blocks in the Claude Code
//!   session transcript);
//! * **tool** — a tool-call headline: the tool name plus a single primary
//!   argument (a file path / pattern, or a Bash `description` — NEVER the raw
//!   command string, NEVER a tool result);
//! * **diff** — a debounced `git diff` snapshot of the worktree;
//! * **user_message** — a HUMAN turn (the initial prompt or a steered
//!   message; `origin.kind == "human"` entries only) — MEMBER-ONLY: the relay
//!   never fans it to anonymous public viewers (EXP-78);
//! * **question** — an interactive prompt the session is blocked on (an
//!   `AskUserQuestion` question or the `ExitPlanMode` plan-approval picker),
//!   with the raw TUI keystroke per option so steering clients can answer —
//!   MEMBER-ONLY like `user_message`.
//!
//! Everything published passes through [`Redactor`] first: exact-match masking
//! of the launcher-created secrets (the JIT GitHub installation token embedded
//! in the worktree remote, the `expu_` personal key in `.exp-mcp.json`) plus
//! gitleaks-style patterns. Tool results are never read; injected system
//! content (`isMeta`, task notifications, `<system-reminder>` blocks) is never
//! published.
//!
//! The emitter runs on a dedicated OS thread (poll-based, blocking file/git
//! I/O) — it never touches gpui or the steer tokio runtime. It publishes via a
//! [`crate::publisher::ActivitySender`] (a cheap clone of the publisher's
//! unbounded control channel); sends after the session ends are harmless
//! no-ops. Best-effort throughout: if the transcript can't be found within
//! [`TRANSCRIPT_WAIT`], it logs and continues with diffs only, never blocking
//! the session.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use regex::Regex;
use serde_json::Value;
use terminal::{display_offset, screen_lines, TermHandle};

use crate::frames::{ActivityEvent, QuestionOption};
use crate::plan_picker::{PlanPickerWatcher, Transition};
use crate::publisher::ActivitySender;
use crate::question_picker::{self, normalize_question_text, QuestionPickerWatcher};

/// The mask token substituted for every redacted secret.
const REDACTED: &str = "[redacted]";

/// Relay-enforced caps (`activityEventSchema`), truncated client-side so a
/// too-large frame is never silently dropped by the relay's zod parse. These
/// are UTF-8 BYTE budgets: the relay caps each string in UTF-16 code units
/// (zod `.max()`) and the whole frame in bytes (`maxPayloadLength`), and for
/// any string UTF-8 bytes >= UTF-16 code units, so staying under the byte
/// budget satisfies both regardless of script.
pub const NARRATION_MAX: usize = 16 * 1024;
pub const TOOL_NAME_MAX: usize = 128;
pub const TOOL_DETAIL_MAX: usize = 1024;
pub const DIFF_MAX: usize = 512 * 1024;
/// Question text shares the narration budget (an ExitPlanMode plan rides it).
pub const QUESTION_TEXT_MAX: usize = NARRATION_MAX;
pub const OPTION_LABEL_MAX: usize = 256;
/// Relay-enforced option-count cap; also the range of digit keys we can map.
const QUESTION_OPTIONS_MAX: usize = 9;

/// Minimum gap between worktree diff snapshots (only emitted when changed).
const DIFF_INTERVAL: Duration = Duration::from_secs(3);
/// Transcript tail poll cadence.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How long to wait for the session transcript to appear before giving up and
/// running diffs-only.
const TRANSCRIPT_WAIT: Duration = Duration::from_secs(20);
/// Exact secrets shorter than this are ignored (never mask a common
/// substring); real tokens/keys are far longer.
const MIN_SECRET_LEN: usize = 8;

/// The worktree MCP config file (mirrors `coding::MCP_JSON_FILE`; `steer` must
/// not depend on `coding`, so the name is duplicated here).
const MCP_JSON_FILE: &str = ".exp-mcp.json";

/// The plan-picker resolution narration (EXP-150/EXP-174). Viewer clients
/// match this EXACT text to retire a pending plan-approval card — the
/// transcript tail lags the grid-emitted plan question, so "any later event"
/// is not a resolution signal for plan cards. Never reword without updating
/// the web / iOS / Android agent-session views.
pub const PLAN_RESOLVED_NARRATION: &str = "Plan approval answered.";

/// Answered-question narration prefix (EXP-197). When the transcript flushes
/// an answered `AskUserQuestion` (claude withholds the entry until the picker
/// resolves), the emitter publishes one `Question answered: <answer>`
/// narration per question — clients match this EXACT prefix to fold the
/// answer into the pending question card instead of rendering a narration
/// row. Never reword without updating the web / iOS / Android views.
pub const QUESTION_ANSWERED_PREFIX: &str = "Question answered: ";

/// Dismissed-question narration (EXP-197) — published when an
/// `AskUserQuestion` resolves WITHOUT answers (Esc / rejected), so viewers
/// retire the pending card instead of leaving it answerable-looking. Clients
/// match the EXACT text; same reword rule as above.
pub const QUESTION_DISMISSED_NARRATION: &str = "Question dismissed.";

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// The gitleaks-style secret patterns applied to every published string, on
/// top of the exact-match masking of the session's own launcher secrets.
const SECRET_PATTERNS: &[&str] = &[
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
    r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b",
    r"\bgithub_pat_[A-Za-z0-9_]{20,}\b",
    r"\bsk-[A-Za-z0-9_-]{20,}\b",
    r"\bAKIA[0-9A-Z]{16}\b",
    r"(?i)bearer\s+[a-z0-9._-]{16,}",
    r"\bexpu_[A-Za-z0-9]{16,}\b",
    r"\bexpw_[A-Za-z0-9]{16,}\b",
];

/// Masks secrets out of any published text. Exact-match masking of the
/// session's own launcher secrets runs first (longest-first so overlapping
/// secrets collapse cleanly), then the [`SECRET_PATTERNS`].
pub struct Redactor {
    exact: Vec<String>,
    patterns: Vec<Regex>,
}

impl Redactor {
    /// Build a redactor from the session's exact secrets (installation token,
    /// `expu_` key, …). Empty/too-short entries are dropped; the patterns are
    /// compiled once (they are static and known-valid).
    pub fn new(exact_secrets: Vec<String>) -> Self {
        let mut exact: Vec<String> = exact_secrets
            .into_iter()
            .filter(|s| s.len() >= MIN_SECRET_LEN)
            .collect();
        exact.sort_by(|a, b| b.len().cmp(&a.len()));
        exact.dedup();
        let patterns = SECRET_PATTERNS
            .iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(re) => Some(re),
                Err(err) => {
                    log::warn!("activity: bad secret pattern {p:?}: {err}");
                    None
                }
            })
            .collect();
        Self { exact, patterns }
    }

    /// Mask every known secret out of `input`.
    pub fn redact(&self, input: &str) -> String {
        let mut out = input.to_string();
        for secret in &self.exact {
            out = out.replace(secret.as_str(), REDACTED);
        }
        for re in &self.patterns {
            out = re.replace_all(&out, REDACTED).into_owned();
        }
        out
    }
}

/// Gather the session's exact secrets from the worktree (best-effort): the JIT
/// installation token embedded in the git remote URL, and the `expu_` personal
/// key written into `.exp-mcp.json`. Both are launcher-created and long-lived only
/// for the session; masking them is belt-and-braces on top of the patterns.
pub fn secrets_from_worktree(worktree: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(token) = git_remote_token(worktree) {
        out.push(token);
    }
    if let Some(key) = mcp_expu_key(worktree) {
        out.push(key);
    }
    out
}

/// Extract the installation token from `git remote get-url origin`
/// (`https://x-access-token:<token>@github.com/<full>.git`).
fn git_remote_token(worktree: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout);
    let after = url.split_once("x-access-token:")?.1;
    let token = after.split_once('@')?.0.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// Extract the `expu_` key from the worktree `.exp-mcp.json`
/// (`mcpServers.exponential.headers.Authorization = "Bearer <key>"`).
fn mcp_expu_key(worktree: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(worktree.join(MCP_JSON_FILE)).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let auth = value
        .get("mcpServers")?
        .get("exponential")?
        .get("headers")?
        .get("Authorization")?
        .as_str()?;
    let key = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
    (!key.is_empty()).then(|| key.to_string())
}

// ---------------------------------------------------------------------------
// Transcript parsing
// ---------------------------------------------------------------------------

/// Cross-line transcript state (EXP-197): which grid-published questions are
/// owed a transcript twin, and which `AskUserQuestion` tool_uses are still
/// awaiting their tool_result (the answers live on the RESULT entry).
#[derive(Default)]
pub struct TranscriptState {
    /// Grid-emitted plan questions whose transcript twins are still owed —
    /// claude flushes the `ExitPlanMode` transcript entry only AFTER the
    /// picker is answered, so each grid emission pre-pays one transcript
    /// plan question that must then be swallowed instead of re-shown as
    /// freshly pending (EXP-150).
    pub suppress_plan_questions: usize,
    /// Normalized texts of grid-published `AskUserQuestion` questions — their
    /// transcript twins (flushed post-answer) are swallowed by text identity
    /// (counting is unreliable: tab revisits and the review screen make grid
    /// emissions ≠ twin count).
    pub recent_grid_questions: Vec<String>,
    /// `AskUserQuestion` tool_use id → its question texts, in order — awaiting
    /// the tool_result entry that carries `toolUseResult.answers`.
    pub pending_asks: Vec<(String, Vec<String>)>,
}

/// Grid-question memory cap — a session never has this many live pickers.
const RECENT_GRID_QUESTIONS_CAP: usize = 16;
/// Un-resulted AskUserQuestion tool_use cap.
const PENDING_ASKS_CAP: usize = 8;

impl TranscriptState {
    /// Remember a grid-published question so its transcript twin is swallowed.
    pub fn remember_grid_question(&mut self, text: &str) {
        self.recent_grid_questions.push(normalize_question_text(text));
        if self.recent_grid_questions.len() > RECENT_GRID_QUESTIONS_CAP {
            let excess = self.recent_grid_questions.len() - RECENT_GRID_QUESTIONS_CAP;
            self.recent_grid_questions.drain(..excess);
        }
    }

    /// Whether `text` matches a remembered grid question — consumes the match.
    /// Substring containment (either way, with a length floor) covers screen
    /// wrapping and a question whose head scrolled off the grid.
    fn consume_grid_question(&mut self, text: &str) -> bool {
        let norm = normalize_question_text(text);
        let matched = self.recent_grid_questions.iter().position(|g| {
            const MIN: usize = 12;
            g == &norm
                || (g.len() >= MIN && norm.contains(g.as_str()))
                || (norm.len() >= MIN && g.contains(norm.as_str()))
        });
        match matched {
            Some(pos) => {
                self.recent_grid_questions.remove(pos);
                true
            }
            None => false,
        }
    }
}

/// Parse one Claude Code transcript JSONL line into activity events.
/// `assistant` entries: `text` blocks become narration, `tool_use` blocks
/// become tool headlines — except `AskUserQuestion`/`ExitPlanMode`, which
/// become interactive `question` events (EXP-78). `user` entries become
/// `user_message` events ONLY when they are genuine human turns
/// (`origin.kind == "human"` — the initial prompt and steered messages);
/// tool RESULTS and injected system content are never published — with ONE
/// targeted exception: an `AskUserQuestion` tool_result's collected answers
/// (human-chosen input, EXP-197) become `Question answered:` narrations.
/// Every string is redacted and truncated to the relay caps.
pub fn process_transcript_line(
    line: &str,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Vec<ActivityEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match entry.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            record_pending_asks(&entry, state);
            parse_assistant_entry(&entry, redactor)
                .into_iter()
                .filter(|event| match event {
                    // The late twin of a plan the grid watcher already
                    // published at pending time (EXP-150).
                    ActivityEvent::Question {
                        plan_mode: Some(true),
                        ..
                    } if state.suppress_plan_questions > 0 => {
                        state.suppress_plan_questions -= 1;
                        false
                    }
                    // The late twin of a grid-published AskUserQuestion —
                    // matched by text, since it flushes only post-answer.
                    ActivityEvent::Question {
                        text,
                        plan_mode: None,
                        ..
                    } => !state.consume_grid_question(text),
                    _ => true,
                })
                .collect()
        }
        Some("user") => {
            let mut events = take_ask_answers(&entry, redactor, state);
            events.extend(parse_user_entry(&entry, redactor));
            events
        }
        // system/summary/etc. → never published.
        _ => Vec::new(),
    }
}

/// Stateless wrapper over [`process_transcript_line`] (kept for callers/tests
/// that don't track cross-line ask state).
pub fn parse_transcript_line(line: &str, redactor: &Redactor) -> Vec<ActivityEvent> {
    process_transcript_line(line, redactor, &mut TranscriptState::default())
}

/// Record every `AskUserQuestion` tool_use (id + question texts, in order) so
/// the answers on its later tool_result entry can be published.
fn record_pending_asks(entry: &Value, state: &mut TranscriptState) {
    let Some(content) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use")
            || block.get("name").and_then(Value::as_str) != Some("AskUserQuestion")
        {
            continue;
        }
        let Some(id) = block.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(questions) = block
            .get("input")
            .and_then(|i| i.get("questions"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let texts: Vec<String> = questions
            .iter()
            .filter_map(|q| q.get("question").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        if texts.is_empty() {
            continue;
        }
        state.pending_asks.push((id.to_string(), texts));
        if state.pending_asks.len() > PENDING_ASKS_CAP {
            let excess = state.pending_asks.len() - PENDING_ASKS_CAP;
            state.pending_asks.drain(..excess);
        }
    }
}

/// An `AskUserQuestion` tool_result → its collected answers, published as one
/// `Question answered: <answer>` narration per question (in question order,
/// from the entry's `toolUseResult.answers` map), or the single dismissal
/// narration when it resolved without answers (Esc / rejected — the
/// `toolUseResult` is a plain string then). ONLY results whose tool_use id
/// was recorded as an AskUserQuestion are ever read — generic tool results
/// stay unpublished (the EXP-78 privacy stance); the answers themselves are
/// human-chosen input.
fn take_ask_answers(
    entry: &Value,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Vec<ActivityEvent> {
    let Some(content) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(tid) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(pos) = state.pending_asks.iter().position(|(id, _)| id == tid) else {
            continue;
        };
        let (_, questions) = state.pending_asks.remove(pos);
        let answers = entry
            .get("toolUseResult")
            .and_then(|v| v.get("answers"))
            .and_then(Value::as_object);
        let mut emitted = false;
        if let Some(map) = answers {
            for question in &questions {
                if let Some(answer) = map.get(question).and_then(Value::as_str) {
                    if answer.trim().is_empty() {
                        continue;
                    }
                    events.push(ActivityEvent::Narration {
                        text: truncate(
                            &format!("{QUESTION_ANSWERED_PREFIX}{}", redactor.redact(answer)),
                            NARRATION_MAX,
                        ),
                    });
                    emitted = true;
                }
            }
        }
        if !emitted {
            events.push(ActivityEvent::Narration {
                text: QUESTION_DISMISSED_NARRATION.to_string(),
            });
        }
    }
    events
}

fn parse_assistant_entry(entry: &Value, redactor: &Redactor) -> Vec<ActivityEvent> {
    let Some(content) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut events = Vec::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    let redacted = truncate(&redactor.redact(text), NARRATION_MAX);
                    if !redacted.trim().is_empty() {
                        events.push(ActivityEvent::Narration { text: redacted });
                    }
                }
            }
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                // Interactive prompts become answerable question events; a
                // malformed input falls through to the generic tool headline.
                if name == "AskUserQuestion" {
                    if let Some(questions) = parse_ask_user_question(block.get("input"), redactor)
                    {
                        events.extend(questions);
                        continue;
                    }
                } else if name == "ExitPlanMode" {
                    events.push(parse_exit_plan_mode(block.get("input"), redactor));
                    continue;
                }
                let detail = tool_detail(name, block.get("input"))
                    .map(|d| truncate(&redactor.redact(&d), TOOL_DETAIL_MAX));
                events.push(ActivityEvent::Tool {
                    name: truncate(name, TOOL_NAME_MAX),
                    detail,
                });
            }
            // tool_result / thinking / anything else → never published.
            _ => {}
        }
    }
    events
}

/// A genuine human turn → one `user_message` event (EXP-78). Requires
/// `origin.kind == "human"` (verified transcript marker for typed/steered
/// messages and the argv-seeded initial prompt); everything injected —
/// task notifications, `isMeta` skill bodies, compaction summaries,
/// `<system-reminder>` blocks — fails the gate or the block filter. Fails
/// CLOSED: if a future claude version drops `origin`, user messages silently
/// stop appearing rather than risking a leak of injected content.
fn parse_user_entry(entry: &Value, redactor: &Redactor) -> Option<ActivityEvent> {
    let origin_kind = entry
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str);
    if origin_kind != Some("human") {
        return None;
    }
    if entry.get("isMeta").and_then(Value::as_bool) == Some(true)
        || entry.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let content = entry.get("message").and_then(|m| m.get("content"))?;
    let text = match content {
        // The argv-seeded initial prompt lands as a plain string.
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let parts: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .filter(|t| !t.trim_start().starts_with("<system-reminder>"))
                .collect();
            parts.join("\n\n")
        }
        _ => return None,
    };
    let redacted = truncate(&redactor.redact(&text), NARRATION_MAX);
    if redacted.trim().is_empty() {
        return None;
    }
    Some(ActivityEvent::UserMessage { text: redacted })
}

/// `AskUserQuestion` input → one `question` event per entry of
/// `input.questions[]`, options mapped positionally to the TUI's digit keys
/// (`1`..`9`). `None` when the input doesn't match the expected shape (the
/// caller falls back to a generic tool headline).
fn parse_ask_user_question(
    input: Option<&Value>,
    redactor: &Redactor,
) -> Option<Vec<ActivityEvent>> {
    let questions = input?.get("questions")?.as_array()?;
    let mut events = Vec::new();
    for question in questions {
        let text = question.get("question").and_then(Value::as_str)?;
        let options: Vec<QuestionOption> = question
            .get("options")?
            .as_array()?
            .iter()
            .filter_map(|o| o.get("label").and_then(Value::as_str))
            .take(QUESTION_OPTIONS_MAX)
            .enumerate()
            .map(|(i, label)| QuestionOption {
                label: truncate(&redactor.redact(label), OPTION_LABEL_MAX),
                key: (i + 1).to_string(),
            })
            .collect();
        if options.is_empty() {
            return None;
        }
        let multi_select =
            matches!(question.get("multiSelect"), Some(Value::Bool(true))).then_some(true);
        events.push(ActivityEvent::Question {
            text: truncate(&redactor.redact(text), QUESTION_TEXT_MAX),
            options,
            multi_select,
            plan_mode: None,
        });
    }
    (!events.is_empty()).then_some(events)
}

/// `ExitPlanMode` → a plan-approval `question` (text = the plan markdown when
/// present). This transcript path is the DEGRADED fallback (EXP-150): the
/// pending-time question normally comes from the grid watcher with the REAL
/// picker rows, and this twin is suppressed. When it does fire (grid
/// detection missed a re-worded picker), only the two approve keys are
/// offered — key "3" is no longer safe to send blind (on claude v2.1.211 it
/// launches "refine with Ultraplan on Claude Code on the web", not "keep
/// planning").
fn parse_exit_plan_mode(input: Option<&Value>, redactor: &Redactor) -> ActivityEvent {
    let plan = input
        .and_then(|i| i.get("plan"))
        .and_then(Value::as_str)
        .map(|p| truncate(&redactor.redact(p), QUESTION_TEXT_MAX))
        .filter(|p| !p.trim().is_empty());
    ActivityEvent::Question {
        text: plan.unwrap_or_else(|| "Plan ready for approval.".to_string()),
        options: vec![
            QuestionOption {
                label: "Approve — auto-accept edits".to_string(),
                key: "1".to_string(),
            },
            QuestionOption {
                label: "Approve — manually approve edits".to_string(),
                key: "2".to_string(),
            },
        ],
        multi_select: None,
        // Marks the question as a plan-approval picker so clients can render
        // a dedicated "Plan ready" card (EXP-97).
        plan_mode: Some(true),
    }
}

/// The single primary argument shown for a tool call — a file path or search
/// pattern, or (for Bash) the human `description`. NEVER the raw command
/// string, a URL, or arbitrary input (any of which could carry secrets); when
/// nothing safe is present the headline shows the tool name alone.
fn tool_detail(name: &str, input: Option<&Value>) -> Option<String> {
    let input = input?;
    if name.eq_ignore_ascii_case("bash") {
        // The command string is NEVER published — only the model's own
        // human-readable description of what it's doing.
        return input
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    for key in ["file_path", "path", "pattern", "notebook_path"] {
        if let Some(v) = input.get(key).and_then(Value::as_str) {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Transcript location
// ---------------------------------------------------------------------------

/// `~/.claude/projects` — the root Claude Code writes per-cwd session
/// transcripts under. `None` when no home dir is resolvable. "projects" is
/// CLAUDE CODE's external directory name, not our renamed product entity —
/// it must never be touched by product vocabulary renames (EXP-191: the
/// EXP-180 project→board sweep rewrote it to `boards` and silenced the
/// activity stream).
pub fn transcript_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Claude Code munges a cwd into its transcript dir name by replacing every
/// non-alphanumeric character with `-` (verified against live dirs, e.g.
/// `/home/x/Projects/2026/foo.com` → `-home-x-Projects-2026-foo-com`).
/// "project" here is Claude Code's vocabulary (see [`transcript_root`]).
pub fn munge_claude_project_dir(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The pending plan's markdown body, read from `~/.claude/plans` (EXP-150).
/// `claude` writes the plan file BEFORE showing the approval picker, so this
/// is the only full-body source available at pending time (the transcript's
/// `input.plan` twin lands only after approval). Best-effort: `None` falls
/// back to a fixed headline.
fn plan_body(after: SystemTime, must_contain: Option<&str>) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let dir = PathBuf::from(home).join(".claude").join("plans");
    plan_body_in(&dir, after, must_contain)
}

/// [`plan_body`] on an explicit dir (unit-testable). Candidates are `*.md`
/// files modified at/after the session spawn; when the picker rendered the
/// plan's first line on screen, a candidate containing it wins (disambiguates
/// concurrent sessions — the dir is global, not per-board). Without a
/// needle (the bare "Exit plan mode?" variant) — or when the needle matches
/// no candidate — a body is attached ONLY when there is exactly one
/// candidate: with two concurrent sessions the newest file may belong to the
/// OTHER session's team, and a missing body is strictly better than a
/// cross-team plan leak.
fn plan_body_in(dir: &Path, after: SystemTime, must_contain: Option<&str>) -> Option<String> {
    let mut candidates: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry.path().extension().and_then(|e| e.to_str()) == Some("md")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
            (modified >= after).then_some((modified, entry.path()))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let needle = must_contain.map(str::trim).filter(|n| !n.is_empty());
    if let Some(needle) = needle {
        for (_, path) in &candidates {
            if let Ok(body) = std::fs::read_to_string(path) {
                if body.contains(needle) {
                    return Some(body);
                }
            }
        }
    }
    match &candidates[..] {
        [(_, only)] => std::fs::read_to_string(only).ok(),
        _ => None,
    }
}

/// The newest non-sidechain session transcript in `dir` modified at/after
/// `after` (the spawn time — so a previous session's stale transcript in a
/// reused worktree is never picked). Sub-agent files (`agent-*.jsonl`) are
/// excluded so tailing never flip-flops between the main session and a
/// sidechain.
fn newest_transcript(dir: &Path, after: SystemTime) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("agent-") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if modified < after {
            continue;
        }
        if best.as_ref().is_none_or(|(t, _)| modified >= *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

// ---------------------------------------------------------------------------
// Worktree diff
// ---------------------------------------------------------------------------

/// A unified diff of the worktree — unstaged plus staged — as one string.
/// Empty when the tree is clean or git fails (best-effort).
fn worktree_diff(worktree: &Path) -> String {
    let mut out = git_diff(worktree, false);
    let cached = git_diff(worktree, true);
    if !cached.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&cached);
    }
    out
}

fn git_diff(worktree: &Path, cached: bool) -> String {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(worktree).arg("diff");
    if cached {
        cmd.arg("--cached");
    }
    match cmd.output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// The emitter thread
// ---------------------------------------------------------------------------

/// What the emitter needs to run: the worktree to tail/diff, plus the live
/// terminal grid for plan-picker detection (EXP-150). `term: None` runs
/// transcript+diff only (tests / headless callers).
pub struct EmitterConfig {
    pub worktree: PathBuf,
    pub term: Option<TermHandle>,
    /// EXP-214: fired (on the emitter thread) whenever the combined
    /// "agent is parked on a picker" flag flips — `true` while a
    /// plan-approval or AskUserQuestion picker is pending on the grid,
    /// `false` once it resolves. The wiring layer forwards it to the synced
    /// `coding_sessions.needs_input` column. Blocking work is fine here (the
    /// emitter thread already shells out for diffs).
    pub on_needs_input: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

/// Start the public activity emitter on a dedicated OS thread. `active` is the
/// shared run flag — flip it to `false` (on session teardown) to stop the
/// emitter promptly. Returns immediately; the thread self-terminates when
/// `active` clears.
pub fn spawn_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("activity-emitter".to_string())
        .spawn(move || run_emitter(config, sender, active))
        .map(|_| ())
        .unwrap_or_else(|err| log::warn!("activity: emitter thread spawn failed: {err}"));
}

fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    let redactor = Redactor::new(secrets_from_worktree(&config.worktree));

    // Announce the session (the viewer shows this immediately, before any
    // transcript line lands).
    sender.send(ActivityEvent::Narration {
        text: "Session started".to_string(),
    });

    let spawn_time = SystemTime::now();
    let transcript_dir =
        transcript_root().map(|root| root.join(munge_claude_project_dir(&config.worktree)));

    let mut current: Option<PathBuf> = None;
    let mut offset: u64 = 0;
    let mut last_diff = String::new();
    let mut last_diff_at: Option<Instant> = None;
    let mut transcript_deadline = Some(Instant::now() + TRANSCRIPT_WAIT);
    let mut picker_watcher = PlanPickerWatcher::new();
    let mut question_watcher = QuestionPickerWatcher::new();
    let mut transcript_state = TranscriptState::default();
    // EXP-214: last "needs input" flag forwarded — fire only on flips.
    let mut needs_input = false;

    while active.load(Ordering::SeqCst) {
        // 0) Picker watch on the live grid: the transcript cannot show a
        //    PENDING plan approval or AskUserQuestion (claude flushes their
        //    entries only once the picker is answered — EXP-150/EXP-197), but
        //    the picker is on screen exactly while it is pending. Runs before
        //    the transcript tail so a same-tick flush can never race the twin
        //    suppression state.
        if let Some(term) = &config.term {
            let lines = screen_lines(term);
            let grid_offset = display_offset(term);
            match picker_watcher.tick(&lines, grid_offset) {
                Some(Transition::Show(snapshot)) => {
                    let text = plan_body(spawn_time, snapshot.plan_box_first_line.as_deref())
                        .map(|raw| truncate(&redactor.redact(&raw), QUESTION_TEXT_MAX))
                        .filter(|t| !t.trim().is_empty())
                        .unwrap_or_else(|| "Plan ready for approval.".to_string());
                    let options = snapshot
                        .options
                        .into_iter()
                        .take(QUESTION_OPTIONS_MAX)
                        .map(|o| QuestionOption {
                            label: truncate(&redactor.redact(&o.label), OPTION_LABEL_MAX),
                            key: o.key,
                        })
                        .collect();
                    sender.send(ActivityEvent::Question {
                        text,
                        options,
                        multi_select: None,
                        plan_mode: Some(true),
                    });
                    transcript_state.suppress_plan_questions += 1;
                }
                Some(Transition::Resolved) => {
                    // Retires the pending plan card on every client the
                    // moment the picker is answered — no protocol change,
                    // clients match the exact text (EXP-174).
                    sender.send(ActivityEvent::Narration {
                        text: PLAN_RESOLVED_NARRATION.to_string(),
                    });
                }
                None => {}
            }
            // AskUserQuestion pickers (EXP-197) — published the moment they
            // settle on screen, so steering clients can answer while the
            // question is actually pending. `question_picker::detect`
            // excludes plan screens itself; the transcript twin (flushed
            // post-answer) is swallowed by text identity, and the answers
            // arrive via the tool_result → `Question answered:` narrations.
            if let Some(snapshot) =
                question_watcher.tick(question_picker::detect(&lines), grid_offset)
            {
                let text = truncate(&redactor.redact(&snapshot.text), QUESTION_TEXT_MAX);
                transcript_state.remember_grid_question(&text);
                let options = snapshot
                    .options
                    .into_iter()
                    .take(QUESTION_OPTIONS_MAX)
                    .map(|o| QuestionOption {
                        label: truncate(&redactor.redact(&o.label), OPTION_LABEL_MAX),
                        key: o.key,
                    })
                    .collect();
                sender.send(ActivityEvent::Question {
                    text,
                    options,
                    multi_select: snapshot.multi_select.then_some(true),
                    plan_mode: None,
                });
            }

            // EXP-214: the combined attention flag — the agent is parked on
            // EITHER picker and waits for a human. Forwarded only on flips
            // (the watchers already debounce mid-render flicker).
            let pending = picker_watcher.is_pending() || question_watcher.is_pending();
            if pending != needs_input {
                needs_input = pending;
                if let Some(on_needs_input) = &config.on_needs_input {
                    on_needs_input(pending);
                }
            }
        }

        // 1) Resolve / re-resolve the transcript file (a newer session file in
        //    the same dir supersedes; reset the read offset when it changes).
        if let Some(dir) = &transcript_dir {
            if let Some(newest) = newest_transcript(dir, spawn_time) {
                if current.as_deref() != Some(newest.as_path()) {
                    current = Some(newest);
                    offset = 0;
                }
                transcript_deadline = None;
            } else if let Some(deadline) = transcript_deadline {
                if Instant::now() >= deadline {
                    log::info!(
                        "activity: no transcript in {} within {}s — diffs only",
                        dir.display(),
                        TRANSCRIPT_WAIT.as_secs()
                    );
                    transcript_deadline = None;
                }
            }
        }

        // 2) Tail any new complete lines from the current transcript.
        if let Some(path) = current.clone() {
            offset = tail_transcript(&path, offset, &redactor, &sender, &mut transcript_state);
        }

        // 3) Debounced worktree diff snapshot (only when changed).
        let due = last_diff_at.is_none_or(|at| at.elapsed() >= DIFF_INTERVAL);
        if due {
            last_diff_at = Some(Instant::now());
            let diff = worktree_diff(&config.worktree);
            if diff != last_diff {
                last_diff = diff.clone();
                if !diff.is_empty() {
                    sender.send(ActivityEvent::Diff {
                        diff: truncate(&redactor.redact(&diff), DIFF_MAX),
                    });
                }
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }

    // Teardown tidiness: never leave the synced attention flag stuck on a
    // session whose emitter is gone (the terminal-exit `end` supersedes, but
    // a steerer takeover only flips `active`).
    if needs_input {
        if let Some(on_needs_input) = &config.on_needs_input {
            on_needs_input(false);
        }
    }
}

/// Read complete newline-terminated lines from `path` starting at byte
/// `offset`, publish their events, and return the new offset (a trailing
/// partial line is left for the next poll). `state` carries the cross-line
/// twin-suppression + pending-ask bookkeeping (EXP-150/EXP-197).
fn tail_transcript(
    path: &Path,
    offset: u64,
    redactor: &Redactor,
    sender: &ActivitySender,
    state: &mut TranscriptState,
) -> u64 {
    use std::io::{Read, Seek, SeekFrom};

    let Ok(mut file) = std::fs::File::open(path) else {
        return offset;
    };
    // A truncated/rotated file (shorter than our offset) resets to 0.
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if len < offset { 0 } else { offset };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return offset;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return offset;
    }

    let mut consumed = 0usize;
    let mut line_start = 0usize;
    while let Some(pos) = buf[line_start..].iter().position(|&b| b == b'\n') {
        let end = line_start + pos;
        let line = String::from_utf8_lossy(&buf[line_start..end]);
        for event in process_transcript_line(&line, redactor, state) {
            sender.send(event);
        }
        line_start = end + 1;
        consumed = line_start;
    }
    start + consumed as u64
}

/// Truncate to at most `max` UTF-8 BYTES, backing up to a char boundary.
/// The relay enforces string caps in UTF-16 code units and the whole-frame
/// cap in bytes; UTF-8 bytes >= UTF-16 code units >= chars for any string,
/// so the byte cap is the strictest of the three. (A char-count cap let
/// CJK/emoji-heavy diffs through at up to 4x the byte budget, and the relay
/// answered an oversize frame by severing the shared publisher socket.)
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_masks_exact_launcher_secrets() {
        let token = "ghs_FAKEinstallationTOKEN1234567890";
        let key = "expu_FAKEpersonalKEY1234567890";
        let redactor = Redactor::new(vec![token.to_string(), key.to_string()]);
        let out = redactor.redact(&format!(
            "cloning https://x-access-token:{token}@github.com/o/r.git with {key}"
        ));
        assert!(!out.contains(token), "install token leaked: {out}");
        assert!(!out.contains(key), "expu key leaked: {out}");
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redactor_masks_pattern_tokens() {
        let redactor = Redactor::new(vec![]);
        // One planted fake per pattern — none may survive.
        let cases = [
            "ghp_abcdefghijklmnopqrstuvwxyz012345",
            "gho_abcdefghijklmnopqrstuvwxyz012345",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnop",
            "sk-abcdefghijklmnopqrstuvwxyz0123456789",
            "AKIAIOSFODNN7EXAMPLE",
            "Bearer abcdef0123456789ABCDEF",
            "expu_abcdefghijklmnop0123456789",
            "expw_abcdefghijklmnop0123456789",
        ];
        for planted in cases {
            let out = redactor.redact(&format!("value = {planted} end"));
            assert!(
                !out.contains(planted),
                "pattern token survived redaction: {planted} -> {out}"
            );
        }
        // A PEM private key block (multi-line, lazy match) is masked whole.
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAA...\nabc\n-----END RSA PRIVATE KEY-----";
        let out = redactor.redact(&format!("key:\n{pem}\ndone"));
        assert!(!out.contains("MIIEowIBAA"), "PEM body leaked: {out}");
    }

    #[test]
    fn redactor_leaves_ordinary_text_alone() {
        let redactor = Redactor::new(vec![]);
        let text = "Editing src/main.rs to fix the parser (see EXP-42).";
        assert_eq!(redactor.redact(text), text);
    }

    #[test]
    fn parse_narration_and_tool_blocks() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "Let me read the file." },
                { "type": "tool_use", "name": "Edit", "input": { "file_path": "src/main.rs" } },
            ]}
        })
        .to_string();
        let events = parse_transcript_line(&line, &redactor);
        assert_eq!(
            events,
            vec![
                ActivityEvent::Narration { text: "Let me read the file.".into() },
                ActivityEvent::Tool { name: "Edit".into(), detail: Some("src/main.rs".into()) },
            ]
        );
    }

    #[test]
    fn bash_tool_uses_description_never_command() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "Bash",
                  "input": { "command": "curl -H 'Authorization: Bearer secrettokenvalue123' x", "description": "Fetch the data" } },
            ]}
        })
        .to_string();
        let events = parse_transcript_line(&line, &redactor);
        assert_eq!(
            events,
            vec![ActivityEvent::Tool {
                name: "Bash".into(),
                detail: Some("Fetch the data".into())
            }]
        );
        // The command string (with its secret) is nowhere in the output.
        let joined = format!("{events:?}");
        assert!(!joined.contains("secrettoken"), "bash command leaked: {joined}");
        assert!(!joined.contains("curl"), "bash command leaked: {joined}");
    }

    #[test]
    fn user_and_tool_result_entries_are_skipped() {
        // No `origin.kind == "human"` marker ⇒ not a genuine human turn (this
        // is the shape of a tool-result delivery) — nothing is published, and
        // the tool_result content in particular never leaks.
        let redactor = Redactor::new(vec![]);
        let user = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "content": "secret file contents" },
                { "type": "text", "text": "please continue" },
            ]}
        })
        .to_string();
        assert!(parse_transcript_line(&user, &redactor).is_empty());
    }

    #[test]
    fn human_user_string_content_becomes_user_message() {
        // The argv-seeded initial prompt: origin.kind == "human", content is a
        // plain string.
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "promptSource": "typed",
            "message": { "role": "user", "content": "Fix the login bug in EXP-42." }
        })
        .to_string();
        assert_eq!(
            parse_transcript_line(&line, &redactor),
            vec![ActivityEvent::UserMessage {
                text: "Fix the login bug in EXP-42.".into()
            }]
        );
    }

    #[test]
    fn human_user_array_content_joins_text_blocks_and_skips_system_reminders() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "message": { "content": [
                { "type": "text", "text": "<system-reminder>injected context</system-reminder>" },
                { "type": "text", "text": "please add tests" },
                { "type": "tool_result", "content": "secret file contents" },
                { "type": "text", "text": "and update the docs" },
            ]}
        })
        .to_string();
        assert_eq!(
            parse_transcript_line(&line, &redactor),
            vec![ActivityEvent::UserMessage {
                text: "please add tests\n\nand update the docs".into()
            }]
        );
    }

    #[test]
    fn task_notification_and_meta_user_entries_are_skipped() {
        let redactor = Redactor::new(vec![]);
        let task_notification = serde_json::json!({
            "type": "user",
            "origin": { "kind": "task-notification" },
            "promptSource": "system",
            "message": { "content": "<task-notification>agent done</task-notification>" }
        })
        .to_string();
        assert!(parse_transcript_line(&task_notification, &redactor).is_empty());

        let meta = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "isMeta": true,
            "message": { "content": "skill body dump" }
        })
        .to_string();
        assert!(parse_transcript_line(&meta, &redactor).is_empty());

        let compact = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "isCompactSummary": true,
            "message": { "content": "summary of prior context" }
        })
        .to_string();
        assert!(parse_transcript_line(&compact, &redactor).is_empty());
    }

    #[test]
    fn user_message_is_redacted_and_truncated() {
        let redactor = Redactor::new(vec![]);
        let big = format!(
            "use key expu_abcdefghijklmnop0123456789 {}",
            "x".repeat(NARRATION_MAX + 500)
        );
        let line = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "message": { "content": big }
        })
        .to_string();
        match &parse_transcript_line(&line, &redactor)[..] {
            [ActivityEvent::UserMessage { text }] => {
                assert!(!text.contains("expu_abcdef"), "expu key leaked: {text}");
                assert!(text.len() <= NARRATION_MAX);
            }
            other => panic!("expected one user_message, got {other:?}"),
        }
    }

    #[test]
    fn ask_user_question_maps_options_to_digit_keys() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "AskUserQuestion", "input": { "questions": [
                    { "question": "Which auth method?", "header": "Auth", "multiSelect": false,
                      "options": [
                        { "label": "OAuth", "description": "..." },
                        { "label": "JWT", "description": "..." },
                        { "label": "Session", "description": "..." },
                      ] },
                    { "question": "Which features?", "header": "Features", "multiSelect": true,
                      "options": [
                        { "label": "Push", "description": "..." },
                        { "label": "Email", "description": "..." },
                      ] },
                ] } },
            ]}
        })
        .to_string();
        assert_eq!(
            parse_transcript_line(&line, &redactor),
            vec![
                ActivityEvent::Question {
                    text: "Which auth method?".into(),
                    options: vec![
                        QuestionOption { label: "OAuth".into(), key: "1".into() },
                        QuestionOption { label: "JWT".into(), key: "2".into() },
                        QuestionOption { label: "Session".into(), key: "3".into() },
                    ],
                    multi_select: None,
                    plan_mode: None,
                },
                ActivityEvent::Question {
                    text: "Which features?".into(),
                    options: vec![
                        QuestionOption { label: "Push".into(), key: "1".into() },
                        QuestionOption { label: "Email".into(), key: "2".into() },
                    ],
                    multi_select: Some(true),
                    plan_mode: None,
                },
            ]
        );
    }

    #[test]
    fn exit_plan_mode_emits_plan_approval_question() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        match &parse_transcript_line(&line, &redactor)[..] {
            [ActivityEvent::Question { text, options, multi_select, plan_mode }] => {
                assert_eq!(text, "## Plan\n1. Do the thing");
                // Degraded-path fallback: approve keys only — "3" is no
                // longer "keep planning" on current claude pickers.
                assert_eq!(
                    options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
                    vec!["1", "2"]
                );
                assert_eq!(*multi_select, None);
                assert_eq!(*plan_mode, Some(true));
            }
            other => panic!("expected one question, got {other:?}"),
        }

        // Plan absent (file-based plans) → fixed headline, same options.
        let bare = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "ExitPlanMode", "input": {} },
            ]}
        })
        .to_string();
        match &parse_transcript_line(&bare, &redactor)[..] {
            [ActivityEvent::Question { text, options, plan_mode, .. }] => {
                assert_eq!(text, "Plan ready for approval.");
                assert_eq!(options.len(), 2);
                assert_eq!(*plan_mode, Some(true));
            }
            other => panic!("expected one question, got {other:?}"),
        }

        // Oversized plan is truncated to the relay cap.
        let big = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "ExitPlanMode",
                  "input": { "plan": "p".repeat(QUESTION_TEXT_MAX + 500) } },
            ]}
        })
        .to_string();
        match &parse_transcript_line(&big, &redactor)[..] {
            [ActivityEvent::Question { text, .. }] => {
                assert_eq!(text.len(), QUESTION_TEXT_MAX);
            }
            other => panic!("expected one question, got {other:?}"),
        }
    }

    #[test]
    fn malformed_ask_user_question_falls_back_to_tool_event() {
        let redactor = Redactor::new(vec![]);
        // No questions array → generic tool headline, never a broken question.
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "AskUserQuestion", "input": {} },
            ]}
        })
        .to_string();
        assert_eq!(
            parse_transcript_line(&line, &redactor),
            vec![ActivityEvent::Tool {
                name: "AskUserQuestion".into(),
                detail: None
            }]
        );

        // A question with an empty options list is malformed too.
        let empty_options = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "AskUserQuestion",
                  "input": { "questions": [ { "question": "Pick one", "options": [] } ] } },
            ]}
        })
        .to_string();
        assert_eq!(
            parse_transcript_line(&empty_options, &redactor),
            vec![ActivityEvent::Tool {
                name: "AskUserQuestion".into(),
                detail: None
            }]
        );
    }

    #[test]
    fn tool_without_safe_arg_has_no_detail() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "WebFetch", "input": { "url": "https://secret.internal/x" } },
            ]}
        })
        .to_string();
        let events = parse_transcript_line(&line, &redactor);
        assert_eq!(events, vec![ActivityEvent::Tool { name: "WebFetch".into(), detail: None }]);
    }

    #[test]
    fn narration_is_truncated_to_the_relay_cap() {
        let redactor = Redactor::new(vec![]);
        let big = "x".repeat(NARRATION_MAX + 500);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [ { "type": "text", "text": big } ] }
        })
        .to_string();
        let events = parse_transcript_line(&line, &redactor);
        match &events[0] {
            ActivityEvent::Narration { text } => assert_eq!(text.len(), NARRATION_MAX),
            other => panic!("expected narration, got {other:?}"),
        }
    }

    #[test]
    fn truncation_caps_utf8_bytes_on_a_char_boundary() {
        // 4 UTF-8 bytes per crab — a char-count cap would overshoot the
        // relay's byte budget fourfold.
        let crabs = "\u{1F980}".repeat(8);
        let out = truncate(&crabs, 10);
        assert_eq!(out, "\u{1F980}\u{1F980}", "backs up to a char boundary");
        assert!(out.len() <= 10);
        assert_eq!(truncate(&crabs, 32), crabs, "under the cap is untouched");
        assert_eq!(truncate("abcdef", 3), "abc");
    }

    #[test]
    fn multibyte_narration_never_exceeds_the_byte_cap() {
        let redactor = Redactor::new(vec![]);
        // 3 UTF-8 bytes (and 1 UTF-16 code unit) per char.
        let big = "\u{898B}".repeat(NARRATION_MAX);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [ { "type": "text", "text": big } ] }
        })
        .to_string();
        let events = parse_transcript_line(&line, &redactor);
        match &events[0] {
            ActivityEvent::Narration { text } => {
                assert!(text.len() <= NARRATION_MAX, "byte cap exceeded: {}", text.len());
                assert!(!text.is_empty());
            }
            other => panic!("expected narration, got {other:?}"),
        }
    }

    #[test]
    fn grid_emitted_plan_suppresses_the_transcript_twin_once() {
        use crate::publisher::PublisherCmd;

        let redactor = Redactor::new(vec![]);
        let (sender, rx) = ActivitySender::test_pair();
        let dir = std::env::temp_dir().join(format!("exp150-suppress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("transcript.jsonl");
        let plan_line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        let narration_line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [ { "type": "text", "text": "On it." } ] }
        })
        .to_string();
        std::fs::write(&path, format!("{plan_line}\n{narration_line}\n{plan_line}\n")).unwrap();

        // One grid-emitted plan question is owed a transcript twin: the FIRST
        // transcript plan question is swallowed, later ones pass through
        // (grid detection missed ⇒ degraded fallback still works).
        let mut state = TranscriptState {
            suppress_plan_questions: 1,
            ..Default::default()
        };
        tail_transcript(&path, 0, &redactor, &sender, &mut state);
        assert_eq!(state.suppress_plan_questions, 0);
        let events: Vec<ActivityEvent> = rx
            .drain()
            .map(|cmd| match cmd {
                PublisherCmd::Activity(event) => event,
                _ => panic!("unexpected publisher command"),
            })
            .collect();
        match &events[..] {
            [ActivityEvent::Narration { text }, ActivityEvent::Question { plan_mode, .. }] => {
                assert_eq!(text, "On it.");
                assert_eq!(*plan_mode, Some(true));
            }
            other => panic!("expected narration + one plan question, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The transcript pair claude flushes once an AskUserQuestion is answered
    /// (captured against v2.1.215): the assistant tool_use entry followed by
    /// the tool_result user entry whose `toolUseResult.answers` maps question
    /// text → the chosen label(s).
    fn answered_ask_lines() -> (String, String) {
        let tool_use = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which toppings do you want?", "multiSelect": true,
                      "options": [ { "label": "Cheese" }, { "label": "Ham" } ] },
                    { "question": "Which size?",
                      "options": [ { "label": "Small" }, { "label": "Large" } ] },
                  ] } },
            ]}
        })
        .to_string();
        let tool_result = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_ask1",
                  "content": "Your questions have been answered: ..." },
            ]},
            "toolUseResult": {
                "questions": [],
                "answers": {
                    "Which toppings do you want?": "Mushrooms, Cheese",
                    "Which size?": "Large"
                }
            }
        })
        .to_string();
        (tool_use, tool_result)
    }

    #[test]
    fn answered_ask_emits_answer_narrations_in_question_order() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let (tool_use, tool_result) = answered_ask_lines();

        let question_events = process_transcript_line(&tool_use, &redactor, &mut state);
        // No grid emission happened (degraded path) — the twins pass through.
        assert_eq!(question_events.len(), 2);
        assert_eq!(state.pending_asks.len(), 1);

        let events = process_transcript_line(&tool_result, &redactor, &mut state);
        assert_eq!(
            events,
            vec![
                ActivityEvent::Narration {
                    text: format!("{QUESTION_ANSWERED_PREFIX}Mushrooms, Cheese")
                },
                ActivityEvent::Narration {
                    text: format!("{QUESTION_ANSWERED_PREFIX}Large")
                },
            ]
        );
        assert!(state.pending_asks.is_empty());
    }

    #[test]
    fn grid_published_question_swallows_its_transcript_twin() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        // The grid watcher published the questions at pending time — the
        // screen re-wraps the text, so the remembered copy differs only in
        // whitespace.
        state.remember_grid_question("Which toppings\ndo you want?");
        state.remember_grid_question("Which size?");

        let (tool_use, tool_result) = answered_ask_lines();
        let events = process_transcript_line(&tool_use, &redactor, &mut state);
        assert_eq!(events, vec![], "post-answer twins must be swallowed");
        assert!(state.recent_grid_questions.is_empty(), "matches are consumed");

        // The answers still flow.
        let events = process_transcript_line(&tool_result, &redactor, &mut state);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn clipped_grid_question_still_matches_its_twin() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        // A long question whose head scrolled off the grid — the remembered
        // text is a suffix of the transcript's full text.
        state.remember_grid_question("toppings do you want?");
        let (tool_use, _) = answered_ask_lines();
        let events = process_transcript_line(&tool_use, &redactor, &mut state);
        // First twin swallowed by containment, second passes through.
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn unrelated_question_is_not_swallowed() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        state.remember_grid_question("A completely different question?");
        let (tool_use, _) = answered_ask_lines();
        let events = process_transcript_line(&tool_use, &redactor, &mut state);
        assert_eq!(events.len(), 2);
        assert_eq!(state.recent_grid_questions.len(), 1);
    }

    #[test]
    fn rejected_ask_emits_the_dismissal_narration() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let (tool_use, _) = answered_ask_lines();
        process_transcript_line(&tool_use, &redactor, &mut state);

        // Esc / reject: the toolUseResult is a plain string, no answers.
        let rejected = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_ask1",
                  "is_error": true,
                  "content": "The user doesn't want to proceed with this tool use." },
            ]},
            "toolUseResult": "User rejected tool use"
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&rejected, &redactor, &mut state),
            vec![ActivityEvent::Narration {
                text: QUESTION_DISMISSED_NARRATION.to_string()
            }]
        );
    }

    #[test]
    fn generic_tool_results_never_publish() {
        // A tool_result whose id was NOT a recorded AskUserQuestion — the
        // EXP-78 privacy stance holds: nothing is read, nothing published.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let generic = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_read1",
                  "content": "secret file contents" },
            ]},
            "toolUseResult": { "answers": { "q": "leak" } }
        })
        .to_string();
        assert_eq!(process_transcript_line(&generic, &redactor, &mut state), vec![]);
    }

    #[test]
    fn ask_answers_are_redacted() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let tool_use = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_ask2", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which key?", "options": [ { "label": "A" } ] },
                  ] } },
            ]}
        })
        .to_string();
        process_transcript_line(&tool_use, &redactor, &mut state);
        let result = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_ask2", "content": "ok" },
            ]},
            "toolUseResult": { "answers": { "Which key?": "use expu_abcdefghijklmnop0123456789" } }
        })
        .to_string();
        match &process_transcript_line(&result, &redactor, &mut state)[..] {
            [ActivityEvent::Narration { text }] => {
                assert!(text.starts_with(QUESTION_ANSWERED_PREFIX));
                assert!(!text.contains("expu_abcdef"), "typed answer leaked a key: {text}");
            }
            other => panic!("expected one answer narration, got {other:?}"),
        }
    }

    /// A fresh temp plans dir with the given `(name, body)` files, plus an
    /// `after` timestamp that predates all of them.
    fn plan_dir(tag: &str, files: &[(&str, &str)]) -> (PathBuf, SystemTime) {
        let dir = std::env::temp_dir().join(format!("exp-plans-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        (dir, SystemTime::now() - Duration::from_secs(60))
    }

    #[test]
    fn needle_less_plan_body_attaches_only_when_unambiguous() {
        // Single candidate ⇒ no ambiguity possible ⇒ body attaches.
        let (dir, after) = plan_dir("single", &[("a.md", "## Plan A")]);
        assert_eq!(
            plan_body_in(&dir, after, None).as_deref(),
            Some("## Plan A")
        );
        std::fs::remove_dir_all(&dir).ok();

        // Two concurrent-session candidates and no on-screen needle ⇒ the
        // newest file may belong to the OTHER session — no body (the
        // question falls back to the fixed headline).
        let (dir, after) = plan_dir("multi", &[("a.md", "## Plan A"), ("b.md", "## Plan B")]);
        assert_eq!(plan_body_in(&dir, after, None), None);
        // A blank needle (trimmed empty) is the same as no needle.
        assert_eq!(plan_body_in(&dir, after, Some("  ")), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_body_needle_disambiguates_concurrent_candidates() {
        let (dir, after) = plan_dir(
            "needle",
            &[("a.md", "## Plan A\nsteps"), ("b.md", "## Plan B\nsteps")],
        );
        assert_eq!(
            plan_body_in(&dir, after, Some("## Plan B")).as_deref(),
            Some("## Plan B\nsteps")
        );
        // A needle matching NO candidate must not fall back to "newest"
        // while multiple candidates exist — same cross-session ambiguity.
        assert_eq!(plan_body_in(&dir, after, Some("## Plan C")), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plan_body_ignores_files_older_than_spawn() {
        let (dir, _) = plan_dir("stale", &[("old.md", "## Stale plan")]);
        // Spawn time after the file's mtime ⇒ no candidates ⇒ no body.
        let after = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(plan_body_in(&dir, after, None), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn munge_matches_claude_code_scheme() {
        assert_eq!(
            munge_claude_project_dir(Path::new("/home/x/Projects/2026/foo.com")),
            "-home-x-Projects-2026-foo-com"
        );
        assert_eq!(
            munge_claude_project_dir(Path::new("/a/b/worktrees/exp/EXP-1")),
            "-a-b-worktrees-exp-EXP-1"
        );
    }

    #[test]
    fn transcript_root_is_claude_code_projects_dir() {
        // `projects` is Claude Code's on-disk name — a product vocabulary
        // rename must never reach it (EXP-191).
        let root = transcript_root().expect("home dir resolvable in tests");
        assert!(
            root.ends_with(Path::new(".claude").join("projects")),
            "transcript root must be ~/.claude/projects, got {}",
            root.display()
        );
    }
}
