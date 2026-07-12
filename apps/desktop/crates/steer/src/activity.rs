//! The PUBLIC live-coding activity emitter (masterplan §P7).
//!
//! When a coding session runs on a `feedback` project with
//! `publicShowCoding == 'live'` (and the user hasn't opted the session private),
//! the app publishes a **stripped, redacted** activity stream over the EXISTING
//! steer publisher socket — never the raw PTY. Three event kinds reach public
//! viewers (relay `activityEventSchema`):
//!
//! * **narration** — assistant prose (`text` content blocks in the Claude Code
//!   session transcript);
//! * **tool** — a tool-call headline: the tool name plus a single primary
//!   argument (a file path / pattern, or a Bash `description` — NEVER the raw
//!   command string, NEVER a tool result);
//! * **diff** — a debounced `git diff` snapshot of the worktree.
//!
//! Everything published passes through [`Redactor`] first: exact-match masking
//! of the launcher-created secrets (the JIT GitHub installation token embedded
//! in the worktree remote, the `expu_` personal key in `.mcp.json`) plus
//! gitleaks-style patterns. Steering input and tool results are never read.
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
const MCP_JSON_FILE: &str = ".mcp.json";

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
/// key written into `.mcp.json`. Both are launcher-created and long-lived only
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

/// Extract the `expu_` key from the worktree `.mcp.json`
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

/// Parse one Claude Code transcript JSONL line into public activity events.
/// Only `assistant` entries produce output — `text` blocks become narration,
/// `tool_use` blocks become tool headlines. `user` entries (which carry
/// steering input and tool RESULTS) are skipped entirely. Every string is
/// redacted and truncated to the relay caps.
pub fn parse_transcript_line(line: &str, redactor: &Redactor) -> Vec<ActivityEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    // Only assistant turns are public; skip user/system/summary/etc. entries.
    if entry.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
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

/// What the emitter needs to run: the worktree to tail/diff.
pub struct EmitterConfig {
    pub worktree: PathBuf,
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

    while active.load(Ordering::SeqCst) {
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
            offset = tail_transcript(&path, offset, &redactor, &sender);
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
/// partial line is left for the next poll).
fn tail_transcript(
    path: &Path,
    offset: u64,
    redactor: &Redactor,
    sender: &ActivitySender,
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
