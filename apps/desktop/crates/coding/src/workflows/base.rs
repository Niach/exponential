//! EXP-983 — DAG-shaped git: the branches a speculative start needs.
//!
//! When a node starts before its blockers landed it bases on THEIR work. One
//! unlanded blocker means that blocker's own branch; two or more mean a
//! SYNTHETIC base, `exp/wf-<id8>-base-<IDENT>`, which is what this module
//! builds: cut from the integration branch, then the blockers' branches
//! merged into it. A refresh merges the moved tips INTO the branch that is
//! already there — never a recreate, never a rebase, never a force-push, so
//! everything a run already has stays an ancestor of what it is told to
//! merge next.
//!
//! Every merge is PRE-TESTED with `git merge-tree --write-tree`: a broken
//! base is never pushed. A conflicting pair comes back as
//! [`BaseOutcome::Conflict`] instead, and the engine serializes the two
//! blockers rather than handing anyone a base that does not build.
//!
//! The work happens in the ENGINE's own scratch worktree
//! (`<clone>.worktrees/.exp-wf-<id8>`), never in a session's worktree: an
//! agent's checkout is its own, and nothing here may move it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::git_worktree::{git_output, run_git, validate_branch_arg, GitError, TokenUrl};

/// The engine's scratch worktree for ONE workflow — beside the session
/// worktrees, marked so nothing mistakes it for a run's checkout.
pub fn engine_worktree(clone: &Path, workflow_id: &str) -> PathBuf {
    let id8: String = workflow_id.chars().take(8).collect();
    crate::git_worktree::worktrees_dir(clone).join(format!(".exp-wf-{id8}"))
}

/// What a build attempt produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseOutcome {
    /// The base is up on origin, built from these `(branch, sha)` pairs —
    /// what the host records so the next pass knows when it moved.
    Built(Vec<(String, String)>),
    /// Two sources cannot both be merged in. NOTHING was pushed: the engine
    /// serializes the pair instead and the dependent waits.
    Conflict { left: String, right: String },
}

/// Build (or refresh) `base_branch` from `sources`, and push it.
///
/// `sources` are branch names (the caller sorts them; the integration branch
/// is one of them). A source origin does not have yet is skipped — it cannot
/// be merged, and the next beat will see it.
pub fn build_base(
    clone: &Path,
    workspace: &Path,
    base_branch: &str,
    integration_branch: &str,
    sources: &[String],
    url: Option<&TokenUrl>,
) -> Result<BaseOutcome, GitError> {
    validate_branch_arg(base_branch, "workflow base branch")?;
    validate_branch_arg(integration_branch, "integration branch")?;
    for source in sources {
        validate_branch_arg(source, "workflow base source")?;
    }
    // One fetch for the whole build: every source and the base itself.
    run_git(Some(clone), &["fetch", "origin"], url, "git fetch origin")?;
    // Reusing a resolution the last build already made is free and quiet.
    let _ = run_git(
        Some(clone),
        &["config", "rerere.enabled", "true"],
        None,
        "git config rerere.enabled",
    );

    // A base that is already up is EXTENDED; a new one is cut from the
    // integration branch. Either way the worktree ends on `base_branch`.
    let start = if origin_ref(clone, base_branch).is_some() {
        format!("refs/remotes/origin/{base_branch}")
    } else {
        format!("refs/remotes/origin/{integration_branch}")
    };
    let workspace = ensure_engine_worktree(clone, workspace, base_branch, &start, url)?;

    let mut built: Vec<(String, String)> = Vec::new();
    let mut merged: Vec<String> = Vec::new();
    for source in sources {
        let Some(sha) = origin_ref(clone, source) else {
            continue;
        };
        built.push((source.clone(), sha));
        let theirs = format!("refs/remotes/origin/{source}");
        // PRE-TEST: a conflict must never reach the pushed base.
        if merge_conflicts(&workspace, "HEAD", &theirs, url)? {
            return Ok(BaseOutcome::Conflict {
                // The first real source already in tells the caller WHO the
                // newcomer collides with; with none, it is the trunk itself.
                left: merged
                    .first()
                    .cloned()
                    .unwrap_or_else(|| integration_branch.to_string()),
                right: source.clone(),
            });
        }
        run_git(
            Some(&workspace),
            &["merge", "--no-ff", "--no-edit", &theirs],
            url,
            &format!("git merge origin/{source}"),
        )?;
        merged.push(source.clone());
    }
    // Fast-forward by construction (the branch was cut from what is there),
    // so a plain push is enough — the engine never forces anything.
    run_git(
        Some(&workspace),
        &["push", "origin", &format!("HEAD:refs/heads/{base_branch}")],
        url,
        &format!("git push origin {base_branch}"),
    )?;
    Ok(BaseOutcome::Built(built))
}

/// Whether merging `theirs` into `ours` would conflict — decided WITHOUT
/// touching a working tree (`git merge-tree --write-tree`, exit 1 = conflict).
pub fn merge_conflicts(
    cwd: &Path,
    ours: &str,
    theirs: &str,
    url: Option<&TokenUrl>,
) -> Result<bool, GitError> {
    let op = format!("git merge-tree {ours} {theirs}");
    let output = git_output(
        Some(cwd),
        &["merge-tree", "--write-tree", ours, theirs],
        url,
        &op,
    )?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(GitError {
            op,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }),
    }
}

/// `git ls-remote --heads origin <pattern>` as `branch → sha` — the host's
/// ONE view of what moved, one call per repository per beat.
pub fn remote_tips(
    clone: &Path,
    pattern: &str,
    url: Option<&TokenUrl>,
) -> Result<HashMap<String, String>, GitError> {
    validate_branch_arg(pattern, "ls-remote pattern")?;
    let raw = run_git(
        Some(clone),
        &["ls-remote", "--heads", "origin", pattern],
        url,
        "git ls-remote origin",
    )?;
    Ok(parse_ls_remote(&raw))
}

/// `<sha>\trefs/heads/<branch>` lines as a map. Anything else is skipped:
/// git prints warnings on the same stream on some hosts.
fn parse_ls_remote(raw: &str) -> HashMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let (sha, reference) = line.split_once('\t')?;
            let branch = reference.trim().strip_prefix("refs/heads/")?;
            (!sha.trim().is_empty()).then(|| (branch.to_string(), sha.trim().to_string()))
        })
        .collect()
}

/// The engine's own summary of what a run is about to merge — the subjects
/// in the range plus the files they touched, ONE line each. Zero agent
/// tokens: the host reads it off git and hands it over as plain text.
pub const MOVEMENT_NOTE_CAP: usize = 1_500;

/// `git log --oneline <from>..<to>` + `git diff --stat <from>..<to>`,
/// flattened and capped. A range git cannot resolve yields the bare `to`,
/// which still tells the run WHAT to merge.
pub fn movement_note(cwd: &Path, from: &str, to: &str, url: Option<&TokenUrl>) -> String {
    let range = format!("{from}..{to}");
    let log = run_git(
        Some(cwd),
        &["log", "--oneline", "--no-decorate", "-n", "20", &range],
        url,
        "git log (workflow upstream)",
    )
    .unwrap_or_default();
    let stat = run_git(
        Some(cwd),
        &["diff", "--stat", &range],
        url,
        "git diff --stat (workflow upstream)",
    )
    .unwrap_or_default();
    let lines: Vec<&str> = log
        .lines()
        .chain(stat.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return to.to_string();
    }
    let joined = lines.join("; ");
    joined.chars().take(MOVEMENT_NOTE_CAP).collect()
}

/// Drop a branch this engine pushed. A branch that is already gone is a
/// success — the sweep runs on every pass until it is.
pub fn delete_remote_branch(
    clone: &Path,
    branch: &str,
    url: Option<&TokenUrl>,
) -> Result<(), GitError> {
    validate_branch_arg(branch, "workflow base branch")?;
    match run_git(
        Some(clone),
        &["push", "origin", "--delete", branch],
        url,
        &format!("git push origin --delete {branch}"),
    ) {
        Ok(_) => Ok(()),
        Err(err) if err.to_string().contains("remote ref does not exist") => Ok(()),
        Err(err) => Err(err),
    }
}

/// The sha `origin/<branch>` resolves to right now, or `None` when origin
/// has no such branch (the answer, not an error).
fn origin_ref(clone: &Path, branch: &str) -> Option<String> {
    run_git(
        Some(clone),
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{branch}"),
        ],
        None,
        &format!("git rev-parse origin/{branch}"),
    )
    .ok()
    .map(|sha| sha.trim().to_string())
    .filter(|sha| !sha.is_empty())
}

/// The engine's scratch worktree, on `branch` at `start`. One worktree per
/// workflow, re-pointed per build: the builds are sequential and nothing
/// here is ever a session's checkout.
fn ensure_engine_worktree(
    clone: &Path,
    workspace: &Path,
    branch: &str,
    start: &str,
    url: Option<&TokenUrl>,
) -> Result<PathBuf, GitError> {
    let path = workspace.to_path_buf();
    let path_str = path.to_string_lossy().into_owned();
    if !path.join(".git").exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| GitError {
                op: "prepare the engine worktree".to_string(),
                detail: err.to_string(),
            })?;
        }
        run_git(
            Some(clone),
            &["worktree", "add", "--force", "-B", branch, &path_str, start],
            url,
            "git worktree add (engine)",
        )?;
        return Ok(path);
    }
    // Reused: drop whatever the last build left and re-point at the base.
    let _ = run_git(
        Some(&path),
        &["reset", "--hard"],
        None,
        "git reset --hard (engine)",
    );
    let _ = run_git(Some(&path), &["clean", "-fd"], None, "git clean (engine)");
    run_git(
        Some(&path),
        &["checkout", "--force", "-B", branch, start],
        url,
        &format!("git checkout -B {branch}"),
    )?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-wf-base-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Engine")
            .env("GIT_AUTHOR_EMAIL", "engine@example.com")
            .env("GIT_COMMITTER_NAME", "Engine")
            .env("GIT_COMMITTER_EMAIL", "engine@example.com")
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    /// A throwaway bare origin with a clone of it, both on `main`, one
    /// commit in. The clone carries an identity so merges can commit.
    fn repo(tag: &str) -> (TempDir, PathBuf, PathBuf) {
        let dir = temp_dir(tag);
        let origin = dir.0.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&dir.0, &["init", "--bare", "-b", "main", "origin.git"]);
        let clone = dir.0.join("clone");
        git(
            &dir.0,
            &["clone", origin.to_string_lossy().as_ref(), "clone"],
        );
        git(&clone, &["config", "user.email", "engine@example.com"]);
        git(&clone, &["config", "user.name", "Engine"]);
        git(&clone, &["config", "commit.gpgsign", "false"]);
        write(&clone, "README.md", "trunk\n");
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-m", "trunk"]);
        git(&clone, &["push", "-u", "origin", "main"]);
        (dir, origin, clone)
    }

    /// Cut `branch` off `main`, write `file`, push it.
    fn push_branch(clone: &Path, branch: &str, file: &str, body: &str) {
        git(clone, &["checkout", "-B", branch, "origin/main"]);
        write(clone, file, body);
        git(clone, &["add", "-A"]);
        git(clone, &["commit", "-m", &format!("{branch}: {file}")]);
        git(clone, &["push", "-u", "origin", branch]);
        git(clone, &["checkout", "main"]);
    }

    fn origin_file(origin: &Path, branch: &str, file: &str) -> Option<String> {
        let output = Command::new("git")
            .args(["show", &format!("{branch}:{file}")])
            .current_dir(origin)
            .output()
            .expect("git runs");
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// A clean multi-source build: the base carries BOTH blockers' work and
    /// is pushed, and the outcome names the tips it was built from.
    #[test]
    fn a_clean_build_merges_every_source_and_pushes_the_base() {
        let (dir, origin, clone) = repo("clean");
        push_branch(&clone, "exp/EXP-1", "one.txt", "one\n");
        push_branch(&clone, "exp/EXP-2", "two.txt", "two\n");
        let workspace = engine_worktree(&clone, "abcdef12-3456");
        let sources = vec![
            "exp/EXP-1".to_string(),
            "exp/EXP-2".to_string(),
            "main".to_string(),
        ];
        let outcome = build_base(
            &clone,
            &workspace,
            "exp/wf-abcdef12-base-EXP-3",
            "main",
            &sources,
            None,
        )
        .expect("the build runs");
        let built = match outcome {
            BaseOutcome::Built(built) => built,
            other => panic!("expected a clean build, got {other:?}"),
        };
        assert_eq!(
            built.iter().map(|(branch, _)| branch.clone()).collect::<Vec<_>>(),
            ["exp/EXP-1", "exp/EXP-2", "main"],
            "the outcome names every source it merged"
        );
        assert_eq!(
            origin_file(&origin, "exp/wf-abcdef12-base-EXP-3", "one.txt").as_deref(),
            Some("one\n")
        );
        assert_eq!(
            origin_file(&origin, "exp/wf-abcdef12-base-EXP-3", "two.txt").as_deref(),
            Some("two\n")
        );
        drop(dir);
    }

    /// A refresh MERGES the moved tip into the base that is already up: the
    /// base's previous commit stays an ancestor, so nothing a run already
    /// pulled is rewritten.
    #[test]
    fn a_refresh_merges_the_moved_tip_into_the_existing_base() {
        let (dir, origin, clone) = repo("refresh");
        push_branch(&clone, "exp/EXP-1", "one.txt", "one\n");
        push_branch(&clone, "exp/EXP-2", "two.txt", "two\n");
        let workspace = engine_worktree(&clone, "abcdef12-3456");
        let base = "exp/wf-abcdef12-base-EXP-3";
        let sources = vec![
            "exp/EXP-1".to_string(),
            "exp/EXP-2".to_string(),
            "main".to_string(),
        ];
        build_base(&clone, &workspace, base, "main", &sources, None).unwrap();
        let first = git(&clone, &["rev-parse", &format!("origin/{base}")]);

        // EXP-1 moves on.
        git(&clone, &["checkout", "-B", "exp/EXP-1", "origin/exp/EXP-1"]);
        write(&clone, "one.txt", "one\nmore\n");
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-m", "more"]);
        git(&clone, &["push", "origin", "exp/EXP-1"]);
        git(&clone, &["checkout", "main"]);

        let outcome = build_base(&clone, &workspace, base, "main", &sources, None).unwrap();
        assert!(matches!(outcome, BaseOutcome::Built(_)));
        git(&clone, &["fetch", "origin"]);
        let second = git(&clone, &["rev-parse", &format!("origin/{base}")]);
        assert_ne!(first, second, "the refresh moved the base");
        assert_eq!(
            origin_file(&origin, base, "one.txt").as_deref(),
            Some("one\nmore\n")
        );
        // Merged forward, never recreated: the old base is still in there.
        let ancestor = Command::new("git")
            .args(["merge-base", "--is-ancestor", &first, &second])
            .current_dir(&clone)
            .status()
            .expect("git runs");
        assert!(ancestor.success(), "the previous base stays an ancestor");
        drop(dir);
    }

    /// A genuinely conflicting pair is DETECTED and nothing is pushed: the
    /// engine serializes the two blockers instead of handing anyone a base
    /// that does not build.
    #[test]
    fn a_conflicting_pair_is_detected_and_never_pushed() {
        let (dir, origin, clone) = repo("conflict");
        push_branch(&clone, "exp/EXP-1", "shared.txt", "left\n");
        push_branch(&clone, "exp/EXP-2", "shared.txt", "right\n");
        let workspace = engine_worktree(&clone, "abcdef12-3456");
        let base = "exp/wf-abcdef12-base-EXP-3";
        let sources = vec![
            "exp/EXP-1".to_string(),
            "exp/EXP-2".to_string(),
            "main".to_string(),
        ];
        let outcome = build_base(&clone, &workspace, base, "main", &sources, None).unwrap();
        assert_eq!(
            outcome,
            BaseOutcome::Conflict {
                left: "exp/EXP-1".to_string(),
                right: "exp/EXP-2".to_string(),
            }
        );
        assert!(
            origin_file(&origin, base, "shared.txt").is_none(),
            "a broken base is never pushed"
        );

        // The same verdict, straight off the two branches.
        assert!(merge_conflicts(
            &clone,
            "refs/remotes/origin/exp/EXP-1",
            "refs/remotes/origin/exp/EXP-2",
            None
        )
        .unwrap());
        assert!(!merge_conflicts(
            &clone,
            "refs/remotes/origin/main",
            "refs/remotes/origin/exp/EXP-1",
            None
        )
        .unwrap());
        drop(dir);
    }

    /// The host's ONE view of what moved, and the branch sweep that ends a
    /// workflow.
    #[test]
    fn remote_tips_read_the_exp_branches_and_a_base_can_be_dropped() {
        let (dir, _origin, clone) = repo("tips");
        push_branch(&clone, "exp/EXP-1", "one.txt", "one\n");
        push_branch(&clone, "exp/EXP-2", "two.txt", "two\n");
        let tips = remote_tips(&clone, "exp/*", None).unwrap();
        assert_eq!(tips.len(), 2, "main is not an exp branch: {tips:?}");
        let head = git(&clone, &["rev-parse", "origin/exp/EXP-1"]);
        assert_eq!(tips.get("exp/EXP-1"), Some(&head));

        delete_remote_branch(&clone, "exp/EXP-1", None).unwrap();
        let tips = remote_tips(&clone, "exp/*", None).unwrap();
        assert_eq!(tips.keys().collect::<Vec<_>>(), ["exp/EXP-2"]);
        // A branch that is already gone is a success, every pass.
        delete_remote_branch(&clone, "exp/EXP-1", None).unwrap();
        drop(dir);
    }

    /// The movement note is the ENGINE's summary of the range, read off
    /// git — the run never spends a token working out what moved.
    #[test]
    fn the_movement_note_summarises_the_range() {
        let (dir, _origin, clone) = repo("note");
        push_branch(&clone, "exp/EXP-1", "one.txt", "one\n");
        let first = git(&clone, &["rev-parse", "origin/exp/EXP-1"]);
        git(&clone, &["checkout", "-B", "exp/EXP-1", "origin/exp/EXP-1"]);
        write(&clone, "one.txt", "one\nmore\n");
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-m", "add the token parser"]);
        let second = git(&clone, &["rev-parse", "HEAD"]);

        let note = movement_note(&clone, &first, &second, None);
        assert!(note.contains("add the token parser"), "{note}");
        assert!(note.contains("one.txt"), "{note}");
        assert!(!note.contains('\n'), "one line, however long the range");
        assert!(note.chars().count() <= MOVEMENT_NOTE_CAP);

        // A range git cannot resolve still names what to merge.
        assert_eq!(movement_note(&clone, "nope", &second, None), second);
        drop(dir);
    }

    /// The parser takes git's tab-separated lines and ignores everything
    /// else on the stream.
    #[test]
    fn ls_remote_lines_parse_into_branch_tips() {
        let parsed = parse_ls_remote(
            "aaa111\trefs/heads/exp/EXP-1\nbbb222\trefs/heads/exp/wf-abcdef12\nwarning: noise\n",
        );
        assert_eq!(parsed.get("exp/EXP-1").map(String::as_str), Some("aaa111"));
        assert_eq!(
            parsed.get("exp/wf-abcdef12").map(String::as_str),
            Some("bbb222")
        );
        assert_eq!(parsed.len(), 2);
    }
}
