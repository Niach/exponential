//! Shared pieces of the live-coding activity stream (masterplan §P7 +
//! EXP-78, steering v2 = EXP-249).
//!
//! EXP-773 deleted the PTY coding path, and with it the transcript-tailing
//! emitter that used to live here. What is left is what the in-process ACP
//! engine (`crates/engine`) and the publisher still share:
//!
//! * [`Redactor`] + [`secrets_from_worktree`] — every published string passes
//!   through exact-match masking of the launcher-created secrets (the JIT
//!   GitHub installation token from the clone's shared
//!   `.git/exp-git-credentials` credential file, EXP-73; the `expu_` personal
//!   key from `.exp-mcp.json`, or handed in by the wiring for codex/pi, which
//!   keep it env-only — REV2-17) plus gitleaks-style patterns;
//! * the relay's `activityEventSchema` caps, so the mapper truncates
//!   client-side before a frame is ever sent;
//! * [`worktree_diff`] + [`DiffSnapshots`] — the debounced `git diff`
//!   snapshot the `diff` event carries;
//! * the publisher↔engine seams: [`AnswerLink`] (a steerer's semantic
//!   answer), [`CommandLink`] (a slash command) and [`ConfigLink`] (a live
//!   model/mode switch);
//! * [`TurnSignal`], [`NeedsInputForwarder`], [`SessionAgent`] and the
//!   truncation helpers.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::Value;

use crate::frames::ActivityEvent;
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
/// Relay-enforced option-count cap; also the range of digit keys we can map.
pub const QUESTION_OPTIONS_MAX: usize = 9;

/// EXP-746 `config_state` caps, mirrored from `protocol.ts`. An over-cap
/// frame fails the relay's zod and the WHOLE frame is dropped in silence, so
/// the chips would simply never paint — [`clamp_config_state`] truncates
/// instead (claude forwards every project skill/custom command, which can
/// clear 64 rows in a skill-heavy repo).
pub const CONFIG_OPTIONS_MAX: usize = 8;
pub const CONFIG_VALUES_MAX: usize = 32;
pub const CONFIG_MODES_MAX: usize = 12;
pub const CONFIG_COMMANDS_MAX: usize = 64;
/// `option.id` / `value.id` / `mode.id` / `command.name` / `currentMode`.
pub const CONFIG_ID_MAX: usize = 64;
/// Every label, plus `option.value` (the zod caps both at 128).
pub const CONFIG_LABEL_MAX: usize = 128;
/// `mode.description` / `command.description`.
pub const CONFIG_DESCRIPTION_MAX: usize = 256;
/// `option.category` — the only 32-byte field of the kind.
pub const CONFIG_CATEGORY_MAX: usize = 32;
/// `command.hint`.
pub const CONFIG_HINT_MAX: usize = 64;

/// Minimum gap between worktree diff snapshots (only emitted when changed).
pub const DIFF_INTERVAL: Duration = Duration::from_secs(3);
/// Transcript tail poll cadence (also the answer-intake timeout).
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// How long a TRANSIENTLY refused remote answer may be retried before it is
/// dropped (EXP-334).
///
/// EXP-347: the viewers' answer-lock timeouts (web `ANSWER_ACK_TIMEOUT_MS`,
/// Android `ANSWER_ACK_TIMEOUT_MS`, iOS `answerLockSeconds` — all 8s) are
/// derived from this budget plus a tick/relay margin. Grow them in lockstep
/// or a worst-case ack lands after the card already flashed "Failed".
pub const ANSWER_RETRY_TTL: Duration = Duration::from_secs(4);

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
pub fn synthetic_question_id(
    session: &str,
    kind: &str,
    text: &str,
    ordinal: u32,
) -> String {
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
pub fn worktree_diff(worktree: &Path, base_ref: Option<&str>) -> Option<String> {
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
// ---------------------------------------------------------------------------
// The publisher ↔ engine answer seam (EXP-249)
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

/// The one-way seam between the publisher task (tokio) and the ACP engine:
/// a steerer's semantic answer goes down the channel and the engine maps it
/// onto the pending ACP request. Clients never guess keystrokes.
pub struct AnswerLink {
    tx: flume::Sender<RemoteAnswer>,
}

impl AnswerLink {
    /// The link plus the engine's receiving end.
    pub fn new() -> (Arc<Self>, flume::Receiver<RemoteAnswer>) {
        let (tx, rx) = flume::unbounded();
        (Arc::new(Self { tx }), rx)
    }

    /// Publisher side: hand one answer to the engine (fire-and-forget — a
    /// dead engine just means the card stays unanswered).
    pub fn submit(&self, answer: RemoteAnswer) {
        let _ = self.tx.send(answer);
    }
}

/// EXP-724: fold an agent's compaction reason onto the wire's two values.
/// The relay's schema accepts `manual` | `auto` ONLY (pi reports
/// `threshold`/`overflow`, a future claude could report anything else), and
/// an unknown trigger would sever the publisher socket.
pub fn normalize_compaction_trigger(trigger: Option<&str>) -> Option<&'static str> {
    match trigger {
        None => None,
        Some(trigger) if trigger.eq_ignore_ascii_case("manual") => Some("manual"),
        Some(_) => Some("auto"),
    }
}

// ---------------------------------------------------------------------------
// The publisher ↔ engine command seam (EXP-724)
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

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// The publisher ↔ ENGINE live-config seam (EXP-746)
// ---------------------------------------------------------------------------

/// EXP-746: one live-config change from a steerer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigChange {
    /// `set_config` — the option id and the value in force from now on. A
    /// BLANK value is the "CLI default / omit the flag" choice.
    Option { id: String, value: String },
    /// `set_mode` — one of the ids the publisher advertised in
    /// `config_state.modes`.
    Mode { id: String },
}

/// The publisher ↔ ENGINE seam for live config, sibling of [`AnswerLink`] and
/// [`CommandLink`] (EXP-746).
///
/// The publisher only ROUTES: it has no ACP session and cannot know whether a
/// model even exists. The engine drains this, calls `session/set_config_option`
/// or `session/set_mode`, and re-emits `config_state` — that re-emit is the
/// ONLY confirmation the wire has, which is why nothing here acks and nothing
/// blocks.
///
/// One `Arc` is shared by both sides (the receiver lives inside), so the
/// wiring builds it once and hands the same handle to
/// [`crate::publisher::PublisherHooks`] and the engine.
pub struct ConfigLink {
    tx: flume::Sender<ConfigChange>,
    rx: flume::Receiver<ConfigChange>,
}

impl ConfigLink {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = flume::unbounded();
        Arc::new(Self { tx, rx })
    }

    /// Publisher side: hand one change to the engine (fire-and-forget — a
    /// dead engine just means it never applies).
    pub fn submit(&self, change: ConfigChange) {
        let _ = self.tx.send(change);
    }

    /// Engine side: the next queued change, if any.
    pub fn try_recv(&self) -> Option<ConfigChange> {
        self.rx.try_recv().ok()
    }

    /// Engine side: the receiver for its own `select!` arm.
    pub fn receiver(&self) -> flume::Receiver<ConfigChange> {
        self.rx.clone()
    }
}

/// EXP-746: clamp an [`ActivityEvent::ConfigState`] to the relay's zod caps.
/// A no-op for every other kind.
///
/// An over-cap frame does NOT degrade — `activityEventSchema` is a
/// discriminated union, so the relay drops the whole frame in silence and the
/// viewer's chips never paint at all. Truncating is always the better answer:
/// a shortened label still names the model, and a 65th agent command is one a
/// `/` menu filtered by draft would rarely have shown anyway.
///
/// EXP-758: the caps are only half the schema. `protocol.ts` also puts
/// `.min(1)` on every MACHINE id — `options[].id`, `options[].values[].id`,
/// `modes[].id` and `commands[].name` — so ONE empty string an adapter let
/// through (an agent command with no name, a model list with a blank entry)
/// drops the whole frame just as silently as an over-cap label. Empty-id rows
/// are dropped here instead, and the CURRENT selections that name them
/// (`current_mode`, `option.value`) are nulled — those two carry no `.min(1)`,
/// but an empty one matches no id and would only paint a blank chip.
pub fn clamp_config_state(event: &mut ActivityEvent) {
    let ActivityEvent::ConfigState {
        options,
        current_mode,
        modes,
        commands,
        ..
    } = event
    else {
        return;
    };
    options.retain(|option| !option.id.is_empty());
    options.truncate(CONFIG_OPTIONS_MAX);
    for option in options.iter_mut() {
        option.id = truncate(&option.id, CONFIG_ID_MAX);
        option.label = truncate(&option.label, CONFIG_LABEL_MAX);
        if let Some(category) = &mut option.category {
            *category = truncate(category, CONFIG_CATEGORY_MAX);
        }
        if option.value.as_deref().is_some_and(str::is_empty) {
            option.value = None;
        }
        if let Some(value) = &mut option.value {
            *value = truncate(value, CONFIG_LABEL_MAX);
        }
        if let Some(values) = &mut option.values {
            values.retain(|value| !value.id.is_empty());
            values.truncate(CONFIG_VALUES_MAX);
            for value in values.iter_mut() {
                value.id = truncate(&value.id, CONFIG_ID_MAX);
                value.label = truncate(&value.label, CONFIG_LABEL_MAX);
            }
        }
    }
    if current_mode.as_deref().is_some_and(str::is_empty) {
        *current_mode = None;
    }
    if let Some(mode) = current_mode {
        *mode = truncate(mode, CONFIG_ID_MAX);
    }
    if let Some(modes) = modes {
        modes.retain(|mode| !mode.id.is_empty());
        modes.truncate(CONFIG_MODES_MAX);
        for mode in modes.iter_mut() {
            mode.id = truncate(&mode.id, CONFIG_ID_MAX);
            mode.label = truncate(&mode.label, CONFIG_LABEL_MAX);
            if let Some(description) = &mut mode.description {
                *description = truncate(description, CONFIG_DESCRIPTION_MAX);
            }
        }
    }
    if let Some(commands) = commands {
        commands.retain(|command| !command.name.is_empty());
        commands.truncate(CONFIG_COMMANDS_MAX);
        for command in commands.iter_mut() {
            command.name = truncate(&command.name, CONFIG_ID_MAX);
            command.description = truncate(&command.description, CONFIG_DESCRIPTION_MAX);
            if let Some(hint) = &mut command.hint {
                *hint = truncate(hint, CONFIG_HINT_MAX);
            }
        }
    }
}

/// EXP-383: which agent CLI the session runs — picks the activity emitter.
/// Local mirror of `coding::CodingAgent` (this crate cannot depend on
/// `coding` — §3.1); the ui wiring converts by id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionAgent {
    #[default]
    Claude,
    Codex,
    Pi,
    /// EXP-746 (D13): a user-configured ACP agent binary. Deliberately
    /// NEUTRAL everywhere the other three get per-agent treatment — its
    /// [`crate::commands::catalog_for`] is empty (the contract knows no such
    /// agent, so the `/` menu carries only what the agent itself advertises
    /// through `config_state.commands`), and the codex sigil guard stays
    /// codex-only. There is no PTY emitter for it: an external agent only
    /// ever runs on the ACP path.
    External,
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

/// EXP-637: how long an agent-declared end waits for the turn to finish
/// before it tears down anyway. Generous: the wait costs nothing while the
/// agent is still producing the output the user wants to read, and the
/// fallback only exists for agents whose idle edge never arrives (a hookless
/// claude, a crashed emitter).
///
/// EXP-746 moved it here from `ui::graceful_stop` so the ACP engine — which
/// is gpui-free and hosts the same teardown for the CLI daemon — obeys the
/// same bound as the desktop; `ui::graceful_stop` re-exports both.
pub const STOP_GRACE: Duration = Duration::from_secs(60);

/// Should the teardown proceed NOW? Pure, so the policy is testable without a
/// runtime: the agent is between turns, or the grace period ran out.
pub fn stop_now(idle: bool, elapsed: Duration) -> bool {
    idle || elapsed >= STOP_GRACE
}

/// The debounced changed-only worktree diff snapshot — step 8 of every
/// emitter, extracted verbatim so the codex/pi emitters share it (EXP-383).
pub struct DiffSnapshots {
    last: String,
    last_at: Option<Instant>,
}

impl DiffSnapshots {
    pub fn new() -> Self {
        Self {
            last: String::new(),
            last_at: None,
        }
    }

    /// EXP-746: the whole rule WITHOUT a sink — the ACP engine drives its own
    /// loop and routes the event to the publisher AND to the local feed, so
    /// it cannot hand a bare [`ActivitySender`] over. `None` means "nothing
    /// to publish this tick" (not due, git failed, or the diff is unchanged).
    /// [`Self::tick`] is this plus the send.
    pub fn next_diff(
        &mut self,
        worktree: &Path,
        base_ref: Option<&str>,
        redactor: &Redactor,
    ) -> Option<ActivityEvent> {
        let due = self.last_at.is_none_or(|at| at.elapsed() >= DIFF_INTERVAL);
        if !due {
            return None;
        }
        self.last_at = Some(Instant::now());
        // A git failure (index lock, rebase in flight) is not an empty diff:
        // keep the last answer and try again next tick.
        let diff = worktree_diff(worktree, base_ref)?;
        if diff == self.last {
            return None;
        }
        let had_diff = !self.last.is_empty();
        self.last = diff.clone();
        // EXP-688: a diff that goes EMPTY publishes an explicit empty frame
        // (the wire allows `""`, and every client treats it as "no diff").
        // Sending nothing left viewers looking at a stale patch forever.
        (!diff.is_empty() || had_diff)
            .then(|| ActivityEvent::diff(truncate(&redactor.redact(&diff), DIFF_MAX)))
    }

    pub fn tick(
        &mut self,
        worktree: &Path,
        base_ref: Option<&str>,
        sender: &ActivitySender,
        redactor: &Redactor,
    ) {
        if let Some(event) = self.next_diff(worktree, base_ref, redactor) {
            sender.send(event);
        }
    }
}

/// The EXP-214 synced needs-input flag, tracked as the last CONFIRMED server
/// value (`None` = the last write failed and wants a retry). The session row
/// is born with the flag off. Forwarded on flips; an unconfirmed write
/// re-attempts every [`NEEDS_INPUT_RETRY`] (EXP-355). Extracted verbatim from
/// the claude emitter so the codex/pi emitters share it (EXP-383).
pub struct NeedsInputForwarder {
    forwarded: Option<bool>,
    retry_at: Option<Instant>,
}

pub type NeedsInputHook = Arc<dyn Fn(bool) -> bool + Send + Sync>;

impl NeedsInputForwarder {
    pub fn new() -> Self {
        Self {
            forwarded: Some(false),
            retry_at: None,
        }
    }

    pub fn tick(&mut self, pending: bool, hook: &Option<NeedsInputHook>) {
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
    pub fn clear_on_teardown(&mut self, hook: &Option<NeedsInputHook>) {
        if self.forwarded != Some(false) {
            if let Some(hook) = hook {
                hook(false);
            }
        }
    }
}

/// Truncate to at most `max` UTF-8 BYTES, backing up to a char boundary.
/// The relay enforces string caps in UTF-16 code units and the whole-frame
/// cap in bytes; UTF-8 bytes >= UTF-16 code units >= chars for any string,
/// so the byte cap is the strictest of the three. (A char-count cap let
/// CJK/emoji-heavy diffs through at up to 4x the byte budget, and the relay
/// answered an oversize frame by severing the shared publisher socket.)
pub fn truncate(s: &str, max: usize) -> String {
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
pub const TRUNCATION_MARKER: &str = "\n\n[truncated]";

/// [`truncate`], but a cut string ends in [`TRUNCATION_MARKER`] (still within
/// `max` bytes) so viewers can tell truncation from completion.
pub fn truncate_marked(s: &str, max: usize) -> String {
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

    /// EXP-746: `next_diff` is the same rule with the send taken out, so the
    /// ACP engine can route the event to the publisher AND its local feed.
    /// Mirrors the sink-driven test above answer for answer.
    #[test]
    fn next_diff_returns_what_tick_would_have_published() {
        let repo = DiffRepo::new("diff-next");
        let redactor = Redactor::new(Vec::new());
        let mut diffs = DiffSnapshots::new();

        repo.write("base.txt", "a\nwip\n");
        match diffs.next_diff(&repo.0, None, &redactor) {
            Some(ActivityEvent::Diff { diff, .. }) => assert!(diff.contains("wip"), "{diff}"),
            other => panic!("expected a diff event, got {other:?}"),
        }

        // The same 3s debounce, and it lives in `next_diff`, not in `tick`.
        assert!(
            diffs.next_diff(&repo.0, None, &redactor).is_none(),
            "the 3s debounce still holds"
        );

        // EXP-688: the clear is an explicit empty event, not silence.
        repo.commit("committed");
        diffs.last_at = None;
        match diffs.next_diff(&repo.0, None, &redactor) {
            Some(ActivityEvent::Diff { diff, .. }) => {
                assert_eq!(diff, "", "an empty event clears the viewer")
            }
            other => panic!("expected an empty diff event, got {other:?}"),
        }

        diffs.last_at = None;
        assert!(
            diffs.next_diff(&repo.0, None, &redactor).is_none(),
            "an unchanged empty diff is silent"
        );
    }

    /// EXP-746: an over-cap `config_state` must be TRUNCATED, never left to
    /// the relay — `activityEventSchema` is a discriminated union, so one
    /// oversize label drops the whole frame in silence and the viewer's chips
    /// never paint at all.
    #[test]
    fn config_state_over_cap_is_truncated_not_dropped() {
        use crate::frames::{ConfigCommand, ConfigMode, ConfigOption, ConfigValue};

        let long = "x".repeat(1000);
        let mut event = ActivityEvent::ConfigState {
            options: (0..CONFIG_OPTIONS_MAX + 3)
                .map(|i| ConfigOption {
                    category: Some(long.clone()),
                    value: Some(long.clone()),
                    values: Some(
                        (0..CONFIG_VALUES_MAX + 5)
                            .map(|j| ConfigValue::new(format!("v{j}{long}"), long.clone()))
                            .collect(),
                    ),
                    ..ConfigOption::new(format!("o{i}{long}"), long.clone())
                })
                .collect(),
            current_mode: Some(long.clone()),
            modes: Some(
                (0..CONFIG_MODES_MAX + 4)
                    .map(|i| ConfigMode {
                        description: Some(long.clone()),
                        ..ConfigMode::new(format!("m{i}{long}"), long.clone())
                    })
                    .collect(),
            ),
            commands: Some(
                (0..CONFIG_COMMANDS_MAX + 20)
                    .map(|i| ConfigCommand {
                        hint: Some(long.clone()),
                        ..ConfigCommand::new(format!("c{i}{long}"), long.clone())
                    })
                    .collect(),
            ),
            at: None,
        };
        clamp_config_state(&mut event);

        let ActivityEvent::ConfigState {
            options,
            current_mode,
            modes,
            commands,
            ..
        } = &event
        else {
            panic!("still a config_state");
        };
        assert_eq!(options.len(), CONFIG_OPTIONS_MAX);
        assert_eq!(modes.as_ref().unwrap().len(), CONFIG_MODES_MAX);
        assert_eq!(commands.as_ref().unwrap().len(), CONFIG_COMMANDS_MAX);
        assert_eq!(current_mode.as_ref().unwrap().len(), CONFIG_ID_MAX);
        for option in options {
            assert_eq!(option.id.len(), CONFIG_ID_MAX);
            assert_eq!(option.label.len(), CONFIG_LABEL_MAX);
            assert_eq!(option.category.as_ref().unwrap().len(), CONFIG_CATEGORY_MAX);
            assert_eq!(option.value.as_ref().unwrap().len(), CONFIG_LABEL_MAX);
            let values = option.values.as_ref().unwrap();
            assert_eq!(values.len(), CONFIG_VALUES_MAX);
            assert!(values
                .iter()
                .all(|v| v.id.len() == CONFIG_ID_MAX && v.label.len() == CONFIG_LABEL_MAX));
        }
        assert!(modes.as_ref().unwrap().iter().all(|mode| {
            mode.id.len() == CONFIG_ID_MAX
                && mode.label.len() == CONFIG_LABEL_MAX
                && mode.description.as_ref().unwrap().len() == CONFIG_DESCRIPTION_MAX
        }));
        assert!(commands.as_ref().unwrap().iter().all(|command| {
            command.name.len() == CONFIG_ID_MAX
                && command.description.len() == CONFIG_DESCRIPTION_MAX
                && command.hint.as_ref().unwrap().len() == CONFIG_HINT_MAX
        }));
        // Ids survive as PREFIXES: the engine still recognises the option a
        // `set_config` names (truncation is a byte cut, not a rewrite).
        assert!(options[0].id.starts_with("o0"));
        // Every other kind passes through untouched.
        let mut narration = ActivityEvent::narration("untouched");
        clamp_config_state(&mut narration);
        assert_eq!(narration, ActivityEvent::narration("untouched"));
    }

    /// EXP-758: the relay's zod schema puts `.min(1)` on every MACHINE id of
    /// a `config_state` (`options[].id`, `options[].values[].id`,
    /// `modes[].id`, `commands[].name`), and `activityEventSchema` is a
    /// discriminated union — so ONE empty id an adapter let through drops the
    /// WHOLE frame in silence and the viewer's chips never paint. The clamp
    /// drops the offending rows instead; every valid sibling survives.
    #[test]
    fn config_state_empty_ids_are_dropped_not_left_to_the_relay() {
        use crate::frames::{ConfigCommand, ConfigMode, ConfigOption, ConfigValue};

        let mut event = ActivityEvent::ConfigState {
            options: vec![
                ConfigOption {
                    // A blank value is a selection that names no id: null it
                    // rather than paint an empty chip.
                    value: Some(String::new()),
                    values: Some(vec![
                        ConfigValue::new("", "nameless"),
                        ConfigValue::new("opus", "Opus"),
                    ]),
                    ..ConfigOption::new("model", "Model")
                },
                ConfigOption::new("", "nameless option"),
                ConfigOption {
                    value: Some("high".to_string()),
                    ..ConfigOption::new("effort", "Effort")
                },
            ],
            current_mode: Some(String::new()),
            modes: Some(vec![
                ConfigMode::new("", "nameless mode"),
                ConfigMode::new("plan", "Plan"),
            ]),
            commands: Some(vec![
                ConfigCommand::new("", "nameless command"),
                ConfigCommand::new("compact", "Compact the transcript"),
            ]),
            at: None,
        };
        clamp_config_state(&mut event);

        let ActivityEvent::ConfigState {
            options,
            current_mode,
            modes,
            commands,
            ..
        } = &event
        else {
            panic!("still a config_state");
        };
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["model", "effort"]
        );
        assert_eq!(options[0].value, None, "a blank selection is nulled");
        assert_eq!(
            options[0]
                .values
                .as_ref()
                .unwrap()
                .iter()
                .map(|v| v.id.as_str())
                .collect::<Vec<_>>(),
            vec!["opus"]
        );
        assert_eq!(
            options[1].value.as_deref(),
            Some("high"),
            "a real selection is untouched"
        );
        assert_eq!(*current_mode, None);
        assert_eq!(
            modes
                .as_ref()
                .unwrap()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["plan"]
        );
        assert_eq!(
            commands
                .as_ref()
                .unwrap()
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["compact"]
        );
    }

    /// EXP-746: the publisher→engine seam is a plain queue — submit, drain,
    /// nothing acks (the re-emitted `config_state` is the confirmation).
    #[test]
    fn the_config_link_queues_changes_for_the_engine() {
        let link = ConfigLink::new();
        assert_eq!(link.try_recv(), None);
        link.submit(ConfigChange::Option {
            id: "model".to_string(),
            value: String::new(),
        });
        link.submit(ConfigChange::Mode {
            id: "plan".to_string(),
        });
        assert_eq!(
            link.try_recv(),
            Some(ConfigChange::Option {
                id: "model".to_string(),
                // A blank value is the "CLI default" choice and must survive
                // the seam intact.
                value: String::new(),
            })
        );
        assert_eq!(
            link.receiver().try_recv().ok(),
            Some(ConfigChange::Mode {
                id: "plan".to_string(),
            })
        );
        assert_eq!(link.try_recv(), None);
    }

    /// EXP-637 (moved here by EXP-746, `ui::graceful_stop` re-exports it).
    #[test]
    fn stop_now_waits_for_idle_but_never_past_the_grace() {
        // Mid-turn: wait.
        assert!(!stop_now(false, Duration::ZERO));
        assert!(!stop_now(false, STOP_GRACE - Duration::from_millis(1)));
        // Between turns: go, however early.
        assert!(stop_now(true, Duration::ZERO));
        // A turn that never ends must not park the teardown forever.
        assert!(stop_now(false, STOP_GRACE));
        assert!(stop_now(false, STOP_GRACE + Duration::from_secs(60)));
        // The grace has to be long enough for a real close-out message and
        // short enough that a hung agent's tab still resolves while someone
        // is watching it.
        assert_eq!(STOP_GRACE, Duration::from_secs(60));
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

}
