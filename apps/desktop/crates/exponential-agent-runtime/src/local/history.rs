//! The CLIs' own session history (EXP-1426 part 5): every past conversation
//! of `claude` / `codex` on this machine, under the name the CLI itself gave
//! it, newest first. Read-only: the files are the CLIs', never written here.
//!
//! - claude: `<CLAUDE_CONFIG_DIR|~/.claude>/projects/*/<session id>.jsonl`.
//!   The title is the latest `custom-title` (a `/rename`, the user's word),
//!   else the latest `ai-title`, else a `summary` line, else the first prompt.
//! - codex: `<CODEX_HOME|~/.codex>/sessions/YYYY/MM/DD/rollout-*.jsonl`
//!   (the first line is the session meta, [`crate::codex_sessions`]), named by
//!   `session_index.jsonl`'s `thread_name` (latest line per id), else the
//!   first `user_message`. Subagent threads are left out.
//!
//! `updated` is the file's mtime: both CLIs append to the file on every turn.
//! A conversation with no name and no prompt (nothing to resume) is skipped.
//! Every title goes through [`crate::wire::normalize_agent_title`].

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::agent::CodingAgent;

/// One past conversation, as a Chats list shows it and a resume reopens it
/// (`RunRequest::resume` takes [`Self::id`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastSession {
    pub agent: CodingAgent,
    /// claude's transcript uuid, codex's thread id: the native resume handle.
    pub id: String,
    pub title: String,
    /// The directory the conversation ran in, as the CLI recorded it.
    pub cwd: Option<PathBuf>,
    pub updated: SystemTime,
}

/// `agent`'s past conversations in its DEFAULT home, newest first; `cwd`
/// keeps those recorded for that directory, `limit` caps the count.
pub fn list_sessions(agent: CodingAgent, cwd: Option<&Path>, limit: Option<usize>) -> Vec<PastSession> {
    match agent {
        CodingAgent::Claude => default_claude_projects_root()
            .map(|root| claude_sessions(&root, cwd, limit))
            .unwrap_or_default(),
        CodingAgent::Codex => default_codex_home()
            .map(|home| codex_sessions(&home, cwd, limit))
            .unwrap_or_default(),
    }
}

/// `$CLAUDE_CONFIG_DIR|~/.claude` + `projects`, as the CLI resolves it.
pub fn default_claude_projects_root() -> Option<PathBuf> {
    let home = match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs::home_dir()?.join(".claude"),
    };
    Some(home.join("projects"))
}

/// `$CODEX_HOME|~/.codex`, as the CLI resolves it.
pub fn default_codex_home() -> Option<PathBuf> {
    crate::codex_sessions::default_codex_sessions_root()
        .and_then(|sessions| sessions.parent().map(Path::to_path_buf))
}

/// claude's conversations under `projects_root` (see the module doc).
pub fn claude_sessions(projects_root: &Path, cwd: Option<&Path>, limit: Option<usize>) -> Vec<PastSession> {
    let mut files = Vec::new();
    for project in read_dir(projects_root) {
        if !project.is_dir() {
            continue;
        }
        for path in read_dir(&project) {
            if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }
    collect(newest_first(files), limit, |path| {
        let id = path.file_stem()?.to_str()?.to_string();
        let head = claude_head(path)?;
        if !cwd_matches(head.cwd.as_deref(), cwd) {
            return None;
        }
        Some(PastSession {
            agent: CodingAgent::Claude,
            id,
            title: head.title?,
            cwd: head.cwd.map(PathBuf::from),
            updated: modified(path),
        })
    })
}

/// codex's conversations under `codex_home` (see the module doc).
pub fn codex_sessions(codex_home: &Path, cwd: Option<&Path>, limit: Option<usize>) -> Vec<PastSession> {
    let names = codex_thread_names(&codex_home.join("session_index.jsonl"));
    let mut rollouts = Vec::new();
    crate::codex_sessions::collect_rollouts(&codex_home.join("sessions"), 0, &mut rollouts);
    let files = rollouts.into_iter().map(|(_, path)| path).collect();
    collect(newest_first(files), limit, |path| {
        let meta = crate::codex_sessions::read_session_meta(path)?;
        if meta.subagent || !cwd_matches(Some(&meta.cwd), cwd) {
            return None;
        }
        let title = names
            .get(&meta.id)
            .cloned()
            .or_else(|| codex_first_prompt(path))?;
        Some(PastSession {
            agent: CodingAgent::Codex,
            id: meta.id,
            title,
            cwd: Some(PathBuf::from(meta.cwd)),
            updated: modified(path),
        })
    })
}

/// What a claude transcript says about itself.
#[derive(Default)]
struct ClaudeHead {
    cwd: Option<String>,
    title: Option<String>,
}

fn claude_head(path: &Path) -> Option<ClaudeHead> {
    let mut cwd = None;
    let (mut custom, mut ai, mut summary, mut prompt) = (None, None, None, None);
    for_lines(path, |line| {
        let has = |needle: &str| contains(line, needle.as_bytes());
        // Cheap byte pre-filter: almost every line is a message the head has
        // no use for once the cwd and the first prompt are known.
        let wanted = has("\"custom-title\"")
            || has("\"ai-title\"")
            || has("\"summary\"")
            || ((cwd.is_none() || prompt.is_none()) && has("\"cwd\""));
        if !wanted {
            return;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else { return };
        let text = |key: &str| value.get(key).and_then(Value::as_str).and_then(crate::wire::normalize_agent_title);
        match value.get("type").and_then(Value::as_str) {
            Some("custom-title") => custom = text("customTitle").or(custom.take()),
            Some("ai-title") => ai = text("aiTitle").or(ai.take()),
            Some("summary") => summary = text("summary").or(summary.take()),
            kind => {
                if cwd.is_none() {
                    cwd = value.get("cwd").and_then(Value::as_str).map(str::to_string);
                }
                if prompt.is_none() && kind == Some("user") {
                    prompt = claude_prompt(&value);
                }
            }
        }
    })?;
    Some(ClaudeHead { cwd, title: custom.or(ai).or(summary).or(prompt) })
}

/// A person's prompt in a claude `user` line: not a meta entry, not a
/// subagent's, not a tool result, not a slash command's tag soup.
fn claude_prompt(value: &Value) -> Option<String> {
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("isMeta") || flag("isSidechain") || flag("isCompactSummary") {
        return None;
    }
    let content = value.get("message")?.get("content")?;
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))?
            .get("text")?
            .as_str()?
            .to_string(),
        _ => return None,
    };
    if text.trim_start().starts_with('<') {
        return None;
    }
    crate::wire::normalize_agent_title(&text)
}

/// `session_index.jsonl`: id → the latest `thread_name` (append-only, so a
/// later line is a rename).
fn codex_thread_names(index: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let _ = for_lines(index, |line| {
        let Ok(value) = serde_json::from_slice::<Value>(line) else { return };
        let id = value.get("id").and_then(Value::as_str);
        let name = value
            .get("thread_name")
            .and_then(Value::as_str)
            .and_then(crate::wire::normalize_agent_title);
        if let (Some(id), Some(name)) = (id, name) {
            names.insert(id.to_string(), name);
        }
    });
    names
}

/// The first `user_message` event of a rollout.
fn codex_first_prompt(path: &Path) -> Option<String> {
    let mut prompt = None;
    for_lines(path, |line| {
        if prompt.is_some() || !contains(line, b"\"user_message\"") {
            return;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else { return };
        let payload = value.get("payload").unwrap_or(&value);
        if payload.get("type").and_then(Value::as_str) == Some("user_message") {
            prompt = payload
                .get("message")
                .and_then(Value::as_str)
                .and_then(crate::wire::normalize_agent_title);
        }
    })?;
    prompt
}

/// The recorded cwd matches the wanted one, raw or canonical (macOS
/// `/tmp` → `/private/tmp`). No filter matches everything.
fn cwd_matches(recorded: Option<&str>, wanted: Option<&Path>) -> bool {
    let Some(wanted) = wanted else { return true };
    let Some(recorded) = recorded.map(Path::new) else { return false };
    recorded == wanted || std::fs::canonicalize(wanted).is_ok_and(|canonical| recorded == canonical)
}

fn collect(
    files: Vec<PathBuf>,
    limit: Option<usize>,
    mut row: impl FnMut(&Path) -> Option<PastSession>,
) -> Vec<PastSession> {
    let limit = limit.unwrap_or(usize::MAX);
    files.iter().filter_map(|path| row(path)).take(limit).collect()
}

fn newest_first(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut stamped: Vec<(SystemTime, PathBuf)> =
        files.into_iter().map(|path| (modified(&path), path)).collect();
    stamped.sort_by(|left, right| right.0.cmp(&left.0));
    stamped.into_iter().map(|(_, path)| path).collect()
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn read_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default()
}

/// Every complete line of `path` (a line still being written is skipped).
fn for_lines(path: &Path, mut each: impl FnMut(&[u8])) -> Option<()> {
    let mut reader = BufReader::new(std::fs::File::open(path).ok()?);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return Some(()),
            Ok(_) if line.last() == Some(&b'\n') => each(&line),
            Ok(_) => return Some(()),
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-local-history-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_lines(path: &Path, lines: &[Value], age: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(path, text).unwrap();
        let stamp = SystemTime::now() - Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
    }

    fn user(cwd: &str, content: Value) -> Value {
        json!({ "type": "user", "cwd": cwd, "sessionId": "s", "message": { "role": "user", "content": content } })
    }

    #[test]
    fn claude_titles_rank_custom_then_ai_then_summary_then_the_first_prompt() {
        let root = temp_dir("claude");
        let project = root.join("-work-app");
        write_lines(
            &project.join("renamed.jsonl"),
            &[
                user("/work/app", json!("fix the login")),
                json!({ "type": "ai-title", "aiTitle": "Login fix" }),
                json!({ "type": "custom-title", "customTitle": "Mine" }),
                json!({ "type": "ai-title", "aiTitle": "Later guess" }),
            ],
            30,
        );
        write_lines(
            &project.join("named.jsonl"),
            &[
                json!({ "type": "summary", "summary": "Old summary" }),
                user("/work/app", json!("hello")),
                json!({ "type": "ai-title", "aiTitle": "First" }),
                json!({ "type": "ai-title", "aiTitle": "  Second\n name " }),
            ],
            20,
        );
        write_lines(
            &project.join("summarized.jsonl"),
            &[json!({ "type": "summary", "summary": "From summary" }), user("/work/app", json!("hi"))],
            10,
        );
        // The prompt fallback skips meta entries and command tags.
        write_lines(
            &root.join("-work-other").join("prompted.jsonl"),
            &[
                json!({ "type": "user", "cwd": "/work/other", "isMeta": true, "message": { "content": "caveat" } }),
                user("/work/other", json!("<command-name>/model</command-name>")),
                user("/work/other", json!([{ "type": "text", "text": "reply with the word ok" }])),
            ],
            5,
        );
        // Nothing to resume: no name, no prompt.
        write_lines(&project.join("empty.jsonl"), &[json!({ "type": "mode", "mode": "x" })], 1);

        let all = claude_sessions(&root, None, None);
        let rows: Vec<(&str, &str)> = all.iter().map(|row| (row.id.as_str(), row.title.as_str())).collect();
        assert_eq!(
            rows,
            [
                ("prompted", "reply with the word ok"),
                ("summarized", "From summary"),
                ("named", "Second name"),
                ("renamed", "Mine"),
            ]
        );
        assert_eq!(all[0].cwd.as_deref(), Some(Path::new("/work/other")));
        assert!(all.iter().all(|row| row.agent == CodingAgent::Claude));

        let here = claude_sessions(&root, Some(Path::new("/work/app")), Some(2));
        let ids: Vec<&str> = here.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["summarized", "named"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn meta(id: &str, cwd: &str, source: Value) -> Value {
        json!({ "type": "session_meta", "payload": { "id": id, "cwd": cwd, "source": source } })
    }

    fn user_message(text: &str) -> Value {
        json!({ "type": "event_msg", "payload": { "type": "user_message", "message": text } })
    }

    #[test]
    fn codex_threads_take_the_index_name_else_the_first_prompt() {
        let home = temp_dir("codex");
        let day = home.join("sessions/2026/10/29");
        write_lines(
            &day.join("rollout-2026-10-29T09-00-00-t-old.jsonl"),
            &[meta("t-old", "/work/app", json!("cli")), user_message("first ask")],
            30,
        );
        write_lines(
            &day.join("rollout-2026-10-29T10-00-00-t-named.jsonl"),
            &[meta("t-named", "/work/app", json!("cli")), user_message("ignored")],
            20,
        );
        write_lines(
            &day.join("rollout-2026-10-29T11-00-00-t-sub.jsonl"),
            &[meta("t-sub", "/work/app", json!({ "subagent": "review" })), user_message("sub")],
            10,
        );
        write_lines(
            &day.join("rollout-2026-10-29T12-00-00-t-else.jsonl"),
            &[meta("t-else", "/work/other", json!("cli")), user_message("elsewhere")],
            5,
        );
        write_lines(
            &home.join("session_index.jsonl"),
            &[
                json!({ "id": "t-named", "thread_name": "Draft", "updated_at": "2026-10-29T10:00:00Z" }),
                json!({ "id": "t-named", "thread_name": "Refactor the pump", "updated_at": "2026-10-29T10:05:00Z" }),
            ],
            0,
        );

        let rows: Vec<(String, String)> = codex_sessions(&home, None, None)
            .into_iter()
            .map(|row| (row.id, row.title))
            .collect();
        assert_eq!(
            rows,
            [
                ("t-else".to_string(), "elsewhere".to_string()),
                ("t-named".to_string(), "Refactor the pump".to_string()),
                ("t-old".to_string(), "first ask".to_string()),
            ]
        );
        let here = codex_sessions(&home, Some(Path::new("/work/app")), Some(1));
        assert_eq!(here.len(), 1);
        assert_eq!(here[0].id, "t-named");
        assert_eq!(here[0].cwd.as_deref(), Some(Path::new("/work/app")));
        assert_eq!(here[0].agent, CodingAgent::Codex);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_missing_home_lists_nothing() {
        let dir = temp_dir("missing");
        assert!(claude_sessions(&dir.join("nope"), None, None).is_empty());
        assert!(codex_sessions(&dir.join("nope"), None, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
