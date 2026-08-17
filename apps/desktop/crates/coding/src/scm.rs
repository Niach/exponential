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

use crate::git_worktree::{run_git, GitError, TokenUrl};

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
/// the change is in the index — reported only; the IDE is view-only (EXP-253),
/// so there is no staging affordance and nothing here calls `git add` /
/// `git restore --staged`.
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
/// `git log --format` (hash, parents, subject, author, relative time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    /// Parent hashes (`%P`, whitespace-split): empty for a root commit, 2+
    /// for a merge. Drives the history pane's lane graph (EXP-509).
    pub parents: Vec<String>,
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
    run_log(repo, &[branch.unwrap_or("HEAD")], skip, limit)
}

/// The history pane's graph walk (EXP-518): HEAD plus every remote-tracking
/// ref, so squash-merged PR branches (kept on origin after merge) render as
/// side lanes instead of the trunk's straight line. The explicit `HEAD` keeps
/// a remote-less repo walking; paging and wire format match [`log_branch`].
pub fn log_graph(repo: &Path, skip: usize, limit: usize) -> Result<Vec<CommitInfo>, GitError> {
    run_log(repo, &["HEAD", "--remotes"], skip, limit)
}

/// Shared `git log` body. `--date-order` turns the lane layout's
/// children-before-parents assumption into a git guarantee ("no parents
/// before all of its children") — the default reverse-chronological order
/// only holds it accidentally and clock skew (rebases) can violate it.
fn run_log(
    repo: &Path,
    revs: &[&str],
    skip: usize,
    limit: usize,
) -> Result<Vec<CommitInfo>, GitError> {
    let skip_arg = format!("--skip={skip}");
    let max_arg = format!("--max-count={limit}");
    let mut args = vec![
        "log",
        "-z",
        "--format=%H%x1f%P%x1f%s%x1f%an%x1f%cr",
        "--date-order",
        &skip_arg,
        &max_arg,
    ];
    args.extend_from_slice(revs);
    let raw = run_git(Some(repo), &args, None, "git log")?;
    Ok(parse_log(&raw))
}

/// HEAD's commit hash (`git rev-parse HEAD`) — seeds the graph layout's trunk
/// lane (EXP-518). Errors on an unborn HEAD; callers treat that as "no seed".
pub fn head_hash(repo: &Path) -> Result<String, GitError> {
    let raw = run_git(Some(repo), &["rev-parse", "HEAD"], None, "git rev-parse")?;
    Ok(raw.trim().to_string())
}

/// Every remote-tracking branch tip as `(branch name, commit hash)` — the
/// branch name WITHOUT its remote prefix (`origin/exp/EXP-7` → `exp/EXP-7`),
/// symbolic `<remote>/HEAD` entries skipped. Feeds the history graph's
/// squash-merge links (EXP-537): the tips are matched against synced PR
/// branches to close merged lanes back into the trunk.
pub fn remote_branch_tips(repo: &Path) -> Result<Vec<(String, String)>, GitError> {
    // NB: for-each-ref hex escapes are `%1f`, not the pretty-format `%x1f`.
    let raw = run_git(
        Some(repo),
        &["for-each-ref", "--format=%(refname:short)%1f%(objectname)", "refs/remotes"],
        None,
        "git for-each-ref",
    )?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let (name, hash) = line.trim().split_once('\x1f')?;
            // Drop the remote segment; a bare remote name (the `origin/HEAD`
            // symref shortens to `origin`) or an explicit HEAD is skipped.
            let branch = name.split_once('/').map(|(_, rest)| rest)?;
            if branch.is_empty() || branch == "HEAD" || hash.is_empty() {
                return None;
            }
            Some((branch.to_string(), hash.to_string()))
        })
        .collect())
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

/// The EXP-253 escape hatch's git core: `git checkout -f <branch>` then
/// `git reset --hard origin/<branch>`. Discards local TRACKED changes only —
/// deliberately NO `clean -fd`, so untracked files survive (this recovers
/// the trunk, it is not a nuke). Callers abort any paused rebase/merge and
/// fetch first; the branch is the server-reported default (never a
/// fabricated `main`) — and server-supplied, so a flag-shaped name is
/// rejected before it reaches argv ([`crate::git_worktree::validate_branch_arg`]).
pub fn hard_reset_to_remote(repo: &Path, branch: &str) -> Result<(), GitError> {
    crate::git_worktree::validate_branch_arg(branch, "git checkout -f")?;
    run_git(
        Some(repo),
        &["checkout", "-f", branch],
        None,
        "git checkout -f",
    )?;
    run_git(
        Some(repo),
        &["reset", "--hard", &format!("origin/{branch}")],
        None,
        "git reset --hard",
    )?;
    Ok(())
}

/// Rebase the checked-out branch onto `origin/<branch>` — the EXP-516
/// commit-and-push catch-up: a trunk both ahead and behind can never push
/// fast-forward, so the push path integrates the remote first (the guided
/// `pull --rebase`). Local + tokenless (callers [`crate::clone_manager::fetch`]
/// first). A conflicted rebase pauses in place — callers surface it via
/// [`detect_conflict`] and the Source Control conflict banner, never
/// auto-abort.
pub fn rebase_onto_remote(repo: &Path, branch: &str) -> Result<(), GitError> {
    crate::git_worktree::validate_branch_arg(branch, "git rebase")?;
    run_git(
        Some(repo),
        &["rebase", &format!("origin/{branch}")],
        None,
        "git rebase",
    )?;
    Ok(())
}

/// Commits on HEAD not on `origin/<branch>`: `git rev-list origin/<b>..HEAD`,
/// newest first. Exact (unlike "the first `ahead` log rows" — a local merge
/// interleaves by date); drives the history pane's unpushed muting (EXP-509).
pub fn unpushed_hashes(repo: &Path, branch: &str) -> Result<Vec<String>, GitError> {
    crate::git_worktree::validate_branch_arg(branch, "git rev-list")?;
    let raw = run_git(
        Some(repo),
        &["rev-list", &format!("origin/{branch}..HEAD")],
        None,
        "git rev-list",
    )?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Cap for inlining an untracked file's contents into the synthesized diff —
/// past it (or non-UTF-8) the file renders as binary ("No textual diff").
const UNTRACKED_DIFF_CAP: u64 = 1024 * 1024;

/// The whole working tree vs HEAD in one patch (`git diff HEAD` — staged and
/// unstaged together), plus synthesized all-addition [`DiffFile`]s for
/// untracked paths (`git ls-files --others --exclude-standard`) so brand-new
/// files show too. The "what changed" view for uncommitted work (EXP-509).
/// An unborn HEAD (no commits yet) degrades to the untracked synthesis alone.
pub fn working_tree_diff(repo: &Path) -> Result<Vec<DiffFile>, GitError> {
    let mut files = match run_git(Some(repo), &["diff", "HEAD"], None, "git diff HEAD") {
        Ok(raw) => parse_unified_diff(&raw),
        // Unborn HEAD — `git diff HEAD` cannot resolve the rev; a genuinely
        // broken repo still errors on the ls-files below.
        Err(_) => Vec::new(),
    };
    let untracked = run_git(
        Some(repo),
        &["ls-files", "--others", "--exclude-standard"],
        None,
        "git ls-files --others",
    )?;
    for path in untracked.lines().map(str::trim).filter(|l| !l.is_empty()) {
        files.push(untracked_diff_file(repo, path));
    }
    Ok(files)
}

/// Synthesize the all-addition [`DiffFile`] for one untracked path (git emits
/// no diff for files it does not track).
fn untracked_diff_file(repo: &Path, path: &str) -> DiffFile {
    let abs = repo.join(path);
    let text = std::fs::metadata(&abs)
        .ok()
        .filter(|meta| meta.len() <= UNTRACKED_DIFF_CAP)
        .and_then(|_| std::fs::read(&abs).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let Some(text) = text else {
        return DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: FileStatus::Added,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            binary: true,
        };
    };
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop(); // trailing newline is not a line
    }
    let count = lines.len() as u32;
    let hunks = if count == 0 {
        Vec::new()
    } else {
        vec![UnifiedHunk {
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: count,
            header: format!("@@ -0,0 +1,{count} @@"),
            lines: lines
                .iter()
                .enumerate()
                .map(|(ix, content)| DiffLine {
                    kind: DiffLineKind::Addition,
                    old_line: None,
                    new_line: Some(ix as u32 + 1),
                    content: (*content).to_string(),
                })
                .collect(),
        }]
    };
    DiffFile {
        path: path.to_string(),
        previous_path: None,
        status: FileStatus::Added,
        additions: count,
        deletions: 0,
        hunks,
        binary: false,
    }
}

/// `git add -A` + `git commit -m <message>` — the EXP-509 commit-and-push
/// affordance's commit half. Clones carry no git identity of their own, so
/// when neither local nor global `user.email` resolves, the signed-in
/// account's `identity` (name, email) rides `-c` overrides; identity needed
/// but absent is an error before anything is staged. The message is safe as
/// argv: it is always the VALUE consumed by `-m`.
pub fn stage_all_and_commit(
    repo: &Path,
    message: &str,
    identity: Option<(&str, &str)>,
) -> Result<(), GitError> {
    let has_identity = run_git(
        Some(repo),
        &["config", "--get", "user.email"],
        None,
        "git config user.email",
    )
    .map(|v| !v.trim().is_empty())
    .unwrap_or(false);
    let identity = if has_identity {
        None
    } else {
        match identity {
            Some(identity) => Some(identity),
            None => {
                return Err(GitError {
                    op: "git commit".to_string(),
                    detail: "no git identity (user.email) configured".to_string(),
                })
            }
        }
    };
    run_git(Some(repo), &["add", "-A"], None, "git add -A")?;
    match identity {
        Some((name, email)) => {
            let name_arg = format!("user.name={name}");
            let email_arg = format!("user.email={email}");
            run_git(
                Some(repo),
                &["-c", &name_arg, "-c", &email_arg, "commit", "-m", message],
                None,
                "git commit",
            )?;
        }
        None => {
            run_git(Some(repo), &["commit", "-m", message], None, "git commit")?;
        }
    }
    Ok(())
}

/// `git push origin <branch>` — the EXP-509 push-upstream affordance. Auth
/// rides the clone's ambient credential helper (EXP-73 — callers run
/// [`crate::git_credentials::ensure`] first); `url` is passed for error
/// scrubbing only. A rejected push (non-fast-forward) surfaces git's message.
pub fn push(repo: &Path, url: &TokenUrl, branch: &str) -> Result<(), GitError> {
    crate::git_worktree::validate_branch_arg(branch, "git push")?;
    run_git(Some(repo), &["push", "origin", branch], Some(url), "git push")?;
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

/// Parse NUL-separated `git log -z --format=%H%x1f%P%x1f%s%x1f%an%x1f%cr`
/// output. `%P` is space-separated parent hashes (empty for a root commit).
pub fn parse_log(raw: &str) -> Vec<CommitInfo> {
    raw.split('\0')
        .map(|record| record.trim_start_matches(['\n', '\r']))
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(5, '\x1f');
            let hash = fields.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(CommitInfo {
                hash: hash.to_string(),
                parents: fields
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
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

    // ---- parse_log ----

    #[test]
    fn parse_log_string_splits_records_and_fields() {
        // h1 = a merge (two parents), h2 = ordinary, h3 = root (empty %P).
        let raw = "h1\x1fh2 hx\x1ffix: login, & stuff\x1fDanny\x1f2 hours ago\0\
                   h2\x1fh3\x1fadd auth\x1fDanny\x1f1 day ago\0\
                   h3\x1f\x1finit\x1fDanny\x1f2 days ago\0";
        let log = parse_log(raw);
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].hash, "h1");
        assert_eq!(log[0].parents, vec!["h2".to_string(), "hx".to_string()]);
        assert_eq!(log[0].subject, "fix: login, & stuff");
        assert_eq!(log[0].author, "Danny");
        assert_eq!(log[0].relative_time, "2 hours ago");
        assert_eq!(log[1].hash, "h2");
        assert_eq!(log[1].parents, vec!["h3".to_string()]);
        assert_eq!(log[1].subject, "add auth");
        assert!(log[2].parents.is_empty());
        assert_eq!(log[2].subject, "init");
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
        // Parents chain the linear history; the root has none.
        assert_eq!(all[0].parents, vec![all[1].hash.clone()]);
        assert_eq!(all[1].parents, vec![all[2].hash.clone()]);
        assert!(all[2].parents.is_empty());

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
    fn log_wrapper_reports_merge_parents() {
        let d = temp_dir("merge-log");
        let r = &d.0;
        init_repo(r);
        write(r, "x.txt", "base\n");
        commit_all(r, "base");
        git(r, &["checkout", "--quiet", "-b", "feature"]);
        write(r, "y.txt", "feature\n");
        commit_all(r, "feature change");
        git(r, &["checkout", "--quiet", "main"]);
        write(r, "z.txt", "main\n");
        commit_all(r, "main change");
        git(r, &["merge", "--quiet", "--no-ff", "-m", "merge feature", "feature"]);

        let all = log_branch(r, None, 0, 10).unwrap();
        assert_eq!(all[0].subject, "merge feature");
        assert_eq!(all[0].parents.len(), 2);
        // Every parent points at a loaded commit.
        for parent in &all[0].parents {
            assert!(all.iter().any(|c| &c.hash == parent));
        }
    }

    #[test]
    fn working_tree_diff_covers_tracked_staged_and_untracked() {
        let d = temp_dir("wtdiff");
        let r = &d.0;
        init_repo(r);
        write(r, "tracked.txt", "a\nb\n");
        commit_all(r, "init");

        write(r, "tracked.txt", "a\nB\n"); // unstaged modification
        write(r, "staged.txt", "s\n");
        git(r, &["add", "staged.txt"]); // staged addition
        write(r, "untracked.txt", "u1\nu2\n"); // untracked

        let files = working_tree_diff(r).unwrap();
        let tracked = files.iter().find(|f| f.path == "tracked.txt").unwrap();
        assert_eq!((tracked.additions, tracked.deletions), (1, 1));
        let staged = files.iter().find(|f| f.path == "staged.txt").unwrap();
        assert_eq!(staged.status, FileStatus::Added);
        let untracked = files.iter().find(|f| f.path == "untracked.txt").unwrap();
        assert_eq!(untracked.status, FileStatus::Added);
        assert_eq!(untracked.additions, 2);
        assert!(!untracked.binary);
        assert_eq!(untracked.hunks[0].lines[0].content, "u1");
        assert_eq!(untracked.hunks[0].lines[0].new_line, Some(1));
    }

    #[test]
    fn working_tree_diff_marks_non_utf8_untracked_as_binary() {
        let d = temp_dir("wtbin");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "x\n");
        commit_all(r, "init");
        fs::write(r.join("blob.bin"), [0u8, 159, 146, 150]).unwrap();

        let files = working_tree_diff(r).unwrap();
        let bin = files.iter().find(|f| f.path == "blob.bin").unwrap();
        assert!(bin.binary);
        assert!(bin.hunks.is_empty());
    }

    #[test]
    fn commit_push_roundtrip_and_unpushed_hashes() {
        let remote_dir = temp_dir("push-remote");
        let bare = remote_dir.0.join("origin.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);

        let d = temp_dir("push-work");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");
        git(r, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(r, &["push", "--quiet", "-u", "origin", "main"]);

        let url = TokenUrl::new("acme/web", "tok");
        assert!(unpushed_hashes(r, "main").unwrap().is_empty());

        // Dirty the tree, commit via the wrapper, verify it is unpushed.
        write(r, "f.txt", "two\n");
        write(r, "new.txt", "n\n");
        stage_all_and_commit(r, "local changes", None).unwrap();
        let unpushed = unpushed_hashes(r, "main").unwrap();
        assert_eq!(unpushed.len(), 1);
        let s = status(r).unwrap();
        assert!(s.changes.is_empty(), "{:?}", s.changes);

        // Push through the wrapper; the remote-tracking ref catches up.
        push(r, &url, "main").unwrap();
        git(r, &["fetch", "--quiet", "origin"]); // refresh origin/main locally
        assert!(unpushed_hashes(r, "main").unwrap().is_empty());
        let log = log_branch(r, None, 0, 10).unwrap();
        assert_eq!(log[0].subject, "local changes");
    }

    #[test]
    fn rebase_onto_remote_integrates_a_diverged_trunk() {
        // The EXP-516 shape: one local commit AND one remote commit on main.
        let remote_dir = temp_dir("rebase-remote");
        let bare = remote_dir.0.join("origin.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);

        let d = temp_dir("rebase-work");
        let r = &d.0;
        init_repo(r);
        write(r, "base.txt", "base\n");
        commit_all(r, "base");
        git(r, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(r, &["push", "--quiet", "-u", "origin", "main"]);

        // Advance origin out-of-band via a second clone.
        let c = temp_dir("rebase-consumer");
        let consumer = c.0.join("consumer");
        git(&c.0, &["clone", "--quiet", bare.to_str().unwrap(), consumer.to_str().unwrap()]);
        git(&consumer, &["config", "user.email", "c@example.com"]);
        git(&consumer, &["config", "user.name", "c"]);
        write(&consumer, "remote.txt", "r\n");
        commit_all(&consumer, "remote work");
        git(&consumer, &["push", "--quiet", "origin", "main"]);

        // Diverge locally, fetch, rebase: both commits land, linear history.
        write(r, "local.txt", "l\n");
        commit_all(r, "local work");
        git(r, &["fetch", "--quiet", "origin"]);
        rebase_onto_remote(r, "main").unwrap();
        assert!(r.join("remote.txt").exists());
        assert!(r.join("local.txt").exists());
        let s = status(r).unwrap();
        assert_eq!((s.ahead, s.behind), (1, 0));
        // The rebased local commit pushes fast-forward now.
        let url = TokenUrl::new("acme/web", "tok");
        push(r, &url, "main").unwrap();
    }

    #[test]
    fn rebase_onto_remote_pauses_on_conflict_for_the_banner() {
        let remote_dir = temp_dir("rebconf-remote");
        let bare = remote_dir.0.join("origin.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);

        let d = temp_dir("rebconf-work");
        let r = &d.0;
        init_repo(r);
        write(r, "x.txt", "base\n");
        commit_all(r, "base");
        git(r, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(r, &["push", "--quiet", "-u", "origin", "main"]);

        let c = temp_dir("rebconf-consumer");
        let consumer = c.0.join("consumer");
        git(&c.0, &["clone", "--quiet", bare.to_str().unwrap(), consumer.to_str().unwrap()]);
        git(&consumer, &["config", "user.email", "c@example.com"]);
        git(&consumer, &["config", "user.name", "c"]);
        write(&consumer, "x.txt", "remote\n");
        commit_all(&consumer, "remote edit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);

        write(r, "x.txt", "local\n");
        commit_all(r, "local edit");
        git(r, &["fetch", "--quiet", "origin"]);

        // Both edited x.txt → the rebase errors and PAUSES (never aborts):
        // detect_conflict picks it up for the Source Control banner.
        assert!(rebase_onto_remote(r, "main").is_err());
        let conflict = detect_conflict(r).expect("rebase should be paused");
        assert_eq!(conflict.kind, ConflictKind::Rebase);
        assert!(conflict.files.contains(&"x.txt".to_string()), "{:?}", conflict.files);
        abort_conflict(r, ConflictKind::Rebase).unwrap();
        assert!(detect_conflict(r).is_none());
    }

    #[test]
    fn log_graph_includes_remote_branch_commits() {
        // A pushed feature branch (the kept PR branch shape, EXP-518): its
        // commit is invisible to the HEAD-only walk but present in the graph
        // walk via origin/feature.
        let remote_dir = temp_dir("graph-remote");
        let bare = remote_dir.0.join("origin.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);

        let d = temp_dir("graph-work");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "base");
        git(r, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(r, &["push", "--quiet", "-u", "origin", "main"]);
        git(r, &["checkout", "--quiet", "-b", "feature"]);
        write(r, "f.txt", "feature\n");
        commit_all(r, "feature work");
        git(r, &["push", "--quiet", "origin", "feature"]);
        git(r, &["checkout", "--quiet", "main"]);
        git(r, &["branch", "--quiet", "-D", "feature"]);
        write(r, "g.txt", "trunk\n");
        commit_all(r, "trunk work");

        let head_only = log_branch(r, None, 0, 10).unwrap();
        assert!(head_only.iter().all(|c| c.subject != "feature work"));

        let graph = log_graph(r, 0, 10).unwrap();
        assert_eq!(graph.len(), 3);
        let feature = graph.iter().find(|c| c.subject == "feature work").unwrap();
        let base = graph.iter().find(|c| c.subject == "base").unwrap();
        assert_eq!(feature.parents, vec![base.hash.clone()]);
        // The local-only trunk commit (never pushed) is walked via HEAD.
        assert!(graph.iter().any(|c| c.subject == "trunk work"));

        // Paging skips across the merged multi-ref stream.
        let rest = log_graph(r, 1, 10).unwrap();
        assert_eq!(rest.len(), 2);
        assert_eq!(rest[1].subject, "base");
    }

    #[test]
    fn remote_branch_tips_lists_tips_without_the_remote_prefix() {
        let remote_dir = temp_dir("tips-remote");
        let bare = remote_dir.0.join("origin.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);

        let d = temp_dir("tips-work");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "base");
        git(r, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(r, &["push", "--quiet", "-u", "origin", "main"]);
        git(r, &["checkout", "--quiet", "-b", "exp/EXP-7"]);
        write(r, "f.txt", "feature\n");
        commit_all(r, "feature work");
        git(r, &["push", "--quiet", "origin", "exp/EXP-7"]);
        let feature_tip = head_hash(r).unwrap();
        git(r, &["checkout", "--quiet", "main"]);
        git(r, &["branch", "--quiet", "-D", "exp/EXP-7"]);
        let main_tip = head_hash(r).unwrap();

        let tips = remote_branch_tips(r).unwrap();
        assert!(tips.contains(&("exp/EXP-7".to_string(), feature_tip)));
        assert!(tips.contains(&("main".to_string(), main_tip)));
        assert!(tips.iter().all(|(branch, _)| !branch.starts_with("origin/")));

        // A remote-less repo has no tips (and no error).
        let plain = temp_dir("tips-plain");
        init_repo(&plain.0);
        write(&plain.0, "f.txt", "x\n");
        commit_all(&plain.0, "init");
        assert!(remote_branch_tips(&plain.0).unwrap().is_empty());
    }

    #[test]
    fn log_graph_without_remotes_is_head_only() {
        // The explicit HEAD rev keeps a remote-less repo walking.
        let d = temp_dir("graph-plain");
        let r = &d.0;
        init_repo(r);
        for msg in ["first", "second"] {
            write(r, "f.txt", msg);
            commit_all(r, msg);
        }
        assert_eq!(log_graph(r, 0, 10).unwrap(), log_branch(r, None, 0, 10).unwrap());
    }

    #[test]
    fn head_hash_resolves_the_tip() {
        let d = temp_dir("head-hash");
        let r = &d.0;
        init_repo(r);
        write(r, "f.txt", "one\n");
        commit_all(r, "init");
        let head = head_hash(r).unwrap();
        assert_eq!(head, log_branch(r, None, 0, 1).unwrap()[0].hash);
    }

    #[test]
    fn stage_all_and_commit_applies_identity_when_repo_has_none() {
        let d = temp_dir("identity");
        let r = &d.0;
        // No init_repo: configure NOTHING locally, and blank the global
        // config via env so the identity fallback is actually exercised.
        fs::create_dir_all(r).unwrap();
        let git_env = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(r)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        git_env(&["init", "--quiet", "-b", "main"]);
        write(r, "f.txt", "x\n");

        // run_git inherits the test process env — blank the globals here too
        // so `git config --get user.email` resolves nothing. Serial-safe:
        // set_var is process-wide, so restore before returning.
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_SYSTEM", "/dev/null");
        let missing = stage_all_and_commit(r, "no identity", None);
        let applied = stage_all_and_commit(r, "with identity", Some(("Danny", "d@example.com")));
        std::env::remove_var("GIT_CONFIG_GLOBAL");
        std::env::remove_var("GIT_CONFIG_SYSTEM");

        assert!(missing.is_err());
        applied.unwrap();
        let author = git_env(&["log", "-1", "--format=%an <%ae>"]);
        assert_eq!(author.trim(), "Danny <d@example.com>");
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

    // ---- config helpers ----

}
