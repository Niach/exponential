//! Which agents have coded in a worktree (EXP-210).
//!
//! Resume is per-AGENT: claude/pi `--continue` is cwd-scoped and errors with
//! "no conversation found to continue" when THAT agent never ran in the
//! worktree (a codex-built worktree, say). The launcher records every agent
//! it spawns into a worktree in a tiny marker file so (a) the Start-coding
//! dialog only offers "Resume previous session" for agents that actually
//! have a conversation there, and (b) `prepare` degrades a mismatched resume
//! request to a fresh session seeded with the resume prompt instead of
//! letting the CLI fail.
//!
//! A worktree WITHOUT the marker is a pre-EXP-210 one — its agent history is
//! unknown, so readers return `None` and callers keep the legacy behavior
//! (any agent may try to resume) rather than dropping resume for every
//! in-flight worktree on update. The file lives in the worktree root (it
//! dies with the worktree) and is kept out of `git add -A` via the shared
//! `.git/info/exclude` alongside `.exp-mcp.json`.

use std::collections::HashSet;
use std::path::Path;

use crate::agent::CodingAgent;

/// Marker file in the worktree root: one agent id (`claude`/`codex`/`pi`)
/// per line, in first-recorded order.
pub const AGENTS_FILE: &str = ".exp-agents";

/// The agents recorded as having run in `worktree`. `None` = no (readable)
/// marker — a pre-marker worktree whose history is unknown; unknown ids are
/// skipped so a newer build's vocabulary never breaks an older one.
pub fn worktree_agents(worktree: &Path) -> Option<Vec<CodingAgent>> {
    let raw = std::fs::read_to_string(worktree.join(AGENTS_FILE)).ok()?;
    let mut seen = HashSet::new();
    Some(
        raw.lines()
            .filter_map(CodingAgent::parse)
            .filter(|agent| seen.insert(*agent))
            .collect(),
    )
}

/// Append `agent` to the worktree's marker (created on first write; no-op
/// when already recorded). Best-effort at the call site — a failed write
/// only costs a resume offer, never a launch.
pub fn record_worktree_agent(worktree: &Path, agent: CodingAgent) -> std::io::Result<()> {
    let recorded = worktree_agents(worktree).unwrap_or_default();
    if recorded.contains(&agent) {
        return Ok(());
    }
    let path = worktree.join(AGENTS_FILE);
    let mut content = std::fs::read_to_string(&path).unwrap_or_default();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(agent.id());
    content.push('\n');
    std::fs::write(&path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-worktree-agents-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_marker_reads_as_unknown_history() {
        let dir = temp_dir("missing");
        assert_eq!(worktree_agents(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_round_trips_and_dedupes() {
        let dir = temp_dir("record");
        record_worktree_agent(&dir, CodingAgent::Codex).unwrap();
        record_worktree_agent(&dir, CodingAgent::Claude).unwrap();
        record_worktree_agent(&dir, CodingAgent::Codex).unwrap();
        assert_eq!(
            worktree_agents(&dir),
            Some(vec![CodingAgent::Codex, CodingAgent::Claude])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_ids_are_skipped_not_fatal() {
        let dir = temp_dir("unknown");
        std::fs::write(dir.join(AGENTS_FILE), "cursor\nclaude\n\n  codex  \n").unwrap();
        assert_eq!(
            worktree_agents(&dir),
            Some(vec![CodingAgent::Claude, CodingAgent::Codex])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
