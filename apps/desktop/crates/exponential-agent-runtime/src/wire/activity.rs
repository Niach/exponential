//! The activity stream's pure half (EXP-1426 part 2, moved out of
//! `steer::activity`; the old path re-exports it): what every published
//! string passes through and the relay's caps.
//!
//! * [`Redactor`] + [`secrets_from_worktree`] — every published string passes
//!   through exact-match masking of the launcher-created secrets (the JIT
//!   GitHub installation token from the clone's shared
//!   `.git/exp-git-credentials` credential file, EXP-73; the `expu_` personal
//!   key from `.exp-mcp.json`, or handed in by the wiring for codex, which
//!   keeps it env-only — REV2-17) plus gitleaks-style patterns;
//! * the relay's `activityEventSchema` caps, so the mapper truncates
//!   client-side before a frame is ever sent ([`clamp_config_state`]);
//! * [`worktree_diff`] — the bounded `git diff` snapshot the `diff` event
//!   carries;
//! * [`SessionAgent`], the blocked-wall vocabulary, the title and compaction
//!   normalisers and the truncation helpers.
//!
//! What stays in `steer::activity`: the publisher↔engine links, the turn and
//! caption signals, the forwarders that write through `api`, and
//! `DiffSnapshots` (it can send on the publisher's sender).

use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use super::frames::ActivityEvent;

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

/// How much of a `git diff` the ticker READS before it stops git: one byte
/// past [`DIFF_MAX`], so [`truncate`] still sees "over" without the whole
/// patch ever being in memory. Each live run polls its diff every 3 s and
/// redacts it (eight regex passes, each a full copy) before truncating; an
/// unbounded read of a changing multi-hundred-MB diff (vendored deps in a
/// scratch repo, generated assets) pegged a core per run and churned
/// gigabytes.
pub const DIFF_READ_MAX: usize = DIFF_MAX + 1;
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
/// `.exp-mcp.json` (claude sessions only — codex keeps the key env-only,
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
    let output = crate::process::background_command("git")
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
    let output = crate::process::background_command("git")
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
            return git_out_bounded(worktree, &["diff", &merge_base], DIFF_READ_MAX);
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
        git_out_bounded(worktree, &["diff", "--cached"], DIFF_READ_MAX)
    } else {
        git_out_bounded(worktree, &["diff"], DIFF_READ_MAX)
    }
}

/// [`git_out`] that stops reading at `max_bytes` and kills git there: the
/// head of an oversize patch is all a frame can carry anyway. The cut is
/// reported as success (the caller truncates); a failure before the cap is
/// `None` like [`git_out`].
fn git_out_bounded(worktree: &Path, args: &[&str], max_bytes: usize) -> Option<String> {
    use std::io::Read as _;
    use std::process::Stdio;
    let mut cmd = crate::process::background_command("git");
    cmd.arg("-C")
        .arg(worktree)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut cut = false;
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = max_bytes.saturating_sub(buf.len());
                if n >= room {
                    buf.extend_from_slice(&chunk[..room]);
                    cut = true;
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    drop(stdout);
    if cut {
        let _ = child.kill();
        let _ = child.wait();
        return Some(String::from_utf8_lossy(&buf).into_owned());
    }
    let status = child.wait().ok()?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&buf).into_owned())
}

/// Stdout of one git command; `None` on a spawn failure or non-zero exit.
fn git_out(worktree: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = crate::process::background_command("git");
    cmd.arg("-C").arg(worktree).args(args);
    match cmd.output() {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
    }
}

/// EXP-724: fold an agent's compaction reason onto the wire's two values.
/// The relay's schema accepts `manual` | `auto` ONLY (a future claude could
/// report anything else), and
/// an unknown trigger would sever the publisher socket.
pub fn normalize_compaction_trigger(trigger: Option<&str>) -> Option<&'static str> {
    match trigger {
        None => None,
        Some(trigger) if trigger.eq_ignore_ascii_case("manual") => Some("manual"),
        Some(_) => Some("auto"),
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
    /// EXP-746 (D13): a user-configured ACP agent binary. Deliberately
    /// NEUTRAL everywhere the builtins get per-agent treatment — its
    /// `steer::commands::catalog_for` is empty (the contract knows no such
    /// agent, so the `/` menu carries only what the agent itself advertises
    /// through `config_state.commands`), and the codex sigil guard stays
    /// codex-only. There is no PTY emitter for it: an external agent only
    /// ever runs on the ACP path.
    External,
}

impl SessionAgent {
    /// The contract `codingAgent` id, or the deliberately un-nameable
    /// `external` (see the variant's own note). The ONE mapping — the command
    /// catalog and the EXP-804 wall write must never disagree about what to
    /// call an agent.
    pub fn id(self) -> &'static str {
        match self {
            SessionAgent::Claude => "claude",
            SessionAgent::Codex => "codex",
            SessionAgent::External => "external",
        }
    }
}

/// EXP-804 contract vocabulary (`codingSessionBlocked`), named rather than
/// indexed out of the generated slices so a call site reads. The test below
/// is what keeps them inside the contract.
pub const BLOCKED_KIND_RATE_LIMIT: &str = "rate_limit";
/// The windows a wall names (FEED-34: `window` and `resetsAt` must describe
/// the SAME window — claude's `rateLimitType` picks it, see
/// `engine::adapters::claude_wire::RateLimitInfo::window`).
pub const BLOCKED_WINDOW_SESSION: &str = "session";
pub const BLOCKED_WINDOW_WEEKLY: &str = "weekly";
pub const BLOCKED_WINDOW_MODEL: &str = "model";

/// EXP-906: the caption a RESUMED run wears until its first prompt. A resume
/// re-enters the recorded run through `session/load` and spawns the agent
/// process only on the next message (EXP-784), so the transcript sits idle
/// in between. The engine sets this on the (local, EXP-1292) caption signal
/// right after the load and clears it on the first turn start.
pub const RESUMED_IDLE_CAPTION: &str = "Resumed, waiting for a message";

/// EXP-905: the agent title's bound (EXP-1292: `sessions.report{title}`). Counted in
/// UTF-16 units, the server's zod `max(255)` unit (never looser than the
/// column's own character count), so a capped title is never refused.
pub const AGENT_TITLE_MAX: usize = 255;

/// EXP-905: the ONE title normaliser — trim, collapse every whitespace run
/// (newlines included) to one space, cap at [`AGENT_TITLE_MAX`] UTF-16 units
/// on a char boundary. `None` for a title that is blank once normalised.
pub fn normalize_agent_title(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut units = 0;
    let capped = collapsed
        .chars()
        .take_while(|ch| {
            units += ch.len_utf16();
            units <= AGENT_TITLE_MAX
        })
        .collect::<String>();
    Some(capped.trim_end().to_string())
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
    use std::process::Command;

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

    /// The diff ticker never holds more than [`DIFF_READ_MAX`] bytes of a
    /// patch: an agent that commits megabytes of generated text must not
    /// cost a full read + eight redaction copies every 3 s.
    #[test]
    fn worktree_diff_reads_at_most_the_frame_budget() {
        let repo = DiffRepo::new("diff-bounded");
        let line = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        let big = line.repeat(DIFF_MAX / line.len() * 3);
        repo.write("big.txt", &big);
        repo.git(&["add", "-A"]);
        let staged = worktree_diff(&repo.0, None).expect("git answers");
        assert!(staged.len() <= DIFF_READ_MAX, "read {} bytes", staged.len());
        assert!(staged.starts_with("diff --git"));
        repo.commit("big");
        let committed = worktree_diff(&repo.0, Some("main")).expect("git answers");
        assert!(committed.len() <= DIFF_READ_MAX, "read {} bytes", committed.len());
        assert_eq!(truncate(&committed, DIFF_MAX).len(), DIFF_MAX);
        // A small diff is still read whole, byte for byte.
        repo.write("base.txt", "a\nlanded\n");
        assert!(worktree_diff(&repo.0, None).unwrap_or_default().contains("landed"));
    }

    /// EXP-746: an over-cap `config_state` must be TRUNCATED, never left to
    /// the relay — `activityEventSchema` is a discriminated union, so one
    /// oversize label drops the whole frame in silence and the viewer's chips
    /// never paint at all.
    #[test]
    fn config_state_over_cap_is_truncated_not_dropped() {
        use crate::wire::frames::{ConfigCommand, ConfigMode, ConfigOption, ConfigValue};

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
        use crate::wire::frames::{ConfigCommand, ConfigMode, ConfigOption, ConfigValue};

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

#[cfg(test)]
mod exp905_agent_title_tests {
    use super::*;
    #[test]
    fn the_title_normaliser_trims_collapses_and_caps_on_a_char_boundary() {
        assert_eq!(
            normalize_agent_title("  Fix the\n\tflaky   login test ").as_deref(),
            Some("Fix the flaky login test")
        );
        assert_eq!(normalize_agent_title(" \n\t "), None);
        assert_eq!(normalize_agent_title(""), None);
        // 300 multi-byte chars cap at 255, never mid-codepoint.
        let long = "\u{e9}".repeat(300);
        let capped = normalize_agent_title(&long).unwrap();
        assert_eq!(capped, "\u{e9}".repeat(AGENT_TITLE_MAX));
        // An astral char is TWO UTF-16 units (the server's zod unit): 200
        // emoji keep 127 whole ones (254 units), never half a pair.
        let emoji = "\u{1f680}".repeat(200);
        let capped = normalize_agent_title(&emoji).unwrap();
        assert_eq!(capped, "\u{1f680}".repeat(127));
        assert!(capped.encode_utf16().count() <= AGENT_TITLE_MAX);
    }
}
