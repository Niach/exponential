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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use regex::Regex;
use serde_json::Value;
use terminal::{display_offset, screen_lines, scroll_to_bottom, TermHandle};

use crate::frames::{ActivityEvent, CompactionPhase, QuestionOption, SubagentStatus};
use crate::hooks::{HookEvent, HookEventKind, HookQuestion};
use crate::login_picker::{self, LoginPhase, LoginWatcher};
use crate::permission_picker::{self, PermissionPickerWatcher, PermissionSnapshot};
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
/// Question text used to share the narration budget, but an ExitPlanMode plan
/// rides here and real plans clear 16KiB (EXP-691) — the relay's
/// `question.text` cap is raised in lockstep (`protocol.ts`). Anything larger
/// still truncates, with an explicit marker ([`truncate_marked`]) instead of
/// a silent mid-sentence cut.
pub const QUESTION_TEXT_MAX: usize = 64 * 1024;
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
/// How long a permission-flavored `Notification` waits for the grid to
/// confirm the dialog before degrading to the legacy informational card
/// (EXP-455) — same posture as [`PLAN_GRID_CONFIRM`]: it only fires when
/// detection missed a re-worded dialog (or the emitter runs term-less).
const PERMISSION_GRID_CONFIRM: Duration = Duration::from_secs(10);

/// How recently a tool headline must have been published for the degraded
/// permission card to name it when the Notification carries no tool of its
/// own (EXP-529 — claude ≥2.1.233's "Session paused" payload): the blocking
/// call's own `tool_use` entry flushes just before the Notification, so a
/// fresh headline almost certainly IS the blocked tool, while a minutes-old
/// one is a different call entirely.
const PERMISSION_TOOL_RECENCY: Duration = Duration::from_secs(30);
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
pub(crate) const PLAN_SUBMIT_PROBE: Duration = Duration::from_millis(500);
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
pub(crate) const ANSWER_RETRY_TTL: Duration = Duration::from_secs(4);
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

/// Substring identifying claude's "refine with Ultraplan on Claude Code on
/// the web" plan-picker option (key "3" on v2.1.211+). We strip it from the
/// remotely-offered plan-approval options — it hands the plan off to
/// claude.ai instead of approving/refining locally, which is not a safe
/// thing for a remote steerer to trigger blind.
const ULTRAPLAN_WEB_OPTION: &str = "Claude Code on the web";

/// Substring identifying claude's "Tell Claude what to change" plan-picker
/// option (key "4" on v2.1.211+). Stripped from the remotely-offered
/// plan-approval options (EXP-529): pressing it remotely only parks the TUI
/// in an inline-feedback editor no remote viewer can see — a dead button
/// that "submits but does nothing". The composer already IS that affordance
/// (free text steered at a pending picker Escs it and lands as feedback),
/// and clients swap their composer placeholder to say so while a plan card
/// is up; same stance as [`ULTRAPLAN_WEB_OPTION`].
const PLAN_FEEDBACK_OPTION: &str = "Tell Claude what to change";

/// Substring identifying the login method picker's "3rd-party platform ·
/// Amazon Bedrock, Microsoft Foundry, or Vertex AI" option (key "3" on
/// v2.1.222). Stripped from the remotely-offered options (EXP-430) — it
/// leads into provider-config sub-screens the login detector does not
/// cover, stranding a remote steerer; same stance as
/// [`ULTRAPLAN_WEB_OPTION`].
const LOGIN_THIRD_PARTY_OPTION: &str = "3rd-party platform";

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

    /// Exact-match masking ONLY — for text where the generic
    /// [`SECRET_PATTERNS`] can shred content that is MEANT for the viewer.
    /// The EXP-430 sign-in URL is a ~700-char base64url blob whose random
    /// substrings can match e.g. `\bsk-…` (a `-` is a word boundary), and a
    /// mid-URL `[redacted]` corrupts the link unrecoverably. The session's
    /// own launcher secrets can never legitimately appear in it, so those
    /// still mask.
    pub fn redact_exact_only(&self, input: &str) -> String {
        let mut out = input.to_string();
        for secret in &self.exact {
            out = out.replace(secret.as_str(), REDACTED);
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
    let output = terminal::process::background_command("git")
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
    let output = terminal::process::background_command("git")
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
    /// EXP-691: the hooks sidecar is wired, so the plan card is (or is about
    /// to be) published from its `PlanProposed` hook by identity — the
    /// transcript's `ExitPlanMode` twin is then ALWAYS swallowed. Mirrors
    /// [`Self::suppress_ask_questions`]: claude no longer withholds the
    /// entry until the picker is answered but flushes it the moment the tool
    /// is CALLED, before the hook drains — the old per-emission counter
    /// (EXP-150) therefore published an id-less duplicate card ahead of the
    /// real one and then stayed armed, eating the next legitimate plan.
    pub suppress_plan_twins: bool,
    /// Armed by the legacy grid-only plan fallback (no sidecar, an old
    /// claude that withholds the entry until answered): the NEXT plan twin
    /// is that publication's post-answer echo. A bool, not a counter — one
    /// plan picker exists at a time, and a bool cannot over-arm.
    pub swallow_next_plan_twin: bool,
    /// `ExitPlanMode` tool_use ids seen on the transcript whose tool_result
    /// has not flushed yet. The result IS the resolution evidence (EXP-691),
    /// and a non-empty list at grid-fallback time means the twin already
    /// flushed (immediate-flush claude) — the fallback must then neither
    /// double the card nor arm [`Self::swallow_next_plan_twin`].
    pub pending_plans: Vec<String>,
    /// `ExitPlanMode` tool_use ids whose tool_result flushed since the last
    /// emitter look — the "plan resolved" signal that still arrives while
    /// the viewport is scrolled (EXP-347; the grid watcher is sticky there).
    /// Set on RESULT flush, never on the tool_use twin: claude now flushes
    /// the twin while the picker is still up (EXP-691), and treating that as
    /// resolution disarmed the free-text reroute mid-approval.
    pub resolved_plans: Vec<String>,
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
    /// EXP-610: the hooks sidecar is wired, so every `AskUserQuestion` is (or
    /// is about to be) published by identity from its hook — the transcript's
    /// ask twin is then ALWAYS swallowed. Matching on
    /// [`Self::hook_published_asks`] alone raced exactly like the Task
    /// headlines ([`Self::suppress_task_headlines`]): an ask that resolves
    /// within one poll tick (the free-text Esc-reroute dismisses a picker the
    /// moment a steered message lands) flushes its twin BEFORE the hook that
    /// announced it drains, and the twin then published every question as a
    /// stale answerable id-less card.
    pub suppress_ask_questions: bool,
    /// EXP-610: ask ids whose tool_result already flushed (truncated to
    /// `ID_MAX`, the hook side's key) — the other half of the same race: the
    /// late-draining `QuestionsAsked` hook must NOT publish cards for a
    /// picker that no longer exists (their `question_resolved` already went
    /// by, so the stepper would wedge forever).
    pub resolved_asks: Vec<String>,
    /// EXP-483: `ExitPlanMode` tool_use ids the plan HOOK saw — when a twin
    /// entry flushes LATER than its hook (a withholding claude), prose in
    /// that same entry anchors above the already-published plan card
    /// (`beforeQuestionId`). On immediate-flush claude the twin usually
    /// beats the hook, the id is never consumed here, and the plan's
    /// tool_result cleans it up instead ([`note_plan_results`]).
    pub hook_published_plans: HashSet<String>,
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
    /// EXP-724: command texts (`/compact keep the diff`) the emitter already
    /// published when it dispatched a REMOTE slash command — claude records
    /// the same command as an origin-less `<command-name>` user entry a
    /// moment later, and this FIFO twin makes that entry consume the memory
    /// instead of doubling the bubble (the `published_queued` pattern).
    pub published_commands: Vec<String>,
    /// EXP-724: a `system`/`compact_boundary` entry flushed since the last
    /// emitter look — the transcript's own "compaction finished" edge, and
    /// the backstop for a claude build whose `PostCompact` hook never fires.
    /// A flag, not an event: only the emitter knows whether a compaction is
    /// open, and an unmatched end must never reach the wire from here.
    pub compact_boundary: bool,
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
/// Un-resulted `ExitPlanMode` tool_use cap (EXP-691) — one plan approval is
/// pending at a time; the cap only bounds a session whose results never
/// flush.
const PENDING_PLANS_CAP: usize = 4;
/// Resolved-ask memory cap (EXP-610) — only a hook racing its own ask's
/// resolution inside one tick ever reads this back.
const RESOLVED_ASKS_CAP: usize = 8;

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
/// (human-chosen input, EXP-197) ride its `question_resolved` event.
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
            // swallowed whole, no text matching involved. EXP-610: with the
            // sidecar wired the hook may not have DRAINED yet (an ask that
            // resolves within one tick flushes its twin first) — the entry's
            // own id is then the identity the hook will publish under.
            let hook_ask = ask_ids
                .iter()
                .find(|id| state.hook_published_asks.contains(*id))
                .cloned()
                .or_else(|| {
                    (state.suppress_ask_questions && !ask_ids.is_empty())
                        .then(|| ask_ids[0].clone())
                });
            // EXP-691: remember the plan tool_use — its tool_result is the
            // resolution evidence, and "a twin already flushed" gates the
            // grid fallback.
            let plan_id = exit_plan_mode_id(&entry);
            if let Some(id) = &plan_id {
                if !state.pending_plans.iter().any(|p| p == id) {
                    state.pending_plans.push(id.clone());
                    if state.pending_plans.len() > PENDING_PLANS_CAP {
                        let excess = state.pending_plans.len() - PENDING_PLANS_CAP;
                        state.pending_plans.drain(..excess);
                    }
                }
            }
            // EXP-483: a hook-published plan whose twin this entry carries —
            // consumed here so the prose can anchor above the published card.
            let hook_plan = plan_id.filter(|id| state.hook_published_plans.remove(id));
            let mut events: Vec<ActivityEvent> = parse_assistant_entry(&entry, redactor)
                .into_iter()
                .filter(|event| match event {
                    // The twin of a plan the hook publishes by identity
                    // (EXP-691: swallowed whether the hook already drained —
                    // `hook_plan` — or is about to, `suppress_plan_twins`),
                    // or the post-answer echo of a legacy grid-only card.
                    // Never a resolution signal: on immediate-flush claude
                    // the picker is still up ([`note_plan_results`] owns
                    // resolution now).
                    ActivityEvent::Question {
                        plan_mode: Some(true),
                        ..
                    } if hook_plan.is_some()
                        || state.suppress_plan_twins
                        || state.swallow_next_plan_twin =>
                    {
                        state.swallow_next_plan_twin = false;
                        false
                    }
                    // The late twin of an already-published AskUserQuestion —
                    // matched by ask id (hooks) or by text (grid-only), since
                    // it flushes only post-answer.
                    ActivityEvent::Question {
                        text,
                        plan_mode: None,
                        ..
                    } => hook_ask.is_none() && !state.consume_grid_question(text),
                    // The hook's subagent card already represents this Task
                    // call (EXP-350).
                    ActivityEvent::Tool {
                        name,
                        subagent_id: None,
                        ..
                    } if name == "Task" || name == "Agent" => !state.suppress_task_headlines,
                    _ => true,
                })
                .collect();
            // EXP-483: claude withholds this whole entry until the picker
            // resolves, so its prose reaches the wire AFTER the card the
            // hook already published. Anchor the prose to that card's
            // identity so clients can splice it back ABOVE the question.
            // Grid-only twins stay unanchored — their cards are id-less.
            if let Some(anchor) = hook_ask.as_deref().or(hook_plan.as_deref()) {
                let anchor = truncate(anchor, ID_MAX);
                for event in &mut events {
                    if let ActivityEvent::Narration {
                        before_question_id, ..
                    } = event
                    {
                        *before_question_id = Some(anchor.clone());
                    }
                }
            }
            events
        }
        Some("user") => {
            collect_task_events(&entry, state);
            note_plan_results(&entry, state);
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
        // EXP-724: `system` entries are still never published wholesale —
        // but two of them carry facts the emitter needs.
        //
        // * `compact_boundary` (with `compactMetadata.{trigger,preTokens,
        //   postTokens,durationMs}`) is claude's own end-of-compaction
        //   marker, and the only end edge a build without the `PostCompact`
        //   hook leaves behind.
        // * `local_command` is where claude 2.1.259 puts a slash command's
        //   `<local-command-stdout>` (older builds used an origin-less USER
        //   entry — both shapes are read). It is also the ONLY end edge a
        //   REFUSED compaction has: "Not enough messages to compact." fires
        //   `PreCompact` but no `PostCompact`, no boundary, and no `Stop`
        //   (the command never starts a turn), so without this the bar would
        //   hang until [`COMPACTION_MAX`].
        //
        // Both edges are recorded as a FLAG: only the emitter knows whether a
        // compaction is open, and an unmatched end must never reach the wire
        // from here.
        Some("system") => {
            match entry.get("subtype").and_then(Value::as_str) {
                Some("compact_boundary") => {
                    state.compact_boundary = true;
                    Vec::new()
                }
                Some("local_command") => {
                    let content = entry.get("content").and_then(Value::as_str).unwrap_or("");
                    if let Some(text) = xml_tag(content, "local-command-stdout") {
                        if is_compaction_result(&strip_ansi(text)) {
                            state.compact_boundary = true;
                        }
                    }
                    parse_command_stdout(content, redactor).into_iter().collect()
                }
                _ => Vec::new(),
            }
        }
        // summary/etc. → never published.
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

/// The tool_use id of an assistant entry's `ExitPlanMode` block, if any —
/// the anchor a withheld plan entry's prose splices against (EXP-483).
fn exit_plan_mode_id(entry: &Value) -> Option<String> {
    let content = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)?;
    content.iter().find_map(|block| {
        if block.get("type").and_then(Value::as_str) != Some("tool_use")
            || block.get("name").and_then(Value::as_str) != Some("ExitPlanMode")
        {
            return None;
        }
        block.get("id").and_then(Value::as_str).map(str::to_string)
    })
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

/// EXP-691: an `ExitPlanMode` tool_result is the transcript's plan-resolution
/// evidence — claude writes it the moment the picker is answered (approved or
/// rejected), on both the withholding and the immediate-flush transcript
/// behaviors, so it works even while the grid watcher is scroll-stuck. Only
/// ids recorded off an `ExitPlanMode` tool_use are read; the result's content
/// is never published (the EXP-78 privacy stance).
fn note_plan_results(entry: &Value, state: &mut TranscriptState) {
    let Some(content) = entry
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(tid) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(pos) = state.pending_plans.iter().position(|id| id == tid) else {
            continue;
        };
        state.pending_plans.remove(pos);
        state.resolved_plans.push(truncate(tid, ID_MAX));
        // The EXP-483 anchor can no longer be consumed by a twin flush (the
        // twin came and went) — drop it so the set never grows across plans.
        state.hook_published_plans.remove(tid);
    }
}

/// An `AskUserQuestion` tool_result → one semantic `question_resolved` keyed
/// by the ask's `tool_use_id` (= the `askId` the question events carried),
/// which retires every card of that ask (EXP-249). It carries the collected
/// answers in question order (from the entry's `toolUseResult.answers` map),
/// or `dismissed` when it resolved without answers (Esc / rejected — the
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
        let (ask_id, questions) = state.pending_asks.remove(pos);
        state.hook_published_asks.remove(&ask_id);
        // EXP-610: remember the resolution under the hook side's key — a
        // `QuestionsAsked` hook draining AFTER this flush must not publish
        // cards for the dead picker (see `TranscriptState::resolved_asks`).
        state.resolved_asks.push(truncate(&ask_id, ID_MAX));
        if state.resolved_asks.len() > RESOLVED_ASKS_CAP {
            let excess = state.resolved_asks.len() - RESOLVED_ASKS_CAP;
            state.resolved_asks.drain(..excess);
        }
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
                    collected.push(truncate(&answer, ANSWER_MAX));
                }
            }
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

    // EXP-672: the deterministic stand-in when a tool_use block carries no id
    // of its own — the entry's uuid is stable across every re-parse of the
    // same transcript line.
    let entry_uuid = entry.get("uuid").and_then(Value::as_str);

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
                let tool_use_id = block.get("id").and_then(Value::as_str);
                if name == "AskUserQuestion" {
                    if let Some(questions) = parse_ask_user_question(
                        tool_use_id,
                        entry_uuid,
                        block.get("input"),
                        redactor,
                    ) {
                        events.extend(questions);
                        continue;
                    }
                } else if name == "ExitPlanMode" {
                    events.push(parse_exit_plan_mode(
                        tool_use_id,
                        entry_uuid,
                        block.get("input"),
                        redactor,
                    ));
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
///
/// EXP-724 adds the ONE deliberate exception: a slash command and its local
/// result carry NO `origin` at all ([`parse_command_entry`],
/// [`parse_command_stdout`]) — they are recognised by their tagged body, and
/// only ever published as the command line itself or one allow-listed
/// result line.
fn parse_user_entry(
    entry: &Value,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Option<ActivityEvent> {
    // Injected content is dropped whatever else the entry looks like — a
    // skill body (`isMeta`) carries a `<command-name>` tag too, and the
    // post-compaction summary is a user entry as well.
    if entry.get("isMeta").and_then(Value::as_bool) == Some(true)
        || entry.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let content = entry.get("message").and_then(|m| m.get("content"))?;
    // EXP-724: a slash command and its result land as origin-LESS user
    // entries with a tagged string body, so they must be recognised before
    // the `origin.kind == "human"` gate would drop them.
    if let Value::String(raw) = content {
        if raw.contains("<command-name>") {
            return parse_command_entry(raw, redactor, state);
        }
        if raw.contains("<local-command-stdout>") {
            return parse_command_stdout(raw, redactor);
        }
    }
    let origin_kind = entry
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str);
    if origin_kind != Some("human") {
        return None;
    }
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

/// Published-command memory cap (see [`TranscriptState::published_commands`]).
const PUBLISHED_COMMANDS_CAP: usize = 8;

/// The value of one `<tag>…</tag>` in a transcript body, untruncated
/// (unlike [`tag_value`], which caps at `ID_MAX` because it reads ids).
fn xml_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

/// EXP-724: a claude slash command → the command line as one `user_message`.
///
/// Verified on claude 2.1.259: a `/compact opus` is recorded as an
/// origin-less `user` entry whose string content is
/// `<command-name>/compact</command-name> <command-message>compact</command-message>
/// <command-args>opus</command-args>` — the same shape a SKILL invocation
/// uses, except a skill's name is BARE (`workflow-authoring`) and its entry
/// carries `isMeta:true`. The leading `/` is therefore the discriminator:
/// without it there is nothing a viewer typed and nothing to echo.
///
/// A command the emitter DISPATCHED already published its own echo at
/// dispatch time, so its twin consumes the [`TranscriptState::
/// published_commands`] memory instead of doubling the bubble; a command the
/// desktop user typed locally has no twin and publishes here.
fn parse_command_entry(
    raw: &str,
    redactor: &Redactor,
    state: &mut TranscriptState,
) -> Option<ActivityEvent> {
    let name = xml_tag(raw, "command-name")?.strip_prefix('/')?;
    if name.is_empty() {
        return None;
    }
    let args = xml_tag(raw, "command-args").unwrap_or_default();
    let text = if args.is_empty() {
        format!("/{name}")
    } else {
        format!("/{name} {args}")
    };
    let redacted = truncate(&redactor.redact(&text), NARRATION_MAX);
    if let Some(pos) = state.published_commands.iter().position(|t| t == &redacted) {
        state.published_commands.remove(pos);
        return None;
    }
    Some(ActivityEvent::user_message(redacted))
}

/// EXP-724: the prefixes of a `<local-command-stdout>` body worth narrating.
///
/// A slash command's local output is an ALLOW-list, never a passthrough: most
/// of it is TUI chrome for a screen no remote viewer is looking at (`/cost`
/// tables, `/status` dumps), and the few lines that matter are the ones that
/// tell a viewer the command took effect.
const COMMAND_RESULT_PREFIXES: [&str; 4] = [
    "Compacted",
    "Not enough messages to compact",
    "Set model to",
    "Kept model as",
];

/// EXP-724: whether a `<local-command-stdout>` body is a `/compact` VERDICT
/// — the compaction ran ("Compacted …") or was refused ("Not enough messages
/// to compact."). Either way the bar must close.
fn is_compaction_result(text: &str) -> bool {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    line.starts_with("Compacted") || line.starts_with("Not enough messages to compact")
}

/// EXP-724: a `<local-command-stdout>` entry → at most one narration.
///
/// Claude writes the raw TUI string in, ANSI SGR codes and all
/// (`\x1b[2mCompacted (ctrl+o to see full summary)\x1b[22m`) — and appends a
/// line PER HOOK it ran, quoting each hook's shell command verbatim
/// (observed on 2.1.259: five lines, four of them
/// `PreCompact [curl -s -m 3 …] completed successfully`). Only the FIRST
/// line is the verdict; the rest is our own sidecar plumbing and has no
/// business on a viewer's feed.
fn parse_command_stdout(raw: &str, redactor: &Redactor) -> Option<ActivityEvent> {
    let body = xml_tag(raw, "local-command-stdout")?;
    let stripped = strip_ansi(body);
    let verdict = command_stdout_verdict(&stripped)?;
    Some(ActivityEvent::narration(truncate(
        &redactor.redact(verdict),
        NARRATION_MAX,
    )))
}

/// The allow-listed first line of an ANSI-stripped `local-command-stdout`.
fn command_stdout_verdict(stripped: &str) -> Option<&str> {
    let line = stripped.lines().map(str::trim).find(|line| !line.is_empty())?;
    COMMAND_RESULT_PREFIXES
        .iter()
        .any(|prefix| line.starts_with(prefix))
        .then_some(line)
}

/// Drop ANSI escape sequences (SGR and friends) from a TUI string. Kept
/// deliberately small: everything published is truncated and redacted
/// downstream, so the only job here is not showing `[2m` to a viewer.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI (`ESC [ … final`) and the two-char sequences; anything else
        // just loses its ESC.
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC — terminated by BEL or ST (`ESC \`).
                for c in chars.by_ref() {
                    if c == '\u{7}' || c == '\u{1b}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// EXP-672: how much of a card's text seeds [`synthetic_question_id`] — long
/// enough that two live cards never collide, short enough that the hash cost
/// is flat for a 64 KiB plan body.
const SYNTHETIC_ID_TEXT_SEED: usize = 256;

/// EXP-672: a STABLE id for a claude card no identity path minted one for.
///
/// Every question the hooks sidecar announces carries claude's own
/// `tool_use_id`; the FALLBACKS (a hookless claude, an old claude, a card the
/// grid found and the transcript never described) used to publish `id: None`,
/// and an id-less card is answerable only by the legacy blind-keystroke path.
/// This mints one from what the fallback DOES know, so the semantic `answer`
/// frame reaches every card.
///
/// The id must be deterministic for the same card — a re-publish (the history
/// buffer's replay, a twin re-parsed off the same transcript line) has to land
/// on the same identity or clients would show the card twice and its
/// `question_resolved` would retire nothing. FNV-1a over the seed parts, with
/// a `\u{1f}` separator so `("a","bc")` can never hash like `("ab","c")`.
///
/// `ordinal` disambiguates two cards whose text is genuinely identical (the
/// same picker re-asked later in the run); pass a per-session counter.
fn synthetic_question_id(session: &str, kind: &str, text: &str, ordinal: u32) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let ordinal = ordinal.to_string();
    let text = truncate(text, SYNTHETIC_ID_TEXT_SEED);
    for part in [session, kind, text.as_str(), ordinal.as_str()] {
        for byte in part.bytes().chain(std::iter::once(0x1f)) {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    // Namespaced so a synthetic id can never be mistaken for one of claude's
    // own `toolu_…` ids, and short enough to clear `ID_MAX` outright.
    format!("syn-{kind}-{hash:016x}")
}

/// `AskUserQuestion` input → one `question` event per entry of
/// `input.questions[]`, options mapped positionally to the TUI's digit keys
/// (`1`..`9`). `None` when the input doesn't match the expected shape (the
/// caller falls back to a generic tool headline).
///
/// EXP-672: the cards carry the SAME identity the `QuestionsAsked` hook would
/// have published them under — ask id = the entry's `tool_use_id`, card id =
/// `{ask_id}#{0-based index}` — so a hookless run is answerable semantically
/// and the ask's `question_resolved` (keyed on that same tool_use_id by
/// [`take_ask_answers`]) retires exactly these cards.
fn parse_ask_user_question(
    tool_use_id: Option<&str>,
    entry_uuid: Option<&str>,
    input: Option<&Value>,
    redactor: &Redactor,
) -> Option<Vec<ActivityEvent>> {
    let input = input?;
    let questions = input.get("questions")?.as_array()?;
    let total = questions.len() as u32;
    // claude always stamps a tool_use id; the entry uuid (plus the raw input,
    // for the pathological entry that has neither) is the deterministic
    // stand-in if a future transcript shape ever drops it.
    let ask_id = tool_use_id
        .map(|id| truncate(id, ID_MAX))
        .unwrap_or_else(|| {
            synthetic_question_id(
                entry_uuid.unwrap_or_default(),
                "ask",
                &input.to_string(),
                0,
            )
        });
    let mut events = Vec::new();
    for (index, question) in questions.iter().enumerate() {
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
            // Byte-identical to the hook's shape (`QuestionsAsked`): the id
            // indexes from 0, the DISPLAYED step from 1.
            id: Some(format!("{ask_id}#{index}")),
            ask_id: Some(ask_id.clone()),
            index: Some(index as u32 + 1),
            total: Some(total),
            header: None,
            at: None,
        });
    }
    (!events.is_empty()).then_some(events)
}

/// `ExitPlanMode` → a plan-approval `question` (text = the plan markdown when
/// present). This transcript path is the hookless fallback (EXP-150): with
/// the sidecar wired the twin is suppressed and the card comes from the hook
/// plus the grid watcher's REAL picker rows. When it does fire, only the two
/// approve keys are offered — key "3" is no longer safe to send blind (on
/// claude v2.1.211 it launches "refine with Ultraplan on Claude Code on the
/// web", not "keep planning").
///
/// EXP-672: the card carries claude's own `tool_use_id` — the SAME id the
/// `PlanProposed` hook publishes the plan under — so a hookless run's plan is
/// answerable semantically, and the two paths cannot mint rival identities for
/// one plan.
fn parse_exit_plan_mode(
    tool_use_id: Option<&str>,
    entry_uuid: Option<&str>,
    input: Option<&Value>,
    redactor: &Redactor,
) -> ActivityEvent {
    let plan = input
        .and_then(|i| i.get("plan"))
        .and_then(Value::as_str)
        .map(|p| truncate_marked(&redactor.redact(p), QUESTION_TEXT_MAX))
        .filter(|p| !p.trim().is_empty());
    let text = plan.unwrap_or_else(|| "Plan ready for approval.".to_string());
    let id = tool_use_id
        .map(|id| truncate(id, ID_MAX))
        .unwrap_or_else(|| synthetic_question_id(entry_uuid.unwrap_or_default(), "plan", &text, 0));
    ActivityEvent::Question {
        text,
        options: vec![
            QuestionOption::new("Approve — auto-accept edits", "1"),
            QuestionOption::new("Approve — manually approve edits", "2"),
        ],
        multi_select: None,
        // Marks the question as a plan-approval picker so clients can render
        // a dedicated "Plan ready" card (EXP-97).
        plan_mode: Some(true),
        id: Some(id),
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

/// EXP-429: the ownership pin for cwd-keyed transcript discovery. The
/// project dir is shared by EVERY claude running in the same cwd — an action
/// run on the trunk clone and a plain "+" agent-shell tab both write into
/// `~/.claude/projects/<munged cwd>/`, and picking the newest file there let
/// the foreign tab's transcript hijack this session's steer feed. Real
/// coding sessions run with the hooks sidecar (EXP-249), whose every payload
/// carries claude's own session id — exactly the transcript's file stem
/// (`<id>.jsonl`; nested sidechains under `<id>/subagents/**`) — while the
/// plain tab gets no settings file and can never enter the pin. An empty pin
/// (no sidecar, an old claude, the window before the first hook lands) keeps
/// the legacy newest-file behavior as the degraded fallback.
#[derive(Default)]
struct TranscriptPin {
    /// Claude session ids seen on this emitter's hook stream. Grows on
    /// /clear (the new session's hooks route back here by cwd) — never
    /// capped or truncated: entries must compare byte-equal to file stems,
    /// and a full set would silently kill the feed after a /clear.
    sessions: HashSet<String>,
    /// Subagent ids from SubagentStart/Stop — the owners of the flat legacy
    /// `agent-<id>.jsonl` layout (claude ≤2.1.21x), whose path carries no
    /// session id to match against.
    agents: HashSet<String>,
}

impl TranscriptPin {
    /// Absorb the identity a hook delivery carries. The `transcript_path`
    /// stem is a supplementary session-id source, so the pin survives a
    /// claude build that drops `session_id` from the payload.
    fn observe(&mut self, event: &HookEvent) {
        if let Some(id) = &event.context.session_id {
            self.sessions.insert(id.clone());
        }
        if let Some(path) = &event.context.transcript_path {
            if let Some(stem) = Path::new(path).file_stem().and_then(|s| s.to_str()) {
                if !stem.is_empty() && !stem.starts_with("agent-") {
                    self.sessions.insert(stem.to_string());
                }
            }
        }
        if let HookEventKind::SubagentStarted {
            agent_id: Some(id), ..
        }
        | HookEventKind::SubagentStopped {
            agent_id: Some(id), ..
        } = &event.kind
        {
            self.agents.insert(id.clone());
        }
    }

    /// EXP-443: pre-seed the pin with the spawn-minted `--session-id` — the
    /// emitter calls this before its first tick, closing the window where an
    /// unpinned emitter tailed whatever was newest in the shared cwd.
    fn seed(&mut self, session_id: &str) {
        self.sessions.insert(session_id.to_string());
    }

    /// Whether discovery is pinned at all — false until the first hook lands.
    fn pinned(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// A main transcript is ours iff its stem is a pinned session id.
    fn owns_main(&self, path: &Path) -> bool {
        if !self.pinned() {
            return true;
        }
        path.file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| self.sessions.contains(stem))
    }

    /// A sidechain is ours iff it nests under a pinned session's dir
    /// (`<dir>/<session-id>/subagents/**`, claude ≥2.1.220) or its flat
    /// `agent-<id>` id was announced by a SubagentStart/Stop hook.
    ///
    /// Known gap (EXP-443, accepted): a FLAT sidechain whose SubagentStart
    /// never fired (hook lost, or only SubagentStop delivered) is dropped
    /// while pinned — the Task/Agent dispatch hook carries no `agent_id`, so
    /// there is nothing correct to admit it by.
    fn owns_sidechain(&self, dir: &Path, path: &Path) -> bool {
        if !self.pinned() {
            return true;
        }
        let nested_owner = path
            .strip_prefix(dir)
            .ok()
            .and_then(|rel| rel.components().next())
            .and_then(|first| first.as_os_str().to_str());
        if nested_owner.is_some_and(|owner| self.sessions.contains(owner)) {
            return true;
        }
        sidechain_agent_id(path).is_some_and(|id| self.agents.contains(&id))
    }
}

/// The newest non-sidechain session transcript in `dir` modified at/after
/// `after` (the spawn time — so a previous session's stale transcript in a
/// reused worktree is never picked) and owned by `pin` (EXP-429 — so a
/// concurrent foreign claude in the same cwd never supersedes ours).
/// Sub-agent files (`agent-*.jsonl`) are excluded so tailing never
/// flip-flops between the main session and a sidechain;
/// [`sidechain_transcripts`] streams those separately.
fn newest_transcript(dir: &Path, after: SystemTime, pin: &TranscriptPin) -> Option<PathBuf> {
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
        if !pin.owns_main(&path) {
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
/// hard visit budget so a huge tree can never stall the poll loop. Files a
/// pinned emitter does not own are dropped (EXP-429) — at the FILE level, not
/// by pruning the BFS: a foreign session's tree still costs walk budget, but
/// a future layout under a non-session dir can never silently vanish.
fn sidechain_transcripts(dir: &Path, after: SystemTime, pin: &TranscriptPin) -> Vec<PathBuf> {
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
            if !pin.owns_sidechain(dir, &path) {
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

/// A unified diff of what this run has produced, as one string. Empty when
/// there is nothing (or git fails — best-effort throughout).
///
/// EXP-688: measured from `git merge-base HEAD <base_ref>` when the launcher
/// knows the branch's base, so the frame is the PR's content. It used to be
/// `git diff` + `--cached` only, which meant "Latest changes" went blank the
/// moment the agent committed — i.e. always, since an agent commits before
/// opening its PR, leaving the viewer's Merge pill standing alone.
///
/// No base (a chat/scratch run, or the ref cannot be resolved) falls back to
/// the old uncommitted view. `--cached` is not needed off the merge base:
/// `git diff <commit>` already includes staged work.
///
/// `None` = git itself failed (an index lock mid-commit, a rebase in flight,
/// a vanished worktree): the caller keeps its last answer rather than
/// publishing an authoritative empty diff off a transient error.
pub(crate) fn worktree_diff(worktree: &Path, base_ref: Option<&str>) -> Option<String> {
    if let Some(base) = base_ref.map(str::trim).filter(|base| !base.is_empty()) {
        if let Some(merge_base) = git_merge_base(worktree, base) {
            return git_out(worktree, &["diff", &merge_base]);
        }
    }
    let mut out = git_diff(worktree, false)?;
    let cached = git_diff(worktree, true)?;
    if !cached.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&cached);
    }
    Some(out)
}

/// `git merge-base HEAD <base>` — `None` when the ref is unknown (a base
/// that was never fetched, an unborn HEAD), which is the fallback signal.
fn git_merge_base(worktree: &Path, base: &str) -> Option<String> {
    let hash = git_out(worktree, &["merge-base", "HEAD", base])?;
    let hash = hash.trim();
    (!hash.is_empty()).then(|| hash.to_string())
}

fn git_diff(worktree: &Path, cached: bool) -> Option<String> {
    if cached {
        git_out(worktree, &["diff", "--cached"])
    } else {
        git_out(worktree, &["diff"])
    }
}

/// Stdout of one git command; `None` on a spawn failure or non-zero exit.
fn git_out(worktree: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = terminal::process::background_command("git");
    cmd.arg("-C").arg(worktree).args(args);
    match cmd.output() {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
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
    /// EXP-513: the typed reply for a `freeText` option — selected with
    /// `keys`, typed into the TUI's inline editor, submitted with Enter.
    pub text: Option<String>,
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
    /// EXP-444: armed when a login screen soliciting free text (the OAuth
    /// code prompt / error retry) left the grid WITHOUT succeeding — a code
    /// pasted after that would land in claude's composer and be submitted,
    /// recorded and journaled as a prompt. While armed (and not expired) the
    /// publisher refuses the next free-text message instead of writing it.
    login_refuse_until: std::sync::Mutex<Option<Instant>>,
    /// The publisher refused a message — the emitter narrates it (consuming).
    login_refusal_noted: AtomicBool,
}

/// EXP-444: how long a closed login screen keeps refusing free text. One
/// refusal disarms early; with no paste attempt, normal input resumes after
/// the window.
const LOGIN_REFUSAL_TTL: Duration = Duration::from_secs(120);

impl AnswerLink {
    /// The link plus the emitter's receiving end.
    pub fn new() -> (Arc<Self>, flume::Receiver<RemoteAnswer>) {
        let (tx, rx) = flume::unbounded();
        (
            Arc::new(Self {
                tx,
                ask_pending: AtomicBool::new(false),
                grid_picker_pending: AtomicBool::new(false),
                login_refuse_until: std::sync::Mutex::new(None),
                login_refusal_noted: AtomicBool::new(false),
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

    /// Emitter side: a code-soliciting login screen closed unsuccessfully —
    /// refuse the next free-text message for [`LOGIN_REFUSAL_TTL`].
    pub fn arm_login_refusal(&self) {
        let mut until = self.login_refuse_until.lock().unwrap_or_else(|p| p.into_inner());
        *until = Some(Instant::now() + LOGIN_REFUSAL_TTL);
    }

    /// Emitter side: the login flow moved on (a new phase painted / success)
    /// — mistimed-paste protection no longer applies.
    pub fn disarm_login_refusal(&self) {
        let mut until = self.login_refuse_until.lock().unwrap_or_else(|p| p.into_inner());
        *until = None;
    }

    /// Publisher side: whether free text must currently be refused.
    pub fn login_refusal_active(&self) -> bool {
        let until = self.login_refuse_until.lock().unwrap_or_else(|p| p.into_inner());
        until.is_some_and(|deadline| Instant::now() < deadline)
    }

    /// Publisher side: a message was refused — note it for the emitter and
    /// disarm (one-shot: the user's re-send must go through).
    pub fn note_login_refusal(&self) {
        self.disarm_login_refusal();
        self.login_refusal_noted.store(true, Ordering::Relaxed);
    }

    /// Emitter side: whether a refusal happened since the last call — a
    /// consuming read that turns into the "nothing was sent" narration.
    pub fn take_login_refusal_note(&self) -> bool {
        self.login_refusal_noted.swap(false, Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// The publisher ↔ emitter command seam (EXP-724)
// ---------------------------------------------------------------------------

/// EXP-724: a pi command dispatch — `(name, args)` onto the observer
/// extension's `/steer` queue. `None` (claude/codex) means the emitter types
/// the command into the TUI instead.
pub type CommandSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// The publisher ↔ emitter seam for remote slash commands, the sibling of
/// [`AnswerLink`] (EXP-724).
///
/// The publisher RECOGNISES a command (its first token is a catalog `/name`
/// for the session's agent, [`crate::commands::parse_command`]) but must not
/// execute one: typing needs the grid and the composer's turn state, both of
/// which the emitter owns. So a whole recognised composer message crosses
/// here and the emitter drains it, exactly like an answer.
///
/// One `Arc` is shared by both sides (the receiver lives inside), so the
/// wiring builds it once and hands the same handle to
/// [`crate::publisher::PublisherHooks`] and [`Steering`].
pub struct CommandLink {
    tx: flume::Sender<crate::commands::ParsedCommand>,
    rx: flume::Receiver<crate::commands::ParsedCommand>,
    /// Emitter side: is the agent between turns? pi's `ctx.compact()` aborts
    /// a streaming turn and codex's mid-task command handling is unverified,
    /// so both hold a command until this reads true (claude's TUI queues
    /// input mid-turn and needs no gate).
    composer_idle: AtomicBool,
    sink: Option<CommandSink>,
}

impl CommandLink {
    /// `sink` dispatches pi commands through the observer extension; pass
    /// `None` for the agents whose commands are typed into the TUI.
    pub fn new(sink: Option<CommandSink>) -> Arc<Self> {
        let (tx, rx) = flume::unbounded();
        Arc::new(Self {
            tx,
            rx,
            composer_idle: AtomicBool::new(false),
            sink,
        })
    }

    /// Publisher side: hand one recognised command to the emitter
    /// (fire-and-forget — a dead emitter just means it never runs).
    pub fn submit(&self, command: crate::commands::ParsedCommand) {
        let _ = self.tx.send(command);
    }

    /// Emitter side: the next queued command, if any.
    pub fn try_recv(&self) -> Option<crate::commands::ParsedCommand> {
        self.rx.try_recv().ok()
    }

    /// Emitter side: publish the between-turns bit.
    pub fn set_composer_idle(&self, idle: bool) {
        self.composer_idle.store(idle, Ordering::Relaxed);
    }

    pub fn composer_idle(&self) -> bool {
        self.composer_idle.load(Ordering::Relaxed)
    }

    /// Emitter side: hand the command to pi's observer extension. `false`
    /// when this session has no sink (claude/codex — type it instead).
    pub fn dispatch_to_sink(&self, command: &crate::commands::ParsedCommand) -> bool {
        match &self.sink {
            Some(sink) => {
                sink(command.command.name, &command.args);
                true
            }
            None => false,
        }
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
    /// EXP-724: the remote slash-command seam. `None` = commands are not
    /// executed for this session (they never reach the publisher either).
    pub commands: Option<Arc<CommandLink>>,
}

/// EXP-724: how long a command waits for a busy composer (codex/pi) before
/// it is refused. Long enough to cover a normal turn, short enough that a
/// steerer learns the command did not run.
pub(crate) const COMMAND_IDLE_WAIT: Duration = Duration::from_secs(60);

/// EXP-724: how long the emitter watches for the typed command to LEAVE the
/// composer after the submitting Enter. A slash popup that accepted the
/// completion instead of submitting leaves the text sitting there — one more
/// Enter then runs it, and never a third (an Enter on an empty composer is a
/// no-op in every agent TUI, so the false-positive costs nothing).
///
/// Verified against a live claude 2.1.259 through this exact path
/// (`examples/exp724_slash_commands.rs`): a BRACKETED `/compact` plus ONE
/// Enter runs immediately — the slash popup opens on TYPED `/` only, and a
/// paste lands as literal text that claude's REPL parses as a command on
/// submit, so the probe finds the composer already empty and no second
/// Enter is sent. The probe stays because codex's popup behaviour differs
/// per build and a stuck composer is otherwise invisible to the steerer.
pub(crate) const COMMAND_SUBMIT_PROBE: Duration = Duration::from_millis(500);

/// EXP-724: how many trailing grid rows count as "the composer" for the
/// submit probe. The input box sits at the very bottom above its hint line;
/// scanning further up would find the SUBMITTED command in the scrollback
/// and re-Enter forever.
const COMPOSER_TAIL_LINES: usize = 4;

/// EXP-724: refusals a remote command can earn, byte-identical on every
/// viewer because they ride the ordinary narration channel.
pub(crate) const COMMAND_REFUSED_PICKER: &str = "Answer the pending prompt first.";
pub(crate) const COMMAND_REFUSED_BUSY: &str = "The agent is busy — try again when it is idle.";

/// Outcome of one command-dispatch attempt (EXP-724), mirroring
/// [`AnswerAttempt`]: `Retry` means the composer is busy and the emitter
/// parks the command for [`COMMAND_IDLE_WAIT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandAttempt {
    /// Handed to the agent — it is running.
    Ran,
    /// Handled for good without running (refused, or nothing to run against).
    Refused,
    /// The composer is busy — park and try again.
    Retry,
}

/// Whether the composer still shows `text` — the submit probe's question.
pub(crate) fn composer_holds(lines: &[String], text: &str) -> bool {
    lines
        .iter()
        .rev()
        .take(COMPOSER_TAIL_LINES)
        .any(|line| line.contains(text))
}

/// EXP-724: type one catalog command into a claude/codex TUI and submit it.
///
/// Bracketed exactly like a steered message (the `write_input` hook brackets
/// anything longer than a keystroke), then [`ENTER_SEPARATION`] and the
/// submitting `\r` — the same choreography a remote message already uses,
/// which is why command chunks never need (and never reach) the EXP-383
/// codex space guard: that guard exists to DEFUSE a leading `/`, and here
/// the `/` is the point.
pub(crate) fn type_command(
    command: &crate::commands::ParsedCommand,
    term: &TermHandle,
    write_input: &InputHook,
) {
    let text = command.text();
    write_input(text.as_bytes());
    std::thread::sleep(crate::publisher::ENTER_SEPARATION);
    write_input(b"\r");
    if !settle_for(COMMAND_SUBMIT_PROBE, || {
        !composer_holds(&screen_lines(term), &text)
    }) {
        // The popup accepted a completion instead of submitting.
        write_input(b"\r");
    }
}

/// EXP-724: run one remote command for a PTY-driven agent (claude, codex).
///
/// A pending grid picker owns the keyboard, and unlike a free-text message a
/// command is NOT worth Esc-ing a plan/permission prompt away for — refuse
/// and say so. `idle_gated` sessions (codex) hold until the composer is
/// between turns.
pub(crate) fn dispatch_command(
    command: &crate::commands::ParsedCommand,
    link: &CommandLink,
    picker_pending: bool,
    idle_gated: bool,
    term: Option<&TermHandle>,
    write_input: &InputHook,
    sender: &ActivitySender,
) -> CommandAttempt {
    if picker_pending {
        sender.send(ActivityEvent::narration(COMMAND_REFUSED_PICKER));
        return CommandAttempt::Refused;
    }
    if idle_gated && !link.composer_idle() {
        return CommandAttempt::Retry;
    }
    // pi never touches the PTY: its commands ride the observer extension,
    // which calls pi's own `ctx.compact()`/`ctx.newSession()`.
    if link.dispatch_to_sink(command) {
        return CommandAttempt::Ran;
    }
    let Some(term) = term else {
        // No grid to type into — nothing safe to do (tests, headless).
        log::debug!("activity: command dropped — no terminal grid");
        return CommandAttempt::Refused;
    };
    type_command(command, term, write_input);
    CommandAttempt::Ran
}

/// EXP-724: one emitter tick's command work — drain the link, echo every new
/// command onto the feed, then run (or park, or refuse) what is queued.
///
/// The echo happens at DRAIN time for every agent, before the command can
/// run: claude records a `<command-name>` twin the `published` memory
/// consumes ([`parse_command_entry`]), codex records no user message for a
/// command at all, and pi's `input` event never fires for one — so without
/// this the steerer's own command would simply never appear in the feed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pump_commands(
    parked: &mut Vec<(crate::commands::ParsedCommand, Instant)>,
    link: &CommandLink,
    picker_pending: bool,
    idle_gated: bool,
    term: Option<&TermHandle>,
    write_input: &InputHook,
    sender: &ActivitySender,
    mut published: Option<&mut Vec<String>>,
    mut on_dispatched: impl FnMut(&crate::commands::ParsedCommand),
) {
    while let Some(command) = link.try_recv() {
        let text = truncate(&command.text(), NARRATION_MAX);
        if let Some(published) = published.as_deref_mut() {
            published.push(text.clone());
            if published.len() > PUBLISHED_COMMANDS_CAP {
                let excess = published.len() - PUBLISHED_COMMANDS_CAP;
                published.drain(..excess);
            }
        }
        sender.send(ActivityEvent::user_message(text));
        parked.push((command, Instant::now()));
    }
    parked.retain_mut(|(command, since)| {
        match dispatch_command(
            command,
            link,
            picker_pending,
            idle_gated,
            term,
            write_input,
            sender,
        ) {
            CommandAttempt::Ran => {
                on_dispatched(command);
                false
            }
            CommandAttempt::Refused => false,
            CommandAttempt::Retry => {
                if since.elapsed() >= COMMAND_IDLE_WAIT {
                    sender.send(ActivityEvent::narration(COMMAND_REFUSED_BUSY));
                    false
                } else {
                    true
                }
            }
        }
    });
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
    /// The `/login` method picker (EXP-430) — grid-born, no hook identity.
    Login,
    /// A permission dialog (EXP-455) — grid-born like [`Self::Login`]; the
    /// permission-flavored `Notification` hook contributes only the tool
    /// name, never an identity.
    Permission,
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

/// A permission-flavored `Notification`, waiting for the grid to confirm the
/// dialog (EXP-455). `tool`/`detail` are stored publish-ready (truncated /
/// redacted at hook time) so the degraded path needs no redactor.
struct PendingPermission {
    tool: String,
    detail: Option<String>,
    seen: Instant,
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
    /// Bounded transitively: entries are card ids, and eviction purges them.
    unbound: VecDeque<String>,
    /// `agent_id` → the id the card was published under. Every value points
    /// at a live `meta` card (eviction purges dead aliases), so this stays
    /// bounded at a few entries per card without its own cap.
    alias: HashMap<String, String>,
    /// Card id → what the card was published with. `SubagentStop` carries no
    /// `agent_type`, so the completion edge re-states these (EXP-350: clients
    /// that render the LAST marker were degrading the label to "agent").
    meta: HashMap<String, CardMeta>,
    /// Card ids in publish order — the eviction queue (EXP-404: eviction
    /// used to pick a RANDOM HashMap key, dropping live cards whose
    /// completion could then never publish).
    order: VecDeque<String>,
    /// Completion edges the cap eviction owes the wire: a LIVE evictee's
    /// Started is already published, and nothing else can ever complete it.
    forced: Vec<(String, String, Option<String>)>,
}

/// What a subagent card was published with (already redacted/truncated).
struct CardMeta {
    agent_type: String,
    detail: Option<String>,
    /// The completion edge already published (EXP-360): a background agent's
    /// end can be seen twice (task-notification + a late hook stop, or repeat
    /// notifications when the agent is resumed) — the card completes ONCE.
    completed: bool,
    /// EXP-360 async-launched: its end arrives as a task-notification turns
    /// after the launch, so the turn-end sweep must spare it (EXP-404).
    background: bool,
}

/// Live subagent cap — a wide fan-out must not grow these maps without bound.
/// Beyond it the oldest finished card is dropped first, and an evicted LIVE
/// card is force-completed on the wire, never silently orphaned (EXP-404: a
/// dynamic-workflow fan-out of 60+ agents left every evicted card's tab
/// spinning forever on all clients).
const SUBAGENTS_CAP: usize = 128;

impl Subagents {
    fn dispatch(&mut self, tool_use_id: String, agent_type: String, detail: Option<String>) {
        self.remember(tool_use_id.clone(), agent_type, detail);
        self.unbound.push_back(tool_use_id);
    }

    /// Record a card's published metadata (dispatch, or an unbound start).
    fn remember(&mut self, card_id: String, agent_type: String, detail: Option<String>) {
        if let Some(meta) = self.meta.get_mut(&card_id) {
            meta.agent_type = agent_type;
            meta.detail = detail;
        } else {
            self.order.push_back(card_id.clone());
            self.meta.insert(
                card_id,
                CardMeta {
                    agent_type,
                    detail,
                    completed: false,
                    background: false,
                },
            );
        }
        self.enforce_cap();
    }

    /// EXP-404: evict beyond the cap — oldest FINISHED card first; only when
    /// every card is still live does the oldest live one go, and its owed
    /// completion edge is queued (`forced`) before the bookkeeping drops.
    /// Random (HashMap-order) eviction used to drop live cards, whose
    /// Started frame then dangled on every client forever.
    fn enforce_cap(&mut self) {
        while self.meta.len() > SUBAGENTS_CAP {
            let evictee = self
                .order
                .iter()
                .find(|id| self.meta.get(*id).is_some_and(|m| m.completed))
                .cloned()
                .or_else(|| self.order.front().cloned());
            let Some(evictee) = evictee else { break };
            if let Some(meta) = self.meta.remove(&evictee) {
                if !meta.completed {
                    self.forced
                        .push((evictee.clone(), meta.agent_type, meta.detail));
                }
            }
            self.order.retain(|id| id != &evictee);
            self.alias.retain(|_, card| card != &evictee);
            self.unbound.retain(|id| id != &evictee);
        }
    }

    /// Drain the completion edges the cap eviction owes the wire (EXP-404).
    fn take_forced_completions(&mut self) -> Vec<(String, String, Option<String>)> {
        std::mem::take(&mut self.forced)
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
        let Some(meta) = self.meta.get_mut(tool_use_id) else {
            return;
        };
        // Its end arrives as a task-notification turns later (SubagentStop
        // never fires for background agents) — the turn-end sweep must
        // spare it (EXP-404).
        meta.background = true;
        self.unbound.retain(|id| id != tool_use_id);
        self.alias
            .insert(agent_id.to_string(), tool_use_id.to_string());
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
        // An evicted card resolves to nothing — its forced completion edge
        // already went out with the eviction (EXP-404).
        let meta = self.meta.get_mut(&card_id)?;
        if meta.completed {
            return None;
        }
        meta.completed = true;
        Some((
            card_id.clone(),
            meta.agent_type.clone(),
            meta.detail.clone(),
        ))
    }

    /// EXP-404: complete every still-open card — the turn/session-end safety
    /// net for stop signals that never arrive (a dynamic workflow's agents
    /// fire SubagentStart, but their stops can be lost wholesale). A turn-end
    /// sweep spares background agents: their end legitimately arrives turns
    /// later as a task-notification (EXP-360). The mis-fire mode is benign by
    /// design — an agent swept early shows "completed" a moment before it
    /// truly is, instead of spinning forever.
    fn sweep_open(&mut self, include_background: bool) -> Vec<(String, String, Option<String>)> {
        let mut swept = Vec::new();
        for id in &self.order {
            let Some(meta) = self.meta.get_mut(id) else {
                continue;
            };
            if meta.completed || (!include_background && meta.background) {
                continue;
            }
            meta.completed = true;
            swept.push((id.clone(), meta.agent_type.clone(), meta.detail.clone()));
        }
        swept
    }
}

/// The emitter's steering brain: what the hooks said, what is published, what
/// is still answerable.
#[derive(Default)]
struct SteerState {
    plan: Option<PendingPlan>,
    ask: Option<PendingAsk>,
    subagents: Subagents,
    /// EXP-637: is the agent BETWEEN turns? Set by `Stop`/`SessionEnd`/
    /// `Idle`, cleared by every dispatch/lifecycle edge. Feeds
    /// [`TurnSignal`], which the graceful stop waits on so an agent that
    /// just called `exponential_sessions_end` finishes writing its close-out
    /// before anything tears the child down.
    turn_idle: bool,
    attention: Option<Attention>,
    live: HashMap<String, LiveQuestion>,
    answered: HashSet<String>,
    /// The live login method-picker question id (EXP-430) — grid-born, so
    /// its lifecycle is owned by the [`LoginWatcher`], not the hooks.
    login: Option<String>,
    /// Login question identity — each (re)appearance of the method picker
    /// gets a fresh id so a retry loop never collides with `answered`.
    login_seq: u32,
    /// The live permission-dialog question id (EXP-455) — grid-born, owned
    /// by the [`PermissionPickerWatcher`] lifecycle.
    permission: Option<String>,
    /// Permission question identity — every dialog gets a fresh id so
    /// back-to-back prompts never collide with `answered`.
    permission_seq: u32,
    /// A permission-flavored `Notification` waiting for the grid to confirm
    /// its dialog (EXP-455). Confirmed ⇒ the answerable question consumes it;
    /// never confirmed ⇒ [`Self::permission_timeout`] degrades it to the
    /// legacy informational card, so a claude whose dialog copy drifted past
    /// the anchors behaves exactly as before.
    pending_permission: Option<PendingPermission>,
    /// Fallback plan identity when a hook payload carries no `tool_use_id`.
    plan_seq: u32,
    /// EXP-672: the ordinal feeding [`synthetic_question_id`] for the two
    /// GRID-only fallbacks (a plan/ask picker the hooks never announced and
    /// the transcript never described). Bumped per publication so two
    /// word-for-word identical pickers in one run get distinct identities.
    grid_seq: u32,
    /// EXP-672: the live grid-only plan card's synthetic id — retired by
    /// [`Self::resolve_plan`] exactly like a hook-born one, so a legacy
    /// hookless run no longer leaves an answerable card behind.
    grid_plan: Option<String>,
    /// EXP-672: the live grid-only ASK cards' synthetic ids. Unlike the plan
    /// slot this is a set: the question watcher re-fires per question of a
    /// multi-question picker, so one picker can leave several cards up. They
    /// carry no `ask_id` (nothing announced the ask), so the ask-keyed retire
    /// in [`Self::observe_published`] can never reach them — they are retired
    /// as a group by [`Self::resolve_grid_asks`].
    grid_asks: Vec<String>,
    /// Fallback subagent-card identity when a `Task` hook payload carries no
    /// `tool_use_id` (EXP-350 — the card used to be dropped entirely).
    task_seq: u32,
    /// EXP-347: a question/plan resolution was learned since the last
    /// [`Self::take_resolution`] — the emitter clears the publisher's
    /// grid-picker flag on it instead of waiting for the next grid tick
    /// (which never comes while the viewport is scrolled).
    resolution_seen: bool,
    /// EXP-404: a Stop/SessionEnd hook landed since the last
    /// [`Self::take_subagent_sweep`] — the emitter completes every still-open
    /// subagent card AFTER the transcript drain (deferred so a same-tick
    /// launch ack still marks its card background first).
    subagent_sweep: Option<SubagentSweep>,
    /// EXP-429: the transcript-discovery ownership pin, fed from every hook
    /// delivery (see [`TranscriptPin`]).
    pin: TranscriptPin,
    /// EXP-443: a SubagentStart landed since the last
    /// [`Self::take_subagent_seen`] — the emitter drops its sidechain-rescan
    /// debounce for one tick so the new agent's file is tailed immediately
    /// instead of up to 3s late.
    subagent_seen: bool,
    /// EXP-444: when the last login-picker answer was injected — the
    /// anchor-drift diagnostic narrates if no recognizable login phase
    /// follows within [`LOGIN_DRIFT_WINDOW`]. Cleared by every login `Show`.
    login_injected_at: Option<Instant>,
    /// EXP-529: the last tool headline the transcript published — the
    /// degraded permission card's tool-name fallback when the Notification
    /// carries none (claude's "Session paused" payload). MAIN transcript
    /// only: sidechain tool headlines bypass [`Self::observe_published`], so
    /// a subagent-raised prompt still falls back to the main transcript's
    /// last tool or the literal "Tool" — no worse than before.
    last_tool: Option<(String, Instant)>,
    /// EXP-724: when the open compaction started (`PreCompact`). `Some` = the
    /// viewers are showing the indeterminate "Compacting context…" bar, so
    /// exactly one `ended` must follow — from `PostCompact`, the transcript's
    /// `compact_boundary`, a `SessionStart{source:"compact"}`, a turn/session
    /// end, [`COMPACTION_MAX`], or teardown, whichever lands first.
    compacting_since: Option<Instant>,
}

/// EXP-444: how long after an injected login answer the emitter waits for a
/// recognizable follow-up screen before flagging probable anchor drift.
const LOGIN_DRIFT_WINDOW: Duration = Duration::from_secs(15);

/// EXP-724: fold an agent's compaction reason onto the wire's two values.
/// The relay's schema accepts `manual` | `auto` ONLY (pi reports
/// `threshold`/`overflow`, a future claude could report anything else), and
/// an unknown trigger would sever the publisher socket.
pub(crate) fn normalize_compaction_trigger(trigger: Option<&str>) -> Option<&'static str> {
    match trigger {
        None => None,
        Some(trigger) if trigger.eq_ignore_ascii_case("manual") => Some("manual"),
        Some(_) => Some("auto"),
    }
}

/// EXP-724: the compaction bar's hard ceiling. Real compactions ran 10–170s
/// locally; past this the end edge is presumed lost and the bar closes on
/// its own. A stuck indeterminate bar is worse than a missing one.
pub(crate) const COMPACTION_MAX: Duration = Duration::from_secs(300);

/// How wide a deferred subagent sweep reaches (EXP-404).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SubagentSweep {
    /// The turn is over — foreground/workflow agents cannot still be
    /// running; background agents (EXP-360) are spared.
    TurnEnd,
    /// The session is over — no signal can ever complete a card again.
    SessionEnd,
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
    /// Whether a hook says the session is parked on a picker right now (or,
    /// for the grid-born permission dialog, a question is live).
    fn has_pending_question(&self) -> bool {
        self.plan.is_some() || self.ask.is_some() || self.permission.is_some()
    }

    /// EXP-529: whether the permission grid detection may drop its "Do you
    /// want to" anchor. True while the hooks sidecar holds an unconfirmed
    /// permission prompt — the hook is the anchor then — and while a
    /// published permission question is still live: `publish_permission_
    /// question` consumes the hold, so without the second half the very next
    /// tick would run strict detect, miss the anchorless dialog, and fire a
    /// phantom `Resolved` that retires the fresh card.
    fn permission_grid_leniency(&self) -> bool {
        self.permission.is_some()
            || self
                .pending_permission
                .as_ref()
                .is_some_and(|pending| !pending.degraded)
    }

    /// The tool-name fallback for a Notification that names none: the last
    /// tool headline the main transcript published, if recent enough to
    /// plausibly be the blocking call ([`PERMISSION_TOOL_RECENCY`] — claude
    /// writes the `tool_use` entry just before the Notification fires).
    fn recent_tool(&self) -> Option<String> {
        self.last_tool
            .as_ref()
            .filter(|(_, at)| at.elapsed() <= PERMISSION_TOOL_RECENCY)
            .map(|(name, _)| name.clone())
    }

    /// EXP-347: whether a resolution was learned since the last call — a
    /// consuming read (`false` until the next resolution).
    fn take_resolution(&mut self) -> bool {
        std::mem::take(&mut self.resolution_seen)
    }

    /// EXP-404: the sweep a Stop/SessionEnd hook armed — a consuming read.
    fn take_subagent_sweep(&mut self) -> Option<SubagentSweep> {
        self.subagent_sweep.take()
    }

    /// EXP-443: whether a SubagentStart landed since the last call — a
    /// consuming read the emitter turns into an immediate sidechain rescan.
    fn take_subagent_seen(&mut self) -> bool {
        std::mem::take(&mut self.subagent_seen)
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
        // Work is happening: whatever the last turn boundary said, we are
        // mid-turn again (EXP-637).
        self.turn_idle = false;
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

    /// EXP-672: retire the grid-only plan card, if one is live. Its own
    /// [`Self::grid_plan`] slot rather than [`Self::plan`]: the fallback fires
    /// exactly when no hook ever announced the plan, so there is no
    /// `PendingPlan` to hang it off.
    fn resolve_grid_plan(&mut self, sender: &ActivitySender) {
        let Some(id) = self.grid_plan.take() else {
            return;
        };
        self.live.remove(&id);
        sender.send(ActivityEvent::QuestionResolved {
            id: Some(id),
            ask_id: None,
            answers: None,
            dismissed: None,
            at: None,
        });
    }

    /// EXP-672: retire every live grid-only ask card. Two edges mean "that
    /// picker is gone": the transcript's own ask resolution (the answers
    /// flush, seen by [`Self::observe_published`]) and the question watcher's
    /// hide edge in the emitter loop — a fully hookless run whose claude
    /// withholds the ask entry until it is answered has only the latter. The
    /// watcher publishes no `Resolved` transition of its own (unlike the plan
    /// one), which is exactly why these cards used to be immortal.
    /// `resolution_seen` is deliberately NOT raised here: it is the
    /// publisher's grid-reroute flag and each caller already owns that edge.
    fn resolve_grid_asks(&mut self, sender: &ActivitySender) {
        for id in std::mem::take(&mut self.grid_asks) {
            self.live.remove(&id);
            sender.send(ActivityEvent::QuestionResolved {
                id: Some(id),
                ask_id: None,
                answers: None,
                dismissed: None,
                at: None,
            });
        }
    }

    /// EXP-672: publish a GRID-only card — one the hooks sidecar never
    /// announced and the transcript never described — with a stable synthetic
    /// identity instead of the pre-v2 `id: None`. It goes through
    /// [`Self::publish_question`] like every other card, so it is registered
    /// as answerable and a viewer never has to fall back to blind keystrokes.
    fn publish_grid_fallback(
        &mut self,
        session_seed: &str,
        kind: QuestionKind,
        text: String,
        options: Vec<QuestionOption>,
        multi_select: bool,
        sender: &ActivitySender,
    ) -> String {
        self.grid_seq += 1;
        let tag = match kind {
            QuestionKind::Plan => "plan",
            _ => "ask",
        };
        let id = synthetic_question_id(session_seed, tag, &text, self.grid_seq);
        // The PLAN slot stays the caller's to set — a superseding plan retires
        // its predecessor BEFORE publishing. An ask picker has no such
        // supersede edge, so its cards are collected here and retired as a
        // group ([`Self::resolve_grid_asks`]).
        if kind == QuestionKind::Ask {
            self.grid_asks.push(id.clone());
        }
        self.publish_question(
            Publishable {
                id: id.clone(),
                kind,
                ask_id: None,
                index: None,
                total: None,
                header: None,
                text,
                options,
                multi_select,
            },
            sender,
        );
        id
    }

    /// A hook delivery → published events + state.
    fn apply_hook(
        &mut self,
        event: HookEvent,
        sender: &ActivitySender,
        redactor: &Redactor,
        transcript: &mut TranscriptState,
    ) {
        // EXP-429: every delivery carries session identity — pin transcript
        // discovery to it before the kind is consumed.
        self.pin.observe(&event);
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
                // claude ≥2.1.233 fires a permission-flavoured Notification
                // for its pickers too, and it can land BEFORE this
                // registration — one TUI can't show a picker and a
                // permission dialog at once, so the hold is the picker's own
                // nudge, not a prompt (EXP-512).
                self.pending_permission = None;
                self.plan_seq += 1;
                // EXP-483: a REAL tool_use id will reappear on the withheld
                // twin entry — remember it so that entry's prose can anchor
                // above the published card. A synthetic `plan-N` id never
                // shows up in the transcript, so it would only leak.
                if let Some(id) = &tool_use_id {
                    transcript
                        .hook_published_plans
                        .insert(truncate(id, ID_MAX));
                }
                let id = tool_use_id.unwrap_or_else(|| format!("plan-{}", self.plan_seq));
                let text = truncate_marked(&redactor.redact(&plan), QUESTION_TEXT_MAX);
                // The transcript twin needs no arming here: with the sidecar
                // wired, `suppress_plan_twins` swallows it whether it flushes
                // before or after this delivery (EXP-691).
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
                // Same picker-nudge race as `PlanProposed` above (EXP-512).
                self.pending_permission = None;
                let Some(ask_id) = tool_use_id else {
                    // Without an id there is nothing to answer against — the
                    // grid path publishes it the legacy way.
                    return;
                };
                let ask_id = truncate(&ask_id, ID_MAX);
                // EXP-610: the ask can be DEAD by the time its hook drains —
                // the free-text Esc-reroute dismisses a picker the moment a
                // steered message lands, and the twin + result then flush in
                // the same tick, before this delivery. Publishing would mint
                // answerable cards (and a stepper) for a picker that no
                // longer exists, wedged forever: their `question_resolved`
                // already went by.
                if transcript.resolved_asks.iter().any(|id| id == &ask_id) {
                    return;
                }
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
                flush_forced_subagent_completions(&mut self.subagents, sender);
            }
            HookEventKind::SubagentStarted {
                agent_id,
                agent_type,
            } => {
                // EXP-443: whatever else this start means, a new sidechain
                // file may exist right now — skip the rescan debounce once.
                self.subagent_seen = true;
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
                flush_forced_subagent_completions(&mut self.subagents, sender);
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
                // Bypass mode used to downgrade this to an idle nudge on the
                // assumption skip-permissions can never prompt (EXP-275) —
                // false since claude flags DANGEROUS commands even under
                // `--dangerously-skip-permissions` ("Dangerous rm operation
                // on possibly-empty variable path", verified live on
                // v2.1.237), and the swallowed hold left those sessions
                // parked with nothing on the relay at all (EXP-564). A
                // bypass prompt now arms exactly like any other.
                self.attention = Some(Attention::Permission);
                // EXP-455: hold the informational card — the grid watcher
                // publishes the dialog as an ANSWERABLE question when it
                // confirms; only a never-confirmed prompt degrades to the
                // card ([`Self::permission_timeout`]). With the question
                // already live (or a hold still fresh), the nudge repeat
                // claude sends while parked adds nothing. A DEGRADED hold
                // already spent its card, and nothing retires it before turn
                // end when the grid detector keeps missing — so a further
                // Notification re-arms instead of being swallowed: each
                // prompt gets its own confirm window and, unconfirmed, its
                // own informational card, the pre-EXP-455 per-Notification
                // behavior (EXP-458). A repeat nudge for the SAME parked
                // prompt re-publishes ~10s later — the old posture too.
                let armable = self
                    .pending_permission
                    .as_ref()
                    .is_none_or(|pending| pending.degraded);
                if self.permission.is_none() && armable {
                    // EXP-529: a tool-less Notification ("Session paused")
                    // borrows the freshest transcript tool headline — the
                    // blocking call's own `tool_use` entry flushes just
                    // before the nudge — over the literal "Tool".
                    let tool = tool
                        .or_else(|| self.recent_tool())
                        .map(|tool| truncate(&tool, ID_MAX))
                        .unwrap_or_else(|| "Tool".to_string());
                    self.pending_permission = Some(PendingPermission {
                        tool,
                        detail: Some(truncate(&redactor.redact(&message), TOOL_DETAIL_MAX)),
                        seen: Instant::now(),
                        degraded: false,
                    });
                }
            }
            HookEventKind::Idle { .. } => {
                // Parked on the input box = between turns (EXP-637).
                self.turn_idle = true;
                self.attention = Some(Attention::Idle);
            }
            // The turn ended: whatever the session was waiting on is over.
            // Besides the attention flag, retire any ask/plan still marked
            // pending — normally the transcript flush resolves them
            // (`observe_published`), but a missed flush used to pin
            // `needs_input` and the clients' steppers forever (EXP-275).
            // A normally-answered ask is already gone here, so this is a
            // silent safety net; the transcript's enriched resolution (with
            // the collected answers) still follows when it does land.
            kind @ (HookEventKind::Stop | HookEventKind::SessionEnd { .. }) => {
                // EXP-404: arm the deferred sweep that completes still-open
                // subagent cards (the emitter runs it after this tick's
                // transcript drain). A SessionEnd never downgrades to a
                // TurnEnd sweep when both land in one drain.
                let sweep = match kind {
                    HookEventKind::SessionEnd { .. } => SubagentSweep::SessionEnd,
                    _ => SubagentSweep::TurnEnd,
                };
                if self.subagent_sweep != Some(SubagentSweep::SessionEnd) {
                    self.subagent_sweep = Some(sweep);
                }
                // EXP-637: the turn is over — the graceful stop may proceed.
                self.turn_idle = true;
                // EXP-679: the `Stop` hook IS claude's end-of-turn edge — the
                // one codex and pi already publish on their turn-complete
                // events. The agent is parked on the input box, so the synced
                // `needs_input` flag flips NOW instead of a minute later:
                // claude's "waiting for your input" Notification
                // ([`HookEventKind::Idle`]) fires ~60s in and stays only as
                // the backstop for a missed/suppressed Stop. Viewers rendered
                // a pulsing "Working…" for that whole window (forever when a
                // late transcript flush kept the flag cleared). A SessionEnd
                // is the process going away, not a human's turn: no parking.
                self.attention = match kind {
                    HookEventKind::SessionEnd { .. } => None,
                    _ => Some(Attention::Idle),
                };
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
                // The turn is over ⇒ no permission dialog is up either —
                // retire the card and drop an unconfirmed hold (EXP-455).
                self.resolve_permission(sender);
                // EXP-724: …and no compaction is still running (claude
                // compacts BETWEEN turns; a bar still open here lost its
                // end edge).
                self.end_compaction(sender);
            }
            // EXP-724: compaction opens the indeterminate bar on every
            // viewer. `custom_instructions` is deliberately NOT published —
            // the command line itself already echoed it, and the payload is
            // unredacted claude input.
            HookEventKind::CompactStarted { trigger, .. } => {
                self.start_compaction(trigger.as_deref(), sender);
            }
            HookEventKind::CompactEnded { .. } => self.end_compaction(sender),
            // EXP-443: pure pin fuel — `pin.observe` above already absorbed
            // the (possibly rotated) session id; no card, no attention edge.
            // EXP-724: `source == "compact"` is claude re-opening the session
            // on the compacted context — an end edge for a build whose
            // `PostCompact` hook never fires.
            HookEventKind::SessionStarted { source } => {
                if source.as_deref() == Some("compact") {
                    self.end_compaction(sender);
                }
            }
        }
    }

    /// EXP-724: open the compaction bar. A second `PreCompact` for an
    /// already-open compaction only refreshes the clock — never a second
    /// `started` on the wire.
    fn start_compaction(&mut self, trigger: Option<&str>, sender: &ActivitySender) {
        let fresh = self.compacting_since.is_none();
        self.compacting_since = Some(Instant::now());
        if fresh {
            sender.send(ActivityEvent::compaction(
                CompactionPhase::Started,
                normalize_compaction_trigger(trigger),
            ));
        }
    }

    /// EXP-724: close the compaction bar, once. Every end edge funnels here,
    /// so the four fallbacks can all fire without ever doubling the marker.
    fn end_compaction(&mut self, sender: &ActivitySender) {
        if self.compacting_since.take().is_some() {
            sender.send(ActivityEvent::compaction(CompactionPhase::Ended, None));
        }
    }

    /// EXP-724: the last-resort end edge — a compaction whose end never
    /// arrived at all ([`COMPACTION_MAX`]).
    fn compaction_timeout(&mut self, sender: &ActivitySender) {
        if self
            .compacting_since
            .is_some_and(|at| at.elapsed() >= COMPACTION_MAX)
        {
            log::debug!("activity: compaction bar timed out — closing it");
            self.end_compaction(sender);
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
        // A plan can exceed the narration budget (EXP-691: the question cap
        // is larger) — re-truncate for this channel.
        sender.send(ActivityEvent::narration(truncate_marked(
            &plan.text,
            NARRATION_MAX,
        )));
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

    /// EXP-691: the flushed `ExitPlanMode` tool_result names a plan — the
    /// transcript's own resolution edge, the one signal that still arrives
    /// while the grid watcher is scroll-stuck. Retires the pending card only
    /// when the id matches (a same-tick NEWER plan proposal must not be
    /// resolved by its predecessor's result); either way the answered picker
    /// is off the keyboard, which is what [`Self::take_resolution`] reports.
    fn resolve_plan_from_result(&mut self, tool_use_id: &str, sender: &ActivitySender) {
        self.resolution_seen = true;
        if self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.id == tool_use_id)
        {
            self.resolve_plan(sender);
            return;
        }
        // EXP-672: hookless (no sidecar, or one that never drained) with an
        // immediate-flush claude, the card on screen is the TRANSCRIPT twin —
        // published under claude's own `tool_use_id` and enrolled in `live` by
        // [`Self::observe_published`], with no `PendingPlan` behind it and the
        // grid fallback skipped (`twin_flushed`). Its result is the only
        // resolution evidence there is, so retire it by that id or an
        // answerable plan card sits on every viewer for the rest of the run.
        // Both sides mint the id through `truncate(.., ID_MAX)`, so they match
        // byte for byte.
        let id = truncate(tool_use_id, ID_MAX);
        if self.live.remove(&id).is_some() {
            sender.send(ActivityEvent::QuestionResolved {
                id: Some(id),
                ask_id: None,
                answers: None,
                dismissed: None,
                at: None,
            });
        }
    }

    /// The plan picker left the screen — answered, dismissed, or superseded.
    fn resolve_plan(&mut self, sender: &ActivitySender) {
        self.resolution_seen = true;
        // EXP-672: the grid-only fallback card retires on the same edge — it
        // has an id now, so it can be.
        self.resolve_grid_plan(sender);
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

    /// The `/login` method picker settled on screen (EXP-430) — publish it
    /// as an ordinary answerable question so every client's existing card UI
    /// carries the choice. Re-appearances (the OAuth-error retry loop lands
    /// back on the picker) retire the previous card and publish a fresh id.
    fn publish_login_question(&mut self, options: Vec<QuestionOption>, sender: &ActivitySender) {
        self.resolve_login(sender);
        self.login_seq += 1;
        let id = format!("login:{}", self.login_seq);
        self.login = Some(id.clone());
        self.publish_question(
            Publishable {
                id,
                kind: QuestionKind::Login,
                ask_id: None,
                index: None,
                total: None,
                header: Some("Claude sign-in required".to_string()),
                text: "Claude Code is signed out on this machine. Select a login method to sign \
                       in again:"
                    .to_string(),
                options,
                multi_select: false,
            },
            sender,
        );
    }

    /// The login method picker moved on — answered (remotely or at the local
    /// TUI), cancelled, or the flow advanced to the URL screen.
    fn resolve_login(&mut self, sender: &ActivitySender) {
        let Some(id) = self.login.take() else { return };
        // EXP-347: clear the publisher's grid-picker flag promptly — a stale
        // flag would Esc-inject in front of the very free text (the pasted
        // OAuth code) the next login phase asks for.
        self.resolution_seen = true;
        self.live.remove(&id);
        sender.send(ActivityEvent::QuestionResolved {
            id: Some(id),
            ask_id: None,
            answers: None,
            dismissed: None,
            at: None,
        });
    }

    /// A permission dialog settled on screen (EXP-455) — publish it as an
    /// ordinary answerable question so every client's existing card UI
    /// carries the approval. A changed dialog (back-to-back prompts) retires
    /// the previous card and publishes a fresh id. Consumes the pending
    /// hook nudge — the grid card supersedes the informational fallback.
    fn publish_permission_question(
        &mut self,
        snapshot: PermissionSnapshot,
        sender: &ActivitySender,
        redactor: &Redactor,
    ) {
        // Consume the hook hold BEFORE the resolve pass clears it — its tool
        // name is the header fallback for a headline-less dialog, and its
        // detail stands in for the question a leniently-detected dialog
        // (EXP-529) may not carry.
        let hook_hold = self.pending_permission.take();
        self.resolve_permission(sender);
        self.permission_seq += 1;
        let id = format!("permission:{}", self.permission_seq);
        self.permission = Some(id.clone());
        let header = snapshot
            .header
            .map(|header| truncate(&redactor.redact(&header), QUESTION_HEADER_MAX))
            .or_else(|| hook_hold.as_ref().map(|hold| hold.tool.clone()))
            .unwrap_or_else(|| "Permission required".to_string());
        let mut text = snapshot.question;
        if text.is_empty() {
            // An anchorless dialog parsed no question line — say what the
            // hook said ("Session paused", already redacted/truncated at arm
            // time) rather than shipping an empty card body.
            text = hook_hold
                .and_then(|hold| hold.detail)
                .unwrap_or_else(|| "Approval required".to_string());
        }
        if !snapshot.context.is_empty() {
            text.push_str("\n\n");
            text.push_str(&snapshot.context.join("\n"));
        }
        let options = snapshot
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
        self.publish_question(
            Publishable {
                id,
                kind: QuestionKind::Permission,
                ask_id: None,
                index: None,
                total: None,
                header: Some(header),
                text: truncate(&redactor.redact(&text), QUESTION_TEXT_MAX),
                options,
                multi_select: false,
            },
            sender,
        );
    }

    /// The permission dialog left the grid — answered (remotely or at the
    /// local TUI) or dismissed.
    fn resolve_permission(&mut self, sender: &ActivitySender) {
        self.pending_permission = None;
        if self.attention == Some(Attention::Permission) {
            self.attention = None;
        }
        let Some(id) = self.permission.take() else { return };
        self.resolution_seen = true;
        self.live.remove(&id);
        sender.send(ActivityEvent::QuestionResolved {
            id: Some(id),
            ask_id: None,
            answers: None,
            dismissed: None,
            at: None,
        });
    }

    /// The permission hook's degraded path: the grid never confirmed a
    /// dialog (claude's copy drifted past the anchors, or there is no term),
    /// so publish the legacy informational card rather than nothing.
    fn permission_timeout(&mut self, sender: &ActivitySender) {
        // A pending ask/plan owns the screen story — its card is the
        // answerable one, and a permission card would claim a block the
        // steerer can't act on (the arming guard's condition, re-checked
        // here because claude ≥2.1.233 sends the picker's own Notification
        // BEFORE the hook registers the picker — EXP-512 saw the raced hold
        // degrade into a phantom "Permission · Tool" card 10s into a
        // perfectly answerable ask).
        if self.ask.is_some() || self.plan.is_some() {
            self.pending_permission = None;
            return;
        }
        let Some(pending) = &mut self.pending_permission else {
            return;
        };
        if pending.degraded || pending.seen.elapsed() < PERMISSION_GRID_CONFIRM {
            return;
        }
        pending.degraded = true;
        sender.send(ActivityEvent::Permission {
            tool: pending.tool.clone(),
            detail: pending.detail.clone(),
            at: None,
        });
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
    /// cards, progress clears a stale permission block, and a new turn's
    /// first event un-parks an idle one (EXP-679). Takes the sender because
    /// retiring a grid-only ask card (EXP-672) has to reach viewers.
    fn observe_published(&mut self, event: &ActivityEvent, sender: &ActivitySender) {
        // EXP-529: remember the freshest tool headline — the tool-less
        // Notification's name fallback (see [`Self::recent_tool`]).
        if let ActivityEvent::Tool { name, .. } = event {
            self.last_tool = Some((name.clone(), Instant::now()));
        }
        // EXP-672: a transcript-born question card now carries a stable id
        // (`parse_ask_user_question` / `parse_exit_plan_mode`) — register it
        // so [`Self::handle_answer`] can drive it and no viewer is pushed onto
        // the legacy keystroke path. Only the FALLBACK lands here: with the
        // hooks sidecar wired every twin is swallowed upstream, and the hook's
        // own cards were inserted by [`Self::publish_question`] already.
        if let ActivityEvent::Question {
            id: Some(id),
            text,
            options,
            multi_select,
            plan_mode,
            ask_id,
            ..
        } = event
        {
            if !self.live.contains_key(id) && !self.answered.contains(id) {
                self.live.insert(
                    id.clone(),
                    LiveQuestion {
                        kind: if *plan_mode == Some(true) {
                            QuestionKind::Plan
                        } else {
                            QuestionKind::Ask
                        },
                        ask_id: ask_id.clone(),
                        text_norm: normalize_question_text(text),
                        options: options.clone(),
                        multi_select: multi_select.unwrap_or(false),
                    },
                );
            }
        }
        if let ActivityEvent::QuestionResolved { ask_id, .. } = event {
            self.resolution_seen = true;
            if ask_id.is_some() {
                // EXP-672: the ask that just resolved is the one whose picker
                // was on the grid — the grid-only cards belong to it but
                // carry no `ask_id` to be matched on, so they retire here.
                self.resolve_grid_asks(sender);
            }
            if let Some(ask_id) = ask_id {
                self.live
                    .retain(|_, live| live.ask_id.as_deref() != Some(ask_id.as_str()));
                if self.ask.as_ref().is_some_and(|ask| &ask.ask_id == ask_id) {
                    self.ask = None;
                }
            }
        }
        // EXP-679: transcript progress clears a `Permission` block as it
        // always has — anything published means the blocking call is through.
        // An `Idle` park is different now that the `Stop` hook sets it: the
        // final assistant entry is often flushed AFTER the hook lands, and a
        // blanket clear here silently un-idled the session for good (claude
        // never re-sends its idle Notification for the same idle period).
        // So `Idle` only lifts on evidence of a NEW turn: a human message, or
        // the agent dispatching a tool. Assistant prose, narration, subagent
        // cards, diffs and resolutions leave it parked.
        // [`Self::note_agent_activity`] is the other clearing edge (tool
        // dispatch + subagent lifecycle, EXP-355).
        match self.attention {
            Some(Attention::Permission) => self.attention = None,
            Some(Attention::Idle) => {
                if matches!(
                    event,
                    ActivityEvent::UserMessage { .. } | ActivityEvent::Tool { .. }
                ) {
                    self.attention = None;
                }
            }
            None => {}
        }
        // NOTE (EXP-455): a held `pending_permission` deliberately survives
        // transcript progress — the pending tool's own `tool_use` entry
        // flushes around the same moment the Notification fires, so clearing
        // here would kill the degraded fallback in the common case. A
        // locally-answered prompt the grid never confirmed can thus still
        // surface its informational card up to 10s late — no worse than the
        // old immediate-card behavior.
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
            // Scrolled into history — the visible grid is not the picker, and
            // nothing will move it back on its own while the TUI is parked on
            // the picker: refusing here refused FOREVER (every tap parked,
            // TTL-dropped, never acked — EXP-611: reading a long plan on the
            // desktop and then approving from the phone always failed). The
            // answer targets the LIVE picker by construction, so snap the
            // viewport to the bottom exactly like local input would and
            // proceed against the live rows.
            scroll_to_bottom(term);
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
            QuestionKind::Login => {
                // Only answerable while the method picker is the visible
                // phase — a tap that lands after the flow advanced (or after
                // a local answer) must not type into the URL/paste screen.
                let visible = match login_picker::detect(&lines) {
                    Some(LoginPhase::MethodPicker { options }) => options,
                    _ => return AnswerAttempt::Retry,
                };
                let Some(key) = answer.keys.first() else {
                    return AnswerAttempt::Settled;
                };
                if !live.options.iter().any(|option| &option.key == key)
                    || !visible.iter().any(|option| &option.key == key)
                {
                    return AnswerAttempt::Settled;
                }
                write_input(key.as_bytes());
                // EXP-444: arm the anchor-drift diagnostic — a recognizable
                // login phase (or success) must follow within the window.
                self.login_injected_at = Some(Instant::now());
                // Same digit-then-probe-then-Enter choreography as the plan
                // picker (EXP-334): whether the digit submits or only moves
                // the cursor, the row ends up activated exactly once.
                let moved = || {
                    !matches!(
                        login_picker::detect(&screen_lines(term)),
                        Some(LoginPhase::MethodPicker { .. })
                    )
                };
                if !settle_for(PLAN_SUBMIT_PROBE, moved) {
                    write_input(b"\r");
                    if !settle(moved) {
                        return AnswerAttempt::Settled; // injected — never twice
                    }
                }
            }
            QuestionKind::Permission => {
                // Only answerable while a permission dialog is actually
                // visible — a tap that lands after a local answer must not
                // type into whatever replaced it. Lenient always (EXP-529):
                // a leniently published card would otherwise Retry forever,
                // and lenient is a strict superset gated by a live Permission
                // card plus the key checks below.
                let Some(visible) = permission_picker::detect_lenient(&lines) else {
                    return AnswerAttempt::Retry;
                };
                let Some(key) = answer.keys.first() else {
                    return AnswerAttempt::Settled;
                };
                if !live.options.iter().any(|option| &option.key == key)
                    || !visible.options.iter().any(|option| &option.key == key)
                {
                    return AnswerAttempt::Settled;
                }
                write_input(key.as_bytes());
                // Digit-then-probe-then-Enter, as ever (EXP-334). "Moved"
                // compares SNAPSHOTS, not mere presence: approving one
                // prompt can paint the next dialog in its place within the
                // probe window, and an Enter fired at that new dialog would
                // activate its highlighted row.
                let moved = || match permission_picker::detect_lenient(&screen_lines(term)) {
                    None => true,
                    Some(next) => next != visible,
                };
                if !settle_for(PLAN_SUBMIT_PROBE, moved) {
                    write_input(b"\r");
                    if !settle(moved) {
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
                    let Some(visible) = snapshot.options.iter().find(|option| &option.key == key)
                    else {
                        // The tab that is up doesn't offer this key — likely a
                        // stale frame between tabs.
                        return AnswerAttempt::Retry;
                    };
                    if visible.free_text {
                        // EXP-513: claude's synthetic free-text row. Its digit
                        // only MOVES the cursor; typed characters then fill
                        // the row in place and Enter submits them as this
                        // question's answer — while Enter on the still-EMPTY
                        // row DECLINES the whole ask (observed v2.1.233). So:
                        // without a reply there is nothing safe to inject —
                        // refuse without an ack rather than nuke every answer
                        // (the pre-EXP-513 blind digit-then-Enter did exactly
                        // that).
                        let text = answer
                            .text
                            .as_deref()
                            .map(sanitize_answer_text)
                            .filter(|text| !text.is_empty());
                        let Some(text) = text else {
                            return AnswerAttempt::Settled;
                        };
                        let key_number: Option<u32> = key.parse().ok();
                        write_input(key.as_bytes());
                        // Type only once the cursor verifiably sits on the
                        // row — characters typed elsewhere are eaten and the
                        // closing Enter would activate the highlighted row.
                        // The move is a single repaint, so the short probe
                        // window keeps the whole choreography inside the
                        // clients' 8s ack budget (see ANSWER_RETRY_TTL docs).
                        let on_row = || {
                            question_picker::selected_option(&screen_lines(term)) == key_number
                        };
                        if key_number.is_none() || !settle_for(PLAN_SUBMIT_PROBE, on_row) {
                            return AnswerAttempt::Settled; // digit injected — never twice
                        }
                        std::thread::sleep(KEYSTROKE_GAP);
                        write_input(text.as_bytes());
                        std::thread::sleep(KEYSTROKE_GAP);
                        write_input(b"\r");
                        if !settle(step_moved) {
                            return AnswerAttempt::Settled; // injected — never twice
                        }
                    } else {
                        write_input(key.as_bytes());
                        // Classic ask pickers submit on the digit; a
                        // PREVIEW-carrying question renders side-by-side and
                        // its digit only MOVES the cursor — Enter activates
                        // the row (EXP-394, the same digit-then-Enter probe
                        // the plan picker needed in EXP-334). Never after a
                        // multiSelect Tab: a trailing Enter there
                        // toggles/answers whatever the cursor sits on next.
                        if !settle_for(PLAN_SUBMIT_PROBE, step_moved) {
                            write_input(b"\r");
                        }
                        if !settle(step_moved) {
                            return AnswerAttempt::Settled; // injected — never twice
                        }
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
pub(crate) enum AnswerAttempt {
    /// Handled for good — acked, or dropped for a reason retrying can't fix.
    Settled,
    /// Transient refusal — the poll loop may try again next tick.
    Retry,
}

/// Poll `done` until it holds or [`ANSWER_SETTLE`] elapses.
pub(crate) fn settle(done: impl FnMut() -> bool) -> bool {
    settle_for(ANSWER_SETTLE, done)
}

/// Poll `done` until it holds or `window` elapses.
pub(crate) fn settle_for(window: Duration, mut done: impl FnMut() -> bool) -> bool {
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

/// Cap on a free-text reply typed into the TUI (EXP-513) — belt-and-braces
/// behind the relay schema's own 4000-char bound.
const ANSWER_TEXT_MAX: usize = 4000;

/// One safe LINE out of a steerer's free-text reply (EXP-513): the TUI's
/// inline editor is single-line and every control byte is a potential
/// keystroke — newlines/tabs would submit or navigate mid-reply, an ESC
/// would dismiss the picker.
fn sanitize_answer_text(text: &str) -> String {
    let mut out: String = text
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    if out.chars().count() > ANSWER_TEXT_MAX {
        out = out.chars().take(ANSWER_TEXT_MAX).collect();
    }
    out.trim().to_string()
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
            // Synthetic rows are grid-only — a hook option is never one.
            free_text: false,
        })
        .collect()
}

/// The picker's REAL rows, with their real keys.
/// EXP-404: publish the completion edges the subagent cap eviction owes —
/// a live card dropped from the bookkeeping already has its Started on the
/// wire, and nothing else can ever complete it. Called after every path
/// that can card a new subagent (dispatch, unbound start, sidechain
/// absorb), so the eviction and its owed edge publish in the same tick.
fn flush_forced_subagent_completions(subagents: &mut Subagents, sender: &ActivitySender) {
    for (id, agent_type, detail) in subagents.take_forced_completions() {
        sender.send(ActivityEvent::Subagent {
            id,
            agent_type,
            status: SubagentStatus::Completed,
            detail,
            at: None,
        });
    }
}

/// The plan picker's remotely-offered options. Two rows are dropped from the
/// wire (their keys stay real, so a keystroke still lands on the right TUI
/// row): the "refine with Ultraplan on Claude Code on the web" hand-off —
/// not something a remote steerer should trigger blind, same stance the
/// transcript fallback (`parse_exit_plan_mode`) takes by construction — and
/// "Tell Claude what to change" (EXP-529), which remotely only parks the TUI
/// in an inline-feedback editor no viewer can see; the composer already IS
/// that affordance (free text steered at a pending picker Escs it and lands
/// as feedback).
fn plan_publish_options(
    options: Vec<QuestionOption>,
    redactor: &Redactor,
) -> Vec<QuestionOption> {
    options
        .into_iter()
        .filter(|o| {
            !o.label.contains(ULTRAPLAN_WEB_OPTION) && !o.label.contains(PLAN_FEEDBACK_OPTION)
        })
        .take(QUESTION_OPTIONS_MAX)
        .map(|o| QuestionOption::new(truncate(&redactor.redact(&o.label), OPTION_LABEL_MAX), o.key))
        .collect()
}

fn grid_options(snapshot: &QuestionSnapshot, redactor: &Redactor) -> Vec<QuestionOption> {
    snapshot
        .options
        .iter()
        .take(QUESTION_OPTIONS_MAX)
        .map(|option| QuestionOption {
            label: truncate(&redactor.redact(&option.label), OPTION_LABEL_MAX),
            key: option.key.clone(),
            description: None,
            // EXP-513: the picker marks the synthetic free-text row — keep
            // the flag on the wire so clients collect a reply first.
            free_text: option.free_text,
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
    /// EXP-688: the ref the published diff is measured from —
    /// `origin/<default branch>` for issue/batch/action runs, `None` for a
    /// chat/scratch run (and every headless caller). See [`worktree_diff`].
    pub base_ref: Option<String>,
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
    /// (`--dangerously-skip-permissions` / codex bypass). Feeds the launch
    /// narration only — a bypass session still hits REAL permission prompts
    /// (claude flags dangerous commands even then, EXP-564), so the
    /// permission machinery runs identically in both postures.
    pub bypass_permissions: bool,
    /// EXP-529: the session launched into plan mode (claude
    /// `--permission-mode plan` / pi's plan extension) — mutually exclusive
    /// with `bypass_permissions` (the launcher derives bypass as
    /// `skip && !plan`). Only feeds the launch narration, so a remote viewer
    /// can tell WHICH posture a run actually started with.
    pub plan_mode: bool,
    /// EXP-443: the launcher-minted claude session id (`--session-id`) — the
    /// transcript pin is seeded with it BEFORE the first tick, so discovery
    /// never runs unpinned on a fresh session. `None` on resume (the
    /// SessionStart hook seeds the pin) and for codex/pi.
    pub claude_session_id: Option<String>,
    /// EXP-443: the per-spawn originator the launcher stamped into codex's
    /// env — rollout discovery prefers a meta whose `originator` matches.
    /// `None` degrades to the legacy cwd+mtime match.
    pub codex_originator: Option<String>,
    /// EXP-443: the exact rollout session id a codex native resume reopens —
    /// discovery pins to it outright when set.
    pub codex_resume_id: Option<String>,
    /// EXP-444/EXP-432: the session was started by a foreign requester on
    /// this SHARED host device. The remote login flow is suppressed for them
    /// (an OAuth sign-in would bind the HOST's machine and billing to the
    /// requester's Anthropic account) — narration only, no tappable flow.
    pub foreign_host: bool,
    /// EXP-637: the shared turn-state signal the graceful stop waits on.
    /// `None` = no graceful stop for this session (tests, hosts that tear
    /// down immediately).
    pub turn_signal: Option<Arc<TurnSignal>>,
}

/// EXP-637 — the "is the agent between turns?" signal, shared by the emitter
/// (which flips it) and the graceful-stop path (which waits on it).
///
/// When the agent declares its run over via `exponential_sessions_end`, the
/// server ends the row while the CLI is still mid-turn — writing its final
/// message, flushing its transcript. Killing it right then truncates exactly
/// the output the close-out is about, so the host waits for the next idle
/// edge (bounded by a timeout) before tearing anything down.
///
/// Deliberately tiny and lock-free on the read path: the emitter thread
/// touches it on every hook/event tick.
#[derive(Debug, Default)]
pub struct TurnSignal {
    idle: AtomicBool,
    waiters: Mutex<Vec<flume::Sender<()>>>,
}

impl TurnSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_idle(&self) -> bool {
        self.idle.load(Ordering::Relaxed)
    }

    /// Record the agent's turn state. A false→true edge wakes every waiter;
    /// repeated `true`s are cheap no-ops.
    pub fn set_idle(&self, idle: bool) {
        let was = self.idle.swap(idle, Ordering::Relaxed);
        if idle && !was {
            self.wake();
        }
    }

    /// A receiver that fires once the agent is between turns — IMMEDIATELY
    /// when it already is, otherwise on the next false→true edge.
    pub fn subscribe(&self) -> flume::Receiver<()> {
        let (tx, rx) = flume::bounded(1);
        if self.is_idle() {
            let _ = tx.try_send(());
            return rx;
        }
        match self.waiters.lock() {
            Ok(mut waiters) => waiters.push(tx),
            Err(poisoned) => poisoned.into_inner().push(tx),
        }
        // Re-check after registering: the edge may have fired in between.
        if self.is_idle() {
            self.wake();
        }
        rx
    }

    fn wake(&self) {
        let waiters = match self.waiters.lock() {
            Ok(mut waiters) => std::mem::take(&mut *waiters),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for waiter in waiters {
            let _ = waiter.try_send(());
        }
    }
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

/// The debounced changed-only worktree diff snapshot — step 8 of every
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

    pub(crate) fn tick(
        &mut self,
        worktree: &Path,
        base_ref: Option<&str>,
        sender: &ActivitySender,
        redactor: &Redactor,
    ) {
        let due = self.last_at.is_none_or(|at| at.elapsed() >= DIFF_INTERVAL);
        if !due {
            return;
        }
        self.last_at = Some(Instant::now());
        // A git failure (index lock, rebase in flight) is not an empty diff:
        // keep the last answer and try again next tick.
        let Some(diff) = worktree_diff(worktree, base_ref) else {
            return;
        };
        if diff == self.last {
            return;
        }
        let had_diff = !self.last.is_empty();
        self.last = diff.clone();
        // EXP-688: a diff that goes EMPTY publishes an explicit empty frame
        // (the wire allows `""`, and every client treats it as "no diff").
        // Sending nothing left viewers looking at a stale patch forever.
        if !diff.is_empty() || had_diff {
            sender.send(ActivityEvent::diff(truncate(
                &redactor.redact(&diff),
                DIFF_MAX,
            )));
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

/// The session-announcement narration, carrying the run's effective
/// permission posture (EXP-529): the user in the incident could not tell
/// whether their run actually skipped permissions — the suffix (or its
/// absence) answers that from any client, with no wire or client changes
/// (static strings, no Redactor pass needed; the only client-matched
/// narrations are the PLAN_RESOLVED/QUESTION_* constants).
pub(crate) fn launch_narration(bypass_permissions: bool, plan_mode: bool) -> &'static str {
    match (bypass_permissions, plan_mode) {
        (true, _) => "Session started · permissions skipped",
        (false, true) => "Session started · plan mode",
        (false, false) => "Session started",
    }
}

/// EXP-564: re-arm the permission watcher when its published question was
/// retired while the dialog is still on the grid. The Stop hook retires the
/// question on the "turn over ⇒ no dialog" assumption — false for a
/// BACKGROUND subagent's dialog, which outlives the main turn (and claude
/// prompts for dangerous commands even in bypass mode, so a fan-out session
/// can park on one exactly like this). Unlatched, the steady dialog re-fires
/// `Show` a debounce later and re-publishes as a fresh question; a dialog
/// that really left the grid re-fires nothing. A pending ask/plan keeps the
/// watcher latched — their screens own the story, and the Show-suppression
/// path already unlatches when they clear (EXP-458).
fn reconcile_permission_watcher(
    watcher: &mut PermissionPickerWatcher,
    steer: &SteerState,
) {
    if watcher.is_pending()
        && steer.permission.is_none()
        && steer.ask.is_none()
        && steer.plan.is_none()
    {
        watcher.unlatch();
    }
}

fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    let mut exact_secrets = secrets_from_worktree(&config.worktree);
    exact_secrets.extend(config.extra_secrets.iter().cloned());
    let redactor = Redactor::new(exact_secrets);

    // Announce the session (the viewer shows this immediately, before any
    // transcript line lands).
    sender.send(ActivityEvent::narration(launch_narration(
        config.bypass_permissions,
        config.plan_mode,
    )));

    let spawn_time = SystemTime::now();
    // EXP-672: seeds the synthetic ids the GRID fallbacks mint. The
    // launcher-minted claude session id when there is one; otherwise a token
    // unique to this run (a resume relaunches claude with `--resume`, not a
    // fresh `--session-id`), so two sessions in the same worktree can never
    // mint the same card identity. A resumed run is its OWN `coding_sessions`
    // row on its own relay room, so the worktree + spawn-nanos seed is enough
    // — no viewer ever sees two runs' cards side by side.
    let session_seed = config.claude_session_id.clone().unwrap_or_else(|| {
        let nanos = spawn_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("{}:{nanos}", config.worktree.display())
    });
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
    // EXP-672: the previous tick's `question_watcher.is_pending()` — its
    // true→false edge retires the grid-only ask cards.
    let mut grid_ask_pending = false;
    let mut login_watcher = LoginWatcher::new();
    let mut permission_watcher = PermissionPickerWatcher::new();
    let mut transcript_state = TranscriptState {
        suppress_task_headlines: config.hooks.is_some(),
        suppress_ask_questions: config.hooks.is_some(),
        suppress_plan_twins: config.hooks.is_some(),
        ..TranscriptState::default()
    };
    let mut steer = SteerState::default();
    // EXP-443: pinned from tick zero — with the spawn-minted `--session-id`
    // seeded, `owns_main` never falls back to the blanket "newest file in
    // the cwd" and a foreign claude sharing the cwd is never tailed. If
    // claude ever ignored the flag, the SessionStart hook self-heals the set
    // within one delivery (the pin is a union).
    if let Some(id) = &config.claude_session_id {
        steer.pin.seed(id);
    }
    // EXP-214/EXP-355: the synced needs-input flag (see [`NeedsInputForwarder`]).
    let mut needs_input = NeedsInputForwarder::new();
    // EXP-334: transiently refused remote answers, retried each tick until
    // [`ANSWER_RETRY_TTL`] — a tap that beats the picker paint must not be
    // dropped on the floor.
    let mut parked_answers: Vec<(RemoteAnswer, Instant)> = Vec::new();
    // EXP-724: remote slash commands waiting on a busy composer. Claude is
    // never gated, so this only ever holds a command refused by a live
    // picker for the length of one tick.
    let mut parked_commands: Vec<(crate::commands::ParsedCommand, Instant)> = Vec::new();
    // EXP-334: whether the last grid look (bottom of scrollback) showed a
    // picker — the publisher's free-text reroute signal. Sticky while
    // scrolled, like the watchers.
    let mut grid_picker_visible = false;
    // EXP-444: the visible login phase solicits free text (the OAuth-code
    // prompt / error-retry screen) — its unsuccessful disappearance arms the
    // publisher's mistimed-paste refusal.
    let mut login_code_soliciting = false;
    // EXP-444: the shared-host suppression narrated for the current login
    // flow already (reset when the flow leaves the grid / succeeds).
    let mut foreign_login_notified = false;

    while active.load(Ordering::SeqCst) {
        // 0) Resolve / re-resolve the transcript file (a newer session file in
        //    the same dir supersedes; reset the read offset when it changes).
        //    Discovery is pinned to the session ids the hooks announced
        //    (EXP-429) — the project dir is cwd-keyed, so without the pin a
        //    plain "+" agent-shell tab sharing the trunk cwd would hijack the
        //    feed of a completed run. (A pin learned from a hook this tick
        //    takes effect next tick — a 1-tick discovery lag, nothing more.)
        if let Some(dir) = &transcript_dir {
            if let Some(newest) = newest_transcript(dir, spawn_time, &steer.pin) {
                if current.as_deref() != Some(newest.as_path()) {
                    current = Some(newest);
                    offset = 0;
                }
                transcript_deadline = None;
            } else {
                // EXP-429: a pinned emitter never keeps tailing an unpinned
                // file — a foreign transcript grabbed in the pre-first-hook
                // window self-heals here.
                if steer.pin.pinned() && current.take().is_some() {
                    offset = 0;
                }
                if let Some(deadline) = transcript_deadline {
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
        }

        // 1) Tail any new complete lines from the current transcript — FIRST,
        //    before the hooks and the grid publish anything (EXP-483): claude
        //    writes an assistant entry to the JSONL before the PreToolUse
        //    hook fires / the picker paints, so tailing first guarantees
        //    already-flushed prose reaches the wire ahead of a same-tick
        //    question card. Twin suppression is identity/flag-based, never
        //    order-based (EXP-610 for asks, EXP-691 for plans): claude may
        //    flush an ExitPlanMode/AskUserQuestion entry the moment the tool
        //    is called — BEFORE its own hook drains — or withhold it until
        //    the picker is answered; the wired flags swallow the twin on
        //    either timing.
        if let Some(path) = current.clone() {
            offset = tail_transcript(
                &path,
                offset,
                &mut |line| process_transcript_line(line, &redactor, &mut transcript_state),
                &mut |event| {
                    steer.observe_published(&event, &sender);
                    sender.send(event);
                },
            );
        }

        // 1-bis) EXP-724: the transcript's own end-of-compaction marker
        //    (`system`/`compact_boundary`) — the backstop for a claude whose
        //    `PostCompact` hook never fires. Only closes a bar this emitter
        //    opened; an unmatched boundary publishes nothing.
        if std::mem::take(&mut transcript_state.compact_boundary) {
            steer.end_compaction(&sender);
        }

        // 2) The hooks sidecar (EXP-249): the structured half. Drained before
        //    the grid so a picker that paints in the same tick is already
        //    known by identity when the watcher confirms it.
        if let Some(hooks) = &config.hooks {
            while let Ok(event) = hooks.try_recv() {
                steer.apply_hook(event, &sender, &redactor, &mut transcript_state);
            }
        }
        steer.plan_timeout(&sender);
        steer.permission_timeout(&sender);
        steer.compaction_timeout(&sender);

        // 3) Picker watch on the live grid: the transcript never carries the
        //    picker's REAL option rows and (on withholding claudes) cannot
        //    show a PENDING plan approval or AskUserQuestion at all
        //    (EXP-150/EXP-197) — the picker is on screen exactly while it is
        //    pending.
        if let Some(term) = &config.term {
            let lines = screen_lines(term);
            let grid_offset = display_offset(term);
            match picker_watcher.tick(&lines, grid_offset) {
                Some(Transition::Show(snapshot)) => {
                    let options = plan_publish_options(snapshot.options, &redactor);
                    if !steer.confirm_plan_from_grid(options.clone(), &sender) {
                        // No hook knows this plan (an old claude, a sidecar
                        // that never came up, or a plan already degraded to
                        // narration): an id-less card with a headline instead
                        // of the body. EXP-249 dropped the `~/.claude/plans`
                        // mtime guessing that used to fill it in — it mixed
                        // up concurrent sessions, and the hook carries the
                        // exact plan. EXP-691: on immediate-flush claude the
                        // twin already flushed (`pending_plans` non-empty) —
                        // hookless, the transcript published the FULL plan
                        // body as an id-less card, and a generic card here
                        // would present the plan twice, so skip it; wired,
                        // that twin was swallowed and this card is still the
                        // only one.
                        // EXP-672: it carries a stable synthetic id now, so
                        // this card is answerable like every other one — the
                        // keystroke path is no longer its only way in.
                        let twin_flushed = !transcript_state.pending_plans.is_empty();
                        if transcript_state.suppress_plan_twins || !twin_flushed {
                            // A superseding plan retires its predecessor's
                            // card rather than orphaning it.
                            steer.resolve_grid_plan(&sender);
                            let id = steer.publish_grid_fallback(
                                &session_seed,
                                QuestionKind::Plan,
                                "Plan ready for approval.".to_string(),
                                options,
                                false,
                                &sender,
                            );
                            steer.grid_plan = Some(id);
                        }
                        // Old-claude withholding, no sidecar: the twin echoes
                        // AFTER the answer — pre-pay exactly one swallow.
                        // Never armed once a twin has already flushed: that
                        // late arm is what doubled the plan and then ate the
                        // next legitimate card (EXP-691).
                        if !transcript_state.suppress_plan_twins && !twin_flushed {
                            transcript_state.swallow_next_plan_twin = true;
                        }
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
                // The login METHOD PICKER counts (free text typed at it would
                // be eaten and its trailing Enter would activate the
                // highlighted row) — the URL/paste and error phases must NOT:
                // free text there IS the OAuth code (or a retry nudge), and
                // the reroute's Esc would abort the login (EXP-430).
                // A permission dialog counts too (EXP-455): the reroute's
                // Esc lands on "No, and tell Claude what to do differently"
                // and the steered text then arrives as exactly that feedback
                // — typed INTO the dialog it would be eaten and its trailing
                // Enter would activate the highlighted row. Leniency-aware
                // (EXP-529): free text steered at an anchorless dialog must
                // reroute exactly like at an anchored one.
                grid_picker_visible = question_detection.is_some()
                    || plan_picker::detect(&lines).is_some()
                    || (if steer.permission_grid_leniency() {
                        permission_picker::detect_lenient(&lines)
                    } else {
                        permission_picker::detect(&lines)
                    })
                    .is_some()
                    || matches!(
                        login_picker::detect(&lines),
                        Some(LoginPhase::MethodPicker { .. })
                    );
            }
            if let Some(snapshot) = question_watcher.tick(question_detection, grid_offset) {
                if !steer.confirm_question_from_grid(&snapshot, &sender, &redactor) {
                    let text = truncate(&redactor.redact(&snapshot.text), QUESTION_TEXT_MAX);
                    transcript_state.remember_grid_question(&text);
                    // EXP-672: the pre-v2 grid-only publication carried
                    // `id: None` and was answerable only by blind keystrokes.
                    // A stable synthetic id makes it an ordinary answerable
                    // card on every viewer.
                    let options = grid_options(&snapshot, &redactor);
                    steer.publish_grid_fallback(
                        &session_seed,
                        QuestionKind::Ask,
                        text,
                        options,
                        snapshot.multi_select,
                        &sender,
                    );
                }
            }
            // EXP-672: the ask picker left the grid — retire any grid-only
            // cards it left behind. The question watcher exposes no
            // `Resolved` transition (see [`QuestionPickerWatcher`]), so its
            // debounced `is_pending` edge IS the hide signal; on a hookless
            // run whose claude withholds the ask entry until it is answered
            // it is the ONLY one (the transcript's answers flush retires them
            // through `observe_published` when it does arrive, and a drained
            // set makes the second edge a no-op).
            let ask_pending = question_watcher.is_pending();
            if grid_ask_pending && !ask_pending {
                steer.resolve_grid_asks(&sender);
            }
            grid_ask_pending = ask_pending;
            // EXP-430: the `/login` flow. A steered `/login` reaches the PTY
            // fine, but the TUI it opens renders only on the grid — without
            // this watcher a headless-server session dead-ends on an expired
            // OAuth token. The method picker becomes an answerable question;
            // the sign-in URL travels as narration (clients linkify it) and
            // the OAuth code comes back as an ordinary free-text message.
            match login_watcher.tick(&lines, grid_offset) {
                Some(login_picker::Transition::Show(phase))
                    if config.foreign_host && !matches!(phase, LoginPhase::Success) =>
                {
                    // EXP-444/EXP-432: a foreign requester on a shared host
                    // never gets the interactive flow — signing in would bind
                    // the HOST's machine and billing to the requester's own
                    // Anthropic account. Narrate once per flow; the login
                    // watcher's pending state still trips needs_input below,
                    // so the row shows blocked.
                    login_code_soliciting = false;
                    steer.login_injected_at = None;
                    if !foreign_login_notified {
                        foreign_login_notified = true;
                        let copy = match phase {
                            LoginPhase::Error { .. } => "Claude sign-in failed on the host device.",
                            _ => {
                                "Claude is signed out on this machine. The device owner needs to \
                                 sign in on the host — remote sign-in is disabled for sessions \
                                 started on a shared device."
                            }
                        };
                        sender.send(ActivityEvent::narration(copy));
                    }
                }
                Some(login_picker::Transition::Show(LoginPhase::MethodPicker { options })) => {
                    login_code_soliciting = false;
                    steer.login_injected_at = None;
                    if let Some(steering) = &config.steering {
                        steering.link.disarm_login_refusal();
                    }
                    let options: Vec<QuestionOption> = options
                        .into_iter()
                        .filter(|o| !o.label.contains(LOGIN_THIRD_PARTY_OPTION))
                        .take(QUESTION_OPTIONS_MAX)
                        .map(|o| {
                            QuestionOption::new(
                                truncate(&redactor.redact(&o.label), OPTION_LABEL_MAX),
                                o.key,
                            )
                        })
                        .collect();
                    if !options.is_empty() {
                        steer.publish_login_question(options, &sender);
                    }
                }
                Some(login_picker::Transition::Show(LoginPhase::UrlPrompt { url })) => {
                    login_code_soliciting = true;
                    steer.login_injected_at = None;
                    if let Some(steering) = &config.steering {
                        steering.link.disarm_login_refusal();
                    }
                    // The method question is done — the flow moved on.
                    steer.resolve_login(&sender);
                    if login_picker::is_trusted_login_url(&url) {
                        // Exact-match redaction only: the generic patterns can
                        // shred the base64url blob mid-URL (see
                        // [`Redactor::redact_exact_only`]) — reachable ONLY
                        // for allowlisted hosts (EXP-444: the sole bypass of
                        // SECRET_PATTERNS must never carry an arbitrary URL
                        // an agent painted onto the grid).
                        sender.send(ActivityEvent::narration(truncate(
                            &redactor.redact_exact_only(&format!(
                                "Claude sign-in: open this link in your browser to authorize, then \
                                 send the code you receive back here as a regular message:\n\n{url}"
                            )),
                            NARRATION_MAX,
                        )));
                    } else {
                        // Full redaction and no invitation to open anything —
                        // a sign-in screen pointing off the Anthropic domains
                        // is a phishing primitive, not a login.
                        sender.send(ActivityEvent::narration(truncate(
                            &format!(
                                "Claude's sign-in screen showed a link on an unrecognized domain \
                                 — it was not shared. The host may need to sign in on the device \
                                 directly. ({})",
                                redactor.redact(&url)
                            ),
                            NARRATION_MAX,
                        )));
                    }
                }
                Some(login_picker::Transition::Show(LoginPhase::Error { message })) => {
                    login_code_soliciting = true;
                    steer.login_injected_at = None;
                    if let Some(steering) = &config.steering {
                        steering.link.disarm_login_refusal();
                    }
                    steer.resolve_login(&sender);
                    sender.send(ActivityEvent::narration(truncate(
                        &format!(
                            "Claude sign-in failed ({}). Send any message to retry — the login \
                             options will come back.",
                            redactor.redact(&message)
                        ),
                        NARRATION_MAX,
                    )));
                }
                Some(login_picker::Transition::Show(LoginPhase::Success)) => {
                    login_code_soliciting = false;
                    steer.login_injected_at = None;
                    foreign_login_notified = false;
                    sender.send(ActivityEvent::narration("Claude sign-in succeeded."));
                    // Dismiss the "Press Enter to continue" screen so the
                    // session resumes without another remote round-trip. A
                    // stray Enter on a variant without that screen is a no-op
                    // submit of claude's empty composer — but ONLY while no
                    // picker owns the keyboard: with a plan-approval or
                    // AskUserQuestion picker up, Enter would activate the
                    // highlighted row (EXP-444).
                    if let Some(steering) = &config.steering {
                        steering.link.disarm_login_refusal();
                        if !grid_picker_visible && !steer.has_pending_question() {
                            (steering.write_input)(b"\r");
                        }
                    }
                }
                Some(login_picker::Transition::Resolved) => {
                    steer.resolve_login(&sender);
                    foreign_login_notified = false;
                    // EXP-444: the screen soliciting the OAuth code left the
                    // grid without succeeding (local Esc, OAuth timeout) — a
                    // code pasted now would be submitted as an ordinary
                    // prompt, recorded and journaled. Refuse the next free
                    // text instead (one-shot, TTL-bounded).
                    if std::mem::take(&mut login_code_soliciting) && !config.foreign_host {
                        if let Some(steering) = &config.steering {
                            steering.link.arm_login_refusal();
                        }
                    }
                }
                None => {}
            }
            // EXP-455: permission dialogs. A session parks on them (bypass
            // mode included — claude flags dangerous commands even under
            // skip-permissions, EXP-564), and remote viewers used to get
            // only the informational card — the dialog becomes an
            // answerable question instead. Runs after the other watchers
            // (the detector rejects their screens outright). Lenient while
            // the hooks hold an unconfirmed prompt or the published question
            // is live (EXP-529) — the hook drain in step 2 already ran, so a
            // hold armed this tick is visible here.
            reconcile_permission_watcher(&mut permission_watcher, &steer);
            let permission_lenient = steer.permission_grid_leniency();
            match permission_watcher.tick(&lines, grid_offset, permission_lenient) {
                Some(permission_picker::Transition::Show(snapshot)) => {
                    // A pending ask/plan owns the screen story; their own
                    // "needs permission" nudge is already suppressed in
                    // apply_hook and must not resurface as a card here.
                    if steer.ask.is_none() && steer.plan.is_none() {
                        steer.publish_permission_question(snapshot, &sender, &redactor);
                    } else {
                        // The watcher latched this snapshot — left alone it
                        // would go silent for good if the ask/plan clears
                        // while the same dialog is still up. Unlatch so the
                        // steady dialog re-fires Show a debounce later and
                        // publishes once the suppression lifts (EXP-458).
                        permission_watcher.unlatch();
                    }
                }
                Some(permission_picker::Transition::Resolved) => {
                    steer.resolve_permission(&sender);
                }
                None => {}
            }
            // EXP-444: a refused paste surfaces as narration, not silence.
            if let Some(steering) = &config.steering {
                if steering.link.take_login_refusal_note() {
                    sender.send(ActivityEvent::narration(
                        "The sign-in window closed before your message arrived — nothing was \
                         typed into the session. Send it again to deliver it as a normal message.",
                    ));
                }
            }
            // EXP-444: anchor-drift diagnostic — a login answer was injected
            // but no known login phase (or the REPL) followed. Anchors are
            // pinned to claude v2.1.222; a copy change would otherwise stall
            // silently with needs_input=false.
            if steer
                .login_injected_at
                .is_some_and(|at| at.elapsed() > LOGIN_DRIFT_WINDOW)
            {
                if !login_watcher.is_pending() {
                    log::warn!(
                        "activity: login answer injected but no known login screen followed — \
                         claude's login UI may have drifted from the pinned anchors"
                    );
                    sender.send(ActivityEvent::narration(
                        "Answered the sign-in picker, but no recognizable login screen followed \
                         — claude's login UI may have changed. The host may need to finish \
                         signing in on the device.",
                    ));
                }
                steer.login_injected_at = None;
            }
        }

        // 4) EXP-214: the combined attention flag — the agent is parked and
        //    waits for a human (a picker on the grid, a picker the hooks know
        //    about, or an unresolved permission/idle notification). Forwarded
        //    only on flips; the watchers already debounce mid-render flicker.
        let picker_pending = picker_watcher.is_pending()
            || question_watcher.is_pending()
            || login_watcher.is_pending()
            || permission_watcher.is_pending()
            || steer.has_pending_question();
        let pending = picker_pending || steer.attention.is_some();
        needs_input.tick(pending, &config.on_needs_input);
        // EXP-637: the graceful-stop signal — a parked picker still counts as
        // between turns (nothing is being written), so `turn_idle` alone
        // decides.
        if let Some(signal) = &config.turn_signal {
            signal.set_idle(steer.turn_idle);
        }
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
            // EXP-724: claude's TUI queues input mid-turn, so its commands
            // are never idle-gated — the flag is published anyway so the
            // seam reads the same on all three agents.
            if let Some(commands) = &steering.commands {
                commands.set_composer_idle(steer.turn_idle);
            }
        }

        // 5) EXP-360: background-subagent lifecycle read off the lines tailed
        //     in step 1 (after the hook drain — the bindings need the
        //     hook-created dispatch cards). A launch ack pins the
        //     agent→dispatch binding; an end —
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

        // 5-bis) EXP-404: a turn/session end sweeps every still-open
        //     subagent card — the safety net for stop signals that never
        //     arrive (a dynamic workflow's fan-out can lose them wholesale).
        //     Runs after the transcript drain so a same-tick launch ack has
        //     already marked its card background; a TurnEnd sweep spares
        //     those (their end arrives as a task-notification turns later,
        //     EXP-360), a SessionEnd sweep takes everything — nothing can
        //     ever complete a card again.
        if let Some(sweep) = steer.take_subagent_sweep() {
            let include_background = sweep == SubagentSweep::SessionEnd;
            for (id, agent_type, detail) in steer.subagents.sweep_open(include_background) {
                sender.send(ActivityEvent::Subagent {
                    id,
                    agent_type,
                    status: SubagentStatus::Completed,
                    detail,
                    at: None,
                });
            }
        }

        // 6) EXP-347: a resolution learned this tick — from the hooks (Stop /
        //     SessionEnd, step 2), the grid (plan Transition::Resolved, step
        //     3), or the transcript flush tailed in step 1 (a QuestionResolved
        //     answers-flush, or an ExitPlanMode tool_result — EXP-691: the
        //     RESULT, not the tool_use twin, is what claude only writes once
        //     the picker is ANSWERED) — means no picker owns the keyboard
        //     anymore. Retire a scroll-stuck plan card by its result id, then
        //     clear the sticky grid memory and push the publisher's flag NOW
        //     rather than at the next step-4 publish: while the viewport is
        //     scrolled the grid recompute never runs, and a stale `true`
        //     reroutes a remote message's Esc into a live turn, cancelling it.
        for id in std::mem::take(&mut transcript_state.resolved_plans) {
            steer.resolve_plan_from_result(&id, &sender);
        }
        if steer.take_resolution() {
            grid_picker_visible = false;
            if let Some(steering) = &config.steering {
                steering.link.set_grid_picker_pending(false);
            }
        }

        // 7) …and from the freshest subagent sidechains, whose tool headlines
        //    are attributed to their agent (EXP-249). Discovery walks a tree,
        //    so it runs on its own slower cadence; the tails themselves are
        //    plain seeks and run every tick.
        if let Some(dir) = &transcript_dir {
            // EXP-443: a SubagentStart this tick means a brand-new sidechain
            // file likely exists RIGHT NOW — skip the debounce once instead
            // of tailing it up to a full interval late.
            if steer.take_subagent_seen() {
                sidechain_scan_at = None;
            }
            if sidechain_scan_at.is_none_or(|at| at.elapsed() >= SIDECHAIN_SCAN_INTERVAL) {
                sidechain_scan_at = Some(Instant::now());
                sidechains = sidechain_transcripts(dir, spawn_time, &steer.pin);
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
                    flush_forced_subagent_completions(&mut steer.subagents, &sender);
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

        // 8) Debounced worktree diff snapshot (only when changed).
        diffs.tick(
            &config.worktree,
            config.base_ref.as_deref(),
            &sender,
            &redactor,
        );

        // 8-bis) EXP-724: remote slash commands. Claude is never idle-gated
        //     (its TUI queues input mid-turn — EXP-356 already publishes the
        //     `queued_command`), but a picker on the grid owns the keyboard,
        //     and a command is not worth Esc-ing an approval away for.
        if let Some(steering) = &config.steering {
            if let Some(commands) = &steering.commands {
                pump_commands(
                    &mut parked_commands,
                    commands,
                    grid_picker_visible || steer.has_pending_question(),
                    false,
                    config.term.as_ref(),
                    &steering.write_input,
                    &sender,
                    Some(&mut transcript_state.published_commands),
                    |_| {},
                );
            }
        }

        // 9) Wait out the poll interval — interrupted by a remote answer, so
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
    // EXP-724: never leave a viewer staring at an indeterminate bar for a
    // session that is gone.
    steer.end_compaction(&sender);
    if let Some(steering) = &config.steering {
        steering.link.set_ask_pending(false);
        steering.link.set_grid_picker_pending(false);
        if let Some(commands) = &steering.commands {
            commands.set_composer_idle(false);
        }
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

/// Appended when [`truncate_marked`] cuts a string — an unmarked hard cut
/// read as the text simply ENDING mid-sentence (EXP-691: a long plan looked
/// finished but wasn't).
pub(crate) const TRUNCATION_MARKER: &str = "\n\n[truncated]";

/// [`truncate`], but a cut string ends in [`TRUNCATION_MARKER`] (still within
/// `max` bytes) so viewers can tell truncation from completion.
pub(crate) fn truncate_marked(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    if max <= TRUNCATION_MARKER.len() {
        return truncate(s, max);
    }
    let mut out = truncate(s, max - TRUNCATION_MARKER.len());
    out.push_str(TRUNCATION_MARKER);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EXP-688: the published diff is the PR's content ─────────────────────

    struct DiffRepo(PathBuf);

    impl Drop for DiffRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl DiffRepo {
        /// A repo on `exp/EXP-1`, cut from `main`, with one base commit.
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-steer-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = Self(path);
            repo.git(&["init", "--quiet", "-b", "main"]);
            repo.git(&["config", "user.email", "t@example.com"]);
            repo.git(&["config", "user.name", "t"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo.write("base.txt", "a\n");
            repo.commit("init");
            repo.git(&["checkout", "--quiet", "-b", "exp/EXP-1"]);
            repo
        }

        fn git(&self, args: &[&str]) {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&self.0)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        }

        fn write(&self, rel: &str, content: &str) {
            std::fs::write(self.0.join(rel), content).unwrap();
        }

        fn commit(&self, message: &str) {
            self.git(&["add", "-A"]);
            self.git(&["commit", "--quiet", "-m", message]);
        }
    }

    /// The bug EXP-688 fixes: an agent COMMITS before opening its PR, and
    /// the old `git diff` (+ `--cached`) view went blank right then. Off the
    /// merge base the committed work is still the diff; with no base it
    /// degrades to the uncommitted view rather than breaking.
    #[test]
    fn worktree_diff_reads_the_branch_off_its_merge_base() {
        let repo = DiffRepo::new("diff-base");
        // A TRACKED edit — `git diff` never showed untracked files either.
        repo.write("base.txt", "a\nlanded\n");

        // Uncommitted: both views agree.
        assert!(worktree_diff(&repo.0, Some("main")).unwrap_or_default().contains("landed"));
        assert!(worktree_diff(&repo.0, None).unwrap_or_default().contains("landed"));

        repo.commit("the agent committed");
        assert!(
            worktree_diff(&repo.0, Some("main")).unwrap_or_default().contains("landed"),
            "a committed change is still the branch's diff"
        );
        assert!(
            worktree_diff(&repo.0, None).unwrap_or_default().is_empty(),
            "the old view is exactly the bug"
        );
        // An unknown base is a fallback, never an error.
        assert!(worktree_diff(&repo.0, Some("origin/nope")).unwrap_or_default().is_empty());
        assert!(worktree_diff(&repo.0, Some("   ")).unwrap_or_default().is_empty());
    }

    /// A diff that goes empty publishes an EMPTY frame: sending nothing left
    /// viewers holding the last patch forever.
    #[test]
    fn diff_snapshots_publish_an_empty_frame_when_the_diff_clears() {
        let repo = DiffRepo::new("diff-clear");
        let (sender, rx) = crate::publisher::ActivitySender::test_pair();
        let redactor = Redactor::new(Vec::new());
        let mut diffs = DiffSnapshots::new();

        repo.write("base.txt", "a\nwip\n");
        diffs.tick(&repo.0, None, &sender, &redactor);
        match rx.try_recv() {
            Ok(crate::publisher::PublisherCmd::Activity(ActivityEvent::Diff { diff, .. })) => {
                assert!(diff.contains("wip"), "{diff}")
            }
            other => panic!("expected a diff frame, got {other:?}"),
        }

        // The debounce holds the next tick back until the interval passes.
        diffs.tick(&repo.0, None, &sender, &redactor);
        assert!(rx.try_recv().is_err(), "the 3s debounce still holds");

        // The agent commits — with no base the diff clears, and THAT is the
        // frame viewers need.
        repo.commit("committed");
        diffs.last_at = None;
        diffs.tick(&repo.0, None, &sender, &redactor);
        match rx.try_recv() {
            Ok(crate::publisher::PublisherCmd::Activity(ActivityEvent::Diff { diff, .. })) => {
                assert_eq!(diff, "", "an empty frame clears the viewer")
            }
            other => panic!("expected an empty diff frame, got {other:?}"),
        }

        // Still empty next time — nothing changed, nothing published.
        diffs.last_at = None;
        diffs.tick(&repo.0, None, &sender, &redactor);
        assert!(rx.try_recv().is_err(), "an unchanged empty diff is silent");
    }

    /// EXP-637: the graceful-stop signal. Already-idle subscribers fire
    /// IMMEDIATELY (an agent that called `sessions_end` between turns must
    /// not wait for a timeout), a busy one on the next false→true edge, and
    /// repeated `true`s never re-fire a consumed waiter.
    #[test]
    fn turn_signal_fires_immediately_when_already_idle() {
        let signal = TurnSignal::new();
        assert!(!signal.is_idle());
        signal.set_idle(true);
        assert!(signal.is_idle());
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_ok(), "an idle signal must fire at once");
    }

    #[test]
    fn turn_signal_fires_on_the_next_idle_edge() {
        let signal = TurnSignal::new();
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_err(), "busy: nothing yet");
        // Staying busy changes nothing.
        signal.set_idle(false);
        assert!(rx.try_recv().is_err());
        signal.set_idle(true);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn turn_signal_wakes_every_waiter_once() {
        let signal = TurnSignal::new();
        let first = signal.subscribe();
        let second = signal.subscribe();
        signal.set_idle(true);
        assert!(first.try_recv().is_ok());
        assert!(second.try_recv().is_ok());
        // The edge is consumed: a second `true` re-fires nothing on the old
        // receivers, but a NEW subscriber still gets its immediate hit.
        signal.set_idle(true);
        assert!(first.try_recv().is_err());
        assert!(signal.subscribe().try_recv().is_ok());
    }

    /// A busy→idle→busy cycle re-arms: the second turn's waiter must wait
    /// for the SECOND boundary, not inherit the first.
    #[test]
    fn turn_signal_rearms_across_turns() {
        let signal = TurnSignal::new();
        signal.set_idle(true);
        signal.set_idle(false);
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_err());
        signal.set_idle(true);
        assert!(rx.try_recv().is_ok());
    }
    use std::process::Command;

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
                { "type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion", "input": { "questions": [
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
                    // EXP-672: byte-identical to what the `QuestionsAsked`
                    // hook would publish for this same ask.
                    id: Some("toolu_ask1#0".into()),
                    ask_id: Some("toolu_ask1".into()),
                    index: Some(1),
                    total: Some(2),
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
                    id: Some("toolu_ask1#1".into()),
                    ask_id: Some("toolu_ask1".into()),
                    index: Some(2),
                    total: Some(2),
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

        // Oversized plan is truncated to the relay cap — with an explicit
        // marker instead of a silent mid-sentence cut (EXP-691).
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
                assert!(text.ends_with(TRUNCATION_MARKER));
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

        // The legacy grid-only publication pre-paid exactly one swallow: the
        // FIRST transcript plan question is swallowed, later ones pass
        // through (grid detection missed ⇒ degraded fallback still works).
        let mut state = TranscriptState {
            swallow_next_plan_twin: true,
            ..Default::default()
        };
        let mut events: Vec<ActivityEvent> = Vec::new();
        tail_transcript(
            &path,
            0,
            &mut |line| process_transcript_line(line, &redactor, &mut state),
            &mut |event| events.push(event),
        );
        assert!(!state.swallow_next_plan_twin);
        // EXP-691: a twin flush is NOT a resolution — on immediate-flush
        // claude the picker is still up; only the tool_result resolves.
        assert!(state.resolved_plans.is_empty());
        match &events[..] {
            [ActivityEvent::Narration { text, .. }, ActivityEvent::Question { plan_mode, .. }] => {
                assert_eq!(text, "On it.");
                assert_eq!(*plan_mode, Some(true));
            }
            other => panic!("expected narration + one plan question, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── EXP-672: no fallback card is id-less any more ───────────────────────

    #[test]
    fn synthetic_ids_are_deterministic_and_seed_separated() {
        // The whole point is REPRODUCIBILITY: a card re-published (history
        // replay, a re-tailed transcript line) must land on the same identity
        // or clients double it and its `question_resolved` retires nothing.
        assert_eq!(
            synthetic_question_id("sess-1", "plan", "Plan ready", 3),
            synthetic_question_id("sess-1", "plan", "Plan ready", 3)
        );
        // Every seed part is load-bearing.
        assert_ne!(
            synthetic_question_id("sess-1", "plan", "Plan ready", 3),
            synthetic_question_id("sess-2", "plan", "Plan ready", 3)
        );
        assert_ne!(
            synthetic_question_id("sess-1", "plan", "Plan ready", 3),
            synthetic_question_id("sess-1", "ask", "Plan ready", 3)
        );
        assert_ne!(
            synthetic_question_id("sess-1", "plan", "Plan ready", 3),
            synthetic_question_id("sess-1", "plan", "Plan ready?", 3)
        );
        assert_ne!(
            synthetic_question_id("sess-1", "plan", "Plan ready", 3),
            synthetic_question_id("sess-1", "plan", "Plan ready", 4)
        );
        // The `\u{1f}` separator: concatenation collisions are impossible.
        assert_ne!(
            synthetic_question_id("a", "ask", "bc", 0),
            synthetic_question_id("ab", "ask", "c", 0)
        );
        // It clears `ID_MAX` (the relay caps `question.id` at 128) even for a
        // 64 KiB plan body.
        let id = synthetic_question_id("sess-1", "plan", &"x".repeat(70_000), 1);
        assert!(id.len() <= ID_MAX, "{} chars", id.len());
    }

    #[test]
    fn transcript_fallback_cards_carry_the_hooks_own_identity() {
        // EXP-672: the hookless transcript fallbacks used to publish
        // `id: None`, so a viewer could only answer them with blind
        // keystrokes. They now mint the SAME identity the sidecar would have
        // — `PlanProposed` publishes a plan under its `tool_use_id`,
        // `QuestionsAsked` its questions under `{tool_use_id}#{index}` — so
        // the twin and the hook can never disagree about which card is which.
        let redactor = Redactor::new(vec![]);
        let plan = serde_json::json!({
            "type": "assistant",
            "uuid": "entry-1",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        match &parse_transcript_line(&plan, &redactor)[..] {
            [ActivityEvent::Question { id, plan_mode, .. }] => {
                assert_eq!(*plan_mode, Some(true));
                assert_eq!(id.as_deref(), Some("toolu_plan1"));
            }
            other => panic!("expected one plan question, got {other:?}"),
        }
        // The hook publishes that very same plan under that very same id.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_plan1".to_string()),
                plan: "## Plan\n1. Do the thing".to_string(),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.confirm_plan_from_grid(
            vec![QuestionOption::new("Approve", "1")],
            &sender
        ));
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, .. }] => {
                assert_eq!(id.as_deref(), Some("toolu_plan1"), "twin and hook agree");
            }
            other => panic!("expected one plan question, got {other:?}"),
        }

        let ask = serde_json::json!({
            "type": "assistant",
            "uuid": "entry-2",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_ask9", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which one?", "options": [ { "label": "A" }, { "label": "B" } ] },
                    { "question": "And then?", "options": [ { "label": "C" } ] },
                  ] } },
            ]}
        })
        .to_string();
        let events = parse_transcript_line(&ask, &redactor);
        let ids: Vec<Option<&str>> = events
            .iter()
            .map(|event| match event {
                ActivityEvent::Question { id, .. } => id.as_deref(),
                other => panic!("expected questions, got {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![Some("toolu_ask9#0"), Some("toolu_ask9#1")]);
        // Re-tailing the same line (an offset reset) reproduces them exactly.
        assert_eq!(parse_transcript_line(&ask, &redactor), events);
    }

    #[test]
    fn a_tool_use_without_an_id_still_gets_a_stable_synthetic_one() {
        // claude always stamps `id`, but the fallback must not silently go
        // back to `id: None` if a transcript shape ever drops it: the entry
        // uuid is the deterministic stand-in.
        let redactor = Redactor::new(vec![]);
        let entry = |uuid: &str| {
            serde_json::json!({
                "type": "assistant",
                "uuid": uuid,
                "message": { "content": [
                    { "type": "tool_use", "name": "ExitPlanMode",
                      "input": { "plan": "## Plan" } },
                ]}
            })
            .to_string()
        };
        let id_of = |line: &str| match &parse_transcript_line(line, &redactor)[..] {
            [ActivityEvent::Question { id: Some(id), .. }] => id.clone(),
            other => panic!("expected one identified plan question, got {other:?}"),
        };
        let first = id_of(&entry("entry-1"));
        assert!(first.starts_with("syn-plan-"), "{first}");
        assert_eq!(first, id_of(&entry("entry-1")), "same entry, same id");
        assert_ne!(first, id_of(&entry("entry-2")), "different entry, new id");
    }

    #[test]
    fn a_transcript_fallback_card_registers_as_answerable() {
        // An id nothing can answer would be worse than none: the transcript
        // path publishes THROUGH `observe_published`, which now enrolls the
        // card in `live` so `handle_answer` finds it.
        let (sender, _rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let line = serde_json::json!({
            "type": "assistant",
            "uuid": "entry-1",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan" } },
            ]}
        })
        .to_string();
        for event in parse_transcript_line(&line, &redactor) {
            steer.observe_published(&event, &sender);
        }
        let live = steer.live.get("toolu_plan1").expect("registered as live");
        assert_eq!(live.kind, QuestionKind::Plan);
        assert_eq!(live.options.len(), 2);
    }

    #[test]
    fn grid_fallback_cards_are_identified_answerable_and_retired() {
        // The legacy grid-only publication (no sidecar, an old claude): its
        // card used to be id-less and keystroke-only. It now carries a stable
        // synthetic id, is enrolled in `live`, and retires by that id.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let id = steer.publish_grid_fallback(
            "sess-1",
            QuestionKind::Plan,
            "Plan ready for approval.".to_string(),
            vec![QuestionOption::new("Approve", "1")],
            false,
            &sender,
        );
        steer.grid_plan = Some(id.clone());
        assert_eq!(
            id,
            synthetic_question_id("sess-1", "plan", "Plan ready for approval.", 1),
            "the ordinal-1 card of this session reproduces exactly"
        );
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id: published,
                plan_mode,
                ..
            }] => {
                assert_eq!(published.as_deref(), Some(id.as_str()));
                assert_eq!(*plan_mode, Some(true));
            }
            other => panic!("expected one plan question, got {other:?}"),
        }
        assert!(steer.live.contains_key(&id), "answerable by id");

        // A second, word-for-word identical picker later in the run is a
        // DIFFERENT card — the ordinal keeps them apart.
        let again = steer.publish_grid_fallback(
            "sess-1",
            QuestionKind::Plan,
            "Plan ready for approval.".to_string(),
            vec![QuestionOption::new("Approve", "1")],
            false,
            &sender,
        );
        assert_ne!(again, id);
        let _ = drained(&rx);

        // And the picker leaving the screen retires the grid card by id.
        steer.grid_plan = Some(again.clone());
        steer.resolve_plan(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id: resolved, .. }] => {
                assert_eq!(resolved.as_deref(), Some(again.as_str()));
            }
            other => panic!("expected one resolution, got {other:?}"),
        }
        assert!(!steer.live.contains_key(&again));
    }

    #[test]
    fn a_grid_ask_fallback_card_is_answerable_by_id() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let id = steer.publish_grid_fallback(
            "sess-1",
            QuestionKind::Ask,
            "Which toppings?".to_string(),
            vec![
                QuestionOption::new("Cheese", "1"),
                QuestionOption::new("Ham", "2"),
            ],
            true,
            &sender,
        );
        assert!(id.starts_with("syn-ask-"), "{id}");
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id: published,
                multi_select,
                plan_mode,
                ..
            }] => {
                assert_eq!(published.as_deref(), Some(id.as_str()));
                assert_eq!(*multi_select, Some(true));
                assert_eq!(*plan_mode, None);
            }
            other => panic!("expected one ask question, got {other:?}"),
        }
        let live = steer.live.get(&id).expect("answerable by id");
        assert_eq!(live.kind, QuestionKind::Ask);
        assert!(live.multi_select);
    }

    #[test]
    fn a_grid_ask_fallback_card_retires_when_the_ask_resolves() {
        // EXP-672: the ask resolution is keyed by claude's own ask id, which
        // a grid-only card never had — without the group retire the card sat
        // on every viewer for the rest of the run.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let id = steer.publish_grid_fallback(
            "sess-1",
            QuestionKind::Ask,
            "Which toppings?".to_string(),
            vec![QuestionOption::new("Cheese", "1")],
            false,
            &sender,
        );
        assert_eq!(steer.grid_asks, vec![id.clone()]);
        let _ = drained(&rx);

        // The transcript's answers flush (the emitter's `observe_published`).
        steer.observe_published(
            &ActivityEvent::QuestionResolved {
                id: None,
                ask_id: Some("toolu_ask1".to_string()),
                answers: Some(vec!["Cheese".to_string()]),
                dismissed: None,
                at: None,
            },
            &sender,
        );
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id: resolved, .. }] => {
                assert_eq!(resolved.as_deref(), Some(id.as_str()));
            }
            other => panic!("expected the grid card's retirement, got {other:?}"),
        }
        assert!(!steer.live.contains_key(&id), "the card is gone");
        assert!(steer.grid_asks.is_empty());

        // The other edge — the picker leaving the grid with no transcript
        // evidence at all — is a no-op once the set is drained.
        steer.resolve_grid_asks(&sender);
        assert!(drained(&rx).is_empty(), "nothing left to retire");
    }

    #[test]
    fn a_hookless_transcript_plan_card_retires_on_its_tool_result() {
        // EXP-672: no sidecar + an immediate-flush claude — the transcript
        // twin IS the card (published under `toolu_…`, enrolled in `live` by
        // `observe_published`, the grid fallback skipped as a duplicate) and
        // nothing hangs it off `self.plan`. Answered at the local TUI, only
        // the ExitPlanMode tool_result names it, so that is where it retires.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut state = TranscriptState::default();
        let tool_use = serde_json::json!({
            "type": "assistant",
            "uuid": "entry-1",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        for event in process_transcript_line(&tool_use, &redactor, &mut state) {
            steer.observe_published(&event, &sender);
        }
        assert!(steer.live.contains_key("toolu_plan1"), "answerable card");
        assert!(steer.plan.is_none(), "no hook ever announced this plan");
        let _ = drained(&rx);

        let tool_result = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_plan1",
                  "content": "User has approved your plan." },
            ]}
        })
        .to_string();
        for event in process_transcript_line(&tool_result, &redactor, &mut state) {
            steer.observe_published(&event, &sender);
        }
        assert_eq!(state.resolved_plans, vec!["toolu_plan1".to_string()]);
        // …the emitter's step 6.
        for id in std::mem::take(&mut state.resolved_plans) {
            steer.resolve_plan_from_result(&id, &sender);
        }
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id: resolved, .. }] => {
                assert_eq!(resolved.as_deref(), Some("toolu_plan1"));
            }
            other => panic!("expected the plan card's retirement, got {other:?}"),
        }
        assert!(!steer.live.contains_key("toolu_plan1"), "the card is gone");
        // Idempotent: a re-tailed result cannot double-retire.
        steer.resolve_plan_from_result("toolu_plan1", &sender);
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn wired_sidecar_swallows_every_transcript_plan_twin() {
        // EXP-691: with hooks wired the twin is swallowed whether it flushes
        // before the hook drains (immediate-flush claude — the EXP-694
        // double) or after the answer (withholding claude) — never counted,
        // so a flush can no longer race the arming.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_plan_twins: true,
            ..Default::default()
        };
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        assert_eq!(process_transcript_line(&entry, &redactor, &mut state), vec![]);
        assert_eq!(state.pending_plans, vec!["toolu_plan1".to_string()]);
        // A re-tailed copy (offset reset) is swallowed too, not re-recorded.
        assert_eq!(process_transcript_line(&entry, &redactor, &mut state), vec![]);
        assert_eq!(state.pending_plans.len(), 1);
    }

    #[test]
    fn exit_plan_mode_tool_result_is_the_resolution_edge() {
        // EXP-691: the RESULT flush — not the tool_use twin — is what claude
        // only writes once the picker is answered; it drives the scrolled-
        // viewport resolution and retires the card by id.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_plan_twins: true,
            ..Default::default()
        };
        let tool_use = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan" } },
            ]}
        })
        .to_string();
        assert_eq!(process_transcript_line(&tool_use, &redactor, &mut state), vec![]);
        // Immediate-flush order: the hook drains AFTER the twin flushed, so
        // its EXP-483 anchor entry is never consumed by a flush.
        state.hook_published_plans.insert("toolu_plan1".to_string());
        let tool_result = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "toolu_plan1",
                  "content": "User has approved your plan." },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&tool_result, &redactor, &mut state),
            vec![],
            "the result's content is never published"
        );
        assert_eq!(state.resolved_plans, vec!["toolu_plan1".to_string()]);
        assert!(state.pending_plans.is_empty());
        assert!(
            state.hook_published_plans.is_empty(),
            "the unconsumed anchor is cleaned up with the result"
        );
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
    fn answered_ask_resolves_with_answers_in_question_order() {
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
                // The EXP-249 semantic retirement of the whole ask.
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

        // The answers still flow (the semantic resolution).
        let events = process_transcript_line(&tool_result, &redactor, &mut state);
        assert_eq!(events.len(), 1);
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
            [ActivityEvent::QuestionResolved {
                ask_id, answers, ..
            }] => {
                assert_eq!(ask_id.as_deref(), Some("toolu_ask1"));
                assert_eq!(answers.as_ref().unwrap().len(), 2);
            }
            other => panic!("expected a resolution, got {other:?}"),
        }
        assert!(state.hook_published_asks.is_empty(), "the ask is over");
    }

    #[test]
    fn a_hook_wired_session_swallows_an_ask_twin_that_beats_its_hook() {
        // EXP-610: an ask that resolves within one poll tick (the free-text
        // Esc-reroute dismisses the picker the moment a steered message
        // lands) flushes its twin BEFORE the QuestionsAsked hook drains —
        // `hook_published_asks` is still empty then, and the twin published
        // every question as a stale answerable id-less card ("shows all
        // questions at once"). With the sidecar wired the twin is swallowed
        // by FLAG, exactly like the Task headlines.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_ask_questions: true,
            ..Default::default()
        };
        let (tool_use, tool_result) = answered_ask_lines();
        assert_eq!(
            process_transcript_line(&tool_use, &redactor, &mut state),
            vec![],
            "the twin never publishes while the sidecar owns asks"
        );
        // The resolution still flows (the semantic retirement).
        assert_eq!(
            process_transcript_line(&tool_result, &redactor, &mut state).len(),
            1
        );
    }

    #[test]
    fn a_hook_draining_after_its_asks_resolution_publishes_nothing() {
        // The other half of the EXP-610 race: by the time the QuestionsAsked
        // hook drains, the twin + result already flushed — the picker is
        // gone and its `question_resolved` already went by. Publishing cards
        // now would wedge a stepper forever.
        let redactor = Redactor::new(vec![]);
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState {
            suppress_ask_questions: true,
            ..Default::default()
        };
        let (tool_use, tool_result) = answered_ask_lines();
        process_transcript_line(&tool_use, &redactor, &mut transcript);
        process_transcript_line(&tool_result, &redactor, &mut transcript);
        steer.apply_hook(
            hook(HookEventKind::QuestionsAsked {
                tool_use_id: Some("toolu_ask1".to_string()),
                questions: vec![HookQuestion {
                    question: "Which toppings do you want?".to_string(),
                    header: None,
                    options: vec![HookQuestionOption {
                        label: "Cheese".to_string(),
                        description: None,
                    }],
                    multi_select: false,
                }],
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(
            drained(&rx)
                .iter()
                .all(|event| !matches!(event, ActivityEvent::Question { .. })),
            "no cards for a dead picker"
        );
        assert!(steer.ask.is_none(), "and no pending ask to route against");
        // A FRESH ask id (claude re-asking the dismissed questions) still
        // publishes normally.
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        assert!(drained(&rx)
            .iter()
            .any(|event| matches!(event, ActivityEvent::Question { .. })));
        assert!(steer.ask.is_some());
    }

    #[test]
    fn prose_still_anchors_when_the_twin_beats_its_hook() {
        // EXP-483 anchoring must survive the EXP-610 race: the entry's own
        // tool_use id IS the identity the hook publishes under, so the prose
        // anchors to it even before `hook_published_asks` learns the id.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_ask_questions: true,
            ..Default::default()
        };
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "Here is the summary of my findings." },
                { "type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which size?",
                      "options": [ { "label": "Small" }, { "label": "Large" } ] },
                  ] } },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&entry, &redactor, &mut state),
            vec![ActivityEvent::Narration {
                text: "Here is the summary of my findings.".to_string(),
                before_question_id: Some("toolu_ask1".to_string()),
                at: None,
            }]
        );
    }

    #[test]
    fn withheld_entry_prose_anchors_above_the_hook_published_ask() {
        // EXP-483: claude withholds the [text, AskUserQuestion] entry until
        // the picker resolves — the prose flushes AFTER the hook-published
        // card, so it must carry the anchor clients splice on.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        state.hook_published_asks.insert("toolu_ask1".to_string());
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "Here is the summary of my findings." },
                { "type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which size?",
                      "options": [ { "label": "Small" }, { "label": "Large" } ] },
                  ] } },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&entry, &redactor, &mut state),
            vec![ActivityEvent::Narration {
                text: "Here is the summary of my findings.".to_string(),
                before_question_id: Some("toolu_ask1".to_string()),
                at: None,
            }]
        );
    }

    #[test]
    fn withheld_plan_entry_prose_anchors_above_the_hook_published_plan() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            suppress_plan_twins: true,
            ..Default::default()
        };
        state.hook_published_plans.insert("toolu_plan1".to_string());
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "The plan is ready — summary first." },
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan\n1. Do the thing" } },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&entry, &redactor, &mut state),
            vec![ActivityEvent::Narration {
                text: "The plan is ready — summary first.".to_string(),
                before_question_id: Some("toolu_plan1".to_string()),
                at: None,
            }]
        );
        // EXP-691: the twin flush is no resolution — only the tool_result is.
        assert!(state.resolved_plans.is_empty());
        assert!(
            state.hook_published_plans.is_empty(),
            "the anchor is consumed with the twin"
        );
    }

    #[test]
    fn grid_only_twin_prose_stays_unanchored() {
        // A grid-published card is id-less — there is nothing to splice
        // against, so the prose appends exactly as before.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        state.remember_grid_question("Which size?");
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "Summary before the ask." },
                { "type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion",
                  "input": { "questions": [
                    { "question": "Which size?",
                      "options": [ { "label": "Small" }, { "label": "Large" } ] },
                  ] } },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&entry, &redactor, &mut state),
            vec![ActivityEvent::narration("Summary before the ask.")]
        );
    }

    #[test]
    fn grid_fallback_plan_twin_prose_stays_unanchored() {
        // The flag-only suppression (id-less grid plan card) swallows the
        // twin but must not anchor the prose — no card id exists to match.
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState {
            swallow_next_plan_twin: true,
            ..Default::default()
        };
        let entry = serde_json::json!({
            "type": "assistant",
            "message": { "content": [
                { "type": "text", "text": "Plan prose." },
                { "type": "tool_use", "id": "toolu_plan1", "name": "ExitPlanMode",
                  "input": { "plan": "## Plan" } },
            ]}
        })
        .to_string();
        assert_eq!(
            process_transcript_line(&entry, &redactor, &mut state),
            vec![ActivityEvent::narration("Plan prose.")]
        );
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
    fn rejected_ask_resolves_as_dismissed() {
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
            vec![ActivityEvent::QuestionResolved {
                id: None,
                ask_id: Some("toolu_ask1".into()),
                answers: None,
                dismissed: Some(true),
                at: None,
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
            [ActivityEvent::QuestionResolved { answers, .. }] => {
                let answers = answers.as_ref().expect("answers");
                assert!(
                    !answers[0].contains("expu_abcdef"),
                    "typed answer leaked a key: {answers:?}"
                );
            }
            other => panic!("expected a resolution, got {other:?}"),
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
        let mut bytes = b"\x1b[2J\x1b[H".to_vec();
        for row in rows {
            bytes.extend_from_slice(row.as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        terminal::advance_bytes(term, &bytes);
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
        assert!(
            transcript.hook_published_plans.contains("toolu_plan"),
            "the twin is swallowed by identity (EXP-691)"
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

        // Resolution retires the card by id.
        steer.resolve_plan(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, .. }] => {
                assert_eq!(id.as_deref(), Some("toolu_plan"));
            }
            other => panic!("expected the resolution, got {other:?}"),
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
        steer.observe_published(&ActivityEvent::narration("moving on"), &sender);
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
    fn eviction_beyond_cap_force_completes_the_oldest_live_card() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        // A dynamic-workflow fan-out: three more dispatches than the cap.
        for i in 0..SUBAGENTS_CAP + 3 {
            steer.apply_hook(
                hook(HookEventKind::SubagentDispatched {
                    tool_use_id: Some(format!("toolu_{i}")),
                    description: Some(format!("task {i}")),
                    subagent_type: Some("workflow-subagent".to_string()),
                }),
                &sender,
                &redactor,
                &mut transcript,
            );
        }
        let events = drained(&rx);
        let completed: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                ActivityEvent::Subagent {
                    id,
                    agent_type,
                    status: SubagentStatus::Completed,
                    detail,
                    ..
                } => Some((id.clone(), agent_type.clone(), detail.clone())),
                _ => None,
            })
            .collect();
        // Exactly the three oldest live cards were evicted, oldest first, and
        // each got its owed completion edge restating type and detail — a
        // silently dropped card left its tab spinning forever on every client.
        assert_eq!(
            completed,
            (0..3)
                .map(|i| {
                    (
                        format!("toolu_{i}"),
                        "workflow-subagent".to_string(),
                        Some(format!("task {i}")),
                    )
                })
                .collect::<Vec<_>>()
        );
        // The forced edge publishes after its card's Started, never before.
        let pos = |want_status: SubagentStatus| {
            events.iter().position(|e| {
                matches!(e, ActivityEvent::Subagent { id, status, .. }
                    if id == "toolu_0" && *status == want_status)
            })
        };
        assert!(pos(SubagentStatus::Started) < pos(SubagentStatus::Completed));
        // A late end signal for an evicted card stays silent — its edge is
        // already on the wire.
        assert!(steer.subagents.complete(None, Some("toolu_0")).is_none());
    }

    #[test]
    fn eviction_prefers_completed_cards_over_live_ones() {
        let mut subagents = Subagents::default();
        for i in 0..SUBAGENTS_CAP {
            subagents.dispatch(format!("toolu_{i}"), "explore".to_string(), None);
        }
        // Bind the two oldest, finish the first, then overflow by one.
        subagents.started("agent_0", Some("explore"));
        subagents.started("agent_1", Some("explore"));
        assert!(subagents.complete(Some("agent_0"), None).is_some());
        subagents.dispatch("toolu_new".to_string(), "explore".to_string(), None);
        // The finished card was reclaimed — no live card owes a forced edge —
        // and its alias died with it.
        assert!(subagents.take_forced_completions().is_empty());
        assert!(!subagents.knows_agent("agent_0"));
        // Surviving cards keep their alias and complete normally.
        let (id, ..) = subagents
            .complete(Some("agent_1"), None)
            .expect("a live card's alias survives the eviction");
        assert_eq!(id, "toolu_1");
        let (id, ..) = subagents
            .complete(None, Some("toolu_new"))
            .expect("the overflowing card is carded");
        assert_eq!(id, "toolu_new");
    }

    #[test]
    fn a_turn_end_sweep_completes_orphans_but_spares_background_agents() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        // (a) The workflow path: an unbound typed start whose stop never
        // arrives.
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("agent_wf".to_string()),
                agent_type: Some("workflow-subagent".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        // (b) A dispatched card whose launch ack marks it background
        // (EXP-360) — its end arrives turns later as a task-notification.
        steer.apply_hook(
            hook(HookEventKind::SubagentDispatched {
                tool_use_id: Some("toolu_bg".to_string()),
                description: None,
                subagent_type: Some("Explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.subagents.bind_launch("agent_bg", "toolu_bg");
        let _ = drained(&rx);
        // The turn ends: the hook arms the sweep, the emitter drives it
        // after the transcript drain.
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        let sweep = steer.take_subagent_sweep().expect("stop arms a sweep");
        assert!(sweep == SubagentSweep::TurnEnd);
        let swept = steer.subagents.sweep_open(sweep == SubagentSweep::SessionEnd);
        // Only the orphan completes; the background card keeps spinning.
        match &swept[..] {
            [(id, agent_type, None)] => {
                assert_eq!(id, "agent_wf");
                assert_eq!(agent_type, "workflow-subagent");
            }
            other => panic!("expected just the orphan, got {other:?}"),
        }
        // The armed sweep was consumed, and a repeat sweep owes nothing.
        assert!(steer.take_subagent_sweep().is_none());
        assert!(steer.subagents.sweep_open(false).is_empty());
        // A SessionEnd sweep takes the background card too…
        match &steer.subagents.sweep_open(true)[..] {
            [(id, agent_type, None)] => {
                assert_eq!(id, "toolu_bg");
                assert_eq!(agent_type, "Explore");
            }
            other => panic!("expected the background card, got {other:?}"),
        }
        // …after which its late task-notification stays silent.
        assert!(steer.subagents.complete(Some("agent_bg"), None).is_none());
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

    /// The Bash permission dialog as captured live on claude v2.0.42 —
    /// mirrors the `permission_picker` fixture.
    fn permission_dialog_rows() -> Vec<String> {
        [
            "────────────────────────────────────────────────────────────────",
            " Bash command",
            "",
            "   curl -s https://example.com/",
            "   Fetch content from example.com",
            "",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "  2. Yes, and don't ask again for curl commands in",
            "  /home/user/project",
            "  3. Tell Claude what to do differently",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    fn permission_hook() -> HookEvent {
        hook(HookEventKind::PermissionPrompt {
            message: "Claude needs your permission to use Bash".to_string(),
            tool: Some("Bash".to_string()),
        })
    }

    #[test]
    fn a_permission_notification_holds_attention_and_waits_for_the_grid() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            permission_hook(),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        // EXP-455: nothing publishes yet — the grid watcher owns the card.
        assert!(drained(&rx).is_empty(), "the informational card is held");
        assert!(steer.attention.is_some(), "the session is blocked");
        steer.observe_published(&ActivityEvent::tool("Bash", None), &sender);
        assert!(steer.attention.is_none(), "progress clears it");
    }

    #[test]
    fn a_grid_confirmed_permission_dialog_becomes_an_answerable_question() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);
        assert!(drained(&rx).is_empty());

        let snapshot =
            crate::permission_picker::detect(&permission_dialog_rows()).expect("dialog detected");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id,
                header,
                text,
                options,
                plan_mode,
                ..
            }] => {
                assert_eq!(id.as_deref(), Some("permission:1"));
                assert_eq!(header.as_deref(), Some("Bash command"));
                assert!(text.starts_with("Do you want to proceed?"));
                assert!(text.contains("curl -s https://example.com/"));
                assert_eq!(options.len(), 3);
                assert_eq!(options[0].key, "1");
                assert_eq!(plan_mode, &None);
            }
            other => panic!("expected a permission question, got {other:?}"),
        }
        assert!(steer.has_pending_question());

        // The consumed hook hold can never degrade to the legacy card.
        steer
            .pending_permission
            .as_mut()
            .map(|pending| pending.seen = Instant::now() - PERMISSION_GRID_CONFIRM);
        steer.permission_timeout(&sender);
        assert!(drained(&rx).is_empty(), "no informational duplicate");

        // The dialog leaving the grid retires the card and the block.
        steer.attention = Some(Attention::Permission);
        steer.resolve_permission(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, .. }] => {
                assert_eq!(id.as_deref(), Some("permission:1"));
            }
            other => panic!("expected a resolution, got {other:?}"),
        }
        assert!(steer.attention.is_none());
        assert!(!steer.has_pending_question());
    }

    #[test]
    fn a_headline_less_dialog_falls_back_to_the_hook_tool_header() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);
        let snapshot = crate::permission_picker::PermissionSnapshot {
            header: None,
            context: vec![],
            question: "Do you want to proceed?".to_string(),
            options: vec![
                QuestionOption::new("Yes", "1"),
                QuestionOption::new("No", "2"),
            ],
        };
        steer.publish_permission_question(snapshot, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question { header, .. }] => {
                assert_eq!(header.as_deref(), Some("Bash"));
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn an_unconfirmed_permission_prompt_degrades_to_the_informational_card() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            permission_hook(),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        // Fresh hold: quiet.
        steer.permission_timeout(&sender);
        assert!(drained(&rx).is_empty());
        // Past the confirm window with no grid dialog: the legacy card.
        steer.pending_permission.as_mut().unwrap().seen =
            Instant::now() - PERMISSION_GRID_CONFIRM;
        steer.permission_timeout(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, detail, .. }] => {
                assert_eq!(tool, "Bash");
                assert!(detail.as_ref().unwrap().contains("permission"));
            }
            other => panic!("expected the informational card, got {other:?}"),
        }
        // Once only.
        steer.permission_timeout(&sender);
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn a_picker_registration_drops_a_raced_permission_hold() {
        // claude ≥2.1.233 fires the picker's own permission-flavoured
        // Notification, and it can land BEFORE the PreToolUse registration —
        // the raced hold must never degrade into a phantom card next to the
        // answerable ask (EXP-512).
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Session paused".to_string(),
                tool: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.pending_permission.is_some());
        steer.apply_hook(
            hook(HookEventKind::QuestionsAsked {
                tool_use_id: Some("toolu_ask".to_string()),
                questions: vec![HookQuestion {
                    question: "Favorite color?".to_string(),
                    ..Default::default()
                }],
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(
            steer.pending_permission.is_none(),
            "the ask owns the screen story"
        );
        drained(&rx);
        // And a hold that somehow survives past the confirm window while an
        // ask is pending is dropped at the timeout instead of publishing.
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);
        steer.pending_permission = Some(PendingPermission {
            tool: "Tool".to_string(),
            detail: Some("Session paused".to_string()),
            seen: Instant::now() - PERMISSION_GRID_CONFIRM,
            degraded: false,
        });
        steer.permission_timeout(&sender);
        assert!(drained(&rx).is_empty(), "no phantom informational card");
        assert!(steer.pending_permission.is_none());
    }

    #[test]
    fn a_fresh_notification_rearms_a_degraded_hold() {
        // EXP-458: with grid detection missing every dialog, nothing retires
        // a degraded hold before turn end — a further Notification must
        // re-arm it, or every later prompt in the turn loses its card.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);

        // A repeat nudge while the hold is still FRESH is swallowed — the
        // original confirm window keeps ticking.
        steer.pending_permission.as_mut().unwrap().seen =
            Instant::now() - PERMISSION_GRID_CONFIRM;
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);
        steer.permission_timeout(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, .. }] => assert_eq!(tool, "Bash"),
            other => panic!("expected the informational card, got {other:?}"),
        }

        // The hold is degraded now: the NEXT prompt's Notification re-arms
        // a fresh hold instead of vanishing.
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Write".to_string(),
                tool: Some("Write".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        let pending = steer.pending_permission.as_ref().expect("re-armed");
        assert!(!pending.degraded, "a fresh confirm window");
        // Quiet inside the new window…
        steer.permission_timeout(&sender);
        assert!(drained(&rx).is_empty());
        // …and past it, the second prompt gets its own card.
        steer.pending_permission.as_mut().unwrap().seen =
            Instant::now() - PERMISSION_GRID_CONFIRM;
        steer.permission_timeout(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, .. }] => assert_eq!(tool, "Write"),
            other => panic!("expected the second informational card, got {other:?}"),
        }

        // A LIVE grid question still swallows nudges outright.
        let snapshot =
            crate::permission_picker::detect(&permission_dialog_rows()).expect("dialog detected");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        drained(&rx);
        steer.apply_hook(permission_hook(), &sender, &redactor, &mut transcript);
        assert!(
            steer.pending_permission.is_none(),
            "no hold behind a live question"
        );
    }

    #[test]
    fn a_remote_permission_answer_is_injected_and_acked() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &permission_dialog_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let snapshot =
            crate::permission_picker::detect(&screen_lines(&term)).expect("dialog detected");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        drained(&rx);

        // The digit approves; the dialog leaves the grid; the answer acks.
        let (write_input, keys) = recording_input(term.clone(), None, "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "permission:1".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "permission:1"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }

        // A duplicate never injects again.
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "permission:1".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(keys.lock().unwrap().len(), 1, "no second injection");
        drained(&rx);
    }

    #[test]
    fn a_permission_answer_without_the_dialog_on_screen_is_retried() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &["✳ Deliberating…".to_string()]);

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let snapshot =
            crate::permission_picker::detect(&permission_dialog_rows()).expect("dialog detected");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        drained(&rx);

        let (write_input, keys) = recording_input(term.clone(), None, "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "permission:1".to_string(),
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
    }

    /// An EXP-529-style dialog: options on screen, but no "Do you want to"
    /// anchor line — mirrors the `permission_picker` fixture.
    fn paused_dialog_rows() -> Vec<String> {
        [
            "────────────────────────────────────────────────────────────────",
            " Bash command",
            "",
            "   curl -s https://example.com/",
            "   Fetch content from example.com",
            "",
            " This command requires approval",
            "",
            " ❯ 1. Yes",
            "   2. No",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    #[test]
    fn a_hook_armed_anchorless_dialog_becomes_an_answerable_question() {
        // EXP-529: the hook says a prompt is up, the dialog carries no
        // anchor — lenient detection still yields an answerable card
        // instead of the dead-end informational one.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Session paused".to_string(),
                tool: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.permission_grid_leniency(), "the hold arms leniency");
        assert!(drained(&rx).is_empty());

        let snapshot = crate::permission_picker::detect_lenient(&paused_dialog_rows())
            .expect("lenient detection");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question {
                id, header, text, options, ..
            }] => {
                assert_eq!(id.as_deref(), Some("permission:1"));
                assert_eq!(header.as_deref(), Some("Bash command"));
                assert!(text.starts_with("This command requires approval"));
                assert_eq!(options.len(), 2);
            }
            other => panic!("expected a permission question, got {other:?}"),
        }
        // The published question keeps leniency on — the very next strict
        // miss must not fake a Resolved (the consumed hold is gone).
        assert!(steer.permission_grid_leniency());

        // A question-less snapshot (nothing but options on the grid) says
        // what the hook said instead of shipping an empty body.
        steer.resolve_permission(&sender);
        drained(&rx);
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Session paused".to_string(),
                tool: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        let bare = crate::permission_picker::PermissionSnapshot {
            header: None,
            context: vec![],
            question: String::new(),
            options: vec![
                QuestionOption::new("Yes", "1"),
                QuestionOption::new("No", "2"),
            ],
        };
        steer.publish_permission_question(bare, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question { text, header, .. }] => {
                assert_eq!(text, "Session paused");
                assert_eq!(header.as_deref(), Some("Tool"));
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn a_remote_answer_lands_on_a_leniently_detected_dialog() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &paused_dialog_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let snapshot = crate::permission_picker::detect_lenient(&screen_lines(&term))
            .expect("lenient detection");
        steer.publish_permission_question(snapshot, &sender, &redactor);
        drained(&rx);

        // The answer path re-detects leniently too — a strict re-detect
        // would Retry forever against the anchorless dialog (EXP-529).
        let (write_input, keys) = recording_input(term.clone(), None, "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "permission:1".to_string(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "permission:1"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    #[test]
    fn a_toolless_permission_hook_names_the_last_tool_headline() {
        // EXP-529: "Session paused" names no tool — the freshest published
        // tool headline stands in, so the degraded card reads
        // "Permission · ToolSearch" instead of "Permission · Tool".
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.observe_published(&ActivityEvent::tool("ToolSearch", None), &sender);
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Session paused".to_string(),
                tool: None,
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        steer.pending_permission.as_mut().unwrap().seen =
            Instant::now() - PERMISSION_GRID_CONFIRM;
        steer.permission_timeout(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, detail, .. }] => {
                assert_eq!(tool, "ToolSearch");
                assert_eq!(detail.as_deref(), Some("Session paused"));
            }
            other => panic!("expected the informational card, got {other:?}"),
        }
    }

    #[test]
    fn a_stale_tool_headline_never_names_the_degraded_card() {
        // A minutes-old headline is a different call entirely — the literal
        // fallback is more honest than a wrong name.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.last_tool = Some((
            "ToolSearch".to_string(),
            Instant::now() - PERMISSION_TOOL_RECENCY - Duration::from_secs(1),
        ));
        steer.apply_hook(
            hook(HookEventKind::PermissionPrompt {
                message: "Session paused".to_string(),
                tool: None,
            }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        steer.pending_permission.as_mut().unwrap().seen =
            Instant::now() - PERMISSION_GRID_CONFIRM;
        steer.permission_timeout(&sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Permission { tool, .. }] => assert_eq!(tool, "Tool"),
            other => panic!("expected the informational card, got {other:?}"),
        }
    }

    #[test]
    fn launch_narration_names_the_effective_permission_mode() {
        assert_eq!(
            launch_narration(true, false),
            "Session started · permissions skipped"
        );
        assert_eq!(launch_narration(false, true), "Session started · plan mode");
        assert_eq!(launch_narration(false, false), "Session started");
    }

    #[test]
    fn the_published_plan_options_drop_the_feedback_row() {
        // EXP-529: "Tell Claude what to change" remotely only opens an
        // invisible inline editor — the composer is the feedback path, so
        // the row never rides the wire. Keys stay real for the survivors.
        let redactor = Redactor::new(vec![]);
        let options = plan_publish_options(
            vec![
                QuestionOption::new("Yes, auto-accept edits", "1"),
                QuestionOption::new("Yes, manually approve edits", "2"),
                QuestionOption::new("No, refine with Ultraplan on Claude Code on the web", "3"),
                QuestionOption::new("Tell Claude what to change", "4"),
            ],
            &redactor,
        );
        assert_eq!(
            options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Yes, auto-accept edits"),
                ("2", "Yes, manually approve edits"),
            ]
        );
    }

    /// EXP-679: the `Stop` hook is the end-of-turn edge — the session parks
    /// on `needs_input` immediately, not a minute later when claude's idle
    /// Notification finally fires.
    #[test]
    fn stop_hook_parks_the_session_as_idle() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::Stop),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(
            steer.attention == Some(Attention::Idle),
            "the turn is over — your turn"
        );
        assert!(steer.turn_idle, "and the graceful stop may proceed");
        // A SessionEnd is the process going away, not a human's turn.
        let mut ending = SteerState::default();
        ending.apply_hook(
            hook(HookEventKind::SessionEnd { reason: None }),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        assert!(ending.attention.is_none(), "a teardown parks nothing");
        drained(&rx);
    }

    /// EXP-679: claude flushes the turn's final assistant entry AFTER the
    /// `Stop` hook — the blanket clear in `observe_published` used to un-idle
    /// the session for good (the idle Notification never re-fires for the
    /// same idle period), which is exactly the stuck "Working…" row.
    #[test]
    fn assistant_flush_after_stop_keeps_idle() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(
            hook(HookEventKind::Stop),
            &sender,
            &Redactor::new(vec![]),
            &mut transcript,
        );
        steer.observe_published(&ActivityEvent::narration("Done — the tests pass."), &sender);
        assert!(
            steer.attention == Some(Attention::Idle),
            "a late prose flush is not a new turn"
        );
        drained(&rx);
    }

    /// EXP-679: only evidence of a NEW turn un-parks an idle session — the
    /// human's message, or the agent dispatching its first tool.
    #[test]
    fn user_message_after_stop_clears_idle() {
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut transcript = TranscriptState::default();

        let mut steer = SteerState::default();
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        steer.observe_published(
            &ActivityEvent::UserMessage {
                text: "and now the migration".to_string(),
                at: None,
            },
            &sender,
        );
        assert!(steer.attention.is_none(), "a human turn landed");

        let mut steer = SteerState::default();
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        steer.observe_published(&ActivityEvent::tool("Read", None), &sender);
        assert!(steer.attention.is_none(), "the agent is working again");
        drained(&rx);
    }

    /// EXP-679 must not regress EXP-455: ANY transcript progress still
    /// clears a permission block — the blocking call is through.
    #[test]
    fn transcript_progress_still_clears_a_permission_block() {
        let (sender, _rx) = ActivitySender::test_pair();
        let mut steer = SteerState {
            attention: Some(Attention::Permission),
            ..Default::default()
        };
        steer.observe_published(&ActivityEvent::narration("running the command"), &sender);
        assert!(steer.attention.is_none(), "progress clears the block");
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

    /// The single-select Size tab with claude's synthetic free-text row
    /// (EXP-513). `cursor_on` places the `❯`.
    fn size_rows(cursor_on: u32, free_text_label: &str) -> Vec<String> {
        [
            "──────────────────────────────────────────",
            "←  ☒ Toppings  ☐ Size  ✔ Submit  →",
            "",
            "Which size?",
            "",
            &format!("{} 1. Small", if cursor_on == 1 { "❯" } else { " " }),
            &format!("{} 2. Large", if cursor_on == 2 { "❯" } else { " " }),
            &format!("{} 3. {free_text_label}", if cursor_on == 3 { "❯" } else { " " }),
            "──────────────────────────────────────────",
            "  4. Chat about this",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    #[test]
    fn a_free_text_answer_types_the_reply_into_the_row() {
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &size_rows(1, "Type something."));

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        let snapshot = question_picker::detect(&screen_lines(&term)).expect("picker");
        steer.confirm_question_from_grid(&snapshot, &sender, &redactor);
        // The grid augmentation re-published the Size question with the
        // flagged free-text row.
        let flagged = drained(&rx).iter().any(|event| {
            matches!(event, ActivityEvent::Question { id, options, .. }
                if id.as_deref() == Some("toolu_01#1")
                    && options.iter().any(|o| o.label == "Type something." && o.free_text))
        });
        assert!(flagged, "the free-text row reaches the wire flagged");

        // digit → cursor onto the row → typed reply fills it → Enter submits.
        let keys = Arc::new(Mutex::new(Vec::new()));
        let write_input: InputHook = {
            let term = term.clone();
            let keys = keys.clone();
            Arc::new(move |bytes| {
                let key = String::from_utf8_lossy(bytes).to_string();
                match key.as_str() {
                    "3" => paint(&term, &size_rows(3, "Type something.")),
                    "medium please" => paint(&term, &size_rows(3, "medium please")),
                    "\r" => paint(&term, &review_rows()),
                    _ => {}
                }
                keys.lock().unwrap().push(key);
            })
        };
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#1".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["3".to_string()],
                text: Some("medium\nplease".to_string()), // sanitized to one line
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["3", "medium please", "\r"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "toolu_01#1"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    #[test]
    fn a_text_less_tap_on_the_free_text_row_is_refused() {
        // Enter on the still-EMPTY free-text row declines the WHOLE ask
        // (observed live on v2.1.233) — a keys-only answer for it (old
        // client) must inject nothing rather than run the blind
        // digit-then-Enter (EXP-513's original failure).
        let emulator = terminal::Emulator::new(100, 30);
        let term = emulator.term();
        paint(&term, &size_rows(1, "Type something."));

        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        let snapshot = question_picker::detect(&screen_lines(&term)).expect("picker");
        steer.confirm_question_from_grid(&snapshot, &sender, &redactor);
        drained(&rx);

        for text in [None, Some("   ".to_string())] {
            let (write_input, keys) = recording_input(term.clone(), None, "never");
            let outcome = steer.handle_answer(
                &RemoteAnswer {
                    question_id: "toolu_01#1".to_string(),
                    ask_id: Some("toolu_01".to_string()),
                    keys: vec!["3".to_string()],
                    text,
                },
                &term,
                &write_input,
                &sender,
            );
            assert_eq!(outcome, AnswerAttempt::Settled);
            assert!(keys.lock().unwrap().is_empty(), "nothing injected");
            assert!(drained(&rx).is_empty(), "no ack — the card stays answerable");
        }
        // An ordinary option on the same tab still answers normally.
        let (write_input, keys) = recording_input(term.clone(), Some(review_rows()), "1");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: "toolu_01#1".to_string(),
                ask_id: Some("toolu_01".to_string()),
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id, .. }] => assert_eq!(id, "toolu_01#1"),
            other => panic!("expected an answer_ack, got {other:?}"),
        }
    }

    #[test]
    fn sanitize_answer_text_flattens_and_bounds() {
        assert_eq!(sanitize_answer_text("a\nb\tc\r\n"), "a b c");
        assert_eq!(sanitize_answer_text("esc\x1b[2Jseq\x07"), "esc[2Jseq");
        assert_eq!(sanitize_answer_text("  padded  "), "padded");
        let long = "x".repeat(ANSWER_TEXT_MAX + 100);
        assert_eq!(sanitize_answer_text(&long).chars().count(), ANSWER_TEXT_MAX);
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
                text: None,
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
            text: None,
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
                text: None,
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
        steer.observe_published(
            &ActivityEvent::QuestionResolved {
                id: None,
                ask_id: Some("toolu_ask".to_string()),
                answers: None,
                dismissed: Some(true),
                at: None,
            },
            &sender,
        );
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
    fn a_bypass_mode_permission_prompt_arms_like_any_other() {
        // EXP-564, reversing EXP-275's downgrade: claude flags dangerous
        // commands even under --dangerously-skip-permissions ("Dangerous rm
        // operation on possibly-empty variable path"), so a bypass session's
        // permission Notification is a REAL prompt — hold the informational
        // card, arm the grid leniency + degraded fallback, mark blocked.
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
        assert!(drained(&rx).is_empty(), "the card is held for the grid");
        assert!(
            steer.attention == Some(Attention::Permission),
            "blocked on approval, not idle"
        );
        assert!(steer.pending_permission.is_some(), "the hold is armed");
    }

    #[test]
    fn a_stop_retired_question_republishes_while_the_dialog_persists() {
        // EXP-564: a BACKGROUND subagent's permission dialog outlives the
        // main turn — the Stop hook retires the published question on the
        // "turn over ⇒ no dialog" assumption, and the still-latched watcher
        // then never re-fired, leaving the session parked with nothing on
        // the relay. The reconcile unlatches so the steady dialog
        // re-publishes as a fresh question.
        let (sender, rx) = ActivitySender::test_pair();
        let redactor = Redactor::new(vec![]);
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        let mut watcher = crate::permission_picker::PermissionPickerWatcher::new();
        let dialog = permission_dialog_rows();

        // The subagent's dialog settles on the grid and publishes.
        assert!(watcher.tick(&dialog, 0, false).is_none());
        let snapshot = match watcher.tick(&dialog, 0, false) {
            Some(crate::permission_picker::Transition::Show(snapshot)) => snapshot,
            other => panic!("expected Show, got {other:?}"),
        };
        steer.publish_permission_question(snapshot, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, .. }] => {
                assert_eq!(id.as_deref(), Some("permission:1"));
            }
            other => panic!("expected the question, got {other:?}"),
        }

        // A live question keeps the watcher latched.
        reconcile_permission_watcher(&mut watcher, &steer);
        assert!(watcher.is_pending());

        // The main turn ends while the dialog is still up — Stop retires it.
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        match &drained(&rx)[..] {
            [ActivityEvent::QuestionResolved { id, .. }] => {
                assert_eq!(id.as_deref(), Some("permission:1"));
            }
            other => panic!("expected the retirement, got {other:?}"),
        }
        assert!(watcher.is_pending(), "the watcher still latches the dialog");

        // The reconcile unlatches; the steady dialog re-fires Show a
        // debounce later and re-publishes as a fresh question.
        reconcile_permission_watcher(&mut watcher, &steer);
        assert!(!watcher.is_pending());
        assert!(watcher.tick(&dialog, 0, false).is_none());
        let snapshot = match watcher.tick(&dialog, 0, false) {
            Some(crate::permission_picker::Transition::Show(snapshot)) => snapshot,
            other => panic!("expected the re-Show, got {other:?}"),
        };
        steer.publish_permission_question(snapshot, &sender, &redactor);
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, .. }] => {
                assert_eq!(id.as_deref(), Some("permission:2"));
            }
            other => panic!("expected the fresh question, got {other:?}"),
        }

        // A pending ask owns the screen story — the reconcile never
        // unlatches under it (the Show-suppression path handles that arc).
        steer.resolve_permission(&sender);
        drained(&rx);
        steer.apply_hook(ask_hook(), &sender, &redactor, &mut transcript);
        drained(&rx);
        reconcile_permission_watcher(&mut watcher, &steer);
        assert!(watcher.is_pending(), "latched while the ask is pending");
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
        // An empty pin (no sidecar / old claude) keeps the legacy behavior.
        let found = sidechain_transcripts(&dir, after, &TranscriptPin::default());
        let names: HashSet<String> = found
            .iter()
            .filter_map(|path| sidechain_agent_id(path))
            .collect();
        assert_eq!(
            names,
            HashSet::from(["flat".to_string(), "nested".to_string()])
        );
        assert_eq!(
            newest_transcript(&dir, after, &TranscriptPin::default()),
            Some(dir.join("sess-1.jsonl"))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── EXP-429: the transcript ownership pin ──────────────────────────────

    fn pin_of(sessions: &[&str], agents: &[&str]) -> TranscriptPin {
        TranscriptPin {
            sessions: sessions.iter().map(|s| s.to_string()).collect(),
            agents: agents.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn pinned_newest_transcript_ignores_foreign_sessions() {
        let dir = std::env::temp_dir().join(format!(
            "exp-pin-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let now = SystemTime::now();
        // The foreign session's transcript is strictly newer (mtimes set
        // explicitly — same-second writes would tie on `modified >= best`).
        std::fs::write(dir.join("ours.jsonl"), "{}\n").unwrap();
        std::fs::File::options()
            .append(true)
            .open(dir.join("ours.jsonl"))
            .unwrap()
            .set_modified(now - Duration::from_secs(30))
            .unwrap();
        std::fs::write(dir.join("foreign.jsonl"), "{}\n").unwrap();
        std::fs::File::options()
            .append(true)
            .open(dir.join("foreign.jsonl"))
            .unwrap()
            .set_modified(now)
            .unwrap();

        let after = now - Duration::from_secs(60);
        // Unpinned = legacy behavior: the newest file wins (this is the very
        // hijack the pin exists to prevent — the degraded fallback).
        assert_eq!(
            newest_transcript(&dir, after, &TranscriptPin::default()),
            Some(dir.join("foreign.jsonl"))
        );
        // Pinned: ours wins despite being older.
        assert_eq!(
            newest_transcript(&dir, after, &pin_of(&["ours"], &[])),
            Some(dir.join("ours.jsonl"))
        );
        // The /clear shape — both ids are ours — still supersedes to the
        // newest pinned file.
        assert_eq!(
            newest_transcript(&dir, after, &pin_of(&["ours", "foreign"], &[])),
            Some(dir.join("foreign.jsonl"))
        );
        // A pin that matches nothing yields nothing (never the foreign file).
        assert_eq!(newest_transcript(&dir, after, &pin_of(&["gone"], &[])), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pinned_sidechain_discovery_filters_foreign_agents() {
        let dir = std::env::temp_dir().join(format!(
            "exp-pin-side-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ours_nested = dir.join("ours-sess").join("subagents");
        let foreign_nested = dir.join("foreign-sess").join("subagents");
        std::fs::create_dir_all(&ours_nested).unwrap();
        std::fs::create_dir_all(&foreign_nested).unwrap();
        std::fs::write(dir.join("agent-mine.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.join("agent-theirs.jsonl"), "{}\n").unwrap();
        std::fs::write(ours_nested.join("agent-nested.jsonl"), "{}\n").unwrap();
        std::fs::write(foreign_nested.join("agent-far.jsonl"), "{}\n").unwrap();

        let after = SystemTime::now() - Duration::from_secs(60);
        let names = |pin: &TranscriptPin| -> HashSet<String> {
            sidechain_transcripts(&dir, after, pin)
                .iter()
                .filter_map(|path| sidechain_agent_id(path))
                .collect()
        };
        // Unpinned: everything (legacy).
        assert_eq!(
            names(&TranscriptPin::default()),
            HashSet::from([
                "mine".to_string(),
                "theirs".to_string(),
                "nested".to_string(),
                "far".to_string(),
            ])
        );
        // Pinned: nested files under our session dir plus hook-announced
        // flat agents — the foreign session's files stay out.
        assert_eq!(
            names(&pin_of(&["ours-sess"], &["mine"])),
            HashSet::from(["mine".to_string(), "nested".to_string()])
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_hook_feeds_the_transcript_pin() {
        let (sender, _rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        let redactor = Redactor::new(vec![]);

        // A context-less delivery pins nothing.
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        assert!(!steer.pin.pinned());

        // session_id + transcript_path stem both land in the pin; an
        // `agent-*` stem never does (a sidechain path is not a session id).
        steer.apply_hook(
            HookEvent {
                context: crate::hooks::HookContext {
                    session_id: Some("s1".to_string()),
                    transcript_path: Some("/x/projects/p/s2.jsonl".to_string()),
                    cwd: None,
                },
                kind: HookEventKind::Stop,
            },
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(
            HookEvent {
                context: crate::hooks::HookContext {
                    session_id: None,
                    transcript_path: Some("/x/projects/p/agent-a1.jsonl".to_string()),
                    cwd: None,
                },
                kind: HookEventKind::SubagentStarted {
                    agent_id: Some("a1".to_string()),
                    agent_type: Some("explore".to_string()),
                },
            },
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.pin.pinned());
        assert_eq!(
            steer.pin.sessions,
            HashSet::from(["s1".to_string(), "s2".to_string()])
        );
        assert_eq!(steer.pin.agents, HashSet::from(["a1".to_string()]));
    }

    /// EXP-443: a spawn-seeded pin means `owns_main` never falls back to the
    /// blanket true — the pre-first-hook leak window is closed — and the set
    /// still grows via hooks (a /clear rotation, the SessionStart self-heal).
    #[test]
    fn seeded_pin_rejects_foreign_transcripts_from_tick_zero() {
        let mut pin = TranscriptPin::default();
        pin.seed("minted-uuid");
        assert!(pin.pinned(), "seeded before any hook");
        assert!(pin.owns_main(Path::new("/p/minted-uuid.jsonl")));
        assert!(!pin.owns_main(Path::new("/p/foreign.jsonl")));

        // The SessionStart hook unions the ACTUAL id in (self-heal if claude
        // ever ignored --session-id, and the /clear contract).
        pin.observe(&HookEvent {
            context: crate::hooks::HookContext {
                session_id: Some("rotated".to_string()),
                transcript_path: None,
                cwd: None,
            },
            kind: HookEventKind::SessionStarted { source: None },
        });
        assert!(pin.owns_main(Path::new("/p/rotated.jsonl")));
        assert!(pin.owns_main(Path::new("/p/minted-uuid.jsonl")));
    }

    /// EXP-443: a SessionStart delivery both feeds the pin (via apply_hook's
    /// unconditional observe) and arms nothing else — no cards, no attention.
    #[test]
    fn apply_hook_session_start_feeds_the_pin_silently() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        let redactor = Redactor::new(vec![]);
        steer.apply_hook(
            HookEvent {
                context: crate::hooks::HookContext {
                    session_id: Some("boot-1".to_string()),
                    transcript_path: None,
                    cwd: None,
                },
                kind: HookEventKind::SessionStarted {
                    source: Some("startup".to_string()),
                },
            },
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.pin.sessions.contains("boot-1"));
        assert!(steer.attention.is_none());
        assert!(drained(&rx).is_empty(), "no events published");
    }

    // ── EXP-724: compaction + remote slash commands ────────────────────────

    fn compaction_of(event: &ActivityEvent) -> (CompactionPhase, Option<String>) {
        match event {
            ActivityEvent::Compaction { phase, trigger, .. } => (*phase, trigger.clone()),
            other => panic!("expected a compaction event, got {other:?}"),
        }
    }

    /// The bar opens on `PreCompact` and closes on `PostCompact` — once.
    #[test]
    fn compaction_hooks_open_and_close_the_bar_exactly_once() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        let redactor = Redactor::new(vec![]);

        steer.apply_hook(
            hook(HookEventKind::CompactStarted {
                trigger: Some("manual".to_string()),
                custom_instructions: Some("keep the diff".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        let events = drained(&rx);
        assert_eq!(events.len(), 1);
        assert_eq!(
            compaction_of(&events[0]),
            (CompactionPhase::Started, Some("manual".to_string()))
        );
        // The custom instructions are claude's unredacted input — never the
        // bar's payload.
        assert!(!format!("{events:?}").contains("keep the diff"));

        // A second start for the same compaction publishes nothing.
        steer.apply_hook(
            hook(HookEventKind::CompactStarted {
                trigger: Some("auto".to_string()),
                custom_instructions: None,
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(drained(&rx).is_empty());

        steer.apply_hook(
            hook(HookEventKind::CompactEnded { trigger: None }),
            &sender,
            &redactor,
            &mut transcript,
        );
        let events = drained(&rx);
        assert_eq!(events.len(), 1);
        assert_eq!(compaction_of(&events[0]), (CompactionPhase::Ended, None));

        // …and every further end edge is a no-op: the bar is shut.
        steer.apply_hook(
            hook(HookEventKind::CompactEnded { trigger: None }),
            &sender,
            &redactor,
            &mut transcript,
        );
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut transcript);
        assert!(
            drained(&rx)
                .iter()
                .all(|event| !matches!(event, ActivityEvent::Compaction { .. })),
            "no second ended"
        );
    }

    /// An unknown trigger is folded onto the wire's two values — the relay
    /// schema accepts `manual`/`auto` only and severs the socket otherwise.
    #[test]
    fn a_compaction_trigger_is_folded_onto_the_wire_vocabulary() {
        assert_eq!(normalize_compaction_trigger(None), None);
        assert_eq!(normalize_compaction_trigger(Some("manual")), Some("manual"));
        assert_eq!(normalize_compaction_trigger(Some("Manual")), Some("manual"));
        for other in ["auto", "threshold", "overflow", "whatever"] {
            assert_eq!(normalize_compaction_trigger(Some(other)), Some("auto"));
        }
    }

    /// Every fallback closes the bar, and none of them fires twice: a
    /// `SessionStart{source:"compact"}`, a turn end, and the ceiling.
    #[test]
    fn every_compaction_fallback_closes_the_bar() {
        let redactor = Redactor::new(vec![]);
        let start = |steer: &mut SteerState, sender: &ActivitySender| {
            steer.apply_hook(
                hook(HookEventKind::CompactStarted {
                    trigger: None,
                    custom_instructions: None,
                }),
                sender,
                &redactor,
                &mut TranscriptState::default(),
            );
        };

        // 1) claude re-opened the session on the compacted context.
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        start(&mut steer, &sender);
        drained(&rx);
        steer.apply_hook(
            hook(HookEventKind::SessionStarted {
                source: Some("compact".to_string()),
            }),
            &sender,
            &redactor,
            &mut TranscriptState::default(),
        );
        assert_eq!(
            compaction_of(&drained(&rx)[0]),
            (CompactionPhase::Ended, None)
        );
        // A startup/resume SessionStart is NOT an end edge.
        start(&mut steer, &sender);
        drained(&rx);
        steer.apply_hook(
            hook(HookEventKind::SessionStarted {
                source: Some("resume".to_string()),
            }),
            &sender,
            &redactor,
            &mut TranscriptState::default(),
        );
        assert!(drained(&rx).is_empty());

        // 2) the turn ended with the bar still open.
        steer.apply_hook(hook(HookEventKind::Stop), &sender, &redactor, &mut TranscriptState::default());
        assert!(drained(&rx)
            .iter()
            .any(|event| matches!(event, ActivityEvent::Compaction { phase: CompactionPhase::Ended, .. })));

        // 3) the ceiling.
        start(&mut steer, &sender);
        drained(&rx);
        steer.compaction_timeout(&sender);
        assert!(drained(&rx).is_empty(), "not yet");
        steer.compacting_since = Some(Instant::now() - COMPACTION_MAX);
        steer.compaction_timeout(&sender);
        assert_eq!(
            compaction_of(&drained(&rx)[0]),
            (CompactionPhase::Ended, None)
        );
    }

    /// The transcript's own `compact_boundary` is recorded as a FLAG, never
    /// published from the parser — only the emitter knows whether a bar is
    /// open, and an unmatched end must not reach the wire from here.
    #[test]
    fn a_compact_boundary_entry_only_arms_the_flag() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let line = r#"{"type":"system","subtype":"compact_boundary","content":"Conversation compacted","compactMetadata":{"trigger":"manual","preTokens":610302,"postTokens":15658,"durationMs":165238}}"#;
        assert!(process_transcript_line(line, &redactor, &mut state).is_empty());
        assert!(state.compact_boundary);

        // Any other system entry stays inert.
        let mut state = TranscriptState::default();
        assert!(process_transcript_line(
            r#"{"type":"system","subtype":"hook_error","content":"x"}"#,
            &redactor,
            &mut state
        )
        .is_empty());
        assert!(!state.compact_boundary);
    }

    /// Verified on claude 2.1.259: a slash command's output lands as a
    /// `system`/`local_command` entry, and a REFUSED compaction's verdict is
    /// its only end edge (no PostCompact, no boundary, no Stop).
    #[test]
    fn a_local_command_system_entry_narrates_and_can_end_a_compaction() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let events = process_transcript_line(
            r#"{"type":"system","subtype":"local_command","content":"<local-command-stdout>Not enough messages to compact.</local-command-stdout>"}"#,
            &redactor,
            &mut state,
        );
        assert!(matches!(
            &events[..],
            [ActivityEvent::Narration { text, .. }] if text == "Not enough messages to compact."
        ));
        assert!(state.compact_boundary, "the refusal closes the bar");

        // A non-compaction verdict narrates without touching the bar.
        let mut state = TranscriptState::default();
        let events = process_transcript_line(
            r#"{"type":"system","subtype":"local_command","content":"<local-command-stdout>Set model to Opus 5</local-command-stdout>"}"#,
            &redactor,
            &mut state,
        );
        assert_eq!(events.len(), 1);
        assert!(!state.compact_boundary);

        // …and TUI chrome is dropped whichever entry shape carries it.
        let mut state = TranscriptState::default();
        assert!(process_transcript_line(
            r#"{"type":"system","subtype":"local_command","content":"<local-command-stdout>Total cost: $1.20</local-command-stdout>"}"#,
            &redactor,
            &mut state,
        )
        .is_empty());
    }

    /// A slash command lands as an origin-LESS user entry — the one entry
    /// shape allowed past the `origin.kind == "human"` gate (EXP-724).
    #[test]
    fn a_slash_command_entry_publishes_the_command_line() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let entry = |body: &str| {
            format!(
                r#"{{"type":"user","message":{{"role":"user","content":{}}}}}"#,
                serde_json::to_string(body).unwrap()
            )
        };
        let events = process_transcript_line(
            &entry(
                "<command-name>/compact</command-name>\n            \
                 <command-message>compact</command-message>\n            \
                 <command-args>opus</command-args>",
            ),
            &redactor,
            &mut state,
        );
        assert!(matches!(
            &events[..],
            [ActivityEvent::UserMessage { text, .. }] if text == "/compact opus"
        ));

        // No args → the bare command line.
        let events = process_transcript_line(
            &entry("<command-name>/clear</command-name>\n<command-args></command-args>"),
            &redactor,
            &mut state,
        );
        assert!(matches!(
            &events[..],
            [ActivityEvent::UserMessage { text, .. }] if text == "/clear"
        ));

        // A SKILL is a bare name (and `isMeta`) — never a command echo.
        assert!(process_transcript_line(
            &entry("<command-name>workflow-authoring</command-name>"),
            &redactor,
            &mut state,
        )
        .is_empty());
        assert!(process_transcript_line(
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<command-name>/compact</command-name>"}}"#,
            &redactor,
            &mut state,
        )
        .is_empty());

        // The twin of a command the emitter DISPATCHED consumes the memory.
        state.published_commands.push("/compact opus".to_string());
        assert!(process_transcript_line(
            &entry("<command-name>/compact</command-name><command-args>opus</command-args>"),
            &redactor,
            &mut state,
        )
        .is_empty());
        assert!(state.published_commands.is_empty());
    }

    /// A command's local stdout is an ALLOW-list: the lines that prove the
    /// command took effect, ANSI-stripped; everything else is TUI chrome.
    #[test]
    fn only_allow_listed_command_output_is_narrated() {
        let redactor = Redactor::new(vec![]);
        let mut state = TranscriptState::default();
        let entry = |body: &str| {
            format!(
                r#"{{"type":"user","message":{{"role":"user","content":{}}}}}"#,
                serde_json::to_string(body).unwrap()
            )
        };
        let events = process_transcript_line(
            &entry(
                "<local-command-stdout>\u{1b}[2mCompacted (ctrl+o to see full summary)\u{1b}[22m</local-command-stdout>",
            ),
            &redactor,
            &mut state,
        );
        assert!(matches!(
            &events[..],
            [ActivityEvent::Narration { text, .. }]
                if text == "Compacted (ctrl+o to see full summary)"
        ));
        for allowed in ["Set model to Opus 5 and saved it", "Kept model as Sonnet"] {
            assert_eq!(
                process_transcript_line(
                    &entry(&format!("<local-command-stdout>{allowed}</local-command-stdout>")),
                    &redactor,
                    &mut state
                )
                .len(),
                1
            );
        }
        // Verified on 2.1.259: claude appends one "<Hook> [<shell command>]
        // completed successfully" line per hook it ran — our own sidecar's
        // curl line included. Only the verdict is published.
        let events = process_transcript_line(
            &entry(
                "<local-command-stdout>Compacted (ctrl+o to see full summary)\n\
                 PreCompact [curl -s -m 3 -X POST -K \"$EXP_HOOK_CONFIG\" …] completed successfully\n\
                 PostCompact [curl -s -m 3 -X POST …] completed successfully</local-command-stdout>",
            ),
            &redactor,
            &mut state,
        );
        assert!(matches!(
            &events[..],
            [ActivityEvent::Narration { text, .. }]
                if text == "Compacted (ctrl+o to see full summary)"
        ));
        // Everything else is dropped.
        for other in ["Total cost: $1.20", "", "Usage limit resets at 5pm"] {
            assert!(process_transcript_line(
                &entry(&format!("<local-command-stdout>{other}</local-command-stdout>")),
                &redactor,
                &mut state
            )
            .is_empty());
        }
    }

    /// The command pump echoes every command once, remembers it for the
    /// transcript twin, and refuses (never queues) while a picker is up.
    #[test]
    fn the_command_pump_echoes_dispatches_and_refuses() {
        let (sender, rx) = ActivitySender::test_pair();
        let link = CommandLink::new(None);
        let write_input: InputHook = Arc::new(|_| {});
        let mut parked = Vec::new();
        let mut published = Vec::new();
        let command = crate::commands::parse_command("/compact keep it", SessionAgent::Claude)
            .expect("catalog command");

        // No grid: the echo still lands (the steerer must see their command).
        link.submit(command.clone());
        pump_commands(
            &mut parked,
            &link,
            false,
            false,
            None,
            &write_input,
            &sender,
            Some(&mut published),
            |_| {},
        );
        let events = drained(&rx);
        assert!(matches!(
            &events[..],
            [ActivityEvent::UserMessage { text, .. }] if text == "/compact keep it"
        ));
        assert_eq!(published, vec!["/compact keep it".to_string()]);
        assert!(parked.is_empty());

        // A picker on the grid refuses outright — a command is not worth
        // Esc-ing an approval away for.
        link.submit(command.clone());
        pump_commands(
            &mut parked,
            &link,
            true,
            false,
            None,
            &write_input,
            &sender,
            None,
            |_| {},
        );
        let events = drained(&rx);
        assert!(matches!(&events[1], ActivityEvent::Narration { text, .. } if text == COMMAND_REFUSED_PICKER));
        assert!(parked.is_empty());

        // Idle-gated (codex/pi): held while the agent works, refused at the
        // ceiling — and never dispatched.
        link.set_composer_idle(false);
        link.submit(command.clone());
        pump_commands(
            &mut parked,
            &link,
            false,
            true,
            None,
            &write_input,
            &sender,
            None,
            |_| panic!("must not dispatch while busy"),
        );
        assert_eq!(parked.len(), 1);
        drained(&rx);
        parked[0].1 = Instant::now() - COMMAND_IDLE_WAIT;
        pump_commands(
            &mut parked,
            &link,
            false,
            true,
            None,
            &write_input,
            &sender,
            None,
            |_| panic!("must not dispatch while busy"),
        );
        assert!(parked.is_empty());
        let events = drained(&rx);
        assert!(matches!(&events[0], ActivityEvent::Narration { text, .. } if text == COMMAND_REFUSED_BUSY));

        // Idle again → the hold lifts on the first tick (this session has no
        // grid to type into, so it settles instead of parking; the typing
        // path itself is covered by the pi-sink test and the live harness).
        link.set_composer_idle(true);
        link.submit(command);
        pump_commands(
            &mut parked,
            &link,
            false,
            true,
            None,
            &write_input,
            &sender,
            None,
            |_| {},
        );
        assert!(parked.is_empty(), "no longer held");
    }

    /// Pi's commands never reach a PTY: the link's sink takes them.
    #[test]
    fn a_pi_command_goes_to_the_sink_not_the_terminal() {
        let (sender, _rx) = ActivitySender::test_pair();
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let link = CommandLink::new(Some(Arc::new(move |name: &str, args: &str| {
            sink_seen
                .lock()
                .unwrap()
                .push((name.to_string(), args.to_string()));
        })));
        link.set_composer_idle(true);
        let write_input: InputHook = Arc::new(|_| panic!("pi never types"));
        let mut parked = Vec::new();
        link.submit(
            crate::commands::parse_command("/compact keep the diff", SessionAgent::Pi).unwrap(),
        );
        pump_commands(
            &mut parked,
            &link,
            false,
            true,
            None,
            &write_input,
            &sender,
            None,
            |_| {},
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[("compact".to_string(), "keep the diff".to_string())]
        );
    }

    /// The submit probe reads the COMPOSER, not the scrollback: the command
    /// echoed into history above the input box must not look like a stuck
    /// composer (that would Enter forever).
    #[test]
    fn the_submit_probe_only_looks_at_the_composer_tail() {
        let lines: Vec<String> = vec![
            "> /compact".to_string(),
            "  Compacting…".to_string(),
            "".to_string(),
            "".to_string(),
            "╭──────────────╮".to_string(),
            "│ >            │".to_string(),
            "╰──────────────╯".to_string(),
        ];
        assert!(!composer_holds(&lines, "/compact"));
        let mut stuck = lines.clone();
        stuck[5] = "│ > /compact   │".to_string();
        assert!(composer_holds(&stuck, "/compact"));
    }

    /// EXP-443: a SubagentStart arms the one-shot rescan flag (the emitter
    /// skips the sidechain debounce with it); the read consumes it.
    #[test]
    fn subagent_start_arms_a_one_shot_sidechain_rescan() {
        let (sender, _rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let mut transcript = TranscriptState::default();
        let redactor = Redactor::new(vec![]);
        assert!(!steer.take_subagent_seen());
        steer.apply_hook(
            hook(HookEventKind::SubagentStarted {
                agent_id: Some("a1".to_string()),
                agent_type: Some("explore".to_string()),
            }),
            &sender,
            &redactor,
            &mut transcript,
        );
        assert!(steer.take_subagent_seen());
        assert!(!steer.take_subagent_seen(), "consuming read");
    }

    /// EXP-444: the mistimed-paste refusal is one-shot — a note disarms it
    /// so the user's re-send flows; disarm clears without a note.
    #[test]
    fn login_refusal_is_one_shot() {
        let (link, _rx) = AnswerLink::new();
        assert!(!link.login_refusal_active());
        link.arm_login_refusal();
        assert!(link.login_refusal_active());
        link.note_login_refusal();
        assert!(!link.login_refusal_active(), "note disarms");
        assert!(link.take_login_refusal_note());
        assert!(!link.take_login_refusal_note(), "consuming read");

        link.arm_login_refusal();
        link.disarm_login_refusal();
        assert!(!link.login_refusal_active());
        assert!(!link.take_login_refusal_note(), "disarm leaves no note");
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

    /// The mid-session `/login` method picker (captured, v2.1.222).
    fn login_method_rows() -> Vec<String> {
        [
            "   Login",
            "",
            "   Select login method:",
            "",
            "   ❯ 1. Claude account with subscription · Pro, Max, Team, or Enterprise",
            "     2. Anthropic Console account · API usage billing",
            "",
            "   Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect()
    }

    fn login_question(steer: &mut SteerState, sender: &ActivitySender) -> String {
        steer.publish_login_question(
            vec![
                QuestionOption::new("Claude account with subscription", "1"),
                QuestionOption::new("Anthropic Console account", "2"),
            ],
            sender,
        );
        steer.login.clone().expect("login question live")
    }

    #[test]
    fn a_login_answer_is_injected_once_and_acked() {
        let emulator = terminal::Emulator::new(120, 30);
        let term = emulator.term();
        paint(&term, &login_method_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let id = login_question(&mut steer, &sender);
        drained(&rx);

        // The digit alone does not move the login picker (probe times out),
        // so Enter follows and the grid repaints past the picker.
        let (write_input, keys) =
            recording_input(term.clone(), Some(vec!["❯ ".to_string()]), "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: id.clone(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(*keys.lock().unwrap(), vec!["1", "\r"]);
        match &drained(&rx)[..] {
            [ActivityEvent::AnswerAck { id: acked, ask_id, .. }] => {
                assert_eq!(acked, &id);
                assert_eq!(ask_id, &None);
            }
            other => panic!("expected an answer_ack, got {other:?}"),
        }

        // A duplicate never injects again but re-acks (EXP-374 semantics).
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: id.clone(),
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert_eq!(keys.lock().unwrap().len(), 2, "no second injection");
        assert!(matches!(
            &drained(&rx)[..],
            [ActivityEvent::AnswerAck { .. }]
        ));
    }

    #[test]
    fn a_login_answer_waits_for_the_method_picker_phase() {
        let emulator = terminal::Emulator::new(120, 30);
        let term = emulator.term();
        // The flow already advanced to the URL/paste screen.
        paint(
            &term,
            &[
                "   Browser didn't open? Use the url below to sign in (c to copy)".to_string(),
                "https://claude.com/cai/oauth/authorize?code=true".to_string(),
                "".to_string(),
                "   Paste code here if prompted >".to_string(),
            ],
        );

        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let id = login_question(&mut steer, &sender);
        drained(&rx);

        let (write_input, keys) = recording_input(term.clone(), None, "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: id,
                ask_id: None,
                keys: vec!["1".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Retry, "typing would hit the paste box");
        assert!(keys.lock().unwrap().is_empty());
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn a_login_answer_with_an_unoffered_key_settles_without_injecting() {
        let emulator = terminal::Emulator::new(120, 30);
        let term = emulator.term();
        paint(&term, &login_method_rows());

        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        // "3" (the filtered 3rd-party option) is not among the published
        // options — even though a row with that key could be on the grid.
        let id = login_question(&mut steer, &sender);
        drained(&rx);

        let (write_input, keys) = recording_input(term.clone(), None, "\r");
        let outcome = steer.handle_answer(
            &RemoteAnswer {
                question_id: id,
                ask_id: None,
                keys: vec!["3".to_string()],
                text: None,
            },
            &term,
            &write_input,
            &sender,
        );
        assert_eq!(outcome, AnswerAttempt::Settled);
        assert!(keys.lock().unwrap().is_empty());
        assert!(drained(&rx).is_empty(), "no ack for a refused key");
    }

    #[test]
    fn a_republished_login_question_retires_the_previous_card() {
        let (sender, rx) = ActivitySender::test_pair();
        let mut steer = SteerState::default();
        let first = login_question(&mut steer, &sender);
        match &drained(&rx)[..] {
            [ActivityEvent::Question { id, header, plan_mode, .. }] => {
                assert_eq!(id.as_deref(), Some(first.as_str()));
                assert_eq!(header.as_deref(), Some("Claude sign-in required"));
                assert_eq!(plan_mode, &None);
            }
            other => panic!("expected the login question, got {other:?}"),
        }

        // The OAuth-error retry loop lands back on the picker: fresh id,
        // previous card resolved first.
        let second = login_question(&mut steer, &sender);
        assert_ne!(first, second);
        match &drained(&rx)[..] {
            [
                ActivityEvent::QuestionResolved { id, .. },
                ActivityEvent::Question { id: new_id, .. },
            ] => {
                assert_eq!(id.as_deref(), Some(first.as_str()));
                assert_eq!(new_id.as_deref(), Some(second.as_str()));
            }
            other => panic!("expected resolve-then-republish, got {other:?}"),
        }
        assert!(steer.resolution_seen, "grid-picker flag clear armed");

        // Resolving without a live question is a no-op.
        steer.resolve_login(&sender);
        drained(&rx);
        steer.resolve_login(&sender);
        assert!(drained(&rx).is_empty());
    }

    #[test]
    fn redact_exact_only_preserves_the_sign_in_url() {
        // A crafted-but-realistic sign-in URL whose base64url params contain
        // pattern-shaped substrings (`&sk-…` has a word boundary before
        // `sk`): the generic patterns WOULD shred it mid-URL — that is why
        // the login narration goes through the exact-only path.
        let url = "https://claude.com/cai/oauth/authorize?code=true&code_challenge=j7BY1qKMJ1Y2LC5xNqD5VUJayK_UZbPl_FCJLsmPZzk&sk-abcdefghijklmnopqrstuv=1&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE";
        let redactor = Redactor::new(vec!["hunter2secret1234".to_string()]);
        assert_ne!(redactor.redact(url), url, "patterns mangle the URL");
        assert_eq!(redactor.redact_exact_only(url), url, "exact-only must not");

        // The session's own launcher secrets still mask on the exact path.
        let leaky = format!("{url}&t=hunter2secret1234");
        assert!(!redactor.redact_exact_only(&leaky).contains("hunter2secret1234"));
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
