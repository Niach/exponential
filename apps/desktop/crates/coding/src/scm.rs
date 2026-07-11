//! Source-control primitives for the **trunk** (masterplan v4 §4.4): every
//! git invocation the Source Control screen, git bar, and history pane need,
//! plus the porcelain-v2 / log / unified-diff parsers that turn git's output
//! into typed models the UI renders.
//!
//! Contract (v4 §4.4 + DNR L5):
//! * **argv git only** — every wrapper routes through [`crate::git_worktree`]'s
//!   shared `run_git` runner (`std::process::Command("git")`, explicit argv,
//!   canonical [`TokenUrl`] scrubbing); never `gh`, never a git library, never
//!   a shell.
//! * Git state is **derived from disk** (`git status --porcelain=v2 --branch`,
//!   `.git/rebase-merge`, `MERGE_HEAD`) — never from session bookkeeping — so
//!   it survives app restarts and out-of-band fixes (v4 §4.2 rule 3).
//! * Parsers are pure (`&str` → model) and unit-tested against fixture repos.
//!
//! The git-diff → renderer adapter lives in `crates/ui/src/diff.rs` (R2.d) and
//! consumes [`DiffFile`].

use std::path::{Path, PathBuf};

use crate::git_worktree::{run_git, GitError};

// ---------------------------------------------------------------------------
// Working-tree status
// ---------------------------------------------------------------------------

/// One-letter working-tree status of a path (`git status --porcelain=v2`
/// XY code, collapsed to the user-facing kind — v4 §4.4 changes list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// A single changed path in the trunk working tree. `staged` reflects whether
/// the change is in the index (the checkbox state in the Source Control
/// screen — `git add` / `git restore --staged`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Repo-relative path (the new path for renames).
    pub path: String,
    pub status: FileStatus,
    pub staged: bool,
}

/// The full `git status --porcelain=v2 --branch` snapshot: the changes list
/// plus branch + upstream + ahead/behind (the git bar's counts, v4 §4.3).
/// Ahead/behind come from the `# branch.ab` header — no extra network (fetch
/// happens separately, v4 §4.1). `upstream` is the `# branch.upstream` ref
/// (`None` for an unpublished branch — the git bar's "Publish" signal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSummary {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changes: Vec<FileChange>,
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/// One commit row for the history pane (v4 §4.4) — from a NUL-separated
/// `git log --format` (hash, subject, author, relative time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub author: String,
    /// Git's `%cr` relative time, e.g. `2 hours ago`.
    pub relative_time: String,
}

// ---------------------------------------------------------------------------
// Diffs (rendered by the shared diff.rs renderer via the R2.d adapter)
// ---------------------------------------------------------------------------

/// A single unified-diff line's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// One line of a hunk with its old/new line numbers (`None` on the side the
/// line does not belong to).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// Line content WITHOUT the leading `+`/`-`/` ` marker or trailing newline.
    pub content: String,
}

/// One `@@ … @@` hunk of a file diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The verbatim `@@ -a,b +c,d @@ …` header line.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// A per-file diff — the canonical scm diff model. The R2.d adapter maps this
/// onto whatever `diff.rs` renders (`api::issues::PullFile` today), so the PR
/// diff and the SCM/commit diff share one renderer (v4 §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    /// Repo-relative path (new path for renames).
    pub path: String,
    /// Pre-rename path when `status == Renamed`.
    pub previous_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    /// Empty when `binary` (git emits no textual hunks).
    pub hunks: Vec<UnifiedHunk>,
    /// Binary or too-large: the renderer shows the "No textual diff" note.
    pub binary: bool,
}

// ---------------------------------------------------------------------------
// Conflict state (rebase/merge paused — v4 §4.4 conflict mode)
// ---------------------------------------------------------------------------

/// Whether a paused operation is a rebase or a merge — detected from
/// `.git/rebase-merge` vs `.git/MERGE_HEAD` (v4 §4.2 rule 3: derived from
/// disk, so it survives restarts and out-of-band fixes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    Rebase,
    Merge,
}

/// A paused rebase/merge with conflicts (the §4.4 banner state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictState {
    pub kind: ConflictKind,
    /// Conflicted (unmerged) repo-relative paths.
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Wrappers — argv git, trunk repo (R2.a)
// ---------------------------------------------------------------------------

/// `git status --porcelain=v2 --branch` → parsed changes + ahead/behind.
pub fn status(repo: &Path) -> Result<StatusSummary, GitError> {
    let raw = run_git(Some(repo), &["status", "--porcelain=v2", "--branch"], None, "git status")?;
    Ok(parse_status(&raw))
}

/// `git log --format=<NUL-separated>` paged (`skip`/`limit`, v4 §4.4 "200 at a
/// time, Load more") for an arbitrary rev — `None` = HEAD; a branch name is
/// the Source Control sidebar's "view another branch's history without
/// checking it out" path. Records are NUL-separated (`-z`); fields inside a
/// record are unit-separated (`%x1f`) so a subject can hold any punctuation.
pub fn log_branch(
    repo: &Path,
    branch: Option<&str>,
    skip: usize,
    limit: usize,
) -> Result<Vec<CommitInfo>, GitError> {
    let skip_arg = format!("--skip={skip}");
    let max_arg = format!("--max-count={limit}");
    let mut args = vec![
        "log",
        "-z",
        "--format=%H%x1f%s%x1f%an%x1f%cr",
        &skip_arg,
        &max_arg,
    ];
    if let Some(branch) = branch {
        args.push(branch);
    }
    let raw = run_git(Some(repo), &args, None, "git log")?;
    Ok(parse_log(&raw))
}

/// Working-tree diff of one path (`git diff [--cached] -- <path>`), parsed to
/// the shared [`DiffFile`] model. An empty diff (path clean on the requested
/// side) yields a zero-hunk [`DiffFile`] rather than an error.
pub fn working_diff(repo: &Path, path: &str, staged: bool) -> Result<DiffFile, GitError> {
    let args: &[&str] = if staged {
        &["diff", "--cached", "--", path]
    } else {
        &["diff", "--", path]
    };
    let raw = run_git(Some(repo), args, None, "git diff")?;
    Ok(parse_unified_diff(&raw)
        .into_iter()
        .next()
        .unwrap_or_else(|| DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: FileStatus::Modified,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            binary: false,
        }))
}

/// A commit's per-file diffs (`git show <hash>`), parsed to [`DiffFile`]s
/// (history-pane selection, v4 §4.4). `--format=` drops the commit header so
/// only the patch body reaches the parser.
pub fn commit_diff(repo: &Path, hash: &str) -> Result<Vec<DiffFile>, GitError> {
    let raw = run_git(Some(repo), &["show", "--format=", hash], None, "git show")?;
    Ok(parse_unified_diff(&raw))
}

/// Stage a path: `git add -- <path>`.
pub fn stage(repo: &Path, path: &str) -> Result<(), GitError> {
    run_git(Some(repo), &["add", "--", path], None, "git add")?;
    Ok(())
}

/// Unstage a path: `git restore --staged -- <path>`.
pub fn unstage(repo: &Path, path: &str) -> Result<(), GitError> {
    run_git(Some(repo), &["restore", "--staged", "--", path], None, "git restore --staged")?;
    Ok(())
}

/// Commit the staged changes: `git commit -m <message>`.
pub fn commit(repo: &Path, message: &str) -> Result<(), GitError> {
    run_git(Some(repo), &["commit", "-m", message], None, "git commit")?;
    Ok(())
}

/// Whether the working tree has ANY change (staged, unstaged, or untracked):
/// `git status --porcelain` non-empty.
pub fn is_dirty(repo: &Path) -> Result<bool, GitError> {
    let out = run_git(Some(repo), &["status", "--porcelain"], None, "git status --porcelain")?;
    Ok(!out.trim().is_empty())
}

// ---------------------------------------------------------------------------
// Stashes (the dirty-branch-switch escape hatch — git bar D-dialog + Source
// Control restore strip)
// ---------------------------------------------------------------------------

/// One stash entry (`git stash list`). `message` is git's reflog subject —
/// for an `-m` stash that is `On <branch>: <message>` (see
/// [`stash_switch_branch`] for the exp-switch tag extraction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

/// The dirty-switch stash tag: `exp-switch: <branch>` (the branch the changes
/// were stashed FROM). Consumers filter on this so user-made stashes are
/// never touched.
pub fn stash_switch_message(branch: &str) -> String {
    format!("exp-switch: {branch}")
}

/// Extract the branch out of an exp-switch stash subject (`On main:
/// exp-switch: feature/x` → `feature/x`). `None` for any stash that was not
/// made by the dirty-switch dialog.
pub fn stash_switch_branch(message: &str) -> Option<&str> {
    let start = message.find("exp-switch: ")?;
    Some(message[start + "exp-switch: ".len()..].trim())
}

/// `git stash push --include-untracked -m <message>` — used by the dirty
/// branch switch with a [`stash_switch_message`] tag.
pub fn stash_push(repo: &Path, message: &str) -> Result<(), GitError> {
    run_git(
        Some(repo),
        &["stash", "push", "--include-untracked", "-m", message],
        None,
        "git stash push",
    )?;
    Ok(())
}

/// `git stash list --format=%gd%x1f%gs` → parsed entries, newest first.
pub fn stash_list(repo: &Path) -> Result<Vec<StashEntry>, GitError> {
    let raw = run_git(
        Some(repo),
        &["stash", "list", "--format=%gd%x1f%gs"],
        None,
        "git stash list",
    )?;
    Ok(parse_stash_list(&raw))
}

/// `git stash pop stash@{N}` — restore + drop (conflicts leave the stash in
/// place, git's own behavior).
pub fn stash_pop(repo: &Path, index: usize) -> Result<(), GitError> {
    let spec = format!("stash@{{{index}}}");
    run_git(Some(repo), &["stash", "pop", &spec], None, "git stash pop")?;
    Ok(())
}

/// `git stash drop stash@{N}` — discard without applying.
pub fn stash_drop(repo: &Path, index: usize) -> Result<(), GitError> {
    let spec = format!("stash@{{{index}}}");
    run_git(Some(repo), &["stash", "drop", &spec], None, "git stash drop")?;
    Ok(())
}

/// Parse `git stash list --format=%gd%x1f%gs` output (`stash@{N}` + subject).
pub fn parse_stash_list(raw: &str) -> Vec<StashEntry> {
    raw.lines()
        .filter_map(|line| {
            let (selector, message) = line.split_once('\x1f')?;
            let index = selector
                .trim()
                .strip_prefix("stash@{")?
                .strip_suffix('}')?
                .parse()
                .ok()?;
            Some(StashEntry { index, message: message.trim().to_string() })
        })
        .collect()
}

/// True when a checkout failure is git's "local changes would be overwritten"
/// refusal (the dirty-switch-dialog trigger). Pure classification of the
/// C-locale error text — the shared runner forces `LC_ALL=C`, so the English
/// phrasing is reliable. Anything unmatched keeps the fail-with-git-message
/// path (never data loss).
pub fn checkout_blocked_by_local_changes(detail: &str) -> bool {
    detail.contains("would be overwritten by checkout")
        || detail.contains("commit your changes or stash them")
}

/// Detect a paused rebase/merge from disk (`<git-dir>/rebase-merge`,
/// `<git-dir>/rebase-apply`, `<git-dir>/MERGE_HEAD`) plus its unmerged files.
/// `None` when nothing is paused (v4 §4.2 rule 3: derived from disk, so it
/// survives restarts and out-of-band fixes).
pub fn detect_conflict(repo: &Path) -> Option<ConflictState> {
    let git_dir = run_git(Some(repo), &["rev-parse", "--absolute-git-dir"], None, "git rev-parse --git-dir").ok()?;
    let git_dir = PathBuf::from(git_dir.trim());
    let kind = if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        ConflictKind::Rebase
    } else if git_dir.join("MERGE_HEAD").exists() {
        ConflictKind::Merge
    } else {
        return None;
    };
    Some(ConflictState { kind, files: unmerged_files(repo) })
}

/// Abort the paused operation: `git rebase --abort` / `git merge --abort`
/// (v4 §4.4 conflict banner).
pub fn abort_conflict(repo: &Path, kind: ConflictKind) -> Result<(), GitError> {
    let args: &[&str] = match kind {
        ConflictKind::Rebase => &["rebase", "--abort"],
        ConflictKind::Merge => &["merge", "--abort"],
    };
    run_git(Some(repo), args, None, "git abort conflict")?;
    Ok(())
}

/// One local branch (the sidebar's Source Control branch list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    /// Checked out right now in THIS clone (`git branch`'s `*` marker).
    pub current: bool,
    /// Checked out in ANOTHER worktree (session worktrees) — git refuses a
    /// second checkout, so the branch switcher must not offer it.
    pub worktree: bool,
}

/// Check out a local branch: `git checkout <name>`. A dirty tree that would
/// be clobbered fails with git's own message — surfaced verbatim, never
/// forced.
pub fn checkout(repo: &Path, branch: &str) -> Result<(), GitError> {
    run_git(Some(repo), &["checkout", branch], None, "git checkout")?;
    Ok(())
}

/// Local branches, current first then most-recently-committed first:
/// `git branch --sort=-committerdate --format=…`.
pub fn branches(repo: &Path) -> Result<Vec<BranchInfo>, GitError> {
    let raw = run_git(
        Some(repo),
        &[
            "branch",
            "--list",
            "--sort=-committerdate",
            "--format=%(HEAD)%09%(refname:short)%09%(worktreepath)",
        ],
        None,
        "git branch",
    )?;
    Ok(parse_branches(&raw))
}

// ---------------------------------------------------------------------------
// Branch diff + worktree removal (moved from the issue Changes tab — the
// crate-canonical local-diff / clean-up primitives)
// ---------------------------------------------------------------------------

/// The live local diff of a worktree vs the default branch (v4 §4.8 tier 1):
/// `git diff <merge-base>` from the merge base of `origin/<default>` (or the
/// local `<default>`) and `HEAD`, capturing committed *and* uncommitted
/// tracked changes. Falls back to `git diff HEAD` when no base ref is present
/// (a freshly cut branch with no fetched upstream).
pub fn branch_diff(worktree: &Path, default_branch: &str) -> Result<Vec<DiffFile>, GitError> {
    let base = merge_base(worktree, &format!("origin/{default_branch}"))
        .or_else(|| merge_base(worktree, default_branch));
    let raw = match base {
        Some(base) => run_git(Some(worktree), &["diff", &base], None, "git diff")?,
        None => run_git(Some(worktree), &["diff", "HEAD"], None, "git diff HEAD")?,
    };
    Ok(parse_unified_diff(&raw))
}

/// `git merge-base <refspec> HEAD` → the base commit, or `None` when the ref
/// is absent (git exits non-zero).
fn merge_base(worktree: &Path, refspec: &str) -> Option<String> {
    run_git(Some(worktree), &["merge-base", refspec, "HEAD"], None, "git merge-base")
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|out| !out.is_empty())
}

/// Clean up an issue worktree (v4 §4.8): refuse a dirty tree, else
/// `git worktree remove` + prune + local branch delete (best-effort branch
/// delete — a checked-out or missing branch is not fatal).
pub fn remove_worktree(clone: &Path, worktree: &Path, branch: &str) -> Result<(), GitError> {
    if is_dirty(worktree).unwrap_or(false) {
        return Err(GitError {
            op: "clean up worktree".to_string(),
            detail: "Worktree has uncommitted changes — commit or discard them before cleaning up."
                .to_string(),
        });
    }
    let worktree_str = worktree.to_string_lossy().into_owned();
    run_git(Some(clone), &["worktree", "remove", &worktree_str], None, "git worktree remove")?;
    let _ = run_git(Some(clone), &["worktree", "prune"], None, "git worktree prune");
    let _ = run_git(Some(clone), &["branch", "-D", branch], None, "git branch -D");
    Ok(())
}

// ---------------------------------------------------------------------------
// Config (the one-time identity prompt + any future key)
// ---------------------------------------------------------------------------

/// `git config --get <key>` in ANY scope, empty string when unset/unreadable.
pub fn config_get(repo: &Path, key: &str) -> String {
    run_git(Some(repo), &["config", "--get", key], None, "git config --get")
        .map(|out| out.trim().to_string())
        .unwrap_or_default()
}

/// Write `<key> = <value>` to the **repo-local** config (`git config` with no
/// scope flag targets `.git/config`).
pub fn config_set_local(repo: &Path, key: &str, value: &str) -> Result<(), GitError> {
    run_git(Some(repo), &["config", key, value], None, "git config")?;
    Ok(())
}

/// Unmerged (conflicted) repo-relative paths: `git diff --name-only
/// --diff-filter=U` (works during both rebase and merge conflicts).
fn unmerged_files(repo: &Path) -> Vec<String> {
    run_git(Some(repo), &["diff", "--name-only", "--diff-filter=U"], None, "git diff --diff-filter=U")
        .map(|out| {
            out.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Pure parsers (unit-tested against fixture output — R2.a)
// ---------------------------------------------------------------------------

/// Parse `git branch --format=%(HEAD)%09%(refname:short)%09%(worktreepath)`
/// output: a `*` in the HEAD column marks the checked-out branch; a non-empty
/// worktree path on a NON-current branch means it lives in another worktree
/// (unswitchable here). The current branch is hoisted to the front regardless
/// of the sort the caller asked git for.
pub fn parse_branches(raw: &str) -> Vec<BranchInfo> {
    let mut out: Vec<BranchInfo> = raw
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let head = fields.next()?.trim();
            let name = fields.next()?.trim();
            let worktree_path = fields.next().unwrap_or("").trim();
            if name.is_empty() {
                return None;
            }
            let current = head == "*";
            Some(BranchInfo {
                name: name.to_string(),
                current,
                worktree: !current && !worktree_path.is_empty(),
            })
        })
        .collect();
    out.sort_by_key(|branch| !branch.current);
    out
}

/// Parse `git status --porcelain=v2 --branch` output. A path with both an index
/// (`X`) and a worktree (`Y`) change emits TWO [`FileChange`]s — one `staged`,
/// one not — so the Source Control screen's Staged/Changes split is faithful
/// (v4 §4.4). Paths are taken verbatim (unquoted): simple names only, which is
/// all the trunk UI feeds it.
pub fn parse_status(raw: &str) -> StatusSummary {
    let mut branch = String::new();
    let mut upstream = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let mut changes = Vec::new();

    for line in raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some(head) = rest.strip_prefix("branch.head ") {
                branch = head.trim().to_string();
            } else if let Some(up) = rest.strip_prefix("branch.upstream ") {
                upstream = Some(up.trim().to_string());
            } else if let Some(ab) = rest.strip_prefix("branch.ab ") {
                for tok in ab.split_whitespace() {
                    if let Some(a) = tok.strip_prefix('+') {
                        ahead = a.parse().unwrap_or(0);
                    } else if let Some(b) = tok.strip_prefix('-') {
                        behind = b.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("? ") {
            changes.push(FileChange {
                path: path.to_string(),
                status: FileStatus::Untracked,
                staged: false,
            });
        } else if line.starts_with("1 ") {
            // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            let parts: Vec<&str> = line.splitn(9, ' ').collect();
            if parts.len() == 9 {
                push_xy(&mut changes, parts[1], parts[8]);
            }
        } else if line.starts_with("2 ") {
            // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <Xscore> <path>\t<orig>
            let parts: Vec<&str> = line.splitn(10, ' ').collect();
            if parts.len() == 10 {
                let path = parts[9].split('\t').next().unwrap_or(parts[9]);
                push_xy(&mut changes, parts[1], path);
            }
        } else if line.starts_with("u ") {
            // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            let parts: Vec<&str> = line.splitn(11, ' ').collect();
            if parts.len() == 11 {
                changes.push(FileChange {
                    path: parts[10].to_string(),
                    status: FileStatus::Modified,
                    staged: false,
                });
            }
        }
        // '!' (ignored) lines never appear without --ignored; skip anything else.
    }

    StatusSummary { branch, upstream, ahead, behind, changes }
}

/// Emit staged/unstaged [`FileChange`]s from a porcelain-v2 `XY` code pair.
fn push_xy(changes: &mut Vec<FileChange>, xy: &str, path: &str) {
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    if x != '.' {
        changes.push(FileChange {
            path: path.to_string(),
            status: status_from_code(x),
            staged: true,
        });
    }
    if y != '.' {
        changes.push(FileChange {
            path: path.to_string(),
            status: status_from_code(y),
            staged: false,
        });
    }
}

/// Collapse a porcelain change code to the user-facing [`FileStatus`].
fn status_from_code(code: char) -> FileStatus {
    match code {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        // Copies present as a new file; type-changes/mode-changes as modified.
        'C' => FileStatus::Added,
        _ => FileStatus::Modified,
    }
}

/// Parse NUL-separated `git log -z --format=%H%x1f%s%x1f%an%x1f%cr` output.
pub fn parse_log(raw: &str) -> Vec<CommitInfo> {
    raw.split('\0')
        .map(|record| record.trim_start_matches(['\n', '\r']))
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(4, '\x1f');
            let hash = fields.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(CommitInfo {
                hash: hash.to_string(),
                subject: fields.next().unwrap_or("").to_string(),
                author: fields.next().unwrap_or("").to_string(),
                relative_time: fields.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse a unified diff (`git diff`/`git show` body) into per-file
/// [`DiffFile`]s. Any commit-header preamble (from `git show`) before the first
/// `diff --git` is ignored; header lines (`new file mode`, `rename …`, `Binary
/// files …`, `---`/`+++`) are only interpreted before a file's first `@@` hunk,
/// so body content that happens to begin with `---` is never mistaken for a
/// header.
pub fn parse_unified_diff(raw: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut cur: Option<DiffFile> = None;
    let mut old_ln = 0u32;
    let mut new_ln = 0u32;

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(done) = cur.take() {
                files.push(done);
            }
            cur = Some(DiffFile {
                path: parse_diff_git_new_path(rest),
                previous_path: None,
                status: FileStatus::Modified,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
                binary: false,
            });
            continue;
        }
        let Some(file) = cur.as_mut() else {
            continue; // preamble before the first file
        };

        // A hunk header can only be a line literally starting with "@@" — body
        // lines are always prefixed (' '/'+'/'-'), so this is unambiguous.
        if line.starts_with("@@") {
            if let Some((os, ol, ns, nl)) = parse_hunk_header(line) {
                old_ln = os;
                new_ln = ns;
                file.hunks.push(UnifiedHunk {
                    old_start: os,
                    old_lines: ol,
                    new_start: ns,
                    new_lines: nl,
                    header: line.to_string(),
                    lines: Vec::new(),
                });
            }
            continue;
        }

        if file.hunks.is_empty() {
            // File-header region (before any hunk).
            if line.starts_with("new file mode") {
                file.status = FileStatus::Added;
            } else if line.starts_with("deleted file mode") {
                file.status = FileStatus::Deleted;
            } else if let Some(p) = line.strip_prefix("rename from ") {
                file.previous_path = Some(p.to_string());
                file.status = FileStatus::Renamed;
            } else if let Some(p) = line.strip_prefix("rename to ") {
                file.path = p.to_string();
                file.status = FileStatus::Renamed;
            } else if let Some(p) = line.strip_prefix("copy from ") {
                file.previous_path = Some(p.to_string());
            } else if let Some(p) = line.strip_prefix("copy to ") {
                file.path = p.to_string();
            } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
                file.binary = true;
            } else if line == "--- /dev/null" {
                file.status = FileStatus::Added;
            } else if let Some(p) = line.strip_prefix("+++ ") {
                if p == "/dev/null" {
                    file.status = FileStatus::Deleted;
                } else if let Some(np) = p.strip_prefix("b/") {
                    if file.status != FileStatus::Renamed {
                        file.path = np.to_string();
                    }
                }
            }
            continue;
        }

        // Hunk body region.
        match line.chars().next() {
            Some('+') => {
                file.additions += 1;
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line: None,
                        new_line: Some(new_ln),
                        content: line[1..].to_string(),
                    });
                }
                new_ln += 1;
            }
            Some('-') => {
                file.deletions += 1;
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Deletion,
                        old_line: Some(old_ln),
                        new_line: None,
                        content: line[1..].to_string(),
                    });
                }
                old_ln += 1;
            }
            Some(' ') => {
                if let Some(hunk) = file.hunks.last_mut() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Context,
                        old_line: Some(old_ln),
                        new_line: Some(new_ln),
                        content: line[1..].to_string(),
                    });
                }
                old_ln += 1;
                new_ln += 1;
            }
            // "\ No newline at end of file" and blank separators: not lines.
            _ => {}
        }
    }

    if let Some(done) = cur.take() {
        files.push(done);
    }
    files
}

/// Best-effort new path from a `diff --git a/<old> b/<new>` remainder (a
/// fallback — the authoritative path comes from `+++`/`rename to`).
fn parse_diff_git_new_path(rest: &str) -> String {
    if let Some((_, b)) = rest.rsplit_once(" b/") {
        return b.to_string();
    }
    rest.trim().to_string()
}

/// Parse an `@@ -old_start[,old_lines] +new_start[,new_lines] @@ …` header into
/// its four numbers (a missing count defaults to 1).
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let after = line.strip_prefix("@@ ")?;
    let end = after.find(" @@")?;
    let mut ranges = after[..end].split(' ');
    let old = ranges.next()?.strip_prefix('-')?;
    let new = ranges.next()?.strip_prefix('+')?;
    let (os, ol) = parse_range(old);
    let (ns, nl) = parse_range(new);
    Some((os, ol, ns, nl))
}

fn parse_range(range: &str) -> (u32, u32) {
    let mut parts = range.split(',');
    let start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let count = parts.next().map(|c| c.parse().unwrap_or(1)).unwrap_or(1);
    (start, count)
}

// ---------------------------------------------------------------------------
// Tests — parsers against crafted git output AND live fixture repos (the R2.a
// phase gate: every parser + wrapper exercised against real `git` output).
// Hermetic: file:// remotes only, no network.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-scm-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    /// Run raw git, asserting success (fixture setup only).
    fn git(cwd: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Run raw git WITHOUT asserting success (conflict setup: rebase/merge
    /// intentionally exit non-zero).
    fn git_may_fail(cwd: &Path, args: &[&str]) {
        let _ = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).unwrap();
        git(path, &["init", "--quiet", "-b", "main"]);
        git(path, &["config", "user.email", "t@example.com"]);
        git(path, &["config", "user.name", "t"]);
        // Keep rebases non-interactive and deterministic under any user config.
        git(path, &["config", "commit.gpgsign", "false"]);
    }

    fn write(path: &Path, rel: &str, content: &str) {
        fs::write(path.join(rel), content).unwrap();
    }

    fn commit_all(path: &Path, msg: &str) {
        git(path, &["add", "-A"]);
        git(path, &["commit", "--quiet", "-m", msg]);
    }

    fn find<'a>(s: &'a StatusSummary, path: &str, staged: bool) -> &'a FileChange {
        s.changes
            .iter()
            .find(|c| c.path == path && c.staged == staged)
            .unwrap_or_else(|| panic!("no change for {path:?} staged={staged} in {:?}", s.changes))
    }

    // ---- parse_status ----

    #[test]
    fn parse_status_string_reads_branch_ab_and_kinds() {
        let raw = "\
# branch.oid abc123
# branch.head feature/x
# branch.upstream origin/feature/x
# branch.ab +3 -2
1 .M N... 100644 100644 100644 aa bb src/app.rs
1 A. N... 000000 100644 100644 00 cc src/new.rs
1 MM N... 100644 100644 100644 dd ee both.rs
2 R. N... 100644 100644 100644 ff gg R100 renamed.rs\told.rs
u UU N... 100644 100644 100644 100644 hh ii jj conflict.rs
? untracked.txt
";
        let s = parse_status(raw);
        assert_eq!(s.branch, "feature/x");
        assert_eq!(s.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!(s.ahead, 3);
        assert_eq!(s.behind, 2);

        assert_eq!(find(&s, "src/app.rs", false).status, FileStatus::Modified);
        assert_eq!(find(&s, "src/new.rs", true).status, FileStatus::Added);
        // MM ⇒ both a staged and an unstaged modification.
        assert_eq!(find(&s, "both.rs", true).status, FileStatus::Modified);
        assert_eq!(find(&s, "both.rs", false).status, FileStatus::Modified);
        // Rename: new path only, staged (X=R).
        assert_eq!(find(&s, "renamed.rs", true).status, FileStatus::Renamed);
        // Unmerged ⇒ single unstaged Modified entry.
        assert_eq!(find(&s, "conflict.rs", false).status, FileStatus::Modified);
        assert_eq!(find(&s, "untracked.txt", false).status, FileStatus::Untracked);
    }

    #[test]
    fn status_wrapper_reads_a_live_worktree() {
        let d = temp_dir("status");
        let r = &d.0;
        init_repo(r);
        write(r, "tracked.txt", "one\n");
        commit_all(r, "init");

        write(r, "tracked.txt", "two\n"); // modified, unstaged
        write(r, "staged.txt", "new\n");
        git(r, &["add", "staged.txt"]); // added, staged
        write(r, "untracked.txt", "u\n"); // untracked

        let s = status(r).unwrap();
        assert_eq!(s.branch, "main");
        assert_eq!(s.upstream, None); // no remote → unpublished
        assert_eq!((s.ahead, s.behind), (0, 0));
        assert_eq!(find(&s, "tracked.txt", false).status, FileStatus::Modified);
        assert_eq!(find(&s, "staged.txt", true).status, FileStatus::Added);
        assert_eq!(find(&s, "untracked.txt", false).status, FileStatus::Untracked);
    }

    #[test]
    fn status_wrapper_reports_a_live_rename() {
        let d = temp_dir("rename");
        let r = &d.0;
        init_repo(r);
        write(r, "old.txt", "content that is long enough to detect a rename\n");
        commit_all(r, "init");
        git(r, &["mv", "old.txt", "new.txt"]);

        let s = status(r).unwrap();
        let renamed = find(&s, "new.txt", true);
        assert_eq!(renamed.status, FileStatus::Renamed);
    }

    // ---- parse_branches ----

    #[test]
    fn parse_branches_marks_current_worktrees_and_hoists_current_first() {
        let raw = " \tfeature/x\t\n*\tmain\t/repo\n \texp/EXP-42\t/repo/.worktrees/exp-EXP-42\n";
        let branches = parse_branches(raw);
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].current);
        // The current branch's own worktree path never marks it unswitchable.
        assert!(!branches[0].worktree);
        assert_eq!(branches[1].name, "feature/x");
        assert!(!branches[1].current);
        assert!(!branches[1].worktree);
        // A session-worktree branch cannot be checked out here.
        assert_eq!(branches[2].name, "exp/EXP-42");
        assert!(branches[2].worktree);
    }

    #[test]
    fn branches_wrapper_reads_a_real_repo() {
        let d = temp_dir("branches");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "x");
        commit_all(r, "init");
        run_git(Some(r), &["branch", "other"], None, "git branch other").unwrap();
        let list = branches(r).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].current);
        assert!(list.iter().any(|b| b.name == "other" && !b.current));
    }

    // ---- parse_log ----

    #[test]
    fn parse_log_string_splits_records_and_fields() {
        let raw = "h1\x1ffix: login, & stuff\x1fDanny\x1f2 hours ago\0\
                   h2\x1fadd auth\x1fDanny\x1f1 day ago\0";
        let log = parse_log(raw);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].hash, "h1");
        assert_eq!(log[0].subject, "fix: login, & stuff");
        assert_eq!(log[0].author, "Danny");
        assert_eq!(log[0].relative_time, "2 hours ago");
        assert_eq!(log[1].hash, "h2");
        assert_eq!(log[1].subject, "add auth");
    }

    #[test]
    fn log_wrapper_pages_newest_first() {
        let d = temp_dir("log");
        let r = &d.0;
        init_repo(r);
        for msg in ["first", "second", "third"] {
            write(r, "f.txt", msg);
            commit_all(r, msg);
        }
        let all = log_branch(r, None, 0, 10).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].subject, "third"); // newest first
        assert_eq!(all[2].subject, "first");
        assert_eq!(all[0].author, "t");
        assert!(!all[0].relative_time.is_empty());

        // Paging: skip the newest, take one ⇒ the second-newest.
        let page = log_branch(r, None, 1, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].subject, "second");
    }

    // ---- parse_unified_diff ----

    #[test]
    fn parse_unified_diff_modification_tracks_line_numbers() {
        let raw = "\
diff --git a/file.txt b/file.txt
index 111..222 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 a
-b
+B
 c
";
        let files = parse_unified_diff(raw);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path, "file.txt");
        assert_eq!(f.status, FileStatus::Modified);
        assert_eq!((f.additions, f.deletions), (1, 1));
        assert!(!f.binary);
        let hunk = &f.hunks[0];
        assert_eq!((hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines), (1, 3, 1, 3));
        // Context "a" is line 1/1; "-b" is old line 2; "+B" is new line 2.
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[0].old_line, Some(1));
        assert_eq!(hunk.lines[0].new_line, Some(1));
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[1].old_line, Some(2));
        assert_eq!(hunk.lines[1].new_line, None);
        assert_eq!(hunk.lines[1].content, "b");
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Addition);
        assert_eq!(hunk.lines[2].new_line, Some(2));
        assert_eq!(hunk.lines[2].old_line, None);
        assert_eq!(hunk.lines[2].content, "B");
    }

    #[test]
    fn parse_unified_diff_body_line_starting_with_dashes_is_not_a_header() {
        // A removed line whose content is "-- foo" arrives as "--- foo": it
        // must be a Deletion, never mistaken for a `--- a/…` file header.
        let raw = "\
diff --git a/f b/f
--- a/f
+++ b/f
@@ -1,1 +1,1 @@
--- keep
+kept
";
        let f = &parse_unified_diff(raw)[0];
        assert_eq!(f.deletions, 1);
        assert_eq!(f.additions, 1);
        let hunk = &f.hunks[0];
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[0].content, "-- keep");
    }

    #[test]
    fn parse_unified_diff_added_and_deleted_files() {
        let added = "\
diff --git a/new.rs b/new.rs
new file mode 100644
index 000..111
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+line1
+line2
";
        let f = &parse_unified_diff(added)[0];
        assert_eq!(f.status, FileStatus::Added);
        assert_eq!(f.path, "new.rs");
        assert_eq!((f.additions, f.deletions), (2, 0));

        let deleted = "\
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
index 111..000
--- a/gone.rs
+++ /dev/null
@@ -1,1 +0,0 @@
-bye
";
        let f = &parse_unified_diff(deleted)[0];
        assert_eq!(f.status, FileStatus::Deleted);
        assert_eq!(f.path, "gone.rs");
        assert_eq!((f.additions, f.deletions), (0, 1));
    }

    #[test]
    fn parse_unified_diff_rename_and_binary() {
        let renamed = "\
diff --git a/old.rs b/new.rs
similarity index 90%
rename from old.rs
rename to new.rs
index 111..222 100644
--- a/old.rs
+++ b/new.rs
@@ -1,1 +1,1 @@
-a
+b
";
        let f = &parse_unified_diff(renamed)[0];
        assert_eq!(f.status, FileStatus::Renamed);
        assert_eq!(f.path, "new.rs");
        assert_eq!(f.previous_path.as_deref(), Some("old.rs"));

        let binary = "\
diff --git a/img.png b/img.png
index 111..222 100644
Binary files a/img.png and b/img.png differ
";
        let f = &parse_unified_diff(binary)[0];
        assert!(f.binary);
        assert!(f.hunks.is_empty());
        assert_eq!(f.path, "img.png");
    }

    #[test]
    fn parse_unified_diff_multiple_files_and_show_preamble() {
        // Emulate `git show`: commit header preamble before the first diff.
        let raw = "\
commit abc123
Author: Danny <d@e.com>

    subject line

diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,1 +1,1 @@
-x
+y
diff --git a/b.rs b/b.rs
new file mode 100644
--- /dev/null
+++ b/b.rs
@@ -0,0 +1,1 @@
+new
";
        let files = parse_unified_diff(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.rs");
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[1].path, "b.rs");
        assert_eq!(files[1].status, FileStatus::Added);
    }

    #[test]
    fn parse_hunk_header_defaults_missing_counts_to_one() {
        assert_eq!(parse_hunk_header("@@ -5 +7 @@ fn main"), Some((5, 1, 7, 1)));
        assert_eq!(parse_hunk_header("@@ -1,3 +1,4 @@"), Some((1, 3, 1, 4)));
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    // ---- working_diff / commit_diff wrappers ----

    #[test]
    fn working_diff_reflects_staged_vs_unstaged() {
        let d = temp_dir("wdiff");
        let r = &d.0;
        init_repo(r);
        write(r, "file.txt", "a\nb\nc\n");
        commit_all(r, "init");
        write(r, "file.txt", "a\nB\nc\n");

        let unstaged = working_diff(r, "file.txt", false).unwrap();
        assert_eq!(unstaged.status, FileStatus::Modified);
        assert_eq!((unstaged.additions, unstaged.deletions), (1, 1));

        // Nothing staged yet ⇒ empty (zero-hunk) diff on the cached side.
        let staged_empty = working_diff(r, "file.txt", true).unwrap();
        assert!(staged_empty.hunks.is_empty());

        git(r, &["add", "file.txt"]);
        let staged = working_diff(r, "file.txt", true).unwrap();
        assert_eq!((staged.additions, staged.deletions), (1, 1));
    }

    #[test]
    fn commit_diff_lists_the_commits_files() {
        let d = temp_dir("cdiff");
        let r = &d.0;
        init_repo(r);
        write(r, "base.txt", "base\n");
        commit_all(r, "init");
        write(r, "added.txt", "hello\nworld\n");
        commit_all(r, "add file");

        let files = commit_diff(r, "HEAD").unwrap();
        let added = files.iter().find(|f| f.path == "added.txt").unwrap();
        assert_eq!(added.status, FileStatus::Added);
        assert_eq!(added.additions, 2);
    }

    // ---- stage / unstage / commit ----

    #[test]
    fn stage_unstage_commit_round_trip() {
        let d = temp_dir("stage");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");
        write(r, "f.txt", "two\n");

        stage(r, "f.txt").unwrap();
        let s = status(r).unwrap();
        assert!(s.changes.iter().any(|c| c.path == "f.txt" && c.staged));

        unstage(r, "f.txt").unwrap();
        let s = status(r).unwrap();
        assert!(s.changes.iter().all(|c| !(c.path == "f.txt" && c.staged)));
        assert!(s.changes.iter().any(|c| c.path == "f.txt" && !c.staged));

        stage(r, "f.txt").unwrap();
        commit(r, "second").unwrap();
        let l = log_branch(r, None, 0, 10).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].subject, "second");
        assert!(status(r).unwrap().changes.is_empty());
    }

    // ---- conflict detection + abort ----

    #[test]
    fn detect_conflict_is_none_on_a_clean_repo() {
        let d = temp_dir("clean");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "x\n");
        commit_all(r, "init");
        assert!(detect_conflict(r).is_none());
    }

    #[test]
    fn detect_conflict_and_abort_a_rebase() {
        let d = temp_dir("rebase");
        let r = &d.0;
        init_repo(r);
        write(r, "x.txt", "base\n");
        commit_all(r, "base");

        git(r, &["checkout", "--quiet", "-b", "feature"]);
        write(r, "x.txt", "feature\n");
        commit_all(r, "feature change");

        git(r, &["checkout", "--quiet", "main"]);
        write(r, "x.txt", "main\n");
        commit_all(r, "main change");

        git(r, &["checkout", "--quiet", "feature"]);
        git_may_fail(r, &["rebase", "main"]); // conflicts on x.txt

        let conflict = detect_conflict(r).expect("rebase should be paused");
        assert_eq!(conflict.kind, ConflictKind::Rebase);
        assert!(conflict.files.contains(&"x.txt".to_string()), "{:?}", conflict.files);

        abort_conflict(r, ConflictKind::Rebase).unwrap();
        assert!(detect_conflict(r).is_none());
    }

    #[test]
    fn detect_conflict_and_abort_a_merge() {
        let d = temp_dir("merge");
        let r = &d.0;
        init_repo(r);
        write(r, "x.txt", "base\n");
        commit_all(r, "base");

        git(r, &["checkout", "--quiet", "-b", "feature"]);
        write(r, "x.txt", "feature\n");
        commit_all(r, "feature change");

        git(r, &["checkout", "--quiet", "main"]);
        write(r, "x.txt", "main\n");
        commit_all(r, "main change");

        git_may_fail(r, &["merge", "feature"]); // conflict ⇒ MERGE_HEAD

        let conflict = detect_conflict(r).expect("merge should be paused");
        assert_eq!(conflict.kind, ConflictKind::Merge);
        assert!(conflict.files.contains(&"x.txt".to_string()), "{:?}", conflict.files);

        abort_conflict(r, ConflictKind::Merge).unwrap();
        assert!(detect_conflict(r).is_none());
    }

    // ---- dirty check ----

    #[test]
    fn is_dirty_sees_tracked_and_untracked_changes() {
        let d = temp_dir("dirty");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");
        assert!(!is_dirty(r).unwrap());

        write(r, "untracked.txt", "u\n");
        assert!(is_dirty(r).unwrap());
        fs::remove_file(r.join("untracked.txt")).unwrap();

        write(r, "f.txt", "two\n");
        assert!(is_dirty(r).unwrap());
    }

    // ---- stashes ----

    #[test]
    fn parse_stash_list_reads_selector_and_subject() {
        let raw = "stash@{0}\x1fOn main: exp-switch: main\nstash@{1}\x1fWIP on feature/x: abc123 subject\n";
        let entries = parse_stash_list(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].index, 0);
        assert_eq!(entries[0].message, "On main: exp-switch: main");
        assert_eq!(entries[1].index, 1);
        assert_eq!(entries[1].message, "WIP on feature/x: abc123 subject");
    }

    #[test]
    fn stash_switch_branch_extracts_only_the_exp_switch_tag() {
        assert_eq!(
            stash_switch_branch("On main: exp-switch: feature/x"),
            Some("feature/x")
        );
        assert_eq!(stash_switch_branch(&stash_switch_message("main")), Some("main"));
        // User-made stashes never match.
        assert_eq!(stash_switch_branch("On main: my own stash"), None);
        assert_eq!(stash_switch_branch("WIP on main: abc123 subject"), None);
    }

    #[test]
    fn stash_push_list_pop_round_trip_including_untracked() {
        let d = temp_dir("stash");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");

        write(r, "f.txt", "dirty\n");
        write(r, "new.txt", "untracked\n");
        stash_push(r, &stash_switch_message("main")).unwrap();
        assert!(!is_dirty(r).unwrap());
        assert!(!r.join("new.txt").exists());

        let stashes = stash_list(r).unwrap();
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].index, 0);
        assert_eq!(stash_switch_branch(&stashes[0].message), Some("main"));

        stash_pop(r, 0).unwrap();
        assert_eq!(fs::read_to_string(r.join("f.txt")).unwrap(), "dirty\n");
        assert_eq!(fs::read_to_string(r.join("new.txt")).unwrap(), "untracked\n");
        assert!(stash_list(r).unwrap().is_empty());
    }

    #[test]
    fn stash_drop_discards_without_applying() {
        let d = temp_dir("stashdrop");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");
        write(r, "f.txt", "dirty\n");
        stash_push(r, &stash_switch_message("main")).unwrap();

        stash_drop(r, 0).unwrap();
        assert!(stash_list(r).unwrap().is_empty());
        assert_eq!(fs::read_to_string(r.join("f.txt")).unwrap(), "one\n");
    }

    // ---- checkout-refusal classifier ----

    #[test]
    fn checkout_blocked_classifier_matches_gits_clobber_refusals() {
        // C-locale fixtures (the runner forces LC_ALL=C).
        assert!(checkout_blocked_by_local_changes(
            "error: Your local changes to the following files would be overwritten by checkout:\n\tf.txt\nPlease commit your changes or stash them before you switch branches.\nAborting"
        ));
        assert!(checkout_blocked_by_local_changes(
            "error: The following untracked working tree files would be overwritten by checkout:\n\tnew.txt\nPlease move or remove them before you switch branches.\nAborting"
        ));
        // Anything else keeps the plain fail-with-message path.
        assert!(!checkout_blocked_by_local_changes(
            "error: pathspec 'nope' did not match any file(s) known to git"
        ));
        assert!(!checkout_blocked_by_local_changes("fatal: not a git repository"));
    }

    #[test]
    fn checkout_refusal_from_a_real_clobber_is_classified() {
        let d = temp_dir("clobber");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "main\n");
        commit_all(r, "init");
        git(r, &["checkout", "--quiet", "-b", "feature"]);
        write(r, "f.txt", "feature\n");
        commit_all(r, "feature edit");
        git(r, &["checkout", "--quiet", "main"]);
        // Local edit that the switch to `feature` would clobber.
        write(r, "f.txt", "local dirt\n");

        let err = checkout(r, "feature").unwrap_err();
        assert!(
            checkout_blocked_by_local_changes(&err.detail),
            "unclassified: {}",
            err.detail
        );
    }

    // ---- branch_diff (moved from the issue Changes tab) ----

    #[test]
    fn branch_diff_spans_committed_and_uncommitted_changes() {
        let d = temp_dir("branchdiff");
        let r = &d.0;
        init_repo(r);
        write(r, "base.txt", "base\n");
        commit_all(r, "init");

        git(r, &["checkout", "--quiet", "-b", "exp/EXP-1"]);
        write(r, "committed.txt", "one\n");
        commit_all(r, "committed change");
        write(r, "base.txt", "uncommitted edit\n");

        // No origin/main here → falls back to the local `main` merge base.
        let files = branch_diff(r, "main").unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"committed.txt"), "{paths:?}");
        assert!(paths.contains(&"base.txt"), "{paths:?}");
    }

    // ---- remove_worktree (moved from the issue Changes tab) ----

    #[test]
    fn remove_worktree_refuses_dirty_then_removes_clean() {
        let d = temp_dir("rmworktree");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "x\n");
        commit_all(r, "init");

        let worktree = d.0.join("wt-exp-EXP-9");
        git(r, &["worktree", "add", "-b", "exp/EXP-9", worktree.to_str().unwrap()]);

        write(&worktree, "dirt.txt", "dirty\n");
        let err = remove_worktree(r, &worktree, "exp/EXP-9").unwrap_err();
        assert!(err.detail.contains("uncommitted changes"), "{err}");
        assert!(worktree.exists());

        fs::remove_file(worktree.join("dirt.txt")).unwrap();
        remove_worktree(r, &worktree, "exp/EXP-9").unwrap();
        assert!(!worktree.exists());
        // The local branch went with it.
        let branches = branches(r).unwrap();
        assert!(branches.iter().all(|b| b.name != "exp/EXP-9"), "{branches:?}");
    }

    // ---- config helpers ----

    #[test]
    fn config_get_and_set_local_round_trip() {
        let d = temp_dir("config");
        let r = &d.0;
        init_repo(r);
        assert_eq!(config_get(r, "exp.someKey"), "");
        config_set_local(r, "exp.someKey", "value 1").unwrap();
        assert_eq!(config_get(r, "exp.someKey"), "value 1");
    }
}
