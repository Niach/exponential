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

/// Parse one Claude Code transcript JSONL line into activity events.
/// `assistant` entries: `text` blocks become narration, `tool_use` blocks
/// become tool headlines — except `AskUserQuestion`/`ExitPlanMode`, which
/// become interactive `question` events (EXP-78). `user` entries become
/// `user_message` events ONLY when they are genuine human turns
/// (`origin.kind == "human"` — the initial prompt and steered messages);
/// tool RESULTS and injected system content are never published. Every string
/// is redacted and truncated to the relay caps.
pub fn parse_transcript_line(line: &str, redactor: &Redactor) -> Vec<ActivityEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match entry.get("type").and_then(Value::as_str) {
        Some("assistant") => parse_assistant_entry(&entry, redactor),
        Some("user") => parse_user_entry(&entry, redactor).into_iter().collect(),
        // system/summary/etc. → never published.
        _ => Vec::new(),
    }
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
/// transcripts under. `None` when no home dir is resolvable.
pub fn transcript_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Claude Code munges a cwd into its transcript dir name by replacing every
/// non-alphanumeric character with `-` (verified against live dirs, e.g.
/// `/home/x/Projects/2026/foo.com` → `-home-x-Projects-2026-foo-com`).
pub fn munge_project_dir(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The pending plan's markdown body, read from `~/.claude/plans` (EXP-150).
/// `claude` writes the plan file BEFORE showing the approval picker, so this
/// is the only full-body source available at pending time (the transcript's
/// `input.plan` twin lands only after approval). Candidates are `*.md` files
/// modified at/after the session spawn; when the picker rendered the plan's
/// first line on screen, a candidate containing it wins (disambiguates
/// concurrent sessions — the dir is global, not per-project), else newest
/// mtime. Best-effort: `None` falls back to a fixed headline.
fn plan_body(after: SystemTime, must_contain: Option<&str>) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let dir = PathBuf::from(home).join(".claude").join("plans");
    let mut candidates: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(&dir)
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
    candidates
        .first()
        .and_then(|(_, path)| std::fs::read_to_string(path).ok())
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
    let transcript_dir = transcript_root().map(|root| root.join(munge_project_dir(&config.worktree)));

    let mut current: Option<PathBuf> = None;
    let mut offset: u64 = 0;
    let mut last_diff = String::new();
    let mut last_diff_at: Option<Instant> = None;
    let mut transcript_deadline = Some(Instant::now() + TRANSCRIPT_WAIT);
    let mut picker_watcher = PlanPickerWatcher::new();
    // Grid-emitted plan questions whose transcript twins are still owed —
    // claude flushes the `ExitPlanMode` transcript entry only AFTER the
    // picker is answered, so each grid emission pre-pays one transcript
    // plan question that must then be swallowed instead of re-shown as
    // freshly pending (EXP-150).
    let mut suppress_plan_questions: usize = 0;

    while active.load(Ordering::SeqCst) {
        // 0) Plan-picker watch on the live grid (EXP-150): the transcript
        //    cannot show a PENDING plan approval (its entry lands only once
        //    the picker is answered), but the picker is on screen exactly
        //    while it is pending. Runs before the transcript tail so a
        //    same-tick flush can never race the suppression counter.
        if let Some(term) = &config.term {
            match picker_watcher.tick(&screen_lines(term), display_offset(term)) {
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
                    suppress_plan_questions += 1;
                }
                Some(Transition::Resolved) => {
                    // Retires the trailing plan card on every client the
                    // moment the picker is answered — no protocol change.
                    sender.send(ActivityEvent::Narration {
                        text: "Plan approval answered.".to_string(),
                    });
                }
                None => {}
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
            offset = tail_transcript(
                &path,
                offset,
                &redactor,
                &sender,
                &mut suppress_plan_questions,
            );
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
}

/// Read complete newline-terminated lines from `path` starting at byte
/// `offset`, publish their events, and return the new offset (a trailing
/// partial line is left for the next poll). Each pending debit in
/// `suppress_plan_questions` swallows one transcript-derived plan question —
/// the late twin of a plan the grid watcher already published at pending
/// time (EXP-150).
fn tail_transcript(
    path: &Path,
    offset: u64,
    redactor: &Redactor,
    sender: &ActivitySender,
    suppress_plan_questions: &mut usize,
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
        for event in parse_transcript_line(&line, redactor) {
            if *suppress_plan_questions > 0
                && matches!(
                    &event,
                    ActivityEvent::Question {
                        plan_mode: Some(true),
                        ..
                    }
                )
            {
                *suppress_plan_questions -= 1;
                continue;
            }
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
        let mut suppress = 1usize;
        tail_transcript(&path, 0, &redactor, &sender, &mut suppress);
        assert_eq!(suppress, 0);
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

    #[test]
    fn munge_matches_claude_code_scheme() {
        assert_eq!(
            munge_project_dir(Path::new("/home/x/Projects/2026/foo.com")),
            "-home-x-Projects-2026-foo-com"
        );
        assert_eq!(
            munge_project_dir(Path::new("/a/b/worktrees/exp/EXP-1")),
            "-a-b-worktrees-exp-EXP-1"
        );
    }
}
