//! EXP-1051 — what a run's context window holds BEFORE the agent has read a
//! single file: the raw byte counts of every layer the launcher itself put
//! there.
//!
//! The launcher is the only place that knows all of them — it composes the
//! system-prompt append ([`crate::skill::system_append`]), renders the seed
//! prompt, picks the cwd the agent's project memory hangs off, and asks the
//! server how big the Exponential MCP surface is. So it measures them here
//! and hands the numbers to the engine on [`crate::launcher::AcpLaunch`];
//! the engine turns them into token estimates
//! (`domain::contract::CONTEXT_LAYOUT_CHARS_PER_TOKEN`) and publishes the
//! `context_layout` steer frame. Nothing in this module estimates, formats or
//! renders anything: bytes in, bytes out.
//!
//! Best-effort throughout, like every other prepare-time enrichment: a file
//! that vanished between the walk and the spawn, or a server that has not
//! learned the budget query yet, costs the picture one segment and never a
//! launch.

use std::path::{Path, PathBuf};

use crate::agent::CodingAgent;

/// The measured layers of ONE launch's starting context, in bytes.
///
/// Every field is what the LAUNCHER put there; the agent's own additions
/// (the files it reads, the tools it calls) are the engine's running total,
/// not this snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextLayers {
    /// The run playbook ([`crate::skill::RUN_SKILL`]) in bytes.
    pub playbook_bytes: usize,
    /// EXP-1025 team prompt bytes, when the team has one.
    pub team_bytes: Option<usize>,
    /// The seed prompt's bytes; `None` on a resume (no seed prompt).
    pub task_bytes: Option<usize>,
    /// The project memory files the agent will load, in load order.
    pub project: Vec<ProjectMemoryFile>,
    /// The Exponential MCP surface the agent gets (always-load tool defs +
    /// server instructions), when the server answered.
    pub tools_bytes: Option<usize>,
    /// A resume's measured base from the previous run, when one was recorded.
    pub carried_base: Option<CarriedBase>,
}

/// One memory file the agent loads on its own at start (`CLAUDE.md` and its
/// siblings, `AGENTS.md`) — named the way the person reading the breakdown
/// would name it, and sized as it sits on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectMemoryFile {
    /// How the file is shown: relative to the run's cwd when it is inside
    /// it, `~/…` under the home dir, else the absolute path.
    pub label: String,
    /// The file's size on disk, symlinks followed.
    pub bytes: usize,
}

/// EXP-1051 — what the PREVIOUS run of a resumed conversation had measured
/// for itself: a resume re-enters a transcript whose base context this run
/// never built, so the only honest number is the recorded one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CarriedBase {
    /// The predecessor's measured base, in tokens (the agent's own count,
    /// not an estimate off bytes).
    pub tokens: u64,
    /// The model it was measured on — a base carried across a model switch
    /// is a different window, so the reader has to see which one it was.
    pub model: String,
}

/// The TEAM PROMPT's contribution to [`crate::skill::system_append`]'s
/// output, in bytes — everything the append carries beyond the playbook
/// itself (the heading, the framing sentence and the owners' text).
///
/// Derived from the composed string rather than from the prompt, because the
/// append is what actually reaches the agent: no team prompt makes
/// `system_append` return the playbook VERBATIM, and anything else is the
/// playbook trimmed of its trailing whitespace plus the team block. `> 0`
/// keeps a degenerate composition (a future shape this build mis-reads) out
/// of the picture instead of reporting a zero-byte layer.
pub fn team_bytes(system_append: &str) -> Option<usize> {
    if system_append == crate::skill::RUN_SKILL {
        return None;
    }
    let playbook = crate::skill::RUN_SKILL.trim_end().len();
    Some(system_append.len().saturating_sub(playbook)).filter(|bytes| *bytes > 0)
}

/// The memory files `agent` will load for a run whose cwd is `cwd`, in LOAD
/// ORDER: outermost ancestor first, the cwd's own files last, then the
/// user-level file. (Outermost-first is the order the more specific file
/// wins in, and the order both agents document.)
///
/// `profile_dir` is the run's account profile config dir (EXP-849:
/// `CLAUDE_CONFIG_DIR`/`CODEX_HOME`); `None` = the ambient login, whose
/// user file sits under `~/.claude` / `~/.codex`.
///
/// Sizes come from [`std::fs::metadata`], which FOLLOWS symlinks on purpose:
/// a repo whose `AGENTS.md` is a symlink to its `CLAUDE.md` (this one) loads
/// the target's bytes, not the link's. Missing, empty and non-file entries
/// are skipped — an empty memory file costs the agent nothing and would only
/// add a zero row to the breakdown.
pub fn project_memory(
    agent: CodingAgent,
    cwd: &Path,
    profile_dir: Option<&Path>,
) -> Vec<ProjectMemoryFile> {
    let home = dirs::home_dir();
    let mut files = Vec::new();
    // cwd → root, then reversed: the walk is naturally innermost-first and
    // the output is outermost-first.
    let mut dirs: Vec<&Path> = Vec::new();
    for dir in cwd.ancestors() {
        dirs.push(dir);
        // codex reads AGENTS.md up to the repository root and stops there. A
        // worktree's `.git` is a FILE, not a dir, so the test is "exists".
        if agent == CodingAgent::Codex && dir.join(".git").exists() {
            break;
        }
    }
    dirs.reverse();

    for dir in dirs {
        for name in memory_file_names(agent) {
            push_file(&mut files, &dir.join(name), cwd, home.as_deref());
        }
    }

    // The user-level file, last: it is the same document for every run on
    // this account, and the launcher can only name it per PROFILE.
    let user_dir = match profile_dir {
        Some(dir) => Some(dir.to_path_buf()),
        None => home.as_ref().map(|home| home.join(agent_home_dir(agent))),
    };
    if let Some(user_dir) = user_dir {
        push_file(
            &mut files,
            &user_dir.join(user_memory_file_name(agent)),
            cwd,
            home.as_deref(),
        );
    }
    files
}

/// The per-directory memory files, in the order the agent reads them.
fn memory_file_names(agent: CodingAgent) -> &'static [&'static str] {
    match agent {
        // `CLAUDE.local.md` is the untracked personal override; `.claude/
        // CLAUDE.md` the directory form.
        CodingAgent::Claude => &["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"],
        CodingAgent::Codex => &["AGENTS.md"],
    }
}

/// The user-level memory file's name (the one under the config dir).
fn user_memory_file_name(agent: CodingAgent) -> &'static str {
    match agent {
        CodingAgent::Claude => "CLAUDE.md",
        CodingAgent::Codex => "AGENTS.md",
    }
}

/// The ambient (profile-less) config dir's name under `$HOME`.
fn agent_home_dir(agent: CodingAgent) -> &'static str {
    match agent {
        CodingAgent::Claude => ".claude",
        CodingAgent::Codex => ".codex",
    }
}

/// Measure one candidate and append it when it is a non-empty regular file.
/// Duplicates are dropped by LABEL: a cwd that IS the repo root would
/// otherwise be walked once as an ancestor and once as itself.
fn push_file(files: &mut Vec<ProjectMemoryFile>, path: &Path, cwd: &Path, home: Option<&Path>) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if !meta.is_file() || meta.len() == 0 {
        return;
    }
    let label = label_for(path, cwd, home);
    if files.iter().any(|file| file.label == label) {
        return;
    }
    files.push(ProjectMemoryFile {
        label,
        bytes: meta.len() as usize,
    });
}

/// How one memory file is NAMED in the breakdown: relative to the run's cwd
/// when it sits inside it, `~/…` when it sits under the home dir, else the
/// absolute path. Separators come straight off [`Path::display`].
fn label_for(path: &Path, cwd: &Path, home: Option<&Path>) -> String {
    if let Ok(relative) = path.strip_prefix(cwd) {
        return relative.display().to_string();
    }
    if let Some(relative) = home.and_then(|home| path.strip_prefix(home).ok()) {
        return PathBuf::from("~").join(relative).display().to_string();
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No tempfile dev-dependency in this crate (see `doctor.rs`) — a unique
    /// dir under the system temp, removed by the test itself.
    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-context-layout-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn labels(files: &[ProjectMemoryFile]) -> Vec<&str> {
        files.iter().map(|file| file.label.as_str()).collect()
    }

    /// EXP-1025: no team prompt = the bare playbook = no team layer.
    #[test]
    fn team_bytes_is_none_without_a_team_prompt() {
        assert_eq!(team_bytes(&crate::skill::system_append(None)), None);
        assert_eq!(team_bytes(&crate::skill::system_append(Some("  "))), None);
    }

    /// Everything the append carries beyond the playbook — heading, framing
    /// sentence and the owners' text — is the team layer.
    #[test]
    fn team_bytes_measures_the_whole_team_block() {
        let prompt = "## Rules\n\n- Always run `bun test`.";
        let append = crate::skill::system_append(Some(prompt));
        let bytes = team_bytes(&append).expect("a team layer");
        assert_eq!(bytes, append.len() - crate::skill::RUN_SKILL.trim_end().len());
        // The block is the prompt plus the framing the launcher adds.
        assert!(bytes > prompt.len());
        assert!(append.ends_with(&format!("{prompt}\n")));
    }

    /// claude reads three filenames per directory, outermost ancestor first,
    /// then the user file under the profile dir.
    #[test]
    fn claude_walks_every_ancestor_and_all_three_filenames() {
        let root = temp_dir("claude-walk");
        let repo = root.join("repo");
        let cwd = repo.join("apps").join("web");
        std::fs::create_dir_all(&cwd).unwrap();
        write(&root.join("CLAUDE.md"), "root");
        write(&repo.join("CLAUDE.md"), "repo memory");
        write(&repo.join("CLAUDE.local.md"), "local");
        write(&repo.join(".claude").join("CLAUDE.md"), "dir form");
        write(&cwd.join("CLAUDE.md"), "leaf");
        let profile = root.join("profile");
        write(&profile.join("CLAUDE.md"), "user level");

        let files = project_memory(CodingAgent::Claude, &cwd, Some(&profile));
        assert_eq!(
            labels(&files),
            vec![
                // `root` and `repo` sit ABOVE the cwd, so they label absolute.
                root.join("CLAUDE.md").display().to_string().as_str(),
                repo.join("CLAUDE.md").display().to_string().as_str(),
                repo.join("CLAUDE.local.md").display().to_string().as_str(),
                repo.join(".claude/CLAUDE.md").display().to_string().as_str(),
                "CLAUDE.md",
                profile.join("CLAUDE.md").display().to_string().as_str(),
            ]
        );
        assert_eq!(files[1].bytes, "repo memory".len());
        assert_eq!(files.last().unwrap().bytes, "user level".len());

        // codex reads none of them.
        assert!(project_memory(CodingAgent::Codex, &cwd, Some(&profile)).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// codex stops at the repository root — the first directory carrying a
    /// `.git` entry, which in a WORKTREE is a file.
    #[test]
    fn codex_stops_at_the_first_git_entry() {
        let root = temp_dir("codex-git");
        let repo = root.join("repo");
        let cwd = repo.join("apps");
        std::fs::create_dir_all(&cwd).unwrap();
        write(&root.join("AGENTS.md"), "above the repo");
        write(&repo.join("AGENTS.md"), "repo memory");
        write(&cwd.join("AGENTS.md"), "leaf");
        // A linked worktree's `.git` is a FILE.
        write(&repo.join(".git"), "gitdir: /elsewhere");

        let files = project_memory(CodingAgent::Codex, &cwd, None);
        assert_eq!(
            labels(&files),
            vec![
                repo.join("AGENTS.md").display().to_string().as_str(),
                "AGENTS.md",
            ],
            "nothing above the repo root is read"
        );

        // A `.git` DIRECTORY stops the walk just the same.
        let plain = temp_dir("codex-git-dir");
        std::fs::create_dir_all(plain.join("repo/.git")).unwrap();
        write(&plain.join("AGENTS.md"), "above");
        write(&plain.join("repo").join("AGENTS.md"), "repo");
        let files = project_memory(CodingAgent::Codex, &plain.join("repo"), None);
        assert_eq!(labels(&files), vec!["AGENTS.md"]);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&plain);
    }

    /// The repo's own `AGENTS.md` is a symlink to `CLAUDE.md` — the agent
    /// loads the TARGET's bytes, so that is what is measured.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_memory_file_reports_its_targets_bytes() {
        let root = temp_dir("symlink");
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let target = repo.join("CLAUDE.md");
        write(&target, "the one document, at some length");
        std::os::unix::fs::symlink(&target, repo.join("AGENTS.md")).unwrap();

        let files = project_memory(CodingAgent::Codex, &repo, None);
        assert_eq!(labels(&files), vec!["AGENTS.md"]);
        assert_eq!(files[0].bytes, "the one document, at some length".len());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Empty files, missing files and directories never make the list.
    #[test]
    fn empty_and_missing_entries_are_skipped() {
        let root = temp_dir("empty");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        write(&repo.join("CLAUDE.md"), "");
        // A DIRECTORY named like a memory file is not a memory file.
        std::fs::create_dir_all(repo.join("CLAUDE.local.md")).unwrap();
        write(&repo.join(".claude").join("CLAUDE.md"), "the only one");

        let files = project_memory(CodingAgent::Claude, &repo, None);
        assert_eq!(labels(&files), vec![".claude/CLAUDE.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The three label forms, in order of preference.
    #[test]
    fn labels_prefer_cwd_then_home_then_the_absolute_path() {
        let cwd = Path::new("/repos/owner/name.worktrees/exp-EXP-1051");
        let home = Path::new("/Users/someone");
        assert_eq!(
            label_for(&cwd.join("apps/web/CLAUDE.md"), cwd, Some(home)),
            "apps/web/CLAUDE.md"
        );
        assert_eq!(
            label_for(Path::new("/Users/someone/.claude/CLAUDE.md"), cwd, Some(home)),
            "~/.claude/CLAUDE.md"
        );
        assert_eq!(
            label_for(Path::new("/etc/CLAUDE.md"), cwd, Some(home)),
            "/etc/CLAUDE.md"
        );
        // No home resolved: the absolute path, never a bare `~`.
        assert_eq!(
            label_for(Path::new("/Users/someone/.claude/CLAUDE.md"), cwd, None),
            "/Users/someone/.claude/CLAUDE.md"
        );
    }

    /// The cwd IS the repo root, and the profile dir sits inside it: neither
    /// file may be counted twice.
    #[test]
    fn the_same_file_is_never_listed_twice() {
        let root = temp_dir("dedupe");
        write(&root.join("CLAUDE.md"), "one");
        let files = project_memory(CodingAgent::Claude, &root, Some(&root));
        assert_eq!(labels(&files), vec!["CLAUDE.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
