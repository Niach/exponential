//! The hub as the MCP CLIENT of team servers (#NEW-148,
//! `packages/emcp/fixtures/hub-client.json`): the capabilities every
//! upstream request declares, MRTR's answers, the task poller's interval
//! and TraceItem, the call's copy, and the skills rules (verify, digest
//! set, the skill dir name and its rewritten SKILL.md). Pure:
//! `tests/fixtures.rs` replays every case, like `src/hub-client.ts`.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The ONE MCP entry a run's CLI gets for every team server (`entry`).
pub const ENTRY_NAME: &str = "team";
/// The env var carrying the run's hub bearer token (`entry.tokenEnv`).
pub const TOKEN_ENV: &str = "EXP_TEAM_MCP_TOKEN";
/// elicitation-card.json's `serverMetaKey` (domain-contract): the `_meta`
/// key naming the asking server, so the ACP elicitation becomes THE card.
/// eMCP cannot depend on eAgent, so this mirrors eagent's ONE definition
/// (`engine::elicitation_card::SERVER_META_KEY`); both are gated against the
/// fixture.
pub const SERVER_META_KEY: &str = "exponential.at/mcpServer";

/// `mrtr.maxRounds`: input rounds one call may take.
pub const MAX_ROUNDS: usize = 10;
/// `mrtr.askWaitMs`: how long ONE question may wait for the person (the
/// CLI bounds every MCP call at 120 s, the hub answers first).
pub const ASK_WAIT_MS: u64 = 100_000;

/// `tasks.storeFile`, under the device's data dir.
pub const TASK_STORE_FILE: &str = "mcp-tasks.json";
/// `tasks.callWaitMs`: how long the CLI's call waits for a task.
pub const CALL_WAIT_MS: u64 = 100_000;
pub const POLL_DEFAULT_MS: u64 = 2000;
pub const POLL_MIN_MS: u64 = 500;
pub const POLL_MAX_MS: u64 = 60_000;
/// `tasks.recordKeys`: one persisted task record's keys.
pub const RECORD_KEYS: [&str; 7] = ["sessionId", "serverId", "server", "taskId", "tool", "createdAt", "pollIntervalMs"];
pub const TRACE_ID_PREFIX: &str = "mcp-task:";

/// `skills.cacheDir`, under the device's data dir.
pub const SKILLS_CACHE_DIR: &str = "mcp-skills-cache";
pub const SKILL_NAME_MAX: usize = 64;
pub const SKILL_DIR_PREFIX: &str = "mcp-";
pub const SKILL_CUT_AT: usize = 55;
pub const SKILL_HASH_HEX: usize = 8;
/// skills.json `limits`: per skill.
pub const SKILL_MAX_FILES: usize = 512;
pub const SKILL_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// `clientCapabilities`: elicitation form + url, the tasks and skills
/// extensions; never sampling or roots.
pub fn client_capabilities() -> Value {
    json!({
        "elicitation": { "form": {}, "url": {} },
        "extensions": { "io.modelcontextprotocol/tasks": {}, "io.modelcontextprotocol/skills": {} },
    })
}

/// `mrtr.roots`: the answer to a `roots/list` input request.
pub fn roots() -> Value {
    json!({ "roots": [] })
}

/// `toolNameCases`: what claude calls a team tool on the `team` entry.
pub fn claude_tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{ENTRY_NAME}__{server}__{tool}")
}

/// `toolNameCases`: codex's (server, tool) of a team tool.
pub fn codex_tool_name(server: &str, tool: &str) -> (String, String) {
    (ENTRY_NAME.to_string(), format!("{server}__{tool}"))
}

/// `pollCases`: the task's `pollIntervalMs` clamped to [min, max], the
/// default when absent.
pub fn clamp_poll(poll_interval_ms: Option<u64>) -> u64 {
    poll_interval_ms.map_or(POLL_DEFAULT_MS, |ms| ms.clamp(POLL_MIN_MS, POLL_MAX_MS))
}

/// A task's status (SEP-2663).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn parse(text: &str) -> Option<TaskStatus> {
        Some(match text {
            "working" => TaskStatus::Working,
            "input_required" => TaskStatus::InputRequired,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "cancelled" => TaskStatus::Cancelled,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Working => "working",
            TaskStatus::InputRequired => "input_required",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
    }

    /// `traceStatus`: the TraceItem status.
    pub fn trace_status(self) -> &'static str {
        match self {
            TaskStatus::Working => "running",
            TaskStatus::InputRequired => "pending",
            TaskStatus::Completed => "done",
            TaskStatus::Failed => "error",
            TaskStatus::Cancelled => "skipped",
        }
    }
}

/// ONE TraceItem of a task in the run's session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskTrace {
    /// `mcp-task:<server>:<taskId>`.
    pub id: String,
    /// `<server>: <statusMessage>`, or `<server>: <tool>` without one.
    pub title: String,
    /// `traceStatus` of the task's status.
    pub status: String,
}

/// `traceCases`: a blank status message counts as none.
pub fn trace(server: &str, task_id: &str, tool: &str, status_message: Option<&str>, status: TaskStatus) -> TaskTrace {
    let label = status_message.map(str::trim).filter(|text| !text.is_empty()).unwrap_or(tool);
    TaskTrace {
        id: format!("{TRACE_ID_PREFIX}{server}:{task_id}"),
        title: format!("{server}: {label}"),
        status: status.trace_status().to_string(),
    }
}

/// The person's action on an elicitation card.
#[derive(Clone, Debug, PartialEq)]
pub enum ElicitAction {
    /// Accept; `content` for a form, none for a url.
    Accept(Option<Map<String, Value>>),
    Decline,
    Cancel,
}

/// `elicitResultCases`: the ElicitResult sent back; `None` = no live
/// session to ask (`copy.noSession`) = cancel.
pub fn elicit_result(action: Option<&ElicitAction>) -> Value {
    match action {
        Some(ElicitAction::Accept(Some(content))) => json!({ "action": "accept", "content": content }),
        Some(ElicitAction::Accept(None)) => json!({ "action": "accept" }),
        Some(ElicitAction::Decline) => json!({ "action": "decline" }),
        Some(ElicitAction::Cancel) | None => json!({ "action": "cancel" }),
    }
}

/// A host's ElicitResult JSON made well-formed: an unknown or missing
/// action = cancel, `content` only on an accept (an object), other keys
/// dropped.
pub fn normalize_elicit_result(answer: &Value) -> Value {
    let action = match answer.get("action").and_then(Value::as_str) {
        Some("accept") => ElicitAction::Accept(answer.get("content").and_then(Value::as_object).cloned()),
        Some("decline") => ElicitAction::Decline,
        _ => ElicitAction::Cancel,
    };
    elicit_result(Some(&action))
}

/// The `copy` strings, `{placeholders}` filled.
pub mod copy {
    pub fn too_many_rounds(server: &str, rounds: usize) -> String {
        format!("{server} kept asking for input; the call was stopped after {rounds} rounds.")
    }

    pub fn sampling(server: &str) -> String {
        format!("{server} asked this device to run a model (sampling), which it does not offer.")
    }

    pub fn cancelled(server: &str) -> String {
        format!("{server} cancelled the task.")
    }

    pub fn still_running(server: &str, task_id: &str, status: &str) -> String {
        format!(
            "{server} is still working on this (task {task_id}: {status}). It keeps running and shows in the session; its result does not come back to this call."
        )
    }

    pub fn sign_in(server: &str) -> String {
        format!("{server}: the sign-in expired and could not be refreshed. Reconnect it in Settings \u{203a} MCP servers.")
    }

    pub fn no_answer(server: &str) -> String {
        format!("{server} asked a question in the session that was not answered in time; once the person answered, call the tool again.")
    }
}

/// One manifest row of a skill (`resources`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SkillEntry {
    pub uri: String,
    pub digest: String,
    pub size: u64,
}

/// What a fetched file failed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    Size,
    Digest,
}

impl Mismatch {
    pub fn as_str(self) -> &'static str {
        match self {
            Mismatch::Size => "size",
            Mismatch::Digest => "digest",
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `sha256:` + the lowercase hex SHA-256 of `bytes`.
pub fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// `verifyCases`: the size first, then the digest.
pub fn verify(entry: &SkillEntry, bytes: &[u8]) -> Result<(), Mismatch> {
    if bytes.len() as u64 != entry.size {
        return Err(Mismatch::Size);
    }
    if sha256_digest(bytes) != entry.digest.to_ascii_lowercase() {
        return Err(Mismatch::Digest);
    }
    Ok(())
}

/// `digestSetCases`: `sha256:` + hex SHA-256 of the canonical JSON (keys
/// sorted, no whitespace, UTF-8) of the rows {digest, size, uri} sorted by
/// uri (code point). `resources` = (uri, digest, size).
pub fn digest_set(resources: &[(&str, &str, u64)]) -> String {
    let mut rows: Vec<&(&str, &str, u64)> = resources.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let text = format!(
        "[{}]",
        rows.iter()
            .map(|(uri, digest, size)| format!(
                "{{\"digest\":{},\"size\":{size},\"uri\":{}}}",
                Value::from(*digest),
                Value::from(*uri)
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    sha256_digest(text.as_bytes())
}

/// [`digest_set`] of manifest rows.
pub fn digest_set_of(entries: &[SkillEntry]) -> String {
    let rows: Vec<(&str, &str, u64)> = entries.iter().map(|entry| (entry.uri.as_str(), entry.digest.as_str(), entry.size)).collect();
    digest_set(&rows)
}

/// `dirNameCases`: `mcp-<server>-<name>`; past [`SKILL_NAME_MAX`] cut to
/// [`SKILL_CUT_AT`], trailing `-` trimmed, `-` + the first 8 hex of the
/// SHA-256 of the full name.
pub fn dir_name(server: &str, name: &str) -> String {
    let full = format!("{SKILL_DIR_PREFIX}{server}-{name}");
    if full.chars().count() <= SKILL_NAME_MAX {
        return full;
    }
    let cut: String = full.chars().take(SKILL_CUT_AT).collect();
    format!("{}-{}", cut.trim_end_matches('-'), &sha256_hex(full.as_bytes())[..SKILL_HASH_HEX])
}

/// `skillMdCases`: SKILL.md with its frontmatter REWRITTEN to exactly
/// {name: `dir_name`, description} (a double-quoted YAML description is
/// read as JSON, a plain one trimmed; every other key, `allowed-tools`
/// first of all, ignored) and the body trimmed + `\n`. No frontmatter (or
/// no description) = `None`: not a skill.
pub fn skill_md(dir_name: &str, bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?.replace("\r\n", "\n");
    let rest = text.strip_prefix("---\n")?;
    let (front, body) = match rest.find("\n---\n") {
        Some(at) => (&rest[..at], &rest[at + 5..]),
        None => (rest.strip_suffix("\n---")?, ""),
    };
    let description = front.lines().find_map(|line| {
        let value = line.strip_prefix("description:")?.trim();
        Some(match value.starts_with('"') {
            true => serde_json::from_str::<String>(value).ok()?,
            false => value.to_string(),
        })
    })?;
    Some(format!("---\nname: {dir_name}\ndescription: {}\n---\n\n{}\n", Value::from(description), body.trim()))
}

/// A skill file's path relative to its skill dir is safe to write: not
/// empty, not absolute, no `..` / `.` / empty segment, no backslash or NUL.
pub fn relative_path_ok(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod elicitation_meta_key {
    /// #NEW-146: the key is THE fixture's (eagent gates its copy the same way).
    #[test]
    fn server_meta_key_is_the_elicitation_card_fixtures() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/elicitation-card.json"
        ))
        .unwrap();
        assert_eq!(fixture["serverMetaKey"], super::SERVER_META_KEY);
    }
}
