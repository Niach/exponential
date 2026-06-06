//! Recover a `claude` CLI session id from its on-disk session log.
//!
//! Interactive runs launch `claude` in the host's embedded terminal WITHOUT
//! `--print`, so there's no machine-readable output to parse — the host can't
//! surface the session id and submits `None`. But claude persists every session
//! to `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl`, where the cwd is
//! encoded by replacing every `/` and `.` with `-` (e.g. `/tmp` → `-tmp`,
//! `/home/u/.exp/wt/FB-1` → `-home-u--exp-wt-FB-1`). After an interactive run we
//! read the newest session file for the run's worktree to recover the id, so a
//! later `--continue` (approve-and-continue) resumes the same session.
//!
//! Keeping this in the shared core means every host driver stays a thin
//! terminal-launcher — none of them re-implement session recovery.

use std::path::Path;
use std::time::SystemTime;

/// Encode a cwd into claude's `projects/` subdirectory name: `/` and `.` → `-`.
pub fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// The id of the most-recently-modified claude session for `cwd`, or `None` if
/// there's no `~/.claude/projects/<encoded-cwd>/` dir or no session files in it.
pub fn find_latest_session_id(cwd: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    find_latest_session_id_in(&format!("{home}/.claude/projects"), cwd)
}

fn find_latest_session_id_in(projects_root: &str, cwd: &str) -> Option<String> {
    let dir = Path::new(projects_root).join(encode_cwd(cwd));
    let mut best: Option<(SystemTime, String)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
            best = Some((mtime, stem.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_slashes_and_dots() {
        assert_eq!(encode_cwd("/tmp"), "-tmp");
        assert_eq!(
            encode_cwd("/home/u/.exp/wt/FB-1"),
            "-home-u--exp-wt-FB-1"
        );
    }

    #[test]
    fn finds_newest_session_file() {
        let root = std::env::temp_dir().join(format!(
            "exp-sess-{}-{}",
            std::process::id(),
            crate::agent_run::new_run_id()
        ));
        let cwd = "/work/.wt/FB-9";
        let dir = root.join(encode_cwd(cwd));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old-session-id.jsonl"), "x").unwrap();
        // Ensure a distinct, later mtime for the newer file.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("new-session-id.jsonl"), "y").unwrap();
        // A non-session file must be ignored.
        std::fs::write(dir.join("notes.txt"), "z").unwrap();

        let id = find_latest_session_id_in(root.to_str().unwrap(), cwd);
        assert_eq!(id.as_deref(), Some("new-session-id"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_dir_is_none() {
        let id = find_latest_session_id_in("/no/such/root", "/whatever");
        assert_eq!(id, None);
    }
}
