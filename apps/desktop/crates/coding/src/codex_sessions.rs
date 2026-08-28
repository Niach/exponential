//! Codex session-id recovery for native resume (EXP-202).
//!
//! Codex has no cwd-scoped `--continue`: `codex resume --last` is the most
//! recent session GLOBALLY, so blindly continuing could resume an unrelated
//! conversation. But codex records every session as a rollout file under
//! `$CODEX_HOME|~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl`
//! whose FIRST line is the session meta (verified against codex-cli 0.144.5):
//!
//! ```json
//! {"timestamp":"…","type":"session_meta","payload":{"id":"<uuid>","cwd":"/abs/path",…}}
//! ```
//!
//! …and `codex resume <SESSION_ID>` reopens that exact session (accepting the
//! same `-m`/`-c`/sandbox/approval flags as a fresh spawn). So THE session a
//! recorded run left behind is recoverable: newest rollout whose meta `cwd`
//! equals the workspace and whose `originator` is the run's own stamp. No
//! recorded session (workspace previously coded by a different agent,
//! rollouts pruned) → the launcher falls back to a fresh session seeded with
//! the resume prompt.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// `$CODEX_HOME|~/.codex` + `sessions` — where codex records its rollouts.
/// `None` when no home directory resolves (headless oddity); the caller
/// falls back to the resume prompt.
pub fn default_codex_sessions_root() -> Option<PathBuf> {
    let home = match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs::home_dir()?.join(".codex"),
    };
    Some(home.join("sessions"))
}

/// The NEWEST recorded codex session whose meta `cwd` is `worktree`, and
/// (EXP-637) whose meta `originator` is `originator` when one is given — the
/// per-session value the launcher stamps into codex's rollout metas
/// ([`crate::argv::CODEX_ORIGINATOR_ENV`]). A worktree can host several codex
/// conversations (the run itself, then a resume, then a shell), so a resume
/// reopens the one that BELONGS to the recorded run; `None` originator is the
/// cwd-only match.
///
/// Filenames embed the full ISO timestamp, so a descending sort over
/// basenames is newest-first regardless of the year/month/day directory
/// fan-out; the scan early-exits on the first match and skips unreadable or
/// malformed files (never an error — resume degrades, it doesn't block).
pub fn find_codex_session_id(
    sessions_root: &Path,
    worktree: &Path,
    originator: Option<&str>,
) -> Option<String> {
    let mut rollouts: Vec<(String, PathBuf)> = Vec::new();
    collect_rollouts(sessions_root, 0, &mut rollouts);
    rollouts.sort_by(|a, b| b.0.cmp(&a.0));
    // The recorded cwd is codex's canonicalized spawn cwd — match either the
    // raw worktree path or its canonical form (macOS `/tmp` → `/private/tmp`).
    let canonical = std::fs::canonicalize(worktree).ok();
    for (_, path) in rollouts {
        let Some(meta) = read_session_meta(&path) else {
            continue;
        };
        let cwd = Path::new(&meta.cwd);
        if cwd != worktree && Some(cwd) != canonical.as_deref() {
            continue;
        }
        if let Some(wanted) = originator {
            if meta.originator.as_deref() != Some(wanted) {
                continue;
            }
        }
        return Some(meta.id);
    }
    None
}

/// One rollout's first-line session meta.
struct SessionMeta {
    cwd: String,
    id: String,
    /// The `CODEX_INTERNAL_ORIGINATOR_OVERRIDE` this spawn carried, when the
    /// build records it. `None` on builds that don't.
    originator: Option<String>,
}

/// Recursive `rollout-*.jsonl` sweep (bounded depth — the layout is
/// `YYYY/MM/DD/…`, anything deeper is not codex's).
fn collect_rollouts(dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, depth + 1, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("rollout-") && name.ends_with(".jsonl") {
            out.push((name.to_string(), path));
        }
    }
}

/// First-line session meta → `(cwd, session id)`. Lenient by design: the
/// payload nesting is current codex; a flat legacy shape still resolves.
fn read_session_meta(path: &Path) -> Option<SessionMeta> {
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let meta = value.get("payload").unwrap_or(&value);
    let cwd = meta.get("cwd")?.as_str()?.to_string();
    let id = meta
        .get("id")
        .or_else(|| meta.get("session_id"))?
        .as_str()?
        .to_string();
    let originator = meta
        .get("originator")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    Some(SessionMeta { cwd, id, originator })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-codex-sessions-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_rollout(root: &Path, day: &str, stamp: &str, id: &str, cwd: &Path) {
        let dir = root.join(day);
        std::fs::create_dir_all(&dir).unwrap();
        let meta = serde_json::json!({
            "timestamp": stamp,
            "type": "session_meta",
            "payload": { "id": id, "cwd": cwd.to_string_lossy(), "originator": "codex-tui" },
        });
        std::fs::write(
            dir.join(format!("rollout-{stamp}-{id}.jsonl")),
            format!("{meta}\n{{\"type\":\"other\"}}\n"),
        )
        .unwrap();
    }

    #[test]
    fn newest_matching_cwd_wins_and_other_cwds_are_skipped() {
        let dir = temp_dir("match");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let elsewhere = dir.join("other");
        write_rollout(&root, "2026/07/19", "2026-07-19T10-00-00", "old-match", &worktree);
        write_rollout(&root, "2026/07/20", "2026-07-20T09-00-00", "wrong-cwd", &elsewhere);
        write_rollout(&root, "2026/07/20", "2026-07-20T08-00-00", "new-match", &worktree);
        assert_eq!(
            find_codex_session_id(&root, &worktree, None),
            Some("new-match".to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn canonicalized_worktree_still_matches_the_recorded_cwd() {
        // codex records its CANONICAL cwd; the launcher's derived worktree
        // path may be the uncanonicalized spelling (macOS /tmp symlink).
        let dir = temp_dir("canon");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let canonical = std::fs::canonicalize(&worktree).unwrap();
        write_rollout(&root, "2026/07/20", "2026-07-20T10-00-00", "canon-id", &canonical);
        assert_eq!(
            find_codex_session_id(&root, &worktree, None),
            Some("canon-id".to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_and_absent_metas_degrade_to_none() {
        let dir = temp_dir("degrade");
        let root = dir.join("sessions");
        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let day = root.join("2026/07/20");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-2026-07-20T10-00-00-bad.jsonl"), "not json\n").unwrap();
        std::fs::write(day.join("unrelated.txt"), "ignored").unwrap();
        assert_eq!(find_codex_session_id(&root, &worktree, None), None);
        // Missing root entirely (fresh install) is a clean None too.
        assert_eq!(
            find_codex_session_id(&dir.join("nope"), &worktree, None),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
