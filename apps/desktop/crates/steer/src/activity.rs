//! The live-coding activity emitter (masterplan §P7 + EXP-78, steering v2 =
//! EXP-249).
//!
//! The app publishes a **stripped, redacted** activity stream over the
//! EXISTING steer publisher socket — never the raw PTY. Event kinds (relay
//! `activityEventSchema`):
//!
//! * **narration** — assistant prose (`text` content blocks in the Claude Code
//!   session transcript);
//! * **tool** — a tool-call headline: the tool name plus a single primary
//!   argument (a file path / pattern, or a Bash `description` — NEVER the raw
//!   command string, NEVER a tool result), attributed with a `subagentId` when
//!   it came from a subagent's sidechain transcript;
//! * **diff** — a debounced `git diff` snapshot of the worktree;
//! * **user_message** — a HUMAN turn (the initial prompt or a steered
//!   message; `origin.kind == "human"` entries only) — MEMBER-ONLY: the relay
//!   never fans it to anonymous public viewers (EXP-78);
//! * **question** / **question_resolved** / **answer_ack** — the interactive
//!   prompts the session is blocked on (an `AskUserQuestion` step or the
//!   `ExitPlanMode` plan approval) plus their lifecycle — MEMBER-ONLY like
//!   `user_message`;
//! * **subagent** / **permission** — subagent lifecycle and the informational
//!   "claude is asking for permission" marker.
//!
//! ## Where the truth comes from (EXP-249)
//!
//! Structured facts come from the claude **hooks sidecar** ([`crate::hooks`]):
//! the plan markdown verbatim, every question of an ask with its options and
//! `tool_use_id`, subagent lifecycle, permission/idle notifications. The
//! terminal grid ([`plan_picker`] / [`question_picker`]) is no longer the
//! source of WHAT is being asked — it confirms that the picker is really on
//! screen, supplies the REAL keystroke rows (including synthetic options like
//! "Type something" that only the TUI knows), tells us which tab is current,
//! and is where a remote answer's keystrokes are choreographed and verified.
//! A session without hooks (an old claude, an unwritable settings file) keeps
//! the pre-v2 grid-only behavior, minus question identity.
//!
//! Everything published passes through [`Redactor`] first: exact-match masking
//! of the launcher-created secrets — the JIT GitHub installation token from
//! the clone's shared `.git/exp-git-credentials` credential file (EXP-73;
//! pre-migration clones may still embed it in the origin URL, kept as a
//! fallback source), the `expu_` personal key from `.exp-mcp.json` (claude)
//! or handed in via [`EmitterConfig::extra_secrets`] by the wiring (codex/pi
//! keep it env-only, so no worktree file can recover it — REV2-17) — plus
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

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use regex::Regex;
use serde_json::Value;
use terminal::{display_offset, screen_lines, TermHandle};

use crate::frames::{ActivityEvent, QuestionOption, SubagentStatus};
use crate::hooks::{HookEvent, HookEventKind, HookQuestion};
use crate::plan_picker::{self, PlanPickerWatcher, Transition};
use crate::publisher::{ActivitySender, InputHook};
use crate::question_picker::{
    self, normalize_question_text, QuestionPickerWatcher, QuestionSnapshot,
};

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
pub const OPTION_DESCRIPTION_MAX: usize = 1024;
pub const QUESTION_HEADER_MAX: usize = 256;
/// `question_resolved.answers` — ≤10 entries of ≤1024 (relay schema).
pub const ANSWER_MAX: usize = 1024;
pub const ANSWERS_MAX: usize = 10;
/// `subagent.id` / `answer_ack.id` / `question.id` / `permission.tool`.
pub const ID_MAX: usize = 128;
pub const AGENT_TYPE_MAX: usize = 64;
/// `subagent.agentType` when a hook payload carries none. Clients treat this
/// exact string as the sentinel their label selection skips past (EXP-350) —
/// never rename it without a protocol bump.
const SUBAGENT_TYPE_FALLBACK: &str = "agent";
/// Relay-enforced option-count cap; also the range of digit keys we can map.
pub(crate) const QUESTION_OPTIONS_MAX: usize = 9;

/// Minimum gap between worktree diff snapshots (only emitted when changed).
pub(crate) const DIFF_INTERVAL: Duration = Duration::from_secs(3);
/// Transcript tail poll cadence (also the answer-intake timeout).
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How long an `ExitPlanMode` hook waits for the grid to confirm the approval
/// picker before the plan is published as a plain narration instead. The
/// picker normally paints within a frame; this only fires when detection
/// missed it (a re-worded picker) or claude auto-approved.
const PLAN_GRID_CONFIRM: Duration = Duration::from_secs(10);
/// Bounded wait for the TUI to move on after injected answer keystrokes. No
/// transition inside this window = no `answer_ack` (the steerer's card stays
/// answerable rather than silently locking).
const ANSWER_SETTLE: Duration = Duration::from_secs(2);
/// Poll step while waiting for that transition.
const ANSWER_SETTLE_STEP: Duration = Duration::from_millis(100);
/// How long a plan-approval digit gets to submit ON ITS OWN before Enter is
/// sent after it. Old claude plan pickers submitted on the digit; on current
/// ones (observed v2.1.220) a digit only MOVES the cursor and Enter activates
/// it — so a lone digit left the picker up forever and the answer was never
/// acked (EXP-334).
const PLAN_SUBMIT_PROBE: Duration = Duration::from_millis(500);
/// How long a TRANSIENTLY refused remote answer is retried before it is
/// dropped (EXP-334). A steerer's tap can beat the picker paint (hook
/// questions publish before the TUI renders) or land on a mid-render frame —
/// refusing those outright meant no ack, and the mobile stepper visibly
/// rolled back to the already-answered step.
///
/// EXP-347: the viewers' answer-lock timeouts (web `ANSWER_ACK_TIMEOUT_MS`,
/// Android `ANSWER_ACK_TIMEOUT_MS`, iOS `answerLockSeconds` — all 8s) are
/// derived from this budget: retry TTL (4s) + [`ANSWER_SETTLE`] (2s) +
/// [`PLAN_SUBMIT_PROBE`] (0.5s) + ~1.5s tick/relay margin. Grow them in
/// lockstep or a worst-case ack lands after the card already flashed "Failed".
const ANSWER_RETRY_TTL: Duration = Duration::from_secs(4);
/// Gap between injected keystrokes — the TUI processes one key per render.
const KEYSTROKE_GAP: Duration = Duration::from_millis(60);
/// Newest subagent sidechain transcripts tailed at once (a Task fan-out can
/// open many; only the freshest are worth streaming).
const SIDECHAIN_TAIL_MAX: usize = 4;
/// Directories walked looking for sidechain transcripts (claude nests them
/// under `<session>/subagents/**` since v2.1.220; older builds wrote them flat).
const SIDECHAIN_WALK_MAX: usize = 64;
/// How often that walk re-runs (the tails themselves run every poll).
const SIDECHAIN_SCAN_INTERVAL: Duration = Duration::from_secs(3);
/// How long to wait for the session transcript to appear before giving up and
/// running diffs-only.
const TRANSCRIPT_WAIT: Duration = Duration::from_secs(20);
/// EXP-355: cooldown between re-attempts of a `needs_input` forward whose
/// write failed — fire-and-forget on flips used to stick the synced badge on
/// its last value until the NEXT flip (which may never come this session).
pub(crate) const NEEDS_INPUT_RETRY: Duration = Duration::from_secs(5);
/// Exact secrets shorter than this are ignored (never mask a common
/// substring); real tokens/keys are far longer.
const MIN_SECRET_LEN: usize = 8;

/// The worktree MCP config file (mirrors `coding::MCP_JSON_FILE`; `steer` must
/// not depend on `coding`, so the name is duplicated here).
const MCP_JSON_FILE: &str = ".exp-mcp.json";

/// The credential file in the clone's shared git dir holding the CURRENT
/// installation token (mirrors `coding::git_credentials::credential_file` —
/// same no-`coding`-dependency rule as [`MCP_JSON_FILE`]).
const GIT_CREDENTIALS_FILE: &str = "exp-git-credentials";

/// The plan-picker resolution narration (EXP-150/EXP-174). Viewer clients
/// match this EXACT text to retire a pending plan-approval card — the
/// transcript tail lags the grid-emitted plan question, so "any later event"
/// is not a resolution signal for plan cards. Never reword without updating
/// the web / iOS / Android agent-session views.
pub const PLAN_RESOLVED_NARRATION: &str = "Plan approval answered.";

/// Substring identifying claude's "refine with Ultraplan on Claude Code on
/// the web" plan-picker option (key "3" on v2.1.211+). We strip it from the
/// remotely-offered plan-approval options — it hands the plan off to
/// claude.ai instead of approving/refining locally, which is not a safe
/// thing for a remote steerer to trigger blind.
const ULTRAPLAN_WEB_OPTION: &str = "Claude Code on the web";

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
/// installation token from the clone's shared credential file (EXP-73 —
/// `origin` stays bare, so the pre-EXP-73 remote-URL extraction survives only
/// as a migration fallback), and the `expu_` personal key written into
/// `.exp-mcp.json` (claude sessions only — codex/pi keep the key env-only,
/// which is what [`EmitterConfig::extra_secrets`] exists for). All are
/// launcher-created and long-lived only for the session; masking them is
/// belt-and-braces on top of the patterns.
pub fn secrets_from_worktree(worktree: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(token) = credential_file_token(worktree) {
        out.push(token);
    }
    if let Some(token) = git_remote_token(worktree) {
        out.push(token);
    }
    if let Some(key) = mcp_expu_key(worktree) {
        out.push(key);
    }
    out
}

/// Extract the installation token from the clone's shared credential file
/// (`.git/exp-git-credentials`, git-credential protocol form
/// `username=x-access-token\npassword=<token>\n` — written by
/// `coding::git_credentials`). The shared git dir is resolved through the
/// worktree (`git rev-parse --git-common-dir`), so linked worktrees find the
/// clone's file; the output is relative for a non-linked checkout (`.git`)
/// and absolute for a linked worktree — both are handled.
fn credential_file_token(worktree: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let common = String::from_utf8_lossy(&output.stdout);
    let common = common.trim();
    if common.is_empty() {
        return None;
    }
    let common_dir = if Path::new(common).is_relative() {
        worktree.join(common)
    } else {
        PathBuf::from(common)
    };
    let raw = std::fs::read_to_string(common_dir.join(GIT_CREDENTIALS_FILE)).ok()?;
    let token = raw
        .lines()
        .find_map(|line| line.strip_prefix("password="))?
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// Extract the installation token from `git remote get-url origin`
/// (`https://x-access-token:<token>@github.com/<full>.git`) — the pre-EXP-73
/// scheme; only a not-yet-healed clone still matches.
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
    /// EXP-249: ask ids the HOOK already published (identity, not text) —
    /// their post-answer transcript twins are swallowed outright.
    pub hook_published_asks: HashSet<String>,
    /// EXP-347: a suppressed plan twin flushed since the last emitter look —
    /// claude only flushes the `ExitPlanMode` entry once the picker is
    /// answered, so this is the "plan resolved" signal that still arrives
    /// while the viewport is scrolled (the grid watcher is sticky there).
    pub plan_twin_flushed: bool,
    /// EXP-350: the hooks sidecar is wired, so every `Task` call already
    /// publishes a descriptive `subagent` card — the main transcript's own
    /// bare "Task" tool headline is then dropped instead of doubling each
    /// fan-out row. Suppression is by flag, not per dispatched id: the
    /// transcript entry can flush before the hook's HTTP delivery drains, so
    /// id matching would race the wrong way.
    pub suppress_task_headlines: bool,
    /// EXP-356: texts already published from `queued_command` attachment
    /// entries — a queued message delivered at a turn BOUNDARY can land as a
    /// regular human `user` entry too, which must not double the bubble.
    pub published_queued: Vec<String>,
    /// EXP-360: subagent lifecycle facts read off the MAIN transcript, drained
    /// by the emitter each tick. claude ≥2.1.220 runs `Agent` subagents in the
    /// BACKGROUND: the tool_result is an immediate launch ack
    /// (`toolUseResult.status == "async_launched"`) and the agent's end never
    /// fires the `SubagentStop` hook — it lands only as a `task-notification`
    /// user entry. Without this channel the completion edge never published
    /// and every background subagent tab spun forever.
    pub task_events: Vec<TaskEvent>,
}

/// One background-subagent lifecycle fact from the main transcript (EXP-360).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    /// An `Agent` launch ack — the ack's `toolUseResult` names BOTH ids, the
    /// deterministic dispatch→agent binding (the `SubagentStart` type-matching
    /// dance can swap children of a same-type fan-out).
    Launched {
        agent_id: String,
        tool_use_id: String,
    },
    /// The agent is done: a `task-notification` entry (background agents), or
    /// a subagent tool_result (foreground runs whose `SubagentStop` the
    /// sidecar missed).
    Ended {
        agent_id: Option<String>,
        tool_use_id: Option<String>,
    },
}

/// Undrained [`TranscriptState::task_events`] cap — the emitter drains every
/// tick, so this only bounds the stateless-wrapper path.
const TASK_EVENTS_CAP: usize = 32;

/// Grid-question memory cap — a session never has this many live pickers.
const RECENT_GRID_QUESTIONS_CAP: usize = 16;
/// Un-resulted AskUserQuestion tool_use cap.
const PENDING_ASKS_CAP: usize = 8;

impl TranscriptState {
    /// Remember a grid-published question so its transcript twin is swallowed.
    pub fn remember_grid_question(&mut self, text: &str) {
        self.recent_grid_questions
            .push(normalize_question_text(text));
        if self.recent_grid_questions.len() > RECENT_GRID_QUESTIONS_CAP {
            let excess = self.recent_grid_questions.len() - RECENT_GRID_QUESTIONS_CAP;
            self.recent_grid_questions.drain(..excess);
        }
    }

    /// Whether `text` matches a remembered grid question — consumes the match.
    fn consume_grid_question(&mut self, text: &str) -> bool {
        let norm = normalize_question_text(text);
        let matched = self
            .recent_grid_questions
            .iter()
            .position(|g| normalized_texts_match(g, &norm));
        match matched {
            Some(pos) => {
                self.recent_grid_questions.remove(pos);
                true
            }
            None => false,
        }
    }
}

/// Whether two ALREADY-normalized question texts are the same question.
/// Substring containment (either way, with a length floor) covers screen
/// wrapping and a question whose head scrolled off the grid.
fn normalized_texts_match(a: &str, b: &str) -> bool {
    const MIN: usize = 12;
    a == b || (a.len() >= MIN && b.contains(a)) || (b.len() >= MIN && a.contains(b))
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
            let ask_ids = record_pending_asks(&entry, state);
            // EXP-249: the hook published this ask by identity — its twin is
            // swallowed whole, no text matching involved.
            let hook_published = ask_ids
                .iter()
                .any(|id| state.hook_published_asks.contains(id));
            parse_assistant_entry(&entry, redactor)
                .into_iter()
                .filter(|event| match event {
                    // The late twin of a plan already published at pending
                    // time (EXP-150 grid watcher / the EXP-249 plan hook).
                    ActivityEvent::Question {
                        plan_mode: Some(true),
                        ..
                    } if state.suppress_plan_questions > 0 => {
                        state.suppress_plan_questions -= 1;
                        state.plan_twin_flushed = true;
                        false
                    }
                    // The late twin of an already-published AskUserQuestion —
                    // matched by ask id (hooks) or by text (grid-only), since
                    // it flushes only post-answer.
                    ActivityEvent::Question {
                        text,
                        plan_mode: None,
                        ..
                    } => !hook_published && !state.consume_grid_question(text),
                    // The hook's subagent card already represents this Task
                    // call (EXP-350).
                    ActivityEvent::Tool {
                        name,
                        subagent_id: None,
                        ..
                    } if name == "Task" || name == "Agent" => !state.suppress_task_headlines,
                    _ => true,
                })
                .collect()
        }
        Some("user") => {
            collect_task_events(&entry, state);
            let mut events = take_ask_answers(&entry, redactor, state);
            events.extend(parse_user_entry(&entry, redactor, state));
            events
        }
        // EXP-356: a MID-TURN steered/typed message never becomes a `user`
        // entry — claude queues it and records only a `queued_command`
        // attachment (the injection reaches the model inside a tool result),
        // so this is the one place the feed can learn it.
        Some("attachment") => parse_queued_command(&entry, redactor, state)
            .into_iter()
            .collect(),
        // system/summary/etc. → never published.
        _ => Vec::new(),
    }
}

/// Published-queued-text memory cap (see `TranscriptState::published_queued`).
const PUBLISHED_QUEUED_CAP: usize = 8;

/// A `queued_command` attachment entry → one `user_message` event (EXP-356).
/// Same fail-closed stance as [`parse_user_entry`]: the nested
/// `origin.kind == "human"` marker is REQUIRED, so injected content can never
/// ride this path.
fn parse_queued_command(
    entry: &Value,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Option<ActivityEvent> {
    let attachment = entry.get("attachment")?;
    if attachment.get("type").and_then(Value::as_str) != Some("queued_command") {
        return None;
    }
    if attachment
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
        != Some("human")
    {
        return None;
    }
    let prompt = attachment.get("prompt").and_then(Value::as_str)?;
    let redacted = truncate(&redactor.redact(prompt), NARRATION_MAX);
    if redacted.trim().is_empty() {
        return None;
    }
    state.published_queued.push(redacted.clone());
    if state.published_queued.len() > PUBLISHED_QUEUED_CAP {
        let excess = state.published_queued.len() - PUBLISHED_QUEUED_CAP;
        state.published_queued.drain(..excess);
    }
    Some(ActivityEvent::user_message(redacted))
}

/// EXP-360: read background-subagent lifecycle facts off a `user` entry into
/// [`TranscriptState::task_events`] (never published directly — the emitter
/// resolves them against the subagent cards):
/// * a `task-notification` entry (`origin.kind == "task-notification"`) is
///   the ONLY end signal a background agent leaves — its tagged body names
///   the agent id and the dispatch's tool_use id;
/// * an async launch ack (`toolUseResult.status == "async_launched"`) names
///   both ids at spawn — the deterministic binding;
/// * any other tool_result whose `toolUseResult` carries an `agentId` is a
///   FOREGROUND subagent finishing (belt for a missed `SubagentStop`).
///   Ordinary tool results carry no `agentId` and stay out of the channel.
fn collect_task_events(entry: &Value, state: &mut TranscriptState) {
    let push = |state: &mut TranscriptState, event: TaskEvent| {
        state.task_events.push(event);
        if state.task_events.len() > TASK_EVENTS_CAP {
            let excess = state.task_events.len() - TASK_EVENTS_CAP;
            state.task_events.drain(..excess);
        }
    };
    if entry
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
        == Some("task-notification")
    {
        let text = entry_text(entry);
        let agent_id = tag_value(&text, "task-id");
        let tool_use_id = tag_value(&text, "tool-use-id");
        if agent_id.is_some() || tool_use_id.is_some() {
            push(
                state,
                TaskEvent::Ended {
                    agent_id,
                    tool_use_id,
                },
            );
        }
        return;
    }
    let Some(blocks) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let result = entry.get("toolUseResult");
        let Some(agent_id) = result
            .and_then(|r| r.get("agentId"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let is_launch_ack = result.and_then(|r| r.get("status")).and_then(Value::as_str)
            == Some("async_launched")
            || result
                .and_then(|r| r.get("isAsync"))
                .and_then(Value::as_bool)
                == Some(true);
        let agent_id = truncate(agent_id, ID_MAX);
        let tool_use_id = truncate(tool_use_id, ID_MAX);
        if is_launch_ack {
            push(
                state,
                TaskEvent::Launched {
                    agent_id,
                    tool_use_id,
                },
            );
        } else {
            // Resolve via the block's OWN tool_use_id only — a foreground
            // dispatch's result carries the dispatch id. Other agent-flavored
            // results (a TaskOutput poll of a still-running agent, say) have
            // ids nothing carded, so they can never complete a card early.
            push(
                state,
                TaskEvent::Ended {
                    agent_id: None,
                    tool_use_id: Some(tool_use_id),
                },
            );
        }
    }
}

/// A user entry's textual content — the plain string, or its text blocks
/// joined (task-notification bodies have landed as both).
fn entry_text(entry: &Value) -> String {
    match entry.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The trimmed body of the FIRST `<tag>…</tag>` in `text`, id-truncated.
fn tag_value(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    let value = text[start..end].trim();
    (!value.is_empty()).then(|| truncate(value, ID_MAX))
}

/// Stateless wrapper over [`process_transcript_line`] (kept for callers/tests
/// that don't track cross-line ask state).
pub fn parse_transcript_line(line: &str, redactor: &Redactor) -> Vec<ActivityEvent> {
    process_transcript_line(line, redactor, &mut TranscriptState::default())
}

/// Record every `AskUserQuestion` tool_use (id + question texts, in order) so
/// the answers on its later tool_result entry can be published. Returns the
/// ask ids seen on this entry (the EXP-249 twin-suppression key).
fn record_pending_asks(entry: &Value, state: &mut TranscriptState) -> Vec<String> {
    let mut ids = Vec::new();
    let Some(content) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return ids;
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
        ids.push(id.to_string());
        state.pending_asks.push((id.to_string(), texts));
        if state.pending_asks.len() > PENDING_ASKS_CAP {
            let excess = state.pending_asks.len() - PENDING_ASKS_CAP;
            state.pending_asks.drain(..excess);
        }
    }
    ids
}

/// An `AskUserQuestion` tool_result → its collected answers, published as one
/// `Question answered: <answer>` narration per question (in question order,
/// from the entry's `toolUseResult.answers` map), or the single dismissal
/// narration when it resolved without answers (Esc / rejected — the
/// `toolUseResult` is a plain string then). ONLY results whose tool_use id
/// was recorded as an AskUserQuestion are ever read — generic tool results
/// stay unpublished (the EXP-78 privacy stance); the answers themselves are
/// human-chosen input.
///
/// EXP-249: each resolution ALSO emits a semantic `question_resolved` keyed by
/// the ask's `tool_use_id` (= the `askId` the question events carried), which
/// retires every card of that ask. The narrations stay for pre-v2 clients that
/// only string-match.
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
        let (ask_id, questions) = state.pending_asks.remove(pos);
        state.hook_published_asks.remove(&ask_id);
        let answers = entry
            .get("toolUseResult")
            .and_then(|v| v.get("answers"))
            .and_then(Value::as_object);
        let mut collected: Vec<String> = Vec::new();
        if let Some(map) = answers {
            for question in &questions {
                if let Some(answer) = map.get(question).and_then(Value::as_str) {
                    if answer.trim().is_empty() {
                        continue;
                    }
                    let answer = redactor.redact(answer);
                    events.push(ActivityEvent::narration(truncate(
                        &format!("{QUESTION_ANSWERED_PREFIX}{answer}"),
                        NARRATION_MAX,
                    )));
                    collected.push(truncate(&answer, ANSWER_MAX));
                }
            }
        }
        if collected.is_empty() {
            events.push(ActivityEvent::narration(QUESTION_DISMISSED_NARRATION));
        }
        let dismissed = collected.is_empty();
        collected.truncate(ANSWERS_MAX);
        events.push(ActivityEvent::QuestionResolved {
            id: None,
            ask_id: Some(truncate(&ask_id, ID_MAX)),
            answers: (!dismissed).then_some(collected),
            dismissed: dismissed.then_some(true),
            at: None,
        });
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
                        events.push(ActivityEvent::narration(redacted));
                    }
                }
            }
            Some("tool_use") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                // Interactive prompts become answerable question events; a
                // malformed input falls through to the generic tool headline.
                if name == "AskUserQuestion" {
                    if let Some(questions) = parse_ask_user_question(block.get("input"), redactor) {
                        events.extend(questions);
                        continue;
                    }
                } else if name == "ExitPlanMode" {
                    events.push(parse_exit_plan_mode(block.get("input"), redactor));
                    continue;
                }
                let detail = tool_detail(name, block.get("input"))
                    .map(|d| truncate(&redactor.redact(&d), TOOL_DETAIL_MAX));
                events.push(ActivityEvent::tool(truncate(name, TOOL_NAME_MAX), detail));
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
fn parse_user_entry(
    entry: &Value,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Option<ActivityEvent> {
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
    // Already published from its `queued_command` attachment (EXP-356) —
    // consume the memory instead of doubling the bubble.
    if let Some(pos) = state.published_queued.iter().position(|t| t == &redacted) {
        state.published_queued.remove(pos);
        return None;
    }
    Some(ActivityEvent::user_message(redacted))
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
            .map(|(i, label)| {
                QuestionOption::new(
                    truncate(&redactor.redact(label), OPTION_LABEL_MAX),
                    (i + 1).to_string(),
                )
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
            id: None,
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
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
            QuestionOption::new("Approve — auto-accept edits", "1"),
            QuestionOption::new("Approve — manually approve edits", "2"),
        ],
        multi_select: None,
        // Marks the question as a plan-approval picker so clients can render
        // a dedicated "Plan ready" card (EXP-97).
        plan_mode: Some(true),
        id: None,
        ask_id: None,
        index: None,
        total: None,
        header: None,
        at: None,
    }
}

/// The single primary argument shown for a tool call — a file path or search
/// pattern, or (for Bash) the human `description`. NEVER the raw command
/// string, a URL, or arbitrary input (any of which could carry secrets); when
/// nothing safe is present the headline shows the tool name alone.
fn tool_detail(name: &str, input: Option<&Value>) -> Option<String> {
    let input = input?;
    if name.eq_ignore_ascii_case("bash") || name == "Task" || name == "Agent" {
        // The command string / delegation prompt is NEVER published — only
        // the model's own human-readable description of what it's doing.
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

/// The newest non-sidechain session transcript in `dir` modified at/after
/// `after` (the spawn time — so a previous session's stale transcript in a
/// reused worktree is never picked). Sub-agent files (`agent-*.jsonl`) are
/// excluded so tailing never flip-flops between the main session and a
/// sidechain; [`sidechain_transcripts`] streams those separately.
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

/// The freshest subagent sidechain transcripts under the session's project dir
/// (EXP-249), newest first, capped at [`SIDECHAIN_TAIL_MAX`]. claude wrote
/// these flat as `agent-<id>.jsonl` through v2.1.21x and nests them under
/// `<session>/subagents/**` since v2.1.220 — both layouts are walked, with a
/// hard visit budget so a huge tree can never stall the poll loop.
fn sidechain_transcripts(dir: &Path, after: SystemTime) -> Vec<PathBuf> {
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::from([dir.to_path_buf()]);
    let mut visited = 0usize;
    while let Some(current) = queue.pop_front() {
        visited += 1;
        if visited > SIDECHAIN_WALK_MAX {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                queue.push_back(path);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("agent-") || !name.ends_with(".jsonl") {
                continue;
            }
            let Ok(modified) = meta.modified() else {
                continue;
            };
            if modified >= after {
                found.push((modified, path));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.truncate(SIDECHAIN_TAIL_MAX);
    found.into_iter().map(|(_, path)| path).collect()
}

/// The subagent id a sidechain file belongs to: `agent-<id>.jsonl` → `<id>`,
/// which is exactly the `agent_id` the `SubagentStart` hook reports (verified
/// against claude v2.1.220, whose entries repeat it as `agentId`).
fn sidechain_agent_id(path: &Path) -> Option<String> {
    let name = path.file_name().and_then(|n| n.to_str())?;
    let id = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
    (!id.is_empty()).then(|| id.to_string())
}

/// What claude ≥2.1.220 writes next to each sidechain as
/// `agent-<id>.meta.json` — the DETERMINISTIC identity correlation the
/// SubagentStart binding dance only approximates (EXP-356): the dispatch's
/// `tool_use_id` (= the published card id), the real agent type, and the
/// delegation description.
#[derive(Default)]
struct SidechainMeta {
    agent_type: Option<String>,
    description: Option<String>,
    tool_use_id: Option<String>,
}

/// Read the sidechain's `.meta.json` twin, if present (older claude wrote
/// none — identity then stays on the hook/alias path).
fn sidechain_meta(path: &Path) -> Option<SidechainMeta> {
    let meta_path = path.to_str()?.strip_suffix(".jsonl")?.to_string() + ".meta.json";
    let raw = std::fs::read_to_string(meta_path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    Some(SidechainMeta {
        agent_type: value
            .get("agentType")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        tool_use_id: value
            .get("toolUseId")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// One sidechain line → the subagent's tool headlines only (EXP-249). A
/// subagent's prose is not published: the parent narrates what it delegated,
/// and a fan-out of five agents would otherwise bury the feed.
fn parse_sidechain_line(
    line: &str,
    fallback_agent_id: &str,
    redactor: &Redactor,
) -> Vec<ActivityEvent> {
    let Ok(entry) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    if entry.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    let agent_id = entry
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or(fallback_agent_id)
        .to_string();
    parse_assistant_entry(&entry, redactor)
        .into_iter()
        .filter_map(|event| match event {
            ActivityEvent::Tool { name, detail, .. } => Some(ActivityEvent::Tool {
                name,
                detail,
                subagent_id: Some(truncate(&agent_id, ID_MAX)),
                at: None,
            }),
            _ => None,
        })
        .collect()
}

/// EXP-350: sidechain tool headlines carry claude's raw agent id, but the
/// subagent card publishes under the Task `tool_use_id` — remap through the
/// alias so tools group under the card on every client. An id with no alias
/// yet passes through raw: the `SubagentStart` hook drains every 1s tick while
/// sidechain discovery runs on a 3s cadence, so the alias is virtually always
/// in place first, and the residual race just degrades to the hookless path
/// for one tick.
fn attribute_to_card(event: ActivityEvent, subagents: &Subagents) -> ActivityEvent {
    match event {
        ActivityEvent::Tool {
            name,
            detail,
            subagent_id: Some(id),
            at,
        } => ActivityEvent::Tool {
            name,
            detail,
            subagent_id: Some(subagents.card_id(&id)),
            at,
        },
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Worktree diff
// ---------------------------------------------------------------------------

/// A unified diff of the worktree — unstaged plus staged — as one string.
/// Empty when the tree is clean or git fails (best-effort).
pub(crate) fn worktree_diff(worktree: &Path) -> String {
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
// The publisher ↔ emitter answer seam (EXP-249)
// ---------------------------------------------------------------------------

/// A steerer's semantic answer, relayed verbatim by the publisher: the
/// question's own id plus the option `key`s of THAT question. The emitter maps
/// them onto whatever the TUI is showing right now — clients never guess
/// keystrokes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAnswer {
    pub question_id: String,
    pub ask_id: Option<String>,
    pub keys: Vec<String>,
}

/// The one-way seam between the publisher task (tokio) and the emitter thread
/// (blocking grid/PTY work): answers go down the channel, and the emitter
/// publishes back two grid facts the publisher's raw-input handling needs —
/// "an ASK picker is up" (swallow a legacy client's Enter cascade; plan
/// pickers are deliberately excluded, their digits never auto-submit so the
/// trailing Enter is required) and "ANY picker is visible on the grid" (a
/// free-text message must Esc the picker away first or the picker eats it —
/// EXP-334).
pub struct AnswerLink {
    tx: flume::Sender<RemoteAnswer>,
    ask_pending: AtomicBool,
    grid_picker_pending: AtomicBool,
}

impl AnswerLink {
    /// The link plus the emitter's receiving end.
    pub fn new() -> (Arc<Self>, flume::Receiver<RemoteAnswer>) {
        let (tx, rx) = flume::unbounded();
        (
            Arc::new(Self {
                tx,
                ask_pending: AtomicBool::new(false),
                grid_picker_pending: AtomicBool::new(false),
            }),
            rx,
        )
    }

    /// Publisher side: hand one answer to the emitter (fire-and-forget — a
    /// dead emitter just means the card stays unanswered).
    pub fn submit(&self, answer: RemoteAnswer) {
        let _ = self.tx.send(answer);
    }

    /// Whether the session is parked on an AskUserQuestion right now.
    pub fn ask_pending(&self) -> bool {
        self.ask_pending.load(Ordering::Relaxed)
    }

    /// Emitter side (and tests): publish the ask-pending bit.
    pub fn set_ask_pending(&self, pending: bool) {
        self.ask_pending.store(pending, Ordering::Relaxed);
    }

    /// Whether ANY interactive picker (plan approval or ask) is on the live
    /// grid right now — the free-text reroute signal (EXP-334).
    pub fn grid_picker_pending(&self) -> bool {
        self.grid_picker_pending.load(Ordering::Relaxed)
    }

    /// Emitter side (and tests): publish the grid-picker bit.
    pub fn set_grid_picker_pending(&self, pending: bool) {
        self.grid_picker_pending.store(pending, Ordering::Relaxed);
    }
}

/// Everything the emitter needs to ACT on a remote answer: the inbox, the flag
/// channel back to the publisher, and the PTY writer the keystrokes go into
/// (the same `Terminal::writer()` local typing uses — the child cannot tell
/// them apart).
pub struct Steering {
    pub answers: flume::Receiver<RemoteAnswer>,
    pub link: Arc<AnswerLink>,
    pub write_input: InputHook,
}

// ---------------------------------------------------------------------------
// Hook-driven question state (EXP-249)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuestionKind {
    /// `ExitPlanMode` approval.
    Plan,
    /// One step of an `AskUserQuestion`.
    Ask,
    /// The review/submit step that closes a multi-question ask.
    Submit,
}

/// A question that is published and still answerable.
#[derive(Clone, Debug)]
struct LiveQuestion {
    kind: QuestionKind,
    ask_id: Option<String>,
    text_norm: String,
    options: Vec<QuestionOption>,
    multi_select: bool,
}

/// The `ExitPlanMode` hook, waiting for the grid to confirm its picker.
struct PendingPlan {
    id: String,
    text: String,
    seen: Instant,
    published: bool,
    degraded: bool,
}

/// The `AskUserQuestion` hook — every question published up front; the grid
/// only augments them as their tabs come up.
struct PendingAsk {
    ask_id: String,
    questions: Vec<HookQuestion>,
    /// Options published per question so far — a grid tab showing MORE than
    /// this is a synthetic row ("Type something") worth re-emitting for.
    published_options: Vec<usize>,
    submit_published: bool,
}

/// Why the session is waiting on a human outside of a picker.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Attention {
    /// A `Notification` classified as a permission prompt — informational,
    /// answered at the local TUI only.
    Permission,
    /// Any other notification (claude's idle nudge).
    Idle,
}

/// Subagent identity bookkeeping: `PreToolUse:Task` gives a `tool_use_id` and
/// the description, `SubagentStart`/`Stop` give an `agent_id`, and the
/// sidechain transcripts are keyed by the latter. The dispatch publishes the
/// card (that is when the user sees the delegation); the start binds the two
/// ids so tool headlines and the completion edge land on the SAME card.
#[derive(Default)]
struct Subagents {
    /// Dispatched `tool_use_id`s awaiting a `SubagentStart` to bind to.
    unbound: VecDeque<String>,
    /// `agent_id` → the id the card was published under.
    alias: HashMap<String, String>,
    /// Card id → what the card was published with. `SubagentStop` carries no
    /// `agent_type`, so the completion edge re-states these (EXP-350: clients
    /// that render the LAST marker were degrading the label to "agent").
    meta: HashMap<String, CardMeta>,
    /// Card ids whose completion edge already published (EXP-360): a
    /// background agent's end can be seen twice (task-notification + a late
    /// hook stop, or repeat notifications when the agent is resumed) — the
    /// card completes ONCE.
    completed: HashSet<String>,
}

/// What a subagent card was published with (already redacted/truncated).
struct CardMeta {
    agent_type: String,
    detail: Option<String>,
}

/// Live subagent cap — a wide fan-out must not grow these maps without bound.
const SUBAGENTS_CAP: usize = 32;

impl Subagents {
    fn dispatch(&mut self, tool_use_id: String, agent_type: String, detail: Option<String>) {
        self.remember(tool_use_id.clone(), agent_type, detail);
        self.unbound.push_back(tool_use_id);
        while self.unbound.len() > SUBAGENTS_CAP {
            self.unbound.pop_front();
        }
    }

    /// Record a card's published metadata (dispatch, or an unbound start).
    fn remember(&mut self, card_id: String, agent_type: String, detail: Option<String>) {
        self.meta.insert(card_id, CardMeta { agent_type, detail });
        while self.meta.len() > SUBAGENTS_CAP {
            let Some(stale) = self.meta.keys().next().cloned() else {
                break;
            };
            self.meta.remove(&stale);
        }
    }

    /// Bind a starting agent to an unbound dispatch — preferring one whose
    /// dispatched type matches the start's, so a mixed fan-out (explore +
    /// review dispatched together) binds each start to the right card. Two
    /// same-type concurrent dispatches can still swap children; both cards
    /// carry the right type and detail, which is acceptable — `SubagentStart`
    /// offers nothing better to correlate on. The published id wins so the
    /// card stays the one the dispatch created. A start whose type MATCHES no
    /// unbound dispatch may still bind an untyped one, but never steals a
    /// dispatch of a DIFFERENT type — claude's internal helper agents also
    /// fire SubagentStart, and blind position-0 binding let them claim a real
    /// delegation's card (EXP-356).
    fn started(&mut self, agent_id: &str, agent_type: Option<&str>) -> Option<String> {
        let typed = |unbound_type: &str| match agent_type {
            Some(t) => unbound_type == t,
            None => false,
        };
        let pos = self
            .unbound
            .iter()
            .position(|id| self.meta.get(id).is_some_and(|m| typed(&m.agent_type)))
            .or_else(|| {
                self.unbound.iter().position(|id| {
                    self.meta
                        .get(id)
                        .is_none_or(|m| m.agent_type == SUBAGENT_TYPE_FALLBACK)
                })
            })?;
        let dispatched = self.unbound.remove(pos)?;
        // A dispatch without a `subagent_type` stored the fallback — the
        // start's real type upgrades it for the completion edge.
        if let Some(t) = agent_type {
            if let Some(meta) = self.meta.get_mut(&dispatched) {
                if meta.agent_type == SUBAGENT_TYPE_FALLBACK {
                    meta.agent_type = t.to_string();
                }
            }
        }
        self.alias.insert(agent_id.to_string(), dispatched.clone());
        while self.alias.len() > SUBAGENTS_CAP {
            let Some(stale) = self.alias.keys().next().cloned() else {
                break;
            };
            self.alias.remove(&stale);
        }
        Some(dispatched)
    }

    /// The card id for an `agent_id` (its dispatch's, or the raw id when the
    /// Task hook never fired).
    fn card_id(&self, agent_id: &str) -> String {
        self.alias
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| agent_id.to_string())
    }

    /// Whether an `agent_id` resolves to a card that was actually published —
    /// bound to a dispatch, remembered from an unbound start, or absorbed
    /// from a sidechain meta. Lifecycle events for anything else are claude's
    /// INTERNAL helper agents (summarizers etc.), which fire SubagentStart/
    /// Stop too and used to mint an endless stream of fallback-labelled cards
    /// (EXP-356).
    fn knows_agent(&self, agent_id: &str) -> bool {
        self.alias.contains_key(agent_id) || self.meta.contains_key(agent_id)
    }

    /// EXP-356: absorb a sidechain's `.meta.json` — the deterministic
    /// `agent_id → tool_use_id` correlation plus the real type/description.
    /// Repairs whatever the hook path missed (a raced SubagentStart, a
    /// fallback-typed dispatch, a sidecar that never came up). An existing
    /// binding or already-published card keeps its id (a redirect would split
    /// the group on every client); otherwise the meta's `tool_use_id` becomes
    /// the card id, exactly as a dispatch would have minted. Returns the
    /// `(card_id, agent_type, detail)` Started marker to publish when the
    /// card was UNKNOWN — no dispatch ever carded it.
    fn absorb_meta(
        &mut self,
        agent_id: &str,
        meta_tool_use_id: Option<String>,
        agent_type: String,
        detail: Option<String>,
    ) -> Option<(String, String, Option<String>)> {
        let card_id = self.alias.get(agent_id).cloned().unwrap_or_else(|| {
            if self.meta.contains_key(agent_id) {
                // An unbound start already published under the raw id.
                agent_id.to_string()
            } else {
                meta_tool_use_id.unwrap_or_else(|| agent_id.to_string())
            }
        });
        // The dispatch this card came from can no longer be claimed by a
        // late (or foreign) SubagentStart.
        self.unbound.retain(|id| id != &card_id);
        let known = match self.meta.get_mut(&card_id) {
            Some(meta) => {
                if meta.agent_type == SUBAGENT_TYPE_FALLBACK {
                    meta.agent_type = agent_type.clone();
                }
                if meta.detail.is_none() {
                    meta.detail = detail.clone();
                }
                true
            }
            None => false,
        };
        if !known {
            self.remember(card_id.clone(), agent_type.clone(), detail.clone());
        }
        self.alias.insert(agent_id.to_string(), card_id.clone());
        while self.alias.len() > SUBAGENTS_CAP {
            let Some(stale) = self.alias.keys().next().cloned() else {
                break;
            };
            self.alias.remove(&stale);
        }
        (!known).then_some((card_id, agent_type, detail))
    }

    /// The published metadata for a card id.
    fn card_meta(&self, card_id: &str) -> Option<&CardMeta> {
        self.meta.get(card_id)
    }

    /// EXP-360: bind an async launch ack's `agent_id → tool_use_id` pairing —
    /// authoritative, so it OVERWRITES whatever the `SubagentStart`
    /// type-matching dance guessed (a same-type parallel fan-out swaps
    /// children there). Only a dispatched card is bound: alias entries must
    /// keep implying "a card was published" (the sidechain meta cards and
    /// binds hookless dispatches on its own).
    fn bind_launch(&mut self, agent_id: &str, tool_use_id: &str) {
        if !self.meta.contains_key(tool_use_id) {
            return;
        }
        self.unbound.retain(|id| id != tool_use_id);
        self.alias
            .insert(agent_id.to_string(), tool_use_id.to_string());
        while self.alias.len() > SUBAGENTS_CAP {
            let Some(stale) = self.alias.keys().next().cloned() else {
                break;
            };
            self.alias.remove(&stale);
        }
    }

    /// EXP-360: resolve an end signal — a `SubagentStop` hook, a
    /// `task-notification` entry, or a dispatched card's tool_result — to its
    /// card and mark it completed, ONCE. `None` = nothing to publish: an id
    /// nothing ever carded (claude's internal machinery), or a card whose
    /// completion already went out. Returns what the completion edge restates
    /// (EXP-350: the stop payload carries no type, and last-marker-wins
    /// clients degrade the label without it).
    fn complete(
        &mut self,
        agent_id: Option<&str>,
        tool_use_id: Option<&str>,
    ) -> Option<(String, String, Option<String>)> {
        let card_id = match agent_id {
            Some(id) if self.knows_agent(id) => Some(self.card_id(id)),
            _ => None,
        }
        .or_else(|| {
            tool_use_id
                .filter(|id| self.meta.contains_key(*id))
                .map(str::to_string)
        })?;
        if !self.completed.insert(card_id.clone()) {
            return None;
        }
        while self.completed.len() > SUBAGENTS_CAP * 2 {
            let Some(stale) = self.completed.iter().next().cloned() else {
                break;
            };
            self.completed.remove(&stale);
        }
        let meta = self.card_meta(&card_id);
        Some((
            card_id.clone(),
            meta.map(|m| m.agent_type.clone())
                .unwrap_or_else(|| SUBAGENT_TYPE_FALLBACK.to_string()),
            meta.and_then(|m| m.detail.clone()),
        ))
    }
}

/// The emitter's steering brain: what the hooks said, what is published, what
/// is still answerable.
#[derive(Default)]
struct SteerState {
    plan: Option<PendingPlan>,
    ask: Option<PendingAsk>,
    subagents: Subagents,
    attention: Option<Attention>,
    live: HashMap<String, LiveQuestion>,
    answered: HashSet<String>,
    /// Fallback plan identity when a hook payload carries no `tool_use_id`.
    plan_seq: u32,
    /// Fallback subagent-card identity when a `Task` hook payload carries no
    /// `tool_use_id` (EXP-350 — the card used to be dropped entirely).
    task_seq: u32,
    /// EXP-275: the session runs with permissions bypassed
    /// (`--dangerously-skip-permissions` / codex bypass). A real permission
    /// prompt cannot happen then, so a permission-flavored Notification is
    /// claude parked on input — never a blocked-on-approval card.
    bypass_permissions: bool,
    /// EXP-347: a question/plan resolution was learned since the last
    /// [`Self::take_resolution`] — the emitter clears the publisher's
    /// grid-picker flag on it instead of waiting for the next grid tick
    /// (which never comes while the viewport is scrolled).
    resolution_seen: bool,
}

/// One question about to go on the wire.
struct Publishable {
    id: String,
    kind: QuestionKind,
    ask_id: Option<String>,
    index: Option<u32>,
    total: Option<u32>,
    header: Option<String>,
    text: String,
    options: Vec<QuestionOption>,
    multi_select: bool,
}

impl SteerState {
    /// Whether a hook says the session is parked on a picker right now.
    fn has_pending_question(&self) -> bool {
        self.plan.is_some() || self.ask.is_some()
    }

    /// EXP-347: whether a resolution was learned since the last call — a
    /// consuming read (`false` until the next resolution).
    fn take_resolution(&mut self) -> bool {
        std::mem::take(&mut self.resolution_seen)
    }

    /// EXP-355: evidence a turn is in flight — claude cannot be parked on the
    /// input box while it dispatches tools or its subagents stream, so a
    /// stale idle nudge is retired. Without this the badge sat on "needs
    /// input" through an entire background-agent fan-out: the main transcript
    /// is silent while delegated work runs, and transcript progress was the
    /// only thing clearing the flag. A [`Attention::Permission`] block
    /// deliberately survives — a parallel/background subagent can stream
    /// while the main agent genuinely waits on an approval (that one still
    /// clears on main-transcript progress / `Stop`, as ever).
    fn note_agent_activity(&mut self) {
        if self.attention == Some(Attention::Idle) {
            self.attention = None;
        }
    }

    fn publish_question(&mut self, question: Publishable, sender: &ActivitySender) {
        let plan_mode = (question.kind == QuestionKind::Plan).then_some(true);
        self.live.insert(
            question.id.clone(),
            LiveQuestion {
                kind: question.kind,
                ask_id: question.ask_id.clone(),
                text_norm: normalize_question_text(&question.text),
                options: question.options.clone(),
                multi_select: question.multi_select,
            },
        );
        sender.send(ActivityEvent::Question {
            text: question.text,
            options: question.options,
            multi_select: question.multi_select.then_some(true),
            plan_mode,
            id: Some(question.id),
            ask_id: question.ask_id,
            index: question.index,
            total: question.total,
            header: question.header,
            at: None,
        });
    }

    /// A hook delivery → published events + state.
    fn apply_hook(
        &mut self,
        event: HookEvent,
        sender: &ActivitySender,
        redactor: &Redactor,
        transcript: &mut TranscriptState,
    ) {
        // Tool dispatches and subagent lifecycle edges disprove "parked on
        // the input box" (EXP-355).
        if matches!(
            event.kind,
            HookEventKind::PlanProposed { .. }
                | HookEventKind::QuestionsAsked { .. }
                | HookEventKind::SubagentDispatched { .. }
                | HookEventKind::SubagentStarted { .. }
                | HookEventKind::SubagentStopped { .. }
        ) {
            self.note_agent_activity();
        }
        match event.kind {
            HookEventKind::PlanProposed { tool_use_id, plan } => {
                self.plan_seq += 1;
                let id = tool_use_id.unwrap_or_else(|| format!("plan-{}", self.plan_seq));
                let text = truncate(&redactor.redact(&plan), QUESTION_TEXT_MAX);
                // claude flushes the ExitPlanMode transcript entry only once
                // the picker is answered — the twin it will produce is this
                // same plan, already published here.
                transcript.suppress_plan_questions += 1;
                self.plan = Some(PendingPlan {
                    id: truncate(&id, ID_MAX),
                    text: if text.trim().is_empty() {
                        "Plan ready for approval.".to_string()
                    } else {
                        text
                    },
                    seen: Instant::now(),
                    published: false,
                    degraded: false,
                });
            }
            HookEventKind::QuestionsAsked {
                tool_use_id,
                questions,
            } => {
                let Some(ask_id) = tool_use_id else {
                    // Without an id there is nothing to answer against — the
                    // grid path publishes it the legacy way.
                    return;
                };
                let ask_id = truncate(&ask_id, ID_MAX);
                transcript.hook_published_asks.insert(ask_id.clone());
                let total = questions.len() as u32;
                let mut published_options = Vec::with_capacity(questions.len());
                for (index, question) in questions.iter().enumerate() {
                    let options = hook_options(question, redactor);
                    published_options.push(options.len());
                    self.publish_question(
                        Publishable {
                            id: format!("{ask_id}#{index}"),
                            kind: QuestionKind::Ask,
                            ask_id: Some(ask_id.clone()),
                            index: Some(index as u32 + 1),
                            total: Some(total),
                            header: question
                                .header
                                .as_ref()
                                .map(|h| truncate(&redactor.redact(h), QUESTION_HEADER_MAX)),
                            text: truncate(&redactor.redact(&question.question), QUESTION_TEXT_MAX),
                            options,
                            multi_select: question.multi_select,
                        },
                        sender,
                    );
                }
                self.ask = Some(PendingAsk {
                    ask_id,
                    questions,
                    published_options,
                    submit_published: false,
                });
            }
            HookEventKind::SubagentDispatched {
                tool_use_id,
                description,
                subagent_type,
            } => {
                let id = tool_use_id.unwrap_or_else(|| {
                    self.task_seq += 1;
                    format!("task-{}", self.task_seq)
                });
                let id = truncate(&id, ID_MAX);
                let agent_type = truncate(
                    subagent_type.as_deref().unwrap_or(SUBAGENT_TYPE_FALLBACK),
                    AGENT_TYPE_MAX,
                );
                let detail =
                    description.map(|detail| truncate(&redactor.redact(&detail), TOOL_DETAIL_MAX));
                self.subagents
                    .dispatch(id.clone(), agent_type.clone(), detail.clone());
                sender.send(ActivityEvent::Subagent {
                    id,
                    agent_type,
                    status: SubagentStatus::Started,
                    detail,
                    at: None,
                });
            }
            HookEventKind::SubagentStarted {
                agent_id,
                agent_type,
            } => {
                let Some(agent_id) = agent_id else { return };
                let agent_id = truncate(&agent_id, ID_MAX);
                let agent_type = agent_type.as_deref().map(|t| truncate(t, AGENT_TYPE_MAX));
                // Already absorbed from the sidechain meta ⇒ carded, and no
                // leftover dispatch may be (re)claimed for it.
                if self.subagents.knows_agent(&agent_id) {
                    return;
                }
                // Bound to a dispatch we already carded ⇒ nothing new to show.
                if self
                    .subagents
                    .started(&agent_id, agent_type.as_deref())
                    .is_some()
                {
                    return;
                }
                // An unbound, TYPE-LESS start is claude's internal machinery
                // (summarizers etc. fire SubagentStart too): a real delegation
                // dispatches first (Task/Agent PreToolUse) or leaves a
                // sidechain meta to absorb. Publishing minted one fallback
                // card per internal agent (EXP-356).
                let Some(agent_type) = agent_type else { return };
                // Hookless-dispatch card — remember the type so the stop edge
                // can re-state it (its payload never carries one).
                self.subagents
                    .remember(agent_id.clone(), agent_type.clone(), None);
                sender.send(ActivityEvent::Subagent {
                    id: agent_id,
                    agent_type,
                    status: SubagentStatus::Started,
                    detail: None,
                    at: None,
                });
            }
            HookEventKind::SubagentStopped {
                agent_id,
                agent_type,
            } => {
                let Some(agent_id) = agent_id else { return };
                let agent_id = truncate(&agent_id, ID_MAX);
                // A stop for an agent no card was ever published for is
                // claude's internal machinery — completing a card nobody saw
                // minted an endless "agent subagent ✓" stream (EXP-356). A
                // card whose completion the transcript already delivered
                // (EXP-360) stays silent too.
                let Some((id, stored_type, detail)) =
                    self.subagents.complete(Some(&agent_id), None)
                else {
                    return;
                };
                // claude's SubagentStop payload carries no agent_type — restate
                // what the card was published with, or last-marker-wins clients
                // degrade the label to the fallback (EXP-350).
                let agent_type = agent_type
                    .as_deref()
                    .map(|t| truncate(t, AGENT_TYPE_MAX))
                    .unwrap_or(stored_type);
                sender.send(ActivityEvent::Subagent {
                    id,
                    agent_type,
                    status: SubagentStatus::Completed,
                    detail,
                    at: None,
                });
            }
            HookEventKind::PermissionPrompt { message, tool } => {
                // A pending picker's own nudge: claude sends a "needs your
                // permission" Notification for AskUserQuestion/ExitPlanMode
                // too (even in bypass mode). The picker already carries the
                // needs-input signal, and a permission card would claim a
                // block the steerer can't act on remotely (EXP-275).
                if self.ask.is_some() || self.plan.is_some() {
                    return;
                }
                // Bypass mode cannot hit a real permission prompt — whatever
                // sent this, claude is merely parked on human input.
                if self.bypass_permissions {
                    self.attention = Some(Attention::Idle);
                    return;
                }
                self.attention = Some(Attention::Permission);
                sender.send(ActivityEvent::Permission {
                    tool: truncate(tool.as_deref().unwrap_or("Tool"), ID_MAX),
                    detail: Some(truncate(&redactor.redact(&message), TOOL_DETAIL_MAX)),
                    at: None,
                });
            }
            HookEventKind::Idle { .. } => self.attention = Some(Attention::Idle),
            // The turn ended: whatever the session was waiting on is over.
            // Besides the attention flag, retire any ask/plan still marked
            // pending — normally the transcript flush resolves them
            // (`observe_published`), but a missed flush used to pin
            // `needs_input` and the clients' steppers forever (EXP-275).
            // A normally-answered ask is already gone here, so this is a
            // silent safety net; the transcript's enriched resolution (with
            // the collected answers) still follows when it does land.
            HookEventKind::Stop | HookEventKind::SessionEnd { .. } => {
                self.attention = None;
                // The turn is over ⇒ no picker can be on the grid, whatever
                // the (possibly scroll-stuck) watcher last saw (EXP-347).
                self.resolution_seen = true;
                if let Some(ask) = self.ask.take() {
                    self.live
                        .retain(|_, live| live.ask_id.as_deref() != Some(ask.ask_id.as_str()));
                    sender.send(ActivityEvent::QuestionResolved {
                        id: None,
                        ask_id: Some(ask.ask_id),
                        answers: None,
                        dismissed: None,
                        at: None,
                    });
                }
                if let Some(plan) = self.plan.take() {
                    self.live.remove(&plan.id);
                    if plan.published {
                        sender.send(ActivityEvent::QuestionResolved {
                            id: Some(plan.id),
                            ask_id: None,
                            answers: None,
                            dismissed: None,
                            at: None,
                        });
                    }
                }
            }
        }
    }

    /// The plan hook's degraded path: the picker never confirmed on the grid,
    /// so publish the plan as prose rather than sitting on it silently.
    fn plan_timeout(&mut self, sender: &ActivitySender) {
        let Some(plan) = &mut self.plan else { return };
        if plan.published || plan.degraded || plan.seen.elapsed() < PLAN_GRID_CONFIRM {
            return;
        }
        plan.degraded = true;
        sender.send(ActivityEvent::narration(plan.text.clone()));
    }

    /// The plan picker settled on screen: publish the hook's real plan body
    /// with the picker's real option rows. `false` when no hook knows about
    /// this plan (the caller falls back to the legacy grid-only question).
    fn confirm_plan_from_grid(
        &mut self,
        options: Vec<QuestionOption>,
        sender: &ActivitySender,
    ) -> bool {
        let Some(plan) = &mut self.plan else {
            return false;
        };
        if plan.published {
            return true;
        }
        plan.published = true;
        let (id, text) = (plan.id.clone(), plan.text.clone());
        self.publish_question(
            Publishable {
                id,
                kind: QuestionKind::Plan,
                ask_id: None,
                index: None,
                total: None,
                header: None,
                text,
                options,
                multi_select: false,
            },
            sender,
        );
        true
    }

    /// The plan picker left the screen — answered, dismissed, or superseded.
    fn resolve_plan(&mut self, sender: &ActivitySender) {
        self.resolution_seen = true;
        // Legacy signal FIRST: pre-v2 clients retire the card on this exact
        // narration and nothing else (EXP-150/EXP-174).
        sender.send(ActivityEvent::narration(PLAN_RESOLVED_NARRATION));
        let Some(plan) = self.plan.take() else { return };
        self.live.remove(&plan.id);
        if plan.published {
            sender.send(ActivityEvent::QuestionResolved {
                id: Some(plan.id),
                ask_id: None,
                answers: None,
                dismissed: None,
                at: None,
            });
        }
    }

    /// The question picker settled on screen. Returns `false` when no hook
    /// knows this ask, so the caller publishes the legacy id-less question.
    fn confirm_question_from_grid(
        &mut self,
        snapshot: &QuestionSnapshot,
        sender: &ActivitySender,
        redactor: &Redactor,
    ) -> bool {
        let Some(ask) = &mut self.ask else {
            return false;
        };
        let ask_id = ask.ask_id.clone();
        let visible = normalize_question_text(&snapshot.text);
        let matched = ask.questions.iter().position(|question| {
            normalized_texts_match(&normalize_question_text(&question.question), &visible)
        });
        // The review step. `snapshot.review` is the tab bar's verdict — but
        // the bar's glyphs are claude-version-dependent and mis-anchoring on
        // the review screen's own ✔ summary rows used to drop the flag
        // (EXP-275), stranding the whole ask on the TUI. While a
        // MULTI-question ask is pending, the only settled ask-shaped picker
        // whose text matches none of the hook's questions AND offers a submit
        // row is the review screen (its copy varies by claude version), so
        // that unmatched text is treated as the submit step too. Without the
        // submit row it can be a REAL question overflow clipped below the
        // match floor (EXP-394) — swallowed below, so parked answers retry
        // instead of landing on a phantom `#submit` card. A single-question
        // ask renders no review step — an unmatched text there keeps the
        // legacy fallback.
        let offers_submit = snapshot
            .options
            .iter()
            .any(|option| option.label.to_ascii_lowercase().starts_with("submit"));
        if snapshot.review || (matched.is_none() && ask.questions.len() > 1 && offers_submit) {
            if ask.submit_published {
                return true;
            }
            ask.submit_published = true;
            let options = grid_options(snapshot, redactor);
            self.publish_question(
                Publishable {
                    id: format!("{ask_id}#submit"),
                    kind: QuestionKind::Submit,
                    ask_id: Some(ask_id),
                    index: None,
                    total: None,
                    header: None,
                    text: truncate(&redactor.redact(&snapshot.text), QUESTION_TEXT_MAX),
                    options,
                    multi_select: false,
                },
                sender,
            );
            return true;
        }
        let Some(index) = matched else {
            // Multi-question: the hook's cards are already live — swallow the
            // unclassifiable sliver rather than publish a legacy duplicate.
            return ask.questions.len() > 1;
        };
        if snapshot.options.len() <= ask.published_options[index] {
            return true; // the hook already knew every row
        }
        ask.published_options[index] = snapshot.options.len();
        let question = ask.questions[index].clone();
        let total = ask.questions.len() as u32;
        // The grid is authoritative on ROWS (it knows the synthetic "Type
        // something"); the hook is authoritative on descriptions.
        let mut options = grid_options(snapshot, redactor);
        for option in &mut options {
            if let Some(hook_option) = question
                .options
                .iter()
                .find(|hook_option| hook_option.label == option.label)
            {
                option.description = hook_option
                    .description
                    .as_ref()
                    .map(|d| truncate(&redactor.redact(d), OPTION_DESCRIPTION_MAX));
            }
        }
        self.publish_question(
            Publishable {
                id: format!("{ask_id}#{index}"),
                kind: QuestionKind::Ask,
                ask_id: Some(ask_id),
                index: Some(index as u32 + 1),
                total: Some(total),
                header: question
                    .header
                    .as_ref()
                    .map(|h| truncate(&redactor.redact(h), QUESTION_HEADER_MAX)),
                text: truncate(&redactor.redact(&question.question), QUESTION_TEXT_MAX),
                options,
                multi_select: question.multi_select,
            },
            sender,
        );
        true
    }

    /// Watch what the transcript published: an ask resolution retires its
    /// cards, and any progress at all clears a stale attention flag.
    fn observe_published(&mut self, event: &ActivityEvent) {
        if let ActivityEvent::QuestionResolved { ask_id, .. } = event {
            self.resolution_seen = true;
            if let Some(ask_id) = ask_id {
                self.live
                    .retain(|_, live| live.ask_id.as_deref() != Some(ask_id.as_str()));
                if self.ask.as_ref().is_some_and(|ask| &ask.ask_id == ask_id) {
                    self.ask = None;
                }
            }
        }
        self.attention = None;
        // A plan whose picker never appeared (auto-approved, or detection
        // missed it) must not pin "needs input" for the rest of the session.
        if self.plan.as_ref().is_some_and(|plan| plan.degraded) {
            self.plan = None;
        }
    }

    /// Inject a steerer's answer into the TUI and, once the grid confirms the
    /// move, acknowledge it. A refusal is either TRANSIENT ([`AnswerAttempt::
    /// Retry`] — the picker isn't painted yet, the grid is mid-render or
    /// scrolled, the wrong tab is up; the poll loop parks the answer and tries
    /// again for [`ANSWER_RETRY_TTL`], EXP-334) or FINAL (`Settled` without an
    /// ack — stale id, bad key, or keystrokes already injected: injecting
    /// twice is never safe, and a card with no ack stays answerable at the
    /// steerer).
    fn handle_answer(
        &mut self,
        answer: &RemoteAnswer,
        term: &TermHandle,
        write_input: &InputHook,
        sender: &ActivitySender,
    ) -> AnswerAttempt {
        if self.answered.contains(&answer.question_id) {
            // Already injected — never twice. But re-ACK it: a viewer that
            // (re)joined after the original ack sees the journal's questions
            // without its own answer state, re-taps, and a silent drop left
            // that card timing out and the whole stepper rolling back to
            // question 1 (EXP-374).
            sender.send(ActivityEvent::AnswerAck {
                id: answer.question_id.clone(),
                ask_id: answer.ask_id.clone(),
                at: None,
            });
            return AnswerAttempt::Settled;
        }
        let Some(live) = self.live.get(&answer.question_id).cloned() else {
            return AnswerAttempt::Settled;
        };
        if display_offset(term) > 0 {
            // Scrolled into history — the visible grid is not the picker.
            return AnswerAttempt::Retry;
        }
        let lines = screen_lines(term);
        match live.kind {
            QuestionKind::Plan => {
                if plan_picker::detect(&lines).is_none() {
                    return AnswerAttempt::Retry;
                }
                let Some(key) = answer.keys.first() else {
                    return AnswerAttempt::Settled;
                };
                if !live.options.iter().any(|option| &option.key == key) {
                    return AnswerAttempt::Settled;
                }
                write_input(key.as_bytes());
                // Old plan pickers submitted on the digit; current ones
                // (observed v2.1.220) only move the cursor and need Enter to
                // activate the row (EXP-334). Probe briefly for the legacy
                // behavior, then confirm with Enter.
                if !settle_for(PLAN_SUBMIT_PROBE, || {
                    plan_picker::detect(&screen_lines(term)).is_none()
                }) {
                    write_input(b"\r");
                    if !settle(|| plan_picker::detect(&screen_lines(term)).is_none()) {
                        return AnswerAttempt::Settled; // injected — never twice
                    }
                }
            }
            QuestionKind::Ask | QuestionKind::Submit => {
                // The during-ask detector: a hook-known question exists, so an
                // overflowing picker with its tab bar (or the review step's
                // footer) scrolled off the grid must still be answerable
                // (EXP-394).
                let Some(snapshot) = question_picker::detect_during_ask(&lines) else {
                    return AnswerAttempt::Retry;
                };
                let visible = normalize_question_text(&snapshot.text);
                match live.kind {
                    // The submit step is only answerable ON the review tab —
                    // recognized by the tab bar OR by the visible text being
                    // the one the submit card was published with (the grid's
                    // own review copy; the bar's `review` verdict can be lost
                    // to glyph mis-anchoring, EXP-275).
                    QuestionKind::Submit
                        if !snapshot.review
                            && !normalized_texts_match(&live.text_norm, &visible) =>
                    {
                        return AnswerAttempt::Retry
                    }
                    QuestionKind::Ask
                        if snapshot.review
                            || !normalized_texts_match(&live.text_norm, &visible) =>
                    {
                        return AnswerAttempt::Retry
                    }
                    _ => {}
                }
                let step_moved = || match question_picker::detect_during_ask(&screen_lines(term))
                {
                    None => true,
                    Some(next) => {
                        !normalized_texts_match(
                            &live.text_norm,
                            &normalize_question_text(&next.text),
                        ) || next.review != snapshot.review
                    }
                };
                if live.multi_select && live.kind == QuestionKind::Ask {
                    // Digits TOGGLE in a multiSelect picker, so drive the
                    // checkboxes to the requested set, then Tab to advance
                    // (Enter would only toggle — see the picker semantics).
                    for (index, option) in snapshot.options.iter().enumerate() {
                        let wanted = answer.keys.iter().any(|key| key == &option.key);
                        let checked = snapshot.checked.get(index).copied().unwrap_or(false);
                        if wanted != checked {
                            write_input(option.key.as_bytes());
                            std::thread::sleep(KEYSTROKE_GAP);
                        }
                    }
                    write_input(b"\t");
                    if !settle(step_moved) {
                        return AnswerAttempt::Settled; // injected — never twice
                    }
                } else {
                    let Some(key) = answer.keys.first() else {
                        return AnswerAttempt::Settled;
                    };
                    if !snapshot.options.iter().any(|option| &option.key == key) {
                        // The tab that is up doesn't offer this key — likely a
                        // stale frame between tabs.
                        return AnswerAttempt::Retry;
                    }
                    write_input(key.as_bytes());
                    // Classic ask pickers submit on the digit; a
                    // PREVIEW-carrying question renders side-by-side and its
                    // digit only MOVES the cursor — Enter activates the row
                    // (EXP-394, the same digit-then-Enter probe the plan
                    // picker needed in EXP-334). Never after a multiSelect
                    // Tab: a trailing Enter there toggles/answers whatever
                    // the cursor sits on next.
                    if !settle_for(PLAN_SUBMIT_PROBE, step_moved) {
                        write_input(b"\r");
                    }
                    if !settle(step_moved) {
                        return AnswerAttempt::Settled; // injected — never twice
                    }
                }
            }
        }
        self.answered.insert(answer.question_id.clone());
        self.live.remove(&answer.question_id);
        sender.send(ActivityEvent::AnswerAck {
            id: answer.question_id.clone(),
            ask_id: live.ask_id,
            at: None,
        });
        AnswerAttempt::Settled
    }
}

/// Outcome of one remote-answer injection attempt (EXP-334).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnswerAttempt {
    /// Handled for good — acked, or dropped for a reason retrying can't fix.
    Settled,
    /// Transient refusal — the poll loop may try again next tick.
    Retry,
}

/// Poll `done` until it holds or [`ANSWER_SETTLE`] elapses.
fn settle(done: impl FnMut() -> bool) -> bool {
    settle_for(ANSWER_SETTLE, done)
}

/// Poll `done` until it holds or `window` elapses.
fn settle_for(window: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + window;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(ANSWER_SETTLE_STEP);
    }
}

/// A hook question's options → wire options, keyed by the TUI's digit
/// positions (the picker numbers its rows in the order the tool declared).
fn hook_options(question: &HookQuestion, redactor: &Redactor) -> Vec<QuestionOption> {
    question
        .options
        .iter()
        .take(QUESTION_OPTIONS_MAX)
        .enumerate()
        .map(|(index, option)| QuestionOption {
            label: truncate(&redactor.redact(&option.label), OPTION_LABEL_MAX),
            key: (index + 1).to_string(),
            description: option
                .description
                .as_ref()
                .map(|d| truncate(&redactor.redact(d), OPTION_DESCRIPTION_MAX)),
        })
        .collect()
}

/// The picker's REAL rows, with their real keys.
fn grid_options(snapshot: &QuestionSnapshot, redactor: &Redactor) -> Vec<QuestionOption> {
    snapshot
        .options
        .iter()
        .take(QUESTION_OPTIONS_MAX)
        .map(|option| {
            QuestionOption::new(
                truncate(&redactor.redact(&option.label), OPTION_LABEL_MAX),
                option.key.clone(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The emitter thread
// ---------------------------------------------------------------------------

/// EXP-383: which agent CLI the session runs — picks the activity emitter.
/// Local mirror of `coding::CodingAgent` (this crate cannot depend on
/// `coding` — §3.1); the ui wiring converts by id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionAgent {
    #[default]
    Claude,
    Codex,
    Pi,
}

/// What the emitter needs to run: the worktree to tail/diff, plus the live
/// terminal grid for plan-picker detection (EXP-150). `term: None` runs
/// transcript+diff only (tests / headless callers).
pub struct EmitterConfig {
    /// EXP-383: dispatches to the per-agent emitter — claude tails
    /// `~/.claude/projects`, codex tails its rollout JSONL, pi drains the
    /// observer-extension sidecar.
    pub agent: SessionAgent,
    pub worktree: PathBuf,
    pub term: Option<TermHandle>,
    /// REV2-17: exact secrets the wiring already holds at spawn time that no
    /// worktree file can recover — the `expu_` personal key for codex/pi
    /// sessions, where it rides only the spawn env (`EXP_MCP_TOKEN`), never
    /// disk. Merged into the [`Redactor`]'s exact-match set on top of
    /// [`secrets_from_worktree`].
    pub extra_secrets: Vec<String>,
    /// EXP-214: fired (on the emitter thread) whenever the combined
    /// "agent is parked on a picker" flag flips — `true` while a
    /// plan-approval or AskUserQuestion picker is pending, a permission
    /// prompt is unresolved, or claude is idling on human input; `false` once
    /// it resolves. The wiring layer forwards it to the synced
    /// `coding_sessions.needs_input` column. Blocking work is fine here (the
    /// emitter thread already shells out for diffs). Returns whether the
    /// write LANDED — a `false` return re-attempts on a cooldown, so an
    /// offline blip can no longer stick the synced badge on its last value
    /// forever (EXP-355).
    pub on_needs_input: Option<Arc<dyn Fn(bool) -> bool + Send + Sync>>,
    /// EXP-249: the claude hooks sidecar's event stream ([`crate::hooks`]).
    /// `None` = grid-only detection (a non-claude agent, an old claude, or an
    /// unwritable settings file) — the session still publishes, just without
    /// question identity.
    pub hooks: Option<flume::Receiver<HookEvent>>,
    /// EXP-249: the semantic-answer seam. `None` = no remote answering (the
    /// publisher then never forwards `answer` frames either).
    pub steering: Option<Steering>,
    /// EXP-383: a pi session's slice of the observer sidecar
    /// ([`crate::pi_observer`]) — the structured event stream the
    /// `.exp-pi-observer.ts` extension POSTs. `None` for claude/codex, or
    /// when the sidecar never came up (the pi feed then degrades to diffs).
    pub pi_events: Option<flume::Receiver<crate::pi_observer::PiEvent>>,
    /// EXP-275: the session was launched with permissions bypassed
    /// (`--dangerously-skip-permissions` / codex bypass) — permission-flavored
    /// Notifications then never become "blocked on approval" cards.
    pub bypass_permissions: bool,
}

/// Start the public activity emitter on a dedicated OS thread. `active` is the
/// shared run flag — flip it to `false` (on session teardown) to stop the
/// emitter promptly. Returns immediately; the thread self-terminates when
/// `active` clears.
pub fn spawn_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("activity-emitter".to_string())
        .spawn(move || match config.agent {
            SessionAgent::Claude => run_emitter(config, sender, active),
            SessionAgent::Codex => crate::codex_activity::run_emitter(config, sender, active),
            SessionAgent::Pi => crate::pi_activity::run_emitter(config, sender, active),
        })
        .map(|_| ())
        .unwrap_or_else(|err| log::warn!("activity: emitter thread spawn failed: {err}"));
}

/// The debounced changed-only worktree diff snapshot — step 6 of every
/// emitter, extracted verbatim so the codex/pi emitters share it (EXP-383).
pub(crate) struct DiffSnapshots {
    last: String,
    last_at: Option<Instant>,
}

impl DiffSnapshots {
    pub(crate) fn new() -> Self {
        Self {
            last: String::new(),
            last_at: None,
        }
    }

    pub(crate) fn tick(&mut self, worktree: &Path, sender: &ActivitySender, redactor: &Redactor) {
        let due = self.last_at.is_none_or(|at| at.elapsed() >= DIFF_INTERVAL);
        if !due {
            return;
        }
        self.last_at = Some(Instant::now());
        let diff = worktree_diff(worktree);
        if diff != self.last {
            self.last = diff.clone();
            if !diff.is_empty() {
                sender.send(ActivityEvent::diff(truncate(
                    &redactor.redact(&diff),
                    DIFF_MAX,
                )));
            }
        }
    }
}

/// The EXP-214 synced needs-input flag, tracked as the last CONFIRMED server
/// value (`None` = the last write failed and wants a retry). The session row
/// is born with the flag off. Forwarded on flips; an unconfirmed write
/// re-attempts every [`NEEDS_INPUT_RETRY`] (EXP-355). Extracted verbatim from
/// the claude emitter so the codex/pi emitters share it (EXP-383).
pub(crate) struct NeedsInputForwarder {
    forwarded: Option<bool>,
    retry_at: Option<Instant>,
}

pub(crate) type NeedsInputHook = Arc<dyn Fn(bool) -> bool + Send + Sync>;

impl NeedsInputForwarder {
    pub(crate) fn new() -> Self {
        Self {
            forwarded: Some(false),
            retry_at: None,
        }
    }

    pub(crate) fn tick(&mut self, pending: bool, hook: &Option<NeedsInputHook>) {
        if self.forwarded != Some(pending) && self.retry_at.is_none_or(|at| Instant::now() >= at) {
            let landed = match hook {
                Some(hook) => hook(pending),
                None => true,
            };
            self.forwarded = landed.then_some(pending);
            self.retry_at = (!landed).then(|| Instant::now() + NEEDS_INPUT_RETRY);
        }
    }

    /// Teardown tidiness: never leave the synced attention flag stuck on a
    /// session whose emitter is gone (the terminal-exit `end` supersedes).
    pub(crate) fn clear_on_teardown(&mut self, hook: &Option<NeedsInputHook>) {
        if self.forwarded != Some(false) {
            if let Some(hook) = hook {
                hook(false);
            }
        }
    }
}

fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    let mut exact_secrets = secrets_from_worktree(&config.worktree);
    exact_secrets.extend(config.extra_secrets.iter().cloned());
    let redactor = Redactor::new(exact_secrets);

    // Announce the session (the viewer shows this immediately, before any
    // transcript line lands).
    sender.send(ActivityEvent::narration("Session started"));

    let spawn_time = SystemTime::now();
    let transcript_dir =
        transcript_root().map(|root| root.join(munge_claude_project_dir(&config.worktree)));

    let mut current: Option<PathBuf> = None;
    let mut offset: u64 = 0;
    let mut sidechains: Vec<PathBuf> = Vec::new();
    let mut sidechain_offsets: HashMap<PathBuf, u64> = HashMap::new();
    let mut sidechain_scan_at: Option<Instant> = None;
    // EXP-356: agent ids whose `.meta.json` was already absorbed — read once.
    let mut absorbed_sidechains: HashSet<String> = HashSet::new();
    let mut diffs = DiffSnapshots::new();
    let mut transcript_deadline = Some(Instant::now() + TRANSCRIPT_WAIT);
    let mut picker_watcher = PlanPickerWatcher::new();
    let mut question_watcher = QuestionPickerWatcher::new();
    let mut transcript_state = TranscriptState {
        suppress_task_headlines: config.hooks.is_some(),
        ..TranscriptState::default()
    };
    let mut steer = SteerState {
        bypass_permissions: config.bypass_permissions,
        ..SteerState::default()
    };
    // EXP-214/EXP-355: the synced needs-input flag (see [`NeedsInputForwarder`]).
    let mut needs_input = NeedsInputForwarder::new();
    // EXP-334: transiently refused remote answers, retried each tick until
    // [`ANSWER_RETRY_TTL`] — a tap that beats the picker paint must not be
    // dropped on the floor.
    let mut parked_answers: Vec<(RemoteAnswer, Instant)> = Vec::new();
    // EXP-334: whether the last grid look (bottom of scrollback) showed a
    // picker — the publisher's free-text reroute signal. Sticky while
    // scrolled, like the watchers.
    let mut grid_picker_visible = false;

    while active.load(Ordering::SeqCst) {
        // 0) The hooks sidecar (EXP-249): the structured half. Drained before
        //    the grid so a picker that paints in the same tick is already
        //    known by identity when the watcher confirms it.
        if let Some(hooks) = &config.hooks {
            while let Ok(event) = hooks.try_recv() {
                steer.apply_hook(event, &sender, &redactor, &mut transcript_state);
            }
        }
        steer.plan_timeout(&sender);

        // 1) Picker watch on the live grid: the transcript cannot show a
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
                    // Drop the "refine with Ultraplan on Claude Code on the
                    // web" option (key "3" on claude v2.1.211+): it bounces
                    // planning to claude.ai and is not something we want a
                    // remote steerer to trigger — same stance the transcript
                    // fallback (`parse_exit_plan_mode`) already takes. The
                    // remaining options keep their real key numbers, so the
                    // keystroke sent to the PTY still lands on the right row.
                    let options: Vec<QuestionOption> = snapshot
                        .options
                        .into_iter()
                        .filter(|o| !o.label.contains(ULTRAPLAN_WEB_OPTION))
                        .take(QUESTION_OPTIONS_MAX)
                        .map(|o| {
                            QuestionOption::new(
                                truncate(&redactor.redact(&o.label), OPTION_LABEL_MAX),
                                o.key,
                            )
                        })
                        .collect();
                    if !steer.confirm_plan_from_grid(options.clone(), &sender) {
                        // No hook knows this plan (an old claude, or a sidecar
                        // that never came up): an id-less card with a headline
                        // instead of the body. EXP-249 dropped the
                        // `~/.claude/plans` mtime guessing that used to fill it
                        // in — it mixed up concurrent sessions, and the hook
                        // carries the exact plan.
                        sender.send(ActivityEvent::Question {
                            text: "Plan ready for approval.".to_string(),
                            options,
                            multi_select: None,
                            plan_mode: Some(true),
                            id: None,
                            ask_id: None,
                            index: None,
                            total: None,
                            header: None,
                            at: None,
                        });
                        transcript_state.suppress_plan_questions += 1;
                    }
                }
                Some(Transition::Resolved) => steer.resolve_plan(&sender),
                None => {}
            }
            // AskUserQuestion pickers (EXP-197) — the hook already published
            // every question of the ask; the grid confirms which tab is up and
            // augments it with rows only the TUI has (the synthetic "Type
            // something"), then publishes the review/submit step. Without a
            // hook this stays the pre-v2 grid-only publication.
            // With a hook-confirmed ask pending, tolerate the anchors an
            // overflowing picker scrolls off the grid (EXP-394); without one,
            // the strict shape keeps footer-carrying lookalikes out of the
            // legacy grid-only publication path.
            let question_detection = if steer.ask.is_some() {
                question_picker::detect_during_ask(&lines)
            } else {
                question_picker::detect(&lines)
            };
            // EXP-334: the raw (undebounced) "a picker is on screen" fact for
            // the publisher's free-text reroute. While the viewport is
            // scrolled the bottom of the grid is not visible — keep the last
            // known state, like the watchers do.
            if grid_offset == 0 {
                grid_picker_visible =
                    question_detection.is_some() || plan_picker::detect(&lines).is_some();
            }
            if let Some(snapshot) = question_watcher.tick(question_detection, grid_offset) {
                if !steer.confirm_question_from_grid(&snapshot, &sender, &redactor) {
                    let text = truncate(&redactor.redact(&snapshot.text), QUESTION_TEXT_MAX);
                    transcript_state.remember_grid_question(&text);
                    sender.send(ActivityEvent::Question {
                        text,
                        options: grid_options(&snapshot, &redactor),
                        multi_select: snapshot.multi_select.then_some(true),
                        plan_mode: None,
                        id: None,
                        ask_id: None,
                        index: None,
                        total: None,
                        header: None,
                        at: None,
                    });
                }
            }
        }

        // 2) EXP-214: the combined attention flag — the agent is parked and
        //    waits for a human (a picker on the grid, a picker the hooks know
        //    about, or an unresolved permission/idle notification). Forwarded
        //    only on flips; the watchers already debounce mid-render flicker.
        let picker_pending = picker_watcher.is_pending()
            || question_watcher.is_pending()
            || steer.has_pending_question();
        let pending = picker_pending || steer.attention.is_some();
        needs_input.tick(pending, &config.on_needs_input);
        if let Some(steering) = &config.steering {
            // The publisher's Enter-cascade guard keys on an ASK picker only
            // (EXP-334): an ask digit selects-and-submits, so its trailing
            // Enter must be swallowed — but a PLAN digit only moves the
            // cursor, and swallowing the Enter there left the picker
            // unanswered forever. And not on attention at large: a steerer's
            // one-character message plus Enter must still submit while the
            // agent merely idles.
            steering
                .link
                .set_ask_pending(question_watcher.is_pending() || steer.ask.is_some());
            steering.link.set_grid_picker_pending(grid_picker_visible);
        }

        // 3) Resolve / re-resolve the transcript file (a newer session file in
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

        // 4) Tail any new complete lines from the current transcript.
        if let Some(path) = current.clone() {
            offset = tail_transcript(
                &path,
                offset,
                &mut |line| process_transcript_line(line, &redactor, &mut transcript_state),
                &mut |event| {
                    steer.observe_published(&event);
                    sender.send(event);
                },
            );
        }

        // 4a) EXP-360: background-subagent lifecycle read off the lines just
        //     tailed. A launch ack pins the agent→dispatch binding; an end —
        //     the task-notification that is a background agent's ONLY stop
        //     signal (SubagentStop never fires for them), or a foreground
        //     subagent's tool_result — publishes the completion edge the
        //     hooks missed, so the tab's spinner actually stops.
        for task_event in std::mem::take(&mut transcript_state.task_events) {
            match task_event {
                TaskEvent::Launched {
                    agent_id,
                    tool_use_id,
                } => steer.subagents.bind_launch(&agent_id, &tool_use_id),
                TaskEvent::Ended {
                    agent_id,
                    tool_use_id,
                } => {
                    if let Some((id, agent_type, detail)) = steer
                        .subagents
                        .complete(agent_id.as_deref(), tool_use_id.as_deref())
                    {
                        sender.send(ActivityEvent::Subagent {
                            id,
                            agent_type,
                            status: SubagentStatus::Completed,
                            detail,
                            at: None,
                        });
                    }
                }
            }
        }

        // 4b) EXP-347: a resolution learned this tick — from the hooks (Stop /
        //     SessionEnd, step 0), the grid (plan Transition::Resolved, step
        //     1), or the transcript flush just tailed (a QuestionResolved or
        //     the suppressed plan twin, both of which claude only writes once
        //     the picker is ANSWERED) — means no picker owns the keyboard
        //     anymore. Clear the sticky grid memory and push the publisher's
        //     flag NOW rather than at the next step-2 publish: while the
        //     viewport is scrolled the grid recompute never runs, and a stale
        //     `true` reroutes a remote message's Esc into a live turn,
        //     cancelling it.
        if steer.take_resolution() || std::mem::take(&mut transcript_state.plan_twin_flushed) {
            grid_picker_visible = false;
            if let Some(steering) = &config.steering {
                steering.link.set_grid_picker_pending(false);
            }
        }

        // 5) …and from the freshest subagent sidechains, whose tool headlines
        //    are attributed to their agent (EXP-249). Discovery walks a tree,
        //    so it runs on its own slower cadence; the tails themselves are
        //    plain seeks and run every tick.
        if let Some(dir) = &transcript_dir {
            if sidechain_scan_at.is_none_or(|at| at.elapsed() >= SIDECHAIN_SCAN_INTERVAL) {
                sidechain_scan_at = Some(Instant::now());
                sidechains = sidechain_transcripts(dir, spawn_time);
                sidechain_offsets.retain(|path, _| sidechains.contains(path));
                // EXP-356: absorb each sidechain's `.meta.json` identity once
                // — deterministic agent→card correlation and the real
                // type/description, whatever the hook path managed.
                for path in &sidechains {
                    let Some(agent_id) = sidechain_agent_id(path) else {
                        continue;
                    };
                    if !absorbed_sidechains.insert(agent_id.clone()) {
                        continue;
                    }
                    let Some(meta) = sidechain_meta(path) else {
                        continue;
                    };
                    let agent_type = truncate(
                        meta.agent_type.as_deref().unwrap_or(SUBAGENT_TYPE_FALLBACK),
                        AGENT_TYPE_MAX,
                    );
                    let detail = meta
                        .description
                        .as_ref()
                        .map(|d| truncate(&redactor.redact(d), TOOL_DETAIL_MAX));
                    let tool_use_id = meta.tool_use_id.as_deref().map(|id| truncate(id, ID_MAX));
                    if let Some((card_id, agent_type, detail)) =
                        steer
                            .subagents
                            .absorb_meta(&agent_id, tool_use_id, agent_type, detail)
                    {
                        sender.send(ActivityEvent::Subagent {
                            id: card_id,
                            agent_type,
                            status: SubagentStatus::Started,
                            detail,
                            at: None,
                        });
                    }
                }
            }
            for path in &sidechains {
                let Some(agent_id) = sidechain_agent_id(path) else {
                    continue;
                };
                let start = sidechain_offsets.get(path).copied().unwrap_or(0);
                let next = tail_transcript(
                    path,
                    start,
                    &mut |line| parse_sidechain_line(line, &agent_id, &redactor),
                    &mut |event| sender.send(attribute_to_card(event, &steer.subagents)),
                );
                if next != start {
                    // A subagent transcript moved ⇒ delegated work is live;
                    // the main transcript stays silent through a background
                    // fan-out, so the idle nudge must clear HERE (EXP-355).
                    steer.note_agent_activity();
                }
                sidechain_offsets.insert(path.clone(), next);
            }
        }

        // 6) Debounced worktree diff snapshot (only when changed).
        diffs.tick(&config.worktree, &sender, &redactor);

        // 7) Wait out the poll interval — interrupted by a remote answer, so
        //    steering never sits a full second behind the steerer's tap.
        //    Transiently refused answers stay parked and are retried each
        //    tick until ANSWER_RETRY_TTL (EXP-334): a tap can beat the picker
        //    paint (hook questions publish before the TUI renders), and
        //    silently dropping it made the mobile stepper roll back to an
        //    already-answered step a few seconds later.
        match &config.steering {
            Some(steering) => {
                let deadline = Instant::now() + POLL_INTERVAL;
                loop {
                    match &config.term {
                        Some(term) => parked_answers.retain_mut(|(answer, since)| {
                            match steer.handle_answer(answer, term, &steering.write_input, &sender)
                            {
                                AnswerAttempt::Settled => false,
                                AnswerAttempt::Retry => since.elapsed() < ANSWER_RETRY_TTL,
                            }
                        }),
                        // No grid to choreograph against ⇒ no injection and no
                        // ack: the card stays answerable at the steerer.
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
                            // A re-tap supersedes the parked attempt for the
                            // same card — never queue both.
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
        steering.link.set_ask_pending(false);
        steering.link.set_grid_picker_pending(false);
    }
}

/// Read complete newline-terminated lines from `path` starting at byte
/// `offset`, run each through `parse`, hand the events to `emit`, and return
/// the new offset (a trailing partial line is left for the next poll). The two
/// closures are what separates a main transcript (stateful parse, observed
/// publication) from a subagent sidechain (tool headlines only).
pub(crate) fn tail_transcript(
    path: &Path,
    offset: u64,
    parse: &mut dyn FnMut(&str) -> Vec<ActivityEvent>,
    emit: &mut dyn FnMut(ActivityEvent),
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
        for event in parse(&line) {
            emit(event);
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
pub(crate) fn truncate(s: &str, max: usize) -> String {
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

    /// A real git clone with a linked worktree and an EXP-73 credential file
    /// in the shared `.git` — the production shape `secrets_from_worktree`
    /// must recover the installation token from (the origin stays BARE, so
    /// the old remote-URL extraction finds nothing).
    #[test]
    fn secrets_from_worktree_read_the_exp73_credential_file() {
        let dir = std::env::temp_dir().join(format!(
            "exp-activity-creds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let git = |cwd: &Path, args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        let clone = dir.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "--quiet", "-b", "main"]);
        std::fs::write(clone.join("README.md"), "seed\n").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "--quiet", "-m", "seed"]);
        git(
            &clone,
            &["remote", "add", "origin", "https://github.com/acme/web.git"],
        );
        let token = "ghs_FAKEcredfileTOKEN1234567890";
        std::fs::write(
            clone.join(".git").join(GIT_CREDENTIALS_FILE),
            format!("username=x-access-token\npassword={token}\n"),
        )
        .unwrap();
        let worktree = dir.join("wt");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "exp/EXP-1",
                worktree.to_str().unwrap(),
            ],
        );

        // The linked worktree resolves the shared git dir (absolute
        // --git-common-dir), the clone root resolves the relative `.git`.
        for tree in [&worktree, &clone] {
            let secrets = secrets_from_worktree(tree);
            assert!(
                secrets.contains(&token.to_string()),
                "token not recovered from {}: {secrets:?}",
                tree.display()
            );
        }

        // A pre-migration clone (token still embedded in origin, no
        // credential file) keeps working via the fallback extraction.
        std::fs::remove_file(clone.join(".git").join(GIT_CREDENTIALS_FILE)).unwrap();
        let legacy = "ghs_FAKElegacyremoteTOKEN567890";
        git(
            &clone,
            &[
                "remote",
                "set-url",
                "origin",
                &format!("https://x-access-token:{legacy}@github.com/acme/web.git"),
            ],
        );
        assert!(secrets_from_worktree(&worktree).contains(&legacy.to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn secrets_from_worktree_outside_a_repo_is_empty() {
        let dir = std::env::temp_dir().join(format!("exp-activity-norepo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(secrets_from_worktree(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
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
        let pem =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAA...\nabc\n-----END RSA PRIVATE KEY-----";
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
                ActivityEvent::narration("Let me read the file."),
                ActivityEvent::tool("Edit", Some("src/main.rs".into())),
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
            vec![ActivityEvent::tool("Bash", Some("Fetch the data".into()))]
        );
        // The command string (with its secret) is nowhere in the output.
        let joined = format!("{events:?}");
        assert!(
            !joined.contains("secrettoken"),
            "bash command leaked: {joined}"
        );
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
            vec![ActivityEvent::user_message("Fix the login bug in EXP-42.")]
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
            vec![ActivityEvent::user_message(
                "please add tests\n\nand update the docs"
            )]
        );
    }

    #[test]
    fn a_queued_command_attachment_becomes_a_user_message() {
        // A MID-TURN steered/typed message never lands as a `user` entry —
        // claude records only the queued_command attachment (EXP-356). The
        // real shape, from claude v2.1.220.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let line = serde_json::json!({
            "type": "attachment",
            "attachment": {
                "type": "queued_command",
                "prompt": "see also EXP-356, maybe we can fix both",
                "commandMode": "prompt",
                "origin": { "kind": "human" }
            }
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&line, &redactor, &mut state),
            vec![ActivityEvent::user_message(
                "see also EXP-356, maybe we can fix both"
            )]
        );
        // A turn-boundary delivery can ALSO flush a regular human user entry
        // with the same text — one bubble, not two.
        let twin = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "message": { "role": "user", "content": "see also EXP-356, maybe we can fix both" }
        })
        .to_string();
        assert!(process_transcript_line(&twin, &redactor, &mut state).is_empty());
        // A LATER, distinct human turn still publishes.
        let fresh = serde_json::json!({
            "type": "user",
            "origin": { "kind": "human" },
            "message": { "role": "user", "content": "and one more thing" }
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&fresh, &redactor, &mut state),
            vec![ActivityEvent::user_message("and one more thing")]
        );
    }

    #[test]
    fn a_non_human_queued_command_is_dropped() {
        // Fail closed like parse_user_entry: no nested origin marker → no
        // bubble, whatever the attachment claims to be.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let unmarked = serde_json::json!({
            "type": "attachment",
            "attachment": { "type": "queued_command", "prompt": "injected" }
        })
        .to_string();
        assert!(process_transcript_line(&unmarked, &redactor, &mut state).is_empty());
        let other = serde_json::json!({
            "type": "attachment",
            "attachment": { "type": "todo_list", "origin": { "kind": "human" } }
        })
        .to_string();
        assert!(process_transcript_line(&other, &redactor, &mut state).is_empty());
    }

    #[test]
    fn an_agent_tool_headline_is_suppressed_when_hooks_card_it() {
        // claude ≥2.1.220 dispatches subagents via `Agent` (EXP-356) — with
        // the sidecar up the hook publishes the card, so the bare headline
        // must be swallowed exactly like `Task` (EXP-350).
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_task_headlines: true,
            ..TranscriptState::default()
        };
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "Agent", "id": "toolu_03",
                  "input": { "description": "Find the flow", "subagent_type": "Explore" } }
            ]}
        })
        .to_string();
        assert!(process_transcript_line(&line, &redactor, &mut state).is_empty());
        // Hookless sessions keep the headline — with the description, never
        // the prompt.
        let mut hookless = TranscriptState::default();
        assert_eq!(
            process_transcript_line(&line, &redactor, &mut hookless),
            vec![ActivityEvent::tool("Agent", Some("Find the flow".into()))]
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
            [ActivityEvent::UserMessage { text, .. }] => {
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
                        QuestionOption::new("OAuth", "1"),
                        QuestionOption::new("JWT", "2"),
                        QuestionOption::new("Session", "3"),
                    ],
                    multi_select: None,
                    plan_mode: None,
                    id: None,
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
                },
                ActivityEvent::Question {
                    text: "Which features?".into(),
                    options: vec![
                        QuestionOption::new("Push", "1"),
                        QuestionOption::new("Email", "2"),
                    ],
                    multi_select: Some(true),
                    plan_mode: None,
                    id: None,
                    ask_id: None,
                    index: None,
                    total: None,
                    header: None,
                    at: None,
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
            [ActivityEvent::Question {
                text,
                options,
                multi_select,
                plan_mode,
                ..
            }] => {
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
            [ActivityEvent::Question {
                text,
                options,
                plan_mode,
                ..
            }] => {
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
            vec![ActivityEvent::tool("AskUserQuestion", None)]
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
            vec![ActivityEvent::tool("AskUserQuestion", None)]
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
        assert_eq!(events, vec![ActivityEvent::tool("WebFetch", None)]);
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
            ActivityEvent::Narration { text, .. } => assert_eq!(text.len(), NARRATION_MAX),
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
            ActivityEvent::Narration { text, .. } => {
                assert!(
                    text.len() <= NARRATION_MAX,
                    "byte cap exceeded: {}",
                    text.len()
                );
                assert!(!text.is_empty());
            }
            other => panic!("expected narration, got {other:?}"),
        }
    }

    #[test]
    fn grid_emitted_plan_suppresses_the_transcript_twin_once() {
        let redactor = Redactor::new(vec![]);
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
        std::fs::write(
            &path,
            format!("{plan_line}\n{narration_line}\n{plan_line}\n"),
        )
        .unwrap();

        // One grid-emitted plan question is owed a transcript twin: the FIRST
        // transcript plan question is swallowed, later ones pass through
        // (grid detection missed ⇒ degraded fallback still works).
        let mut state = TranscriptState {
            suppress_plan_questions: 1,
            ..Default::default()
        };
        let mut events: Vec<ActivityEvent> = Vec::new();
        tail_transcript(
            &path,
            0,
            &mut |line| process_transcript_line(line, &redactor, &mut state),
            &mut |event| events.push(event),
        );
        assert_eq!(state.suppress_plan_questions, 0);
        // EXP-347: a suppressed twin only flushes once the plan picker is
        // ANSWERED — the emitter reads this flag to drop the publisher's
        // grid-picker reroute signal even while the viewport is scrolled.
        assert!(
            state.plan_twin_flushed,
            "the consumed twin flags a resolution"
        );
        match &events[..] {
            [ActivityEvent::Narration { text, .. }, ActivityEvent::Question { plan_mode, .. }] => {
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
                // Legacy narrations (pre-v2 clients string-match these)…
                ActivityEvent::narration(format!("{QUESTION_ANSWERED_PREFIX}Mushrooms, Cheese")),
                ActivityEvent::narration(format!("{QUESTION_ANSWERED_PREFIX}Large")),
                // …plus the EXP-249 semantic retirement of the whole ask.
                ActivityEvent::QuestionResolved {
                    id: None,
                    ask_id: Some("toolu_ask1".into()),
                    answers: Some(vec!["Mushrooms, Cheese".into(), "Large".into()]),
                    dismissed: None,
                    at: None,
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
        assert!(
            state.recent_grid_questions.is_empty(),
            "matches are consumed"
        );

        // The answers still flow (2 narrations + the semantic resolution).
        let events = process_transcript_line(&tool_result, &redactor, &mut state);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn a_hook_published_ask_swallows_its_twin_by_id() {
        // EXP-249: identity beats text — the hook published this ask, so its
        // post-answer transcript twin is dropped whatever the wording.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        state.hook_published_asks.insert("toolu_ask1".to_string());
        let (tool_use, tool_result) = answered_ask_lines();
        assert_eq!(
            process_transcript_line(&tool_use, &redactor, &mut state),
            vec![]
        );
        // …and the resolution retires the hook-published cards by askId.
        match &process_transcript_line(&tool_result, &redactor, &mut state)[..] {
            [_, _, ActivityEvent::QuestionResolved {
                ask_id, answers, ..
            }] => {
                assert_eq!(ask_id.as_deref(), Some("toolu_ask1"));
                assert_eq!(answers.as_ref().unwrap().len(), 2);
            }
            other => panic!("expected two narrations + a resolution, got {other:?}"),
        }
        assert!(state.hook_published_asks.is_empty(), "the ask is over");
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
            vec![
                ActivityEvent::narration(QUESTION_DISMISSED_NARRATION),
                ActivityEvent::QuestionResolved {
                    id: None,
                    ask_id: Some("toolu_ask1".into()),
                    answers: None,
                    dismissed: Some(true),
                    at: None,
                },
            ]
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
        assert_eq!(
            process_transcript_line(&generic, &redactor, &mut state),
            vec![]
        );
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
            [ActivityEvent::Narration { text, .. }, ActivityEvent::QuestionResolved { answers, .. }] =>
            {
                assert!(text.starts_with(QUESTION_ANSWERED_PREFIX));
                assert!(
                    !text.contains("expu_abcdef"),
                    "typed answer leaked a key: {text}"
                );
                let answers = answers.as_ref().expect("answers");
                assert!(
                    !answers[0].contains("expu_abcdef"),
                    "typed answer leaked a key: {answers:?}"
                );
            }
            other => panic!("expected an answer narration + resolution, got {other:?}"),
        }
    }

    // ── EXP-249: the hook-driven question pipeline ─────────────────────────

    use crate::hooks::HookQuestionOption;
    use crate::publisher::PublisherCmd;
    use std::sync::Mutex;

    /// Drain everything the state machine published.
    fn drained(rx: &flume::Receiver<PublisherCmd>) -> Vec<ActivityEvent> {
        rx.drain()
            .map(|cmd| match cmd {
                PublisherCmd::Activity(event) => event,
                other => panic!("the emitter only ever sends activity: {other:?}"),
            })
            .collect()
    }

    fn hook(kind: HookEventKind) -> HookEvent {
        HookEvent {
            context: crate::hooks::HookContext::default(),
            kind,
        }
    }

    fn ask_hook() -> HookEvent {
        hook(HookEventKind::QuestionsAsked {
            tool_use_id: Some("toolu_01".to_string()),
            questions: vec![
                HookQuestion {
                    question: "Which toppings do you want?".to_string(),
                    header: Some("Toppings".to_string()),
                    options: vec![
                        HookQuestionOption {
                            label: "Cheese".to_string(),
                            description: Some("classic".to_string()),
                        },
                        HookQuestionOption {
                            label: "Ham".to_string(),
                            description: None,
                        },
                        HookQuestionOption {
                            label: "Mushrooms".to_string(),
                            description: None,
                        },
                    ],
                    multi_select: true,
                },
                HookQuestion {
                    question: "Which size?".to_string(),
                    header: Some("Size".to_string()),
                    options: vec![
                        HookQuestionOption {
                            label: "Small".to_string(),
                            description: None,
                        },
                        HookQuestionOption {
                            label: "Large".to_string(),
                            description: None,
                        },
                    ],
                    multi_select: false,
                },
            ],
        })
    }

    /// The live toppings tab as claude paints it — the grid knows one row the
    /// hook never mentioned ("Type something").
    fn toppings_rows() -> Vec<String> {
        [
            "──────────────────────────────────────────",
            "←  ☐ Toppings  ☐ Size  ✔ Submit  →",
            "",
            "Which toppings do you want?",
            "",
            "❯ 1. [✔] Cheese",
            "  2. [ ] Ham",
            "  3. [✔] Mushrooms",
            "  4. [ ] Type something",
            "──────────────────────────────────────────",
            "  5. Chat about this",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    fn review_rows() -> Vec<String> {
        [
            "←  ☒ Toppings  ☒ Size  ✔ Submit  →",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    /// Paint a screen into a live emulator (the grid the watchers read).
    fn paint(term: &TermHandle, rows: &[String]) {
        let mut processor = vte::ansi::Processor::<vte::ansi::StdSyncHandler>::new();
        let mut bytes = b"\x1b[2J\x1b[H".to_vec();
        for row in rows {
            bytes.extend_from_slice(row.as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        processor.advance(&mut *term.lock(), &bytes);
    }

    #[test]
    fn the_ask_hook_publishes_every_question_up_front() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &Redactor::new(vec![]), &mut transcript);

        let events = drained(&rx);
        assert_eq!(events.len(), 2, "one question event per entry, immediately");
        match &events[0] {
            ActivityEvent::Question {
                text,
                options,
                multi_select,
                id,
                ask_id,
                index,
                total,
                header,
                ..
            } => {
                assert_eq!(text, "Which toppings do you want?");
                assert_eq!(id.as_deref(), Some("toolu_01#0"));
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
                assert_eq!((*index, *total), (Some(1), Some(2)));
                assert_eq!(header.as_deref(), Some("Toppings"));
                assert_eq!(*multi_select, Some(true));
                assert_eq!(
                    options
                        .iter()
                        .map(|o| (o.key.as_str(), o.label.as_str()))
                        .collect::<Vec<_>>(),
                    vec![("1", "Cheese"), ("2", "Ham"), ("3", "Mushrooms")]
                );
                assert_eq!(options[0].description.as_deref(), Some("classic"));
            }
            other => panic!("expected a question, got {other:?}"),
        }
        match &events[1] {
            ActivityEvent::Question { id, index, .. } => {
                assert_eq!(id.as_deref(), Some("toolu_01#1"));
                assert_eq!(*index, Some(2));
            }
            other => panic!("expected a question, got {other:?}"),
        }
        // The transcript twin of this ask is now swallowed by identity.
        assert!(transcript.hook_published_asks.contains("toolu_01"));
    }

    #[test]
    fn the_grid_augments_a_hook_question_with_the_row_only_the_tui_knows() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        let snapshot = question_picker::detect(&toppings_rows()).expect("picker");
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, options, .. }] => {
                assert_eq!(id.as_deref(), Some("toolu_01#0"), "the SAME card, replaced");
                assert_eq!(
                    options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
                    vec!["Cheese", "Ham", "Mushrooms", "Type something"]
                );
                // The hook's descriptions survive the re-emission.
                assert_eq!(options[0].description.as_deref(), Some("classic"));
            }
            other => panic!("expected one re-emitted question, got {other:?}"),
        }

        // A second sighting of the same rows changes nothing.
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        assert!(drained(&rx).is_empty());

        // The review tab publishes the ask's submit step.
        let review = question_picker::detect(&review_rows()).expect("review picker");
        assert!(steer.confirm_question_from_grid(&review, &sender, &redactor));
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id,
                ask_id,
                index,
                total,
                options,
                ..
            }] => {
                assert_eq!(id.as_deref(), Some("toolu_01#submit"));
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
                assert_eq!(
                    (*index, *total),
                    (None, None),
                    "the submit step has no index"
                );
                assert_eq!(options[0].label, "Submit answers");
            }
            other => panic!("expected the submit step, got {other:?}"),
        }
    }

    #[test]
    fn the_plan_hook_supplies_the_body_the_grid_supplies_the_options() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_plan".to_string()),
                plan: "## Plan\n1. Do the thing".to_string(),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        // Nothing is published until the picker confirms on screen.
        assert!(drained(&rx).is_empty());
        assert_eq!(
            transcript.suppress_plan_questions, 1,
            "the twin is pre-paid"
        );

        let published = steer.confirm_plan_from_grid(
            vec![
                QuestionOption::new("Yes, auto-accept edits", "1"),
                QuestionOption::new("Yes, manually approve edits", "2"),
            ],
            &sender,
        );
        assert!(published);
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                text,
                id,
                plan_mode,
                options,
                ..
            }] => {
                assert_eq!(text, "## Plan\n1. Do the thing");
                assert_eq!(id.as_deref(), Some("toolu_plan"));
                assert_eq!(*plan_mode, Some(true));
                assert_eq!(options.len(), 2);
            }
            other => panic!("expected the plan question, got {other:?}"),
        }

        // Resolution keeps the legacy narration AND retires the card by id.
        steer.resolve_plan(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Narration { text, .. }, ActivityEvent::QuestionResolved { id, .. }] => {
                assert_eq!(text, PLAN_RESOLVED_NARRATION);
                assert_eq!(id.as_deref(), Some("toolu_plan"));
            }
            other => panic!("expected narration + resolution, got {other:?}"),
        }
        assert!(!steer.has_pending_question());
    }

    #[test]
    fn a_plan_the_grid_never_confirms_is_published_as_narration() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: None,
                plan: "## Plan\nship it".to_string(),
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        // Not yet due.
        steer.plan_timeout(&sender);
        assert!(drained(&rx).is_empty());

        steer.plan.as_mut().unwrap().seen = Instant::now() - PLAN_GRID_CONFIRM;
        steer.plan_timeout(&sender);
        assert_eq!(
            drained(&rx),
            vec![ActivityEvent::narration("## Plan\nship it")]
        );
        // …and only once.
        steer.plan_timeout(&sender);
        assert!(drained(&rx).is_empty());
        // Transcript progress clears the degraded plan so "needs input"
        // cannot stick for the rest of the session.
        steer.observe_published(&ActivityEvent::narration("moving on"));
        assert!(!steer.has_pending_question());
    }

    #[test]
    fn subagent_edges_share_one_card_across_the_two_hook_ids() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::SubagentDispatched {
                tool_use_id: Some("toolu_task".to_string()),
                description: Some("Map the steer crate".to_string()),
                subagent_type: Some("explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("agent_01".to_string()),
                agent_type: Some("explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Subagent {
                id,
                agent_type,
                status,
                detail,
                ..
            }, ActivityEvent::Subagent {
                id: done_id,
                agent_type: done_type,
                status: done,
                detail: done_detail,
                ..
            }] => {
                assert_eq!(id, "toolu_task");
                assert_eq!(agent_type, "explore");
                assert_eq!(*status, SubagentStatus::Started);
                assert_eq!(detail.as_deref(), Some("Map the steer crate"));
                // The start bound agent_01 to the dispatch — no second card,
                // and the stop (whose payload carries no agent_type) restates
                // what the card was published with (EXP-350).
                assert_eq!(done_id, "toolu_task");
                assert_eq!(done_type, "explore");
                assert_eq!(*done, SubagentStatus::Completed);
                assert_eq!(done_detail.as_deref(), Some("Map the steer crate"));
            }
            other => panic!("expected started + completed on one card, got {other:?}"),
        }
    }

    #[test]
    fn sidechain_tools_land_on_the_dispatch_card() {
        let mut subagents = Subagents::default();
        subagents.dispatch(
            "toolu_task".to_string(),
            "explore".to_string(),
            Some("Map the steer crate".to_string()),
        );
        subagents.started("agent_01", Some("explore"));
        let tool = ActivityEvent::Tool {
            name: "Grep".to_string(),
            detail: Some("fn main".to_string()),
            subagent_id: Some("agent_01".to_string()),
            at: None,
        };
        match attribute_to_card(tool, &subagents) {
            ActivityEvent::Tool { subagent_id, .. } => {
                assert_eq!(subagent_id.as_deref(), Some("toolu_task"));
            }
            other => panic!("expected a tool, got {other:?}"),
        }
        // An id with no alias (hookless / pre-alias tick) passes through raw.
        let orphan = ActivityEvent::Tool {
            name: "Read".to_string(),
            detail: None,
            subagent_id: Some("agent_99".to_string()),
            at: None,
        };
        match attribute_to_card(orphan, &subagents) {
            ActivityEvent::Tool { subagent_id, .. } => {
                assert_eq!(subagent_id.as_deref(), Some("agent_99"));
            }
            other => panic!("expected a tool, got {other:?}"),
        }
    }

    #[test]
    fn subagent_stop_without_dispatch_keeps_type_from_start() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        // No Task dispatch (hookless PreToolUse) — the start cards the raw id.
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("agent_01".to_string()),
                agent_type: Some("review".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Subagent { .. }, ActivityEvent::Subagent {
                id,
                agent_type,
                status,
                ..
            }] => {
                assert_eq!(id, "agent_01");
                assert_eq!(agent_type, "review");
                assert_eq!(*status, SubagentStatus::Completed);
            }
            other => panic!("expected started + completed, got {other:?}"),
        }
    }

    #[test]
    fn internal_agent_lifecycle_noise_is_dropped() {
        // claude's internal helper agents (summarizers etc.) fire
        // SubagentStart/Stop too — type-less, never dispatched, no sidechain.
        // They used to mint one fallback "agent" card each (EXP-356).
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("internal_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("internal_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        // A stop for an id nothing ever carded is equally silent.
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("internal_02".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "internal agents publish nothing");
    }

    #[test]
    fn a_typeless_start_never_steals_a_typed_dispatch() {
        let mut subagents = Subagents::default();
        subagents.dispatch("toolu_explore".to_string(), "Explore".to_string(), None);
        // An internal agent's type-less start must not claim the card…
        assert_eq!(subagents.started("internal_01", None), None);
        // …so the real start still binds it.
        assert_eq!(
            subagents.started("agent_e", Some("Explore")).as_deref(),
            Some("toolu_explore")
        );
    }

    #[test]
    fn sidechain_meta_absorbs_identity_and_cards_once() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        // No dispatch fired (e.g. the sidecar missed it) — the meta.json is
        // the identity source: card under the dispatch's tool_use_id.
        let published = steer.subagents.absorb_meta(
            "ac66d",
            Some("toolu_meta".to_string()),
            "Explore".to_string(),
            Some("Find the flow".to_string()),
        );
        assert_eq!(
            published,
            Some((
                "toolu_meta".to_string(),
                "Explore".to_string(),
                Some("Find the flow".to_string())
            ))
        );
        // Sidechain tools group under the card…
        let tool = ActivityEvent::Tool {
            name: "Grep".to_string(),
            detail: None,
            subagent_id: Some("ac66d".to_string()),
            at: None,
        };
        match attribute_to_card(tool, &steer.subagents) {
            ActivityEvent::Tool { subagent_id, .. } => {
                assert_eq!(subagent_id.as_deref(), Some("toolu_meta"));
            }
            other => panic!("expected a tool, got {other:?}"),
        }
        // …a late SubagentStart for the absorbed agent publishes nothing…
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("ac66d".to_string()),
                agent_type: Some("Explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "already carded by the meta");
        // …and the stop edge completes the SAME card with the real label.
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("ac66d".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Subagent {
                id,
                agent_type,
                status,
                detail,
                ..
            }] => {
                assert_eq!(id, "toolu_meta");
                assert_eq!(agent_type, "Explore");
                assert_eq!(*status, SubagentStatus::Completed);
                assert_eq!(detail.as_deref(), Some("Find the flow"));
            }
            other => panic!("expected one completed card, got {other:?}"),
        }
    }

    #[test]
    fn sidechain_meta_repairs_a_dispatch_card_without_republishing() {
        let mut subagents = Subagents::default();
        // A dispatch whose payload carried no subagent_type stored the
        // fallback label.
        subagents.dispatch(
            "toolu_x".to_string(),
            SUBAGENT_TYPE_FALLBACK.to_string(),
            None,
        );
        let published = subagents.absorb_meta(
            "ac66d",
            Some("toolu_x".to_string()),
            "Explore".to_string(),
            Some("Find the flow".to_string()),
        );
        assert_eq!(published, None, "the dispatch already published the card");
        assert_eq!(subagents.card_id("ac66d"), "toolu_x");
        let meta = subagents.card_meta("toolu_x").expect("meta");
        assert_eq!(meta.agent_type, "Explore");
        assert_eq!(meta.detail.as_deref(), Some("Find the flow"));
        // The consumed dispatch can no longer be claimed by a foreign start.
        assert_eq!(subagents.started("internal_01", None), None);
    }

    /// The real background-agent lifecycle on claude ≥2.1.220 (EXP-360):
    /// dispatch hook → immediate async launch ack → (no SubagentStop, ever) →
    /// task-notification user entry. The notification must complete the
    /// dispatch's card exactly once.
    #[test]
    fn a_task_notification_completes_a_background_subagent_card() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::SubagentDispatched {
                tool_use_id: Some("toolu_bg".to_string()),
                description: Some("Explore glassy design system".to_string()),
                subagent_type: Some("Explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        drained(&rx); // the Started card

        // The launch ack (shape captured from a real 2.1.220 transcript).
        let ack = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_bg",
                "content": [{ "type": "text", "text": "Async agent launched successfully." }]
            }]},
            "toolUseResult": {
                "isAsync": true,
                "status": "async_launched",
                "agentId": "a3c7e29c06c05ef9b",
                "description": "Explore glassy design system"
            }
        })
        .to_string();
        assert!(
            process_transcript_line(&ack, &redactor, &mut transcript).is_empty(),
            "a launch ack is internal metadata, never published"
        );
        assert_eq!(
            transcript.task_events,
            vec![TaskEvent::Launched {
                agent_id: "a3c7e29c06c05ef9b".to_string(),
                tool_use_id: "toolu_bg".to_string(),
            }]
        );
        for event in std::mem::take(&mut transcript.task_events) {
            if let TaskEvent::Launched {
                agent_id,
                tool_use_id,
            } = event
            {
                steer.subagents.bind_launch(&agent_id, &tool_use_id);
            }
        }

        // …the end lands ONLY as a task-notification entry.
        let notification = serde_json::json!({
            "type": "user",
            "origin": { "kind": "task-notification" },
            "promptSource": "system",
            "message": { "role": "user", "content": "<task-notification>\n<task-id>a3c7e29c06c05ef9b</task-id>\n<tool-use-id>toolu_bg</tool-use-id>\n<status>failed</status>\n<summary>Agent failed: API Error 529</summary>\n</task-notification>" }
        })
        .to_string();
        assert!(
            process_transcript_line(&notification, &redactor, &mut transcript).is_empty(),
            "a task notification is injected content, never a user bubble"
        );
        let ended = std::mem::take(&mut transcript.task_events);
        assert_eq!(
            ended,
            vec![TaskEvent::Ended {
                agent_id: Some("a3c7e29c06c05ef9b".to_string()),
                tool_use_id: Some("toolu_bg".to_string()),
            }]
        );
        let (id, agent_type, detail) = steer
            .subagents
            .complete(Some("a3c7e29c06c05ef9b"), Some("toolu_bg"))
            .expect("the notification completes the card");
        assert_eq!(id, "toolu_bg");
        assert_eq!(agent_type, "Explore");
        assert_eq!(detail.as_deref(), Some("Explore glassy design system"));

        // A repeat notification (the agent was resumed) and a late hook stop
        // both stay silent — the card completed once.
        assert_eq!(
            steer
                .subagents
                .complete(Some("a3c7e29c06c05ef9b"), Some("toolu_bg")),
            None
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("a3c7e29c06c05ef9b".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "no doubled completion edge");
    }

    #[test]
    fn launch_acks_repair_a_swapped_same_type_binding() {
        let mut subagents = Subagents::default();
        subagents.dispatch("toolu_a".to_string(), "Explore".to_string(), None);
        subagents.dispatch("toolu_b".to_string(), "Explore".to_string(), None);
        // The type-matching dance can only guess between same-type dispatches
        // — here it guesses wrong…
        assert_eq!(
            subagents.started("agent_1", Some("Explore")).as_deref(),
            Some("toolu_a")
        );
        // …and the acks state the REAL pairing.
        subagents.bind_launch("agent_1", "toolu_b");
        subagents.bind_launch("agent_2", "toolu_a");
        assert_eq!(subagents.card_id("agent_1"), "toolu_b");
        assert_eq!(subagents.card_id("agent_2"), "toolu_a");
        // An ack for a dispatch nothing carded binds nothing (alias must keep
        // implying "a card exists").
        subagents.bind_launch("agent_9", "toolu_unknown");
        assert!(!subagents.knows_agent("agent_9"));
    }

    #[test]
    fn a_foreground_subagent_tool_result_completes_a_missed_stop() {
        let redactor = Redactor::new(vec![]);
        let mut subagents = Subagents::default();
        let mut transcript = TranscriptState::default();
        subagents.dispatch(
            "toolu_fg".to_string(),
            "review".to_string(),
            Some("Check the diff".to_string()),
        );
        // A foreground subagent's result carries the agentId but no async
        // marker — the agent is done even if SubagentStop never arrived.
        let result = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_fg",
                "content": "All good."
            }]},
            "toolUseResult": { "agentId": "agent_fg", "content": [] }
        })
        .to_string();
        assert!(process_transcript_line(&result, &redactor, &mut transcript).is_empty());
        // The end resolves via the dispatch's own tool_use_id — never the
        // agent id, so an agent-flavored result for an uncarded id (a
        // TaskOutput poll, say) can't complete a live card early.
        assert_eq!(
            transcript.task_events,
            vec![TaskEvent::Ended {
                agent_id: None,
                tool_use_id: Some("toolu_fg".to_string()),
            }]
        );
        let (id, agent_type, detail) = subagents
            .complete(None, Some("toolu_fg"))
            .expect("the result completes the dispatch card");
        assert_eq!(id, "toolu_fg");
        assert_eq!(agent_type, "review");
        assert_eq!(detail.as_deref(), Some("Check the diff"));
    }

    #[test]
    fn ordinary_tool_results_stay_out_of_the_task_channel() {
        let redactor = Redactor::new(vec![]);
        let mut transcript = TranscriptState::default();
        // No agentId in toolUseResult ⇒ not a subagent result.
        let bash = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_bash",
                "content": "ok"
            }]},
            "toolUseResult": { "stdout": "ok", "stderr": "" }
        })
        .to_string();
        assert!(process_transcript_line(&bash, &redactor, &mut transcript).is_empty());
        assert!(transcript.task_events.is_empty());
        // A tag-less notification (nothing to correlate) pushes nothing.
        let bare = serde_json::json!({
            "type": "user",
            "origin": { "kind": "task-notification" },
            "message": { "content": "<task-notification>agent done</task-notification>" }
        })
        .to_string();
        assert!(process_transcript_line(&bare, &redactor, &mut transcript).is_empty());
        assert!(transcript.task_events.is_empty());
        // And an end for ids nothing ever carded completes nothing.
        let mut subagents = Subagents::default();
        assert_eq!(subagents.complete(Some("agent_x"), Some("toolu_x")), None);
    }

    #[test]
    fn concurrent_fanout_binds_starts_by_type() {
        let mut subagents = Subagents::default();
        subagents.dispatch("toolu_explore".to_string(), "explore".to_string(), None);
        subagents.dispatch("toolu_review".to_string(), "review".to_string(), None);
        // The review start arrives first — it must bind to the review
        // dispatch, not FIFO-steal the explore card.
        assert_eq!(
            subagents.started("agent_r", Some("review")).as_deref(),
            Some("toolu_review")
        );
        assert_eq!(
            subagents.started("agent_e", Some("explore")).as_deref(),
            Some("toolu_explore")
        );
        assert_eq!(subagents.card_id("agent_r"), "toolu_review");
        assert_eq!(subagents.card_id("agent_e"), "toolu_explore");
    }

    #[test]
    fn dispatch_without_tool_use_id_still_cards() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::SubagentDispatched {
                tool_use_id: None,
                description: Some("Audit the tests".to_string()),
                subagent_type: Some("review".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("agent_01".to_string()),
                agent_type: Some("review".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Subagent { id, detail, .. }, ActivityEvent::Subagent {
                id: done_id,
                agent_type: done_type,
                status: done,
                ..
            }] => {
                assert_eq!(id, "task-1");
                assert_eq!(detail.as_deref(), Some("Audit the tests"));
                // The start bound to the synthesized card — no second card.
                assert_eq!(done_id, "task-1");
                assert_eq!(done_type, "review");
                assert_eq!(*done, SubagentStatus::Completed);
            }
            other => panic!("expected started + completed on the task-1 card, got {other:?}"),
        }
    }

    #[test]
    fn task_headline_suppressed_when_hooks_wired_and_descriptive_when_not() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "Task",
                  "input": {
                      "description": "Map the steer crate",
                      "prompt": "Read every file under crates/steer and report...",
                      "subagent_type": "explore",
                  } },
            ]}
        })
        .to_string();
        // Hooks wired: the subagent card already represents the call.
        let mut hooked = TranscriptState {
            suppress_task_headlines: true,
            ..TranscriptState::default()
        };
        assert!(process_transcript_line(&line, &redactor, &mut hooked).is_empty());
        // Hookless: the Task row is the only subagent visibility — it carries
        // the description (never the prompt).
        let events = parse_transcript_line(&line, &redactor);
        assert_eq!(
            events,
            vec![ActivityEvent::tool(
                "Task",
                Some("Map the steer crate".into())
            )]
        );
        let joined = format!("{events:?}");
        assert!(
            !joined.contains("Read every file"),
            "task prompt leaked: {joined}"
        );
    }

    #[test]
    fn a_permission_notification_publishes_and_holds_attention() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Bash".to_string(),
                tool: Some("Bash".to_string()),
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, detail, .. }] => {
                assert_eq!(tool, "Bash");
                assert!(detail.as_ref().unwrap().contains("permission"));
            }
            other => panic!("expected a permission event, got {other:?}"),
        }
        assert!(steer.attention.is_some(), "the session is blocked");
        steer.observe_published(&ActivityEvent::tool("Bash", None));
        assert!(steer.attention.is_none(), "progress clears it");
    }

    #[test]
    fn subagent_activity_retires_a_stale_idle_nudge() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        // Claude handed work to background subagents, its turn ended, and the
        // idle nudge fired — the badge would flip to "needs input"…
        steer.apply_hook(
            hook(HookEventKind::Idle {
                message: "Claude is waiting for your input".to_string(),
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(steer.attention == Some(Attention::Idle), "nudge parked it");
        // …but a subagent lifecycle edge proves delegated work is running:
        // the main transcript stays silent through the fan-out, so the edge
        // itself must clear the flag (EXP-355).
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(
            steer.attention.is_none(),
            "delegated work is live — not parked"
        );
        drained(&rx);
    }

    #[test]
    fn a_permission_block_survives_subagent_activity() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Bash".to_string(),
                tool: Some("Bash".to_string()),
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(steer.attention == Some(Attention::Permission));
        // A parallel/background subagent can stream while the main agent
        // genuinely waits on an approval — the block must NOT clear (EXP-355).
        steer.apply_hook(
            hook(HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".to_string()),
                agent_type: None,
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(
            steer.attention == Some(Attention::Permission),
            "an approval block outlives subagent chatter"
        );
        drained(&rx);
    }

    /// A `write_input` that records keystrokes and repaints the grid when the
    /// TUI would move on.
    fn recording_input(
        term: TermHandle,
        next_screen: Option<Vec<String>>,
        repaint_on: &'static str,
    ) -> (InputHook, Arc<Mutex<Vec<String>>>) {
        let keys = Arc::new(Mutex::new(Vec::new()));
        let recorded = keys.clone();
        let hook: InputHook = Arc::new(move |bytes| {
            let key = String::from_utf8_lossy(bytes).to_string();
            let repaint = key == repaint_on;
            recorded.lock().unwrap().push(key);
            if repaint {
                paint(&term, next_screen.as_deref().unwrap_or(&[]));
            }
        });
        (hook, keys)
    }

    #[test]
    fn a_remote_answer_is_injected_once_and_acked_when_the_grid_moves() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &toppings_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        let snapshot = question_picker::detect(&screen_lines(&term)).expect("picker");
        steer.confirm_question_from_grid(&snapshot, &sender, &redactor);
        drained(&rx);

        // multiSelect: digits TOGGLE, so only the differences are injected —
        // Cheese off, Ham on, Mushrooms already on — then Tab advances.
        let (write_input, keys) = recording_input(term.clone(), Some(review_rows()), "\t");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#0".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["2".to_string(), "3".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1", "2", "\t"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, ask_id, .. }] => {
                assert_eq!(id, "toolu_01#0");
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
            }
            other => panic!("expected an answer_ack, got {other:?}"),
        }

        // A duplicate (two taps, a re-delivered frame, or a re-tap from a
        // viewer that rejoined after the ack replayed) never injects again —
        // but it IS re-acked, so that viewer's card locks instead of timing
        // out and rolling the stepper back (EXP-374).
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#0".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["2".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(keys.lock().unwrap().len(), 3, "no second injection");
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, ask_id, .. }] => {
                assert_eq!(id, "toolu_01#0");
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
            }
            other => panic!("expected a re-ack, got {other:?}"),
        }
    }

    #[test]
    fn an_answer_for_a_tab_that_is_not_up_is_refused() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &toppings_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        // Question 2 is published, but the grid is showing question 1 — a
        // TRANSIENT state (the tab advances any moment), so the answer is
        // retryable (EXP-334).
        let (write_input, keys) = recording_input(term.clone(), None, "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#1".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["2".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Retry);
        assert!(keys.lock().unwrap().is_empty(), "nothing may be injected");
        assert!(drained(&rx).is_empty(), "and nothing is acked");

        // An unknown id can never become answerable — settled, not retried.
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_99#0".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert!(keys.lock().unwrap().is_empty());
    }

    #[test]
    fn a_single_select_answer_sends_exactly_one_digit() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        let size_rows: Vec<String> = [
            "←  ☒ Toppings  ☐ Size  ✔ Submit  →",
            "",
            "Which size?",
            "",
            "❯ 1. Small",
            "  2. Large",
            "  3. Type something",
            "──────────────────────────────────────────",
            "  4. Chat about this",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect();
        paint(&term, &size_rows);

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        // The digit both selects AND submits a single-select question, so the
        // TUI auto-advances — no Tab, no Enter.
        let (write_input, keys) = recording_input(term.clone(), Some(review_rows()), "2");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#1".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["2".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["2"]);
        assert!(matches!(
            drained(&rx)[..],
            [ActivityEvent::AnswerAck { .. }]
        ));
    }

    #[test]
    fn an_unmatched_picker_while_a_multi_question_ask_is_pending_publishes_the_submit_step() {
        // EXP-275: the review screen's copy varies by claude version and its
        // tab bar can be mis-anchored — but while a multi-question ask is
        // pending, an ask-shaped picker whose text matches none of the hook's
        // questions can only be the review step.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        let snapshot = QuestionSnapshot {
            text: "All set — send these answers?".to_string(),
            options: vec![
                QuestionOption::new("Submit answers", "1"),
                QuestionOption::new("Cancel", "2"),
            ],
            multi_select: false,
            checked: vec![false, false],
            tabs: Vec::new(),
            current_tab: None,
            review: false,
        };
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id,
                ask_id,
                text,
                options,
                ..
            }] => {
                assert_eq!(id.as_deref(), Some("toolu_01#submit"));
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
                assert_eq!(text, "All set — send these answers?");
                assert_eq!(options[0].label, "Submit answers");
            }
            other => panic!("expected the submit step, got {other:?}"),
        }

        // The dedupe guard holds across re-sightings.
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn an_unmatched_sliver_without_a_submit_row_is_not_the_submit_step() {
        // EXP-394 edge: overflow can leave fewer than the match floor's 12
        // normalized chars of a REAL question visible — unmatched, but not
        // the review step. Without a submit row nothing is published (the
        // hook's cards are already live; parked answers retry) instead of a
        // phantom `#submit` card.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        let snapshot = QuestionSnapshot {
            text: "want?".to_string(),
            options: vec![
                QuestionOption::new("Cheese", "1"),
                QuestionOption::new("Ham", "2"),
            ],
            multi_select: false,
            checked: vec![false, false],
            tabs: Vec::new(),
            current_tab: None,
            review: false,
        };
        assert!(
            steer.confirm_question_from_grid(&snapshot, &sender, &redactor),
            "handled — the legacy id-less publication must not fire either"
        );
        assert!(drained(&rx).is_empty(), "no card at all, no submit step");
        assert!(
            !steer.ask.as_ref().unwrap().submit_published,
            "the real review step can still publish later"
        );
    }

    #[test]
    fn a_submit_answer_is_accepted_by_text_match_when_review_is_not_detected() {
        // A review screen whose ✔-carrying answer-summary row steals the
        // tab-bar anchor: `review` comes out false, but the submit card was
        // published with the grid's own text, so the text match must carry
        // the remote answer through (EXP-275).
        let review_with_summary: Vec<String> = [
            "←  ☒ Toppings  ☒ Size  ✔ Submit  →",
            "",
            "Review your answers",
            "",
            " ✔ Cheese  ✔ Mushrooms",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect();
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &review_with_summary);

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        let snapshot = question_picker::detect(&screen_lines(&term)).expect("picker");
        assert!(!snapshot.review, "the fixture must exercise the lost flag");
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        drained(&rx);

        // Submitting empties the picker (the ask is over).
        let (write_input, keys) = recording_input(term.clone(), Some(Vec::new()), "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#submit".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["1".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "toolu_01#submit"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    #[test]
    fn a_footerless_review_screen_publishes_the_submit_step_and_takes_the_answer() {
        // EXP-374: claude ≥2.1.220 paints the review step WITHOUT the
        // "Enter to select" footer (and without the rule + "Chat about
        // this"). The old footer anchor made detection fail there, so the
        // `#submit` card never published and every remote multi-question ask
        // stranded on the review screen.
        let footerless_review: Vec<String> = [
            "←  ☒ Toppings  ☒ Size  ✔ Submit  →",
            "",
            "Review your answers",
            "",
            " ● Which toppings do you want?",
            "   → Cheese, Mushrooms",
            " ● Which size?",
            "   → Small",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect();
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &footerless_review);

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        let snapshot = question_picker::detect(&screen_lines(&term)).expect("picker");
        assert!(snapshot.review, "the fully answered bar flags the review step");
        assert!(steer.confirm_question_from_grid(&snapshot, &sender, &redactor));
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, options, .. }] => {
                assert_eq!(id.as_deref(), Some("toolu_01#submit"));
                assert_eq!(options[0].label, "Submit answers");
            }
            other => panic!("expected the submit step, got {other:?}"),
        }

        // Submitting empties the picker (the ask is over).
        let (write_input, keys) = recording_input(term.clone(), Some(Vec::new()), "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#submit".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["1".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "toolu_01#submit"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    /// The v2.1.220 plan picker as painted live (see plan_picker.rs — digits
    /// only MOVE the cursor on it, Enter activates).
    fn plan_rows() -> Vec<String> {
        [
            "Ready to code?",
            " Here is Claude's plan:",
            "## Plan",
            "",
            " Claude has written up a plan and is ready to execute. Would you like to proceed?",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
            "   3. No, refine with Ultraplan on Claude Code on the web",
            "   4. Tell Claude what to change",
            " ctrl+g to edit in Vim",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    /// Hook + grid-confirm a pending plan so `toolu_plan` is live/answerable.
    fn arm_plan(
        steer: &mut SteerState,
        term: &TermHandle,
        sender: &ActivitySender,
        rx: &flume::Receiver<PublisherCmd>,
    ) {
        let redactor = Redactor::new(vec![]);
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_plan".to_string()),
                plan: "## Plan".to_string(),
            }),
            sender,
            &redactor,
            &mut transcript,
        );
        let snapshot = plan_picker::detect(&screen_lines(term)).expect("plan picker");
        assert!(steer.confirm_plan_from_grid(snapshot.options, sender));
        drained(rx);
    }

    #[test]
    fn a_plan_answer_presses_enter_when_the_digit_alone_does_not_submit() {
        // Current claude plan pickers (observed v2.1.220): a digit only moves
        // the cursor — without the follow-up Enter the picker stayed up
        // forever and remote plan answers were never acked (EXP-334).
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &plan_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        arm_plan(&mut steer, &term, &sender, &rx);

        // The grid only clears when Enter lands (digit = cursor move).
        let (write_input, keys) = recording_input(term.clone(), Some(Vec::new()), "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_plan".to_string(),
                ask_id: None,
                keys: vec!["2".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["2", "\r"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "toolu_plan"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    #[test]
    fn a_plan_answer_sends_no_enter_when_the_digit_already_submits() {
        // Legacy claude plan pickers submitted on the digit — the Enter probe
        // must notice the picker left and NOT chase it with a stray Enter.
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &plan_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        arm_plan(&mut steer, &term, &sender, &rx);

        let (write_input, keys) = recording_input(term.clone(), Some(Vec::new()), "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_plan".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        assert!(matches!(
            drained(&rx)[..],
            [ActivityEvent::AnswerAck { .. }]
        ));
    }

    #[test]
    fn an_answer_that_beats_the_picker_paint_is_retried_and_lands() {
        // The hook publishes every ask step BEFORE the TUI paints the picker —
        // a steerer's instant tap used to be dropped silently, and the mobile
        // stepper rolled back to the answered step seconds later (EXP-334).
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &[] as &[String]); // nothing painted yet

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        // Keep only Cheese: Mushrooms' pre-ticked box must be toggled OFF.
        let answer = RemoteAnswer {
            question_id: "toolu_01#0".to_string(),
            ask_id: Some("toolu_01".to_string()),
            keys: vec!["1".to_string()],
        };
        let (write_input, keys) = recording_input(term.clone(), Some(review_rows()), "\t");
        let outcome = steer.handle_answer(&answer, &term, &write_input, &sender);
        assert_eq!(outcome, AnswerAttempt::Retry, "no picker yet — retryable");
        assert!(keys.lock().unwrap().is_empty());
        assert!(drained(&rx).is_empty());

        // Next tick: the picker painted — the SAME parked answer now lands.
        paint(&term, &toppings_rows());
        let outcome = steer.handle_answer(&answer, &term, &write_input, &sender);
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["3", "\t"]);
        assert!(matches!(
            drained(&rx)[..],
            [ActivityEvent::AnswerAck { .. }]
        ));
    }

    #[test]
    fn stop_clears_a_pending_ask_and_retires_its_cards() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &toppings_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        // The turn ended with the ask still marked pending (the transcript
        // flush was missed): the safety net retires the cards and unpins
        // "needs input" instead of sticking forever (EXP-275).
        steer.apply_hook(
            hook(HookEventKind::Stop),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved {
                id,
                ask_id,
                answers,
                dismissed,
                ..
            }] => {
                assert_eq!(*id, None);
                assert_eq!(ask_id.as_deref(), Some("toolu_01"));
                assert_eq!((answers, dismissed), (&None, &None), "neutral retire");
            }
            other => panic!("expected one ask resolution, got {other:?}"),
        }
        assert!(!steer.has_pending_question());
        // EXP-347: the Stop doubles as a resolution signal — one consuming
        // read clears the publisher's grid-picker flag without a grid tick.
        assert!(steer.take_resolution());
        assert!(!steer.take_resolution(), "consuming read");

        // The retired cards are no longer answerable — settled, not retried.
        let (write_input, keys) = recording_input(term.clone(), None, "\t");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#0".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["2".to_string()],
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert!(keys.lock().unwrap().is_empty());
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn stop_clears_a_pending_plan() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_plan".to_string()),
                plan: "## Plan".to_string(),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.confirm_plan_from_grid(vec![QuestionOption::new("Yes", "1")], &sender);
        drained(&rx);

        steer.apply_hook(
            hook(HookEventKind::Stop),
            &sender,
            &redactor,
            &mut transcript,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, ask_id, .. }] => {
                assert_eq!(id.as_deref(), Some("toolu_plan"));
                assert_eq!(*ask_id, None);
            }
            other => panic!("expected the plan resolution, got {other:?}"),
        }
        assert!(!steer.has_pending_question());

        // An UNPUBLISHED plan (the picker never confirmed) clears silently.
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_plan2".to_string()),
                plan: "## Plan 2".to_string(),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        drained(&rx);
        steer.apply_hook(
            hook(HookEventKind::Stop),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "nothing was on the wire to retire");
        assert!(!steer.has_pending_question());
    }

    #[test]
    fn resolutions_flag_a_consuming_take_resolution() {
        // EXP-347: every path the emitter learns of a picker resolution
        // through must raise the flag that clears the publisher's grid-picker
        // reroute signal — the grid recompute alone never runs while the
        // viewport is scrolled, and a stale `true` Escs (cancels) a turn the
        // desktop user already started by answering locally.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        assert!(!steer.take_resolution());

        // A transcript-flushed ask resolution (observe_published).
        steer.observe_published(&ActivityEvent::QuestionResolved {
            id: None,
            ask_id: Some("toolu_ask".to_string()),
            answers: None,
            dismissed: Some(true),
            at: None,
        });
        assert!(steer.take_resolution());
        assert!(!steer.take_resolution(), "consuming read");

        // The plan picker leaving the grid (Transition::Resolved).
        steer.resolve_plan(&sender);
        assert!(steer.take_resolution());
        drained(&rx);
    }

    #[test]
    fn a_permission_notification_while_a_picker_is_pending_is_swallowed() {
        // Claude fires "needs your permission" notifications for
        // AskUserQuestion too — the picker's own nudge must not become a
        // "blocked on approval" card (EXP-275).
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);

        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use AskUserQuestion".to_string(),
                tool: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "no permission card");
        assert!(
            steer.attention.is_none(),
            "the picker already carries needs-input"
        );
    }

    #[test]
    fn bypass_permissions_downgrades_permission_prompts_to_idle() {
        // With --dangerously-skip-permissions a real permission prompt cannot
        // happen — whatever notified, claude is merely parked on input.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState {
            bypass_permissions: true,
            ..SteerState::default()
        };
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Bash".to_string(),
                tool: Some("Bash".to_string()),
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(drained(&rx).is_empty(), "no permission card in bypass mode");
        assert!(
            steer.attention == Some(Attention::Idle),
            "parked on input, not blocked"
        );
    }

    #[test]
    fn sidechain_lines_publish_attributed_tool_headlines_only() {
        let redactor = Redactor::new(vec![]);
        let line = serde_json::json!({
            "type": "assistant",
            "isSidechain": true,
            "agentId": "agent_01",
            "message": { "content": [
                { "type": "text", "text": "Looking around." },
                { "type": "tool_use", "name": "Grep", "input": { "pattern": "fn main" } },
            ]}
        })
        .to_string();
        assert_eq!(
            parse_sidechain_line(&line, "fallback", &redactor),
            vec![ActivityEvent::Tool {
                name: "Grep".into(),
                detail: Some("fn main".into()),
                subagent_id: Some("agent_01".into()),
                at: None,
            }]
        );
        // Without the entry-level agentId, the file name carries the identity.
        let line = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "name": "Read", "input": { "file_path": "a.rs" } },
            ]}
        })
        .to_string();
        match &parse_sidechain_line(&line, "agent_from_file", &redactor)[..] {
            [ActivityEvent::Tool { subagent_id, .. }] => {
                assert_eq!(subagent_id.as_deref(), Some("agent_from_file"));
            }
            other => panic!("expected one attributed tool, got {other:?}"),
        }
    }

    #[test]
    fn sidechain_discovery_finds_both_claude_layouts() {
        let dir = std::env::temp_dir().join(format!(
            "exp-sidechains-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = dir.join("sess-1").join("subagents").join("workflows");
        std::fs::create_dir_all(&nested).unwrap();
        // The main transcript is never a sidechain.
        std::fs::write(dir.join("sess-1.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.join("agent-flat.jsonl"), "{}\n").unwrap();
        std::fs::write(nested.join("agent-nested.jsonl"), "{}\n").unwrap();

        let after = SystemTime::now() - Duration::from_secs(60);
        let found = sidechain_transcripts(&dir, after);
        let names: HashSet<String> = found
            .iter()
            .filter_map(|path| sidechain_agent_id(path))
            .collect();
        assert_eq!(
            names,
            HashSet::from(["flat".to_string(), "nested".to_string()])
        );
        assert_eq!(
            newest_transcript(&dir, after),
            Some(dir.join("sess-1.jsonl"))
        );
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
