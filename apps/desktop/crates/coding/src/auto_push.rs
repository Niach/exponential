//! #EXP-1618: a session's work never lives only on one machine.
//!
//! Before this, the device pushed a session's branch only when the AGENT
//! did, and parked what was left on `exp/leftover-<id8>` when the session
//! ENDED. A device that went offline (or a session moved off one) took
//! every unpushed commit and every uncommitted edit with it: the
//! continuation on another device started over.
//!
//! Now the device pushes on its own, all the time, but ONLY to a hidden
//! ref, never to the branch (#EXP-1622: a pushed branch with an open PR got
//! commits the agent had not released and restarted its CI; the branch is
//! the agent's to push):
//!
//! - **the commits** ([`branch_state`]): whenever `HEAD` moved and carries
//!   commits `origin`'s branch lacks, they ride the snapshot below (or, on a
//!   clean tree, `HEAD` itself is parked on the ref);
//! - **the uncommitted work** ([`push_wip`]): a snapshot commit of the
//!   worktree (tracked changes and new files, never ignored ones) built in a
//!   TEMPORARY index (the agent's index, staging and `HEAD` are never
//!   touched), parented on `HEAD`, force-pushed to the hidden ref
//!   `refs/exp-wip/<branch>` (no branch in GitHub's list, no workflow run).
//!   Secret-looking files, big files and token-bearing contents stay out
//!   ([`secret_path`], [`secret_content`]);
//! - **when**: a 30 s `HEAD` check (after every agent commit), a snapshot
//!   every 3 minutes ([`AutoPusher`]), and on every run exit and every host
//!   shutdown ([`sync_worktree`], [`sync_live_runs`]): stop, interrupt, a
//!   move's kill, a quit, a daemon stop.
//!
//! A worktree prepared for the session elsewhere (a move, a resume on a
//! reclaimed worktree) continues from the pushed branch, fast-forwards to
//! the commits the snapshot carries AND puts the snapshot back as
//! uncommitted changes ([`restore_wip`]). The leftover
//! branch stays the last resort for what never reached origin (no network
//! at the end).
//!
//! Everything here is best effort and logged: a failed push is retried on
//! the next pass, never surfaced as a session error.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::git_worktree::{origin_has_ref, run_git, TokenUrl};

/// The hidden ref namespace a branch's uncommitted-work snapshot lives in,
/// on `origin` and (as "what was last pushed") in the clone.
pub const WIP_REF_PREFIX: &str = "refs/exp-wip/";

/// The subject every snapshot commit carries ([`restore_wip`] tells a
/// snapshot from a plain `HEAD` parked on the ref by it).
pub const WIP_SNAPSHOT_SUBJECT: &str = "Exponential WIP snapshot";

/// How often a live run's `HEAD` is checked (a commit is pushed within it).
pub const HEAD_CHECK_EVERY: Duration = Duration::from_secs(30);

/// How often a live run's uncommitted work is snapshotted.
pub const WIP_EVERY: Duration = Duration::from_secs(3 * 60);

/// A file bigger than this stays out of a snapshot (build output, media).
pub const MAX_WIP_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// More changed paths than this: only the TRACKED ones are snapshotted (a
/// flood of new files is build output nobody ignored, not work).
pub const MAX_WIP_PATHS: usize = 2000;

/// The identity snapshot commits are made with (never the agent's: they are
/// the device's, and a worktree without one must not fail them).
const WIP_NAME: &str = "Exponential";
const WIP_EMAIL: &str = "noreply@exponential.local";

/// `refs/exp-wip/<branch>`.
pub fn wip_ref(branch: &str) -> String {
    format!("{WIP_REF_PREFIX}{branch}")
}

/// Where `HEAD` stands against `origin`'s branch ([`branch_state`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchPush {
    /// No git work tree here.
    NotARepo,
    /// `HEAD` is detached, unborn, or on another branch than the session's.
    NotOnBranch,
    /// The session's branch is the repository's default (or its base): the
    /// device never pushes there.
    Protected,
    /// A rebase, merge, cherry-pick or revert is in progress: not now.
    Busy,
    /// `origin` has every commit.
    UpToDate,
    /// `HEAD` carries `n` commits `origin`'s branch lacks: the agent's to
    /// push; meanwhile the WIP snapshot carries them.
    Unpushed(usize),
    /// Inspection failed; retried next pass.
    Failed(String),
}

impl BranchPush {
    /// `HEAD` is on `origin`'s branch (nothing of it lives only here).
    pub fn on_origin(&self) -> bool {
        matches!(self, Self::UpToDate)
    }
}

/// What one [`push_wip`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WipPush {
    /// Nothing uncommitted, and no snapshot on `origin` to take back.
    Clean,
    /// The snapshot on `origin` already is this tree on this `HEAD`.
    Unchanged,
    /// Pushed `commit` (`skipped` = paths left out as secret or too big).
    Pushed { commit: String, skipped: Vec<String> },
    /// The tree is clean again: the old snapshot was deleted from `origin`.
    Dropped,
    /// Not attempted (the reason is for the log).
    Skipped(String),
    /// Snapshot or push failed; retried next pass.
    Failed(String),
}

/// One [`sync`] pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncReport {
    pub branch: BranchPush,
    pub wip: Option<WipPush>,
}

/// The session branch a worktree pushes to, and what it must never push to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTarget {
    pub worktree: PathBuf,
    /// The session's branch (`exp/session-<id8>`, `exp/<IDENT>`).
    pub branch: String,
    /// What the branch was cut from: never pushed to.
    pub base_branch: Option<String>,
}

impl PushTarget {
    /// The worktree's own current branch as the target (a run exit, which
    /// knows only its cwd). `None` when detached or not a repo.
    pub fn of_worktree(worktree: &Path) -> Option<Self> {
        let branch = current_branch(worktree)?;
        Some(Self { worktree: worktree.to_path_buf(), branch, base_branch: None })
    }
}

fn is_repo(worktree: &Path) -> bool {
    // A worktree has a `.git` FILE, a clone a `.git` dir; a scratch dir has
    // neither (and may sit inside some unrelated repo, which is not ours).
    worktree.join(".git").exists()
}

fn git(worktree: &Path, args: &[&str], op: &str) -> Result<String, String> {
    run_git(Some(worktree), args, None, op).map_err(|err| err.detail)
}

/// `git symbolic-ref --short -q HEAD`: the branch, `None` when detached or
/// unborn-and-unnamed.
pub fn current_branch(worktree: &Path) -> Option<String> {
    if !is_repo(worktree) {
        return None;
    }
    git(worktree, &["symbolic-ref", "--short", "-q", "HEAD"], "git symbolic-ref HEAD")
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|branch| !branch.is_empty())
}

fn rev(worktree: &Path, spec: &str) -> Option<String> {
    git(worktree, &["rev-parse", "--verify", "--quiet", spec], "git rev-parse")
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|sha| !sha.is_empty())
}

/// The worktree's own git dir (`<clone>/.git/worktrees/<name>`).
fn git_dir(worktree: &Path) -> Option<PathBuf> {
    git(worktree, &["rev-parse", "--absolute-git-dir"], "git rev-parse --git-dir")
        .ok()
        .map(|out| PathBuf::from(out.trim()))
}

/// A history-rewriting operation is half done: pushing now would publish
/// its intermediate state.
fn op_in_progress(git_dir: &Path) -> bool {
    ["rebase-merge", "rebase-apply", "MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"]
        .iter()
        .any(|marker| git_dir.join(marker).exists())
}

/// The repository's default branch as `origin` names it (`origin/HEAD`),
/// else `main`/`master` when present.
fn default_branch(worktree: &Path) -> Option<String> {
    if let Ok(out) = git(
        worktree,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
        "git symbolic-ref origin/HEAD",
    ) {
        if let Some(name) = out.trim().strip_prefix("origin/") {
            return Some(name.to_string());
        }
    }
    None
}

fn protected(target: &PushTarget) -> bool {
    let branch = target.branch.as_str();
    if target.base_branch.as_deref() == Some(branch) {
        return true;
    }
    match default_branch(&target.worktree) {
        Some(default) => default == branch,
        None => matches!(branch, "main" | "master"),
    }
}

/// Where `HEAD` stands against `origin/<branch>`. Read-only: the device
/// never pushes the branch (#EXP-1622), it only tells whether the WIP ref
/// has to carry commits.
pub fn branch_state(target: &PushTarget) -> BranchPush {
    let worktree = &target.worktree;
    if !is_repo(worktree) {
        return BranchPush::NotARepo;
    }
    if current_branch(worktree).as_deref() != Some(target.branch.as_str()) {
        return BranchPush::NotOnBranch;
    }
    if protected(target) {
        return BranchPush::Protected;
    }
    if git_dir(worktree).is_some_and(|dir| op_in_progress(&dir)) {
        return BranchPush::Busy;
    }
    let tracking = format!("refs/remotes/origin/{}", target.branch);
    let range: Vec<String> = if rev(worktree, &tracking).is_some() {
        vec!["HEAD".into(), "--not".into(), tracking]
    } else {
        vec!["HEAD".into(), "--not".into(), "--remotes=origin".into()]
    };
    let mut args = vec!["rev-list", "--count"];
    args.extend(range.iter().map(String::as_str));
    match git(worktree, &args, "git rev-list --count (auto-push)") {
        Ok(out) => match out.trim().parse::<usize>().unwrap_or(0) {
            0 => BranchPush::UpToDate,
            ahead => BranchPush::Unpushed(ahead),
        },
        Err(detail) => BranchPush::Failed(detail),
    }
}

/// Basenames that read like credentials and never leave the machine in a
/// snapshot (the agent may still commit them: its commits are its own).
pub fn secret_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name == ".env" || (name.starts_with(".env.") && !env_template(&name)) {
        return true;
    }
    if name.ends_with(".env") && name != ".env" && !name.ends_with(".example.env") {
        return true;
    }
    const EXACT: &[&str] = &[
        ".npmrc",
        ".pypirc",
        ".netrc",
        "_netrc",
        ".git-credentials",
        "exp-git-credentials",
        ".mcp.json",
        "id_rsa",
        "id_dsa",
        "id_ecdsa",
        "id_ed25519",
        "credentials",
        "credentials.json",
        "secrets.json",
        "secrets.yaml",
        "secrets.yml",
        "service-account.json",
        ".htpasswd",
        "keystore.properties",
        "local.properties",
    ];
    const EXTENSIONS: &[&str] =
        &[".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".ppk", ".age", ".gpg", ".asc", ".kdbx"];
    EXACT.contains(&name.as_str())
        || EXTENSIONS.iter().any(|ext| name.ends_with(ext))
        || (name.starts_with("id_") && !name.contains('.'))
        || (name.starts_with("service-account") && name.ends_with(".json"))
}

fn env_template(name: &str) -> bool {
    [".example", ".sample", ".template", ".dist", ".defaults"].iter().any(|suffix| name.ends_with(suffix))
}

/// Contents carrying a live credential: a private key block or a token in a
/// known shape (a bare prefix in source code is no token).
pub fn secret_content(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    if text.contains("PRIVATE KEY-----") {
        return true;
    }
    const PREFIXES: &[(&str, usize)] = &[
        ("ghp_", 30),
        ("gho_", 30),
        ("ghs_", 30),
        ("ghu_", 30),
        ("github_pat_", 30),
        ("sk-ant-", 30),
        ("sk-proj-", 30),
        ("xoxb-", 20),
        ("xoxp-", 20),
        ("expu_", 24),
        ("glpat-", 20),
    ];
    let token_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    for (prefix, min) in PREFIXES {
        let mut from = 0;
        while let Some(at) = text[from..].find(prefix) {
            let start = from + at + prefix.len();
            let run = text[start..].chars().take_while(|c| token_char(*c)).count();
            if run >= *min {
                return true;
            }
            from = start;
        }
    }
    // AWS access key ids: AKIA + 16 upper-case alphanumerics.
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(at) = text[from..].find("AKIA") {
        let start = from + at + 4;
        if bytes.len() >= start + 16
            && bytes[start..start + 16].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return true;
        }
        from = start;
    }
    false
}

/// One `git status --porcelain=v1 -z` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    path: String,
    untracked: bool,
}

fn status(worktree: &Path) -> Result<Vec<Change>, String> {
    // `--no-optional-locks`: never take the index lock under a running
    // agent (its own `git add` would fail on it).
    let out = git(
        worktree,
        &["--no-optional-locks", "status", "--porcelain=v1", "-z", "--untracked-files=all"],
        "git status (auto-push)",
    )?;
    let mut changes = Vec::new();
    let mut fields = out.split('\0').filter(|field| !field.is_empty());
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let (code, path) = entry.split_at(3);
        let x = code.as_bytes()[0];
        changes.push(Change { path: path.to_string(), untracked: code.starts_with("??") });
        // A rename/copy carries its source as the next field: it is a change
        // too (its removal).
        if matches!(x, b'R' | b'C') {
            if let Some(source) = fields.next() {
                changes.push(Change { path: source.to_string(), untracked: false });
            }
        }
    }
    Ok(changes)
}

/// A snapshot commit of the worktree's uncommitted work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub commit: String,
    pub tree: String,
    pub parent: String,
    /// Paths left out as secret-looking or too big.
    pub skipped: Vec<String>,
}

/// Why a path stays out of a snapshot, if it does.
fn exclude_reason(worktree: &Path, change: &Change) -> Option<&'static str> {
    if secret_path(&change.path) {
        return Some("secret");
    }
    let file = worktree.join(&change.path);
    let Ok(meta) = std::fs::symlink_metadata(&file) else {
        return None; // deleted: its removal is part of the snapshot
    };
    if !meta.is_file() {
        return None;
    }
    if meta.len() > MAX_WIP_FILE_BYTES {
        return Some("too big");
    }
    let mut bytes = Vec::new();
    if std::fs::File::open(&file).and_then(|mut f| f.read_to_end(&mut bytes)).is_ok() && secret_content(&bytes) {
        return Some("secret");
    }
    None
}

/// Build a snapshot of `worktree`'s uncommitted work WITHOUT touching it:
/// a temporary index seeded from `HEAD`, the changed paths added to it
/// (minus [`exclude_reason`]'s), written as a tree and committed on `HEAD`.
/// `Ok(None)` = nothing uncommitted survives the filters.
pub fn snapshot(worktree: &Path, branch: &str) -> Result<Option<Snapshot>, String> {
    let head = rev(worktree, "HEAD").ok_or_else(|| "no HEAD commit".to_string())?;
    let mut changes = status(worktree)?;
    if changes.is_empty() {
        return Ok(None);
    }
    if changes.len() > MAX_WIP_PATHS {
        changes.retain(|change| !change.untracked);
        if changes.len() > MAX_WIP_PATHS {
            return Err(format!("{} changed paths: too many to snapshot", changes.len()));
        }
    }
    let mut skipped = Vec::new();
    let mut paths = Vec::new();
    for change in changes {
        match exclude_reason(worktree, &change) {
            Some(_) => skipped.push(change.path),
            None => paths.push(change.path),
        }
    }
    let head_tree = rev(worktree, "HEAD^{tree}").ok_or_else(|| "no HEAD tree".to_string())?;
    if paths.is_empty() {
        return Ok(None);
    }
    let dir = git_dir(worktree).ok_or_else(|| "no git dir".to_string())?;
    let index = dir.join("exp-wip-index");
    let pathspec = dir.join("exp-wip-paths");
    let _ = std::fs::remove_file(&index);
    let mut list = Vec::new();
    for path in &paths {
        list.extend_from_slice(path.as_bytes());
        list.push(0);
    }
    std::fs::write(&pathspec, &list).map_err(|err| format!("write pathspec: {err}"))?;
    let result = (|| {
        let local = |args: &[&str]| -> Result<String, String> {
            let mut command = terminal::process::background_command("git");
            command
                .args(args)
                .current_dir(worktree)
                .env("GIT_INDEX_FILE", &index)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("LC_ALL", "C")
                .env("GIT_AUTHOR_NAME", WIP_NAME)
                .env("GIT_AUTHOR_EMAIL", WIP_EMAIL)
                .env("GIT_COMMITTER_NAME", WIP_NAME)
                .env("GIT_COMMITTER_EMAIL", WIP_EMAIL);
            let output = command.output().map_err(|err| err.to_string())?;
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        };
        local(&["read-tree", "HEAD"])?;
        let spec = pathspec.to_string_lossy().into_owned();
        local(&[
            "--literal-pathspecs",
            "add",
            "-A",
            "--ignore-errors",
            &format!("--pathspec-from-file={spec}"),
            "--pathspec-file-nul",
        ])?;
        let tree = local(&["write-tree"])?;
        if tree == head_tree {
            return Ok(None);
        }
        let message = format!(
            "{WIP_SNAPSHOT_SUBJECT}\n\nUncommitted work on {branch}, saved by the device so the session can \
continue anywhere. Not part of the branch.\n"
        );
        let commit = local(&["-c", "commit.gpgsign=false", "commit-tree", &tree, "-p", &head, "-m", &message])?;
        Ok(Some(Snapshot { commit, tree, parent: head.clone(), skipped: skipped.clone() }))
    })();
    let _ = std::fs::remove_file(&index);
    let _ = std::fs::remove_file(&pathspec);
    result
}

/// What `refs/exp-wip/<branch>` last pushed from this clone points at.
fn pushed_wip(worktree: &Path, branch: &str) -> Option<String> {
    rev(worktree, &wip_ref(branch))
}

fn tree_and_parent(worktree: &Path, commit: &str) -> Option<(String, Option<String>)> {
    let tree = rev(worktree, &format!("{commit}^{{tree}}"))?;
    Some((tree, rev(worktree, &format!("{commit}^1"))))
}

pub(crate) fn push_wip_commit(target: &PushTarget, commit: &str) -> Result<(), String> {
    let wip = wip_ref(&target.branch);
    git(
        &target.worktree,
        &["push", "--no-verify", "--quiet", "--force", "origin", &format!("{commit}:{wip}")],
        &format!("git push origin {wip} (auto-push)"),
    )?;
    // The clone remembers what origin holds now (no tracking ref exists for
    // a hidden namespace): the next pass compares against it offline.
    git(&target.worktree, &["update-ref", &wip, commit], "git update-ref (wip)")?;
    Ok(())
}

/// Delete the branch's snapshot from `origin` (and the clone's memory of
/// it): the work it held is committed and pushed, or deliberately gone.
pub fn drop_wip(target: &PushTarget) -> Result<bool, String> {
    let wip = wip_ref(&target.branch);
    if pushed_wip(&target.worktree, &target.branch).is_none() {
        return Ok(false);
    }
    match git(
        &target.worktree,
        &["push", "--no-verify", "--quiet", "origin", &format!(":{wip}")],
        &format!("git push origin :{wip} (auto-push)"),
    ) {
        Ok(_) => {}
        // Already gone on origin: only the local memory is left to clear.
        Err(detail) if detail.contains("unable to delete") || detail.contains("does not exist") => {}
        Err(detail) => return Err(detail),
    }
    git(&target.worktree, &["update-ref", "-d", &wip], "git update-ref -d (wip)")?;
    Ok(true)
}

/// Push the worktree's uncommitted work as `refs/exp-wip/<branch>`, or —
/// `head_unpushed` (`origin`'s branch lacks commits of `HEAD`) — `HEAD` itself
/// when nothing is uncommitted, so its commits are on origin all the same.
/// A clean tree whose `HEAD` is on origin drops an old snapshot.
pub fn push_wip(target: &PushTarget, head_unpushed: bool) -> WipPush {
    let worktree = &target.worktree;
    if !is_repo(worktree) {
        return WipPush::Skipped("not a repository".into());
    }
    if protected(target) {
        return WipPush::Skipped("protected branch".into());
    }
    if git_dir(worktree).is_some_and(|dir| op_in_progress(&dir)) {
        return WipPush::Skipped("a rebase or merge is in progress".into());
    }
    let snap = match snapshot(worktree, &target.branch) {
        Ok(snap) => snap,
        Err(detail) => return WipPush::Failed(detail),
    };
    let (commit, skipped, parked) = match snap {
        Some(snap) => (snap.commit, snap.skipped, false),
        None if head_unpushed => match rev(worktree, "HEAD") {
            Some(head) => (head, Vec::new(), true),
            None => return WipPush::Skipped("no HEAD".into()),
        },
        None => {
            return match drop_wip(target) {
                Ok(true) => WipPush::Dropped,
                Ok(false) => WipPush::Clean,
                Err(detail) => WipPush::Failed(detail),
            };
        }
    };
    if let Some(pushed) = pushed_wip(worktree, &target.branch) {
        // A parked `HEAD` must be that very commit (an old snapshot of the
        // same tree would bring back the edits but not the agent's commit).
        let same = pushed == commit
            || (!parked && tree_and_parent(worktree, &pushed) == tree_and_parent(worktree, &commit));
        if same {
            return WipPush::Unchanged;
        }
    }
    match push_wip_commit(target, &commit) {
        Ok(()) => WipPush::Pushed { commit, skipped },
        Err(detail) => WipPush::Failed(detail),
    }
}

/// One pass: the snapshot, when due (`with_wip`) or when `HEAD` carries
/// commits `origin` lacks. The branch itself is never pushed (#EXP-1622).
pub fn sync(target: &PushTarget, with_wip: bool) -> SyncReport {
    let branch = branch_state(target);
    let wip = match &branch {
        BranchPush::NotARepo | BranchPush::NotOnBranch | BranchPush::Protected | BranchPush::Busy => None,
        BranchPush::Unpushed(_) => Some(push_wip(target, true)),
        state => with_wip.then(|| push_wip(target, !state.on_origin())),
    };
    SyncReport { branch, wip }
}

/// A run's exit (stop, interrupt, a move's kill, a suspend at quit): one
/// full [`sync`] of the worktree's current branch. Blocking (git +
/// network): call it off any UI thread.
pub fn sync_worktree(worktree: &Path) -> Option<SyncReport> {
    let target = PushTarget::of_worktree(worktree)?;
    let base = registry_base(worktree);
    let target = PushTarget { base_branch: base, ..target };
    let report = sync(&target, true);
    log_report(&target, &report, "exit");
    Some(report)
}

/// The base branch the run registry recorded for the run in `worktree`.
fn registry_base(worktree: &Path) -> Option<String> {
    let data_dir = STARTED.get().map(|pusher| pusher.data_dir.clone())?;
    crate::run_registry::all(&data_dir)
        .into_iter()
        .find(|record| record.cwd == worktree)
        .and_then(|record| record.base_branch)
}

fn log_report(target: &PushTarget, report: &SyncReport, why: &str) {
    let quiet = matches!(report.branch, BranchPush::UpToDate | BranchPush::NotOnBranch | BranchPush::NotARepo)
        && matches!(report.wip, None | Some(WipPush::Clean | WipPush::Unchanged));
    if quiet {
        return;
    }
    let failed = matches!(report.branch, BranchPush::Failed(_)) || matches!(report.wip, Some(WipPush::Failed(_)));
    let line = format!("auto-push ({why}) {} in {}: {report:?}", target.branch, target.worktree.display());
    if failed {
        log::warn!("{line}");
    } else {
        log::info!("{line}");
    }
}

/// What a prepared worktree got back from `origin`'s snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WipRestore {
    /// No snapshot on origin for the branch.
    None,
    /// The worktree already has local changes or is elsewhere: untouched.
    Skipped(String),
    /// The snapshot is older than the branch (its work was committed since).
    Stale,
    /// `HEAD` fast-forwarded to the commits the snapshot parked.
    FastForwarded,
    /// The snapshot's changes are back as uncommitted work (`fast_forwarded`:
    /// `HEAD` first moved up to the snapshot's parent).
    Restored { fast_forwarded: bool },
}

fn is_ancestor(worktree: &Path, a: &str, b: &str) -> bool {
    git(worktree, &["merge-base", "--is-ancestor", a, b], "git merge-base --is-ancestor").is_ok()
}

/// A worktree just prepared for `branch` (a move's continuation, a resume
/// that re-created a reclaimed worktree): fetch `refs/exp-wip/<branch>` and
/// put it back. Only on a CLEAN worktree on `branch`; only when the
/// snapshot sits on (or ahead of) `HEAD`; the index stays `HEAD`'s (the
/// work comes back unstaged, new files untracked).
pub fn restore_wip(clone: &Path, worktree: &Path, branch: &str, url: Option<&TokenUrl>) -> WipRestore {
    if current_branch(worktree).as_deref() != Some(branch) {
        return WipRestore::Skipped("not on the branch".into());
    }
    let wip = wip_ref(branch);
    // A fresh branch never has a snapshot: asked first, since a fetch of
    // the missing ref stalled the launch ([`origin_has_ref`]).
    if origin_has_ref(clone, &wip, url) == Some(false) {
        return WipRestore::None;
    }
    let fetched = run_git(
        Some(clone),
        &["fetch", "--quiet", "--no-tags", "origin", &format!("+{wip}:{wip}")],
        url,
        &format!("git fetch origin {wip}"),
    );
    if fetched.is_err() {
        // No snapshot on origin (the common case), or offline.
        return WipRestore::None;
    }
    let Some(snapshot) = rev(worktree, &wip) else {
        return WipRestore::None;
    };
    match status(worktree) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => return WipRestore::Skipped("the worktree has local changes".into()),
        Err(detail) => return WipRestore::Skipped(detail),
    }
    let Some(head) = rev(worktree, "HEAD") else {
        return WipRestore::Skipped("no HEAD".into());
    };
    let subject = git(worktree, &["log", "-1", "--format=%s", &snapshot], "git log (wip)").unwrap_or_default();
    if subject.trim() != WIP_SNAPSHOT_SUBJECT {
        // `HEAD` parked on the ref (its branch push was refused).
        if head == snapshot || is_ancestor(worktree, &snapshot, &head) {
            return WipRestore::Stale;
        }
        if !is_ancestor(worktree, &head, &snapshot) {
            return WipRestore::Skipped("the parked HEAD diverged from the branch".into());
        }
        return match git(worktree, &["merge", "--ff-only", "--quiet", &snapshot], "git merge --ff-only (wip)") {
            Ok(_) => WipRestore::FastForwarded,
            Err(detail) => WipRestore::Skipped(detail),
        };
    }
    let Some(parent) = rev(worktree, &format!("{snapshot}^1")) else {
        return WipRestore::Skipped("snapshot without a parent".into());
    };
    let mut fast_forwarded = false;
    if parent != head {
        if !is_ancestor(worktree, &head, &parent) {
            // The branch moved past the snapshot: its work was committed.
            return WipRestore::Stale;
        }
        if let Err(detail) = git(worktree, &["merge", "--ff-only", "--quiet", &parent], "git merge --ff-only (wip)") {
            return WipRestore::Skipped(detail);
        }
        fast_forwarded = true;
    }
    match git(
        worktree,
        &["restore", &format!("--source={snapshot}"), "--worktree", "--", ":/"],
        "git restore (wip)",
    ) {
        Ok(_) => WipRestore::Restored { fast_forwarded },
        Err(detail) => WipRestore::Skipped(detail),
    }
}

/// One live run the pusher watches.
#[derive(Default)]
struct Watch {
    head: Option<String>,
    last_wip: Option<Instant>,
    /// Consecutive failed passes; each doubles the wait (capped at
    /// [`MAX_BACKOFF`]) so an expired token on an old worktree is no log
    /// flood.
    failures: u32,
    retry_at: Option<Instant>,
}

/// The longest a failing worktree waits between attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);

/// The device's background pusher: every [`HEAD_CHECK_EVERY`] it looks at
/// the runs THIS host process drives (run registry, `host_pid` = us) and the
/// worktrees no live host drives any more (a crash, a quit before the end),
/// snapshotting a moved, unpushed `HEAD` at once and the worktree every
/// [`WIP_EVERY`].
pub struct AutoPusher {
    data_dir: PathBuf,
    watches: Mutex<HashMap<PathBuf, Watch>>,
    stop: AtomicBool,
}

static STARTED: OnceLock<std::sync::Arc<AutoPusher>> = OnceLock::new();

/// Start the device's pusher once per process (idempotent; both hosts call
/// it at start). `None` when the thread could not start.
pub fn ensure_started(data_dir: &Path) -> Option<std::sync::Arc<AutoPusher>> {
    if let Some(pusher) = STARTED.get() {
        return Some(std::sync::Arc::clone(pusher));
    }
    let pusher = std::sync::Arc::new(AutoPusher {
        data_dir: data_dir.to_path_buf(),
        watches: Mutex::new(HashMap::new()),
        stop: AtomicBool::new(false),
    });
    if STARTED.set(std::sync::Arc::clone(&pusher)).is_err() {
        return STARTED.get().cloned();
    }
    let looping = std::sync::Arc::clone(&pusher);
    match std::thread::Builder::new().name("exp-auto-push".to_string()).spawn(move || looping.run()) {
        Ok(_) => Some(pusher),
        Err(err) => {
            log::warn!("auto-push: thread not started: {err}");
            None
        }
    }
}

/// The session worktrees of the run registry: `(target, driven_here)`.
/// `driven_here` = this process hosts the run; the others are worktrees no
/// live host drives (theirs to push is nobody's but ours).
pub fn registry_targets(data_dir: &Path) -> Vec<(PushTarget, bool)> {
    let me = std::process::id();
    crate::run_registry::all(data_dir)
        .into_iter()
        .filter_map(|record| {
            let (Some(_), Some(branch)) = (record.clone.as_ref(), record.branch.as_ref()) else {
                return None;
            };
            let here = record.host_pid == Some(me);
            let other_live = record.host_pid.is_some_and(|pid| pid != me && crate::process::is_alive(pid));
            if other_live || !is_repo(&record.cwd) {
                return None;
            }
            Some((
                PushTarget {
                    worktree: record.cwd.clone(),
                    branch: branch.clone(),
                    base_branch: record.base_branch.clone(),
                },
                here,
            ))
        })
        .collect()
}

impl AutoPusher {
    fn run(&self) {
        // A fresh start first pushes whatever an earlier process left (a
        // crash, a power cut, a quit that outran its pushes).
        let mut first = true;
        while !self.stop.load(Ordering::Relaxed) {
            self.pass(first);
            first = false;
            std::thread::sleep(HEAD_CHECK_EVERY);
        }
    }

    /// Stop the loop after the current pass (tests, shutdown).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn pass(&self, everything: bool) {
        let now = Instant::now();
        let targets = registry_targets(&self.data_dir);
        let mut watches = match self.watches.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        watches.retain(|path, _| targets.iter().any(|(target, _)| &target.worktree == path));
        for (target, here) in targets {
            let watch = watches.entry(target.worktree.clone()).or_default();
            if !everything && watch.retry_at.is_some_and(|at| now < at) {
                continue;
            }
            let wip_due = everything || watch.last_wip.is_none_or(|at| now.duration_since(at) >= WIP_EVERY);
            // A worktree no live host drives changes only through a person:
            // the snapshot interval is enough for it.
            if !here && !wip_due {
                continue;
            }
            let head = rev(&target.worktree, "HEAD");
            let moved = head != watch.head;
            if !moved && !wip_due {
                continue;
            }
            let report = sync(&target, wip_due);
            log_report(&target, &report, if wip_due { "snapshot" } else { "commit" });
            let failed =
                matches!(report.branch, BranchPush::Failed(_)) || matches!(report.wip, Some(WipPush::Failed(_)));
            if failed {
                // Retried after a growing wait (the HEAD stays "unseen").
                watch.failures = watch.failures.saturating_add(1);
                let wait = HEAD_CHECK_EVERY.saturating_mul(1 << watch.failures.min(6)).min(MAX_BACKOFF);
                watch.retry_at = Some(now + wait);
            } else {
                watch.failures = 0;
                watch.retry_at = None;
                watch.head = head;
            }
            if wip_due && !failed {
                watch.last_wip = Some(now);
            }
        }
    }
}

/// Shutdown (a quit, a daemon stop): push every run this host drives NOW,
/// in parallel, waiting at most `budget`. Returns how many finished.
pub fn sync_live_runs(data_dir: &Path, budget: Duration) -> usize {
    let targets: Vec<PushTarget> =
        registry_targets(data_dir).into_iter().filter(|(_, here)| *here).map(|(target, _)| target).collect();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let total = targets.len();
    for target in targets {
        let done = done_tx.clone();
        let spawned = std::thread::Builder::new().name("exp-auto-push-now".to_string()).spawn(move || {
            let report = sync(&target, true);
            log_report(&target, &report, "shutdown");
            let _ = done.send(());
        });
        if let Err(err) = spawned {
            log::warn!("auto-push: shutdown push not started: {err}");
        }
    }
    drop(done_tx);
    let deadline = Instant::now() + budget;
    let mut finished = 0;
    while finished < total {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || done_rx.recv_timeout(left).is_err() {
            break;
        }
        finished += 1;
    }
    finished
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    pub(super) fn git_ok(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .expect("git");
        assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-auto-push-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A bare `origin` with `main`, and a device: a clone with a session
    /// worktree on `exp/session-1a2b3c4d`.
    pub(super) struct Fixture {
        pub root: PathBuf,
        pub origin: PathBuf,
    }

    impl Fixture {
        pub fn new(tag: &str) -> Self {
            let root = temp_dir(tag);
            let origin = root.join("origin.git");
            std::fs::create_dir_all(&origin).unwrap();
            git_ok(&origin, &["init", "--bare", "--initial-branch=main", "."]);
            let seed = root.join("seed");
            std::fs::create_dir_all(&seed).unwrap();
            git_ok(&seed, &["init", "--initial-branch=main", "."]);
            std::fs::write(seed.join("README.md"), "hello\n").unwrap();
            std::fs::write(seed.join("lib.rs"), "fn a() {}\n").unwrap();
            git_ok(&seed, &["add", "."]);
            git_ok(&seed, &["commit", "-m", "base"]);
            git_ok(&seed, &["remote", "add", "origin", origin.to_str().unwrap()]);
            git_ok(&seed, &["push", "-u", "origin", "main"]);
            Self { root, origin }
        }

        /// A device's clone + a worktree on `branch` (cut from origin/main,
        /// or checked out from origin/<branch> when it exists there).
        pub fn device(&self, name: &str, branch: &str) -> (PathBuf, PathBuf) {
            let clone = self.root.join(name);
            git_ok(&self.root, &["clone", "--quiet", self.origin.to_str().unwrap(), clone.to_str().unwrap()]);
            let worktree = self.root.join(format!("{name}-wt"));
            let remote = git_ok(&clone, &["ls-remote", "origin", &format!("refs/heads/{branch}")]);
            if remote.is_empty() {
                git_ok(&clone, &["worktree", "add", "-q", "-b", branch, worktree.to_str().unwrap(), "origin/main"]);
            } else {
                git_ok(&clone, &["worktree", "add", "-q", worktree.to_str().unwrap(), branch]);
            }
            (clone, worktree)
        }

        pub fn origin_rev(&self, refname: &str) -> Option<String> {
            let output = Command::new("git")
                .args(["rev-parse", "--verify", "--quiet", refname])
                .current_dir(&self.origin)
                .output()
                .unwrap();
            output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const BRANCH: &str = "exp/session-1a2b3c4d";

    fn target(worktree: &Path) -> PushTarget {
        PushTarget { worktree: worktree.to_path_buf(), branch: BRANCH.into(), base_branch: Some("main".into()) }
    }

    /// The agent's own push (`git push origin HEAD:<branch>`).
    fn agent_push(worktree: &Path) {
        git_ok(worktree, &["push", "--quiet", "origin", &format!("HEAD:refs/heads/{BRANCH}")]);
    }

    fn commit(worktree: &Path, file: &str, subject: &str) {
        std::fs::write(worktree.join(file), format!("{subject}\n")).unwrap();
        git_ok(worktree, &["add", "."]);
        git_ok(worktree, &["commit", "-m", subject]);
    }

    /// #EXP-1622: an agent commit on a branch with an open PR never reaches
    /// the branch through the device (no unreleased commit on the PR, no CI
    /// restart); it reaches origin on the hidden ref only.
    #[test]
    fn a_commit_rides_the_wip_ref_and_the_branch_is_never_pushed() {
        let fx = Fixture::new("commit");
        let (_, wt) = fx.device("a", BRANCH);
        assert_eq!(branch_state(&target(&wt)), BranchPush::UpToDate);
        commit(&wt, "work.txt", "released");
        agent_push(&wt);
        let released = git_ok(&wt, &["rev-parse", "HEAD"]);
        assert_eq!(branch_state(&target(&wt)), BranchPush::UpToDate);

        commit(&wt, "next.txt", "not released yet");
        let report = sync(&target(&wt), false);
        assert_eq!(report.branch, BranchPush::Unpushed(1));
        assert!(matches!(report.wip, Some(WipPush::Pushed { .. })));
        assert_eq!(fx.origin_rev(&format!("refs/heads/{BRANCH}")), Some(released.clone()));
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), Some(git_ok(&wt, &["rev-parse", "HEAD"])));
        // The exit and shutdown passes never push it either.
        assert!(sync_worktree(&wt).is_some());
        assert_eq!(fx.origin_rev(&format!("refs/heads/{BRANCH}")), Some(released));

        // The agent amends: still only the hidden ref follows it.
        std::fs::write(wt.join("next.txt"), "amended\n").unwrap();
        git_ok(&wt, &["commit", "-a", "--amend", "-m", "amended"]);
        assert!(matches!(sync(&target(&wt), false).wip, Some(WipPush::Pushed { .. })));
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), Some(git_ok(&wt, &["rev-parse", "HEAD"])));

        // The agent pushes its branch: the clean tree's next pass drops the ref.
        agent_push(&wt);
        let report = sync(&target(&wt), true);
        assert_eq!(report.branch, BranchPush::UpToDate);
        assert_eq!(report.wip, Some(WipPush::Dropped));
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), None);
    }

    #[test]
    fn the_default_branch_and_the_base_are_never_pushed() {
        let fx = Fixture::new("protected");
        let (clone, _) = fx.device("a", BRANCH);
        std::fs::write(clone.join("x.txt"), "x\n").unwrap();
        git_ok(&clone, &["add", "."]);
        git_ok(&clone, &["commit", "-m", "on main"]);
        let main = PushTarget { worktree: clone.clone(), branch: "main".into(), base_branch: None };
        assert_eq!(branch_state(&main), BranchPush::Protected);
        assert_eq!(push_wip(&main, true), WipPush::Skipped("protected branch".into()));
        let based = PushTarget { worktree: clone, branch: "main".into(), base_branch: Some("main".into()) };
        assert_eq!(branch_state(&based), BranchPush::Protected);
    }

    #[test]
    fn a_rebase_in_progress_is_left_alone() {
        let fx = Fixture::new("busy");
        let (_, wt) = fx.device("a", BRANCH);
        std::fs::write(wt.join("work.txt"), "work\n").unwrap();
        git_ok(&wt, &["add", "."]);
        git_ok(&wt, &["commit", "-m", "work"]);
        let dir = git_dir(&wt).unwrap();
        std::fs::create_dir_all(dir.join("rebase-merge")).unwrap();
        assert_eq!(branch_state(&target(&wt)), BranchPush::Busy);
        assert_eq!(sync(&target(&wt), true), SyncReport { branch: BranchPush::Busy, wip: None });
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), None);
        std::fs::remove_dir_all(dir.join("rebase-merge")).unwrap();
        assert_eq!(branch_state(&target(&wt)), BranchPush::Unpushed(1));
    }

    #[test]
    fn a_snapshot_leaves_the_agents_index_and_head_alone_and_skips_secrets() {
        let fx = Fixture::new("snapshot");
        let (_, wt) = fx.device("a", BRANCH);
        let head = git_ok(&wt, &["rev-parse", "HEAD"]);
        std::fs::write(wt.join("lib.rs"), "fn a() { b() }\n").unwrap();
        std::fs::write(wt.join("staged.txt"), "staged\n").unwrap();
        git_ok(&wt, &["add", "staged.txt"]);
        std::fs::write(wt.join("new file [x].rs"), "fn new() {}\n").unwrap();
        std::fs::write(wt.join(".env"), "TOKEN=abc\n").unwrap();
        std::fs::write(wt.join("config.ts"), "const k = 'ghp_0123456789abcdefghijklmnopqrstuvwxyzAB'\n").unwrap();
        std::fs::remove_file(wt.join("README.md")).unwrap();
        let status_before = git_ok(&wt, &["status", "--porcelain"]);

        let snap = snapshot(&wt, BRANCH).unwrap().expect("a snapshot");
        assert_eq!(snap.parent, head);
        let mut skipped = snap.skipped.clone();
        skipped.sort();
        assert_eq!(skipped, vec![".env".to_string(), "config.ts".to_string()]);
        let files = git_ok(&wt, &["ls-tree", "-r", "--name-only", &snap.commit]);
        let files: Vec<&str> = files.lines().collect();
        assert_eq!(files, vec!["lib.rs", "new file [x].rs", "staged.txt"]);
        assert_eq!(git_ok(&wt, &["show", &format!("{}:lib.rs", snap.commit)]), "fn a() { b() }");
        // Nothing the agent sees moved.
        assert_eq!(git_ok(&wt, &["rev-parse", "HEAD"]), head);
        assert_eq!(git_ok(&wt, &["status", "--porcelain"]), status_before);
        assert_eq!(git_ok(&wt, &["diff", "--cached", "--name-only"]), "staged.txt");
    }

    #[test]
    fn a_clean_tree_has_no_snapshot_and_drops_the_old_one() {
        let fx = Fixture::new("drop");
        let (_, wt) = fx.device("a", BRANCH);
        assert_eq!(push_wip(&target(&wt), false), WipPush::Clean);
        std::fs::write(wt.join("lib.rs"), "fn a() { 1 }\n").unwrap();
        let WipPush::Pushed { commit, .. } = push_wip(&target(&wt), false) else { panic!("pushed") };
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), Some(commit));
        assert_eq!(push_wip(&target(&wt), false), WipPush::Unchanged);
        git_ok(&wt, &["checkout", "--", "lib.rs"]);
        assert_eq!(push_wip(&target(&wt), false), WipPush::Dropped);
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), None);
        // A tree holding only secrets has nothing to push either.
        std::fs::write(wt.join(".env"), "TOKEN=abc\n").unwrap();
        assert_eq!(push_wip(&target(&wt), false), WipPush::Clean);
    }

    /// #EXP-1618's acceptance case: a session killed mid-work on device A
    /// (no end, no hand-off: A just goes away) continues on device B with
    /// its commits AND its uncommitted changes intact.
    #[test]
    fn a_session_killed_mid_work_resumes_on_another_device_with_its_changes() {
        let fx = Fixture::new("move");
        let (_, a) = fx.device("a", BRANCH);
        std::fs::write(a.join("work.txt"), "committed\n").unwrap();
        git_ok(&a, &["add", "."]);
        git_ok(&a, &["commit", "-m", "committed work"]);
        std::fs::write(a.join("lib.rs"), "fn a() { half_done() }\n").unwrap();
        std::fs::write(a.join("draft.rs"), "// new, never added\n").unwrap();
        std::fs::remove_file(a.join("README.md")).unwrap();
        // The device's periodic pass, then the machine disappears. The
        // agent never pushed: the commit lives on the hidden ref only.
        let report = sync(&target(&a), true);
        assert_eq!(report.branch, BranchPush::Unpushed(1));
        assert!(matches!(report.wip, Some(WipPush::Pushed { .. })));
        assert_eq!(fx.origin_rev(&format!("refs/heads/{BRANCH}")), None);
        let a_head = git_ok(&a, &["rev-parse", "HEAD"]);

        let (b_clone, b) = fx.device("b", BRANCH);
        assert_ne!(git_ok(&b, &["rev-parse", "HEAD"]), a_head);
        assert_eq!(restore_wip(&b_clone, &b, BRANCH, None), WipRestore::Restored { fast_forwarded: true });
        assert_eq!(std::fs::read_to_string(b.join("lib.rs")).unwrap(), "fn a() { half_done() }\n");
        assert_eq!(std::fs::read_to_string(b.join("draft.rs")).unwrap(), "// new, never added\n");
        assert!(!b.join("README.md").exists());
        assert_eq!(std::fs::read_to_string(b.join("work.txt")).unwrap(), "committed\n");
        // Back as uncommitted work: nothing staged, HEAD = A's commit.
        assert_eq!(git_ok(&b, &["diff", "--cached", "--name-only"]), "");
        assert_eq!(git_ok(&b, &["rev-parse", "HEAD"]), a_head);
        let status = git_ok(&b, &["status", "--porcelain"]);
        let mut lines: Vec<&str> = status.lines().map(str::trim).collect();
        lines.sort();
        assert_eq!(lines, vec!["?? draft.rs", "D README.md", "M lib.rs"]);

        // B commits it all and pushes its branch: its next pass drops the
        // snapshot, and a later prepare finds nothing to restore.
        git_ok(&b, &["add", "-A"]);
        git_ok(&b, &["commit", "-m", "finished on b"]);
        agent_push(&b);
        let report = sync(&target(&b), true);
        assert_eq!(report.branch, BranchPush::UpToDate);
        assert_eq!(report.wip, Some(WipPush::Dropped));
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), None);
    }

    /// The background pass, driven from the run registry: a live run of
    /// THIS process pushes its new commit and its edits without anyone
    /// asking; the process then dies (no exit, no end) and another device
    /// continues with everything.
    #[test]
    fn the_background_pass_pushes_a_live_run_so_a_dead_device_loses_nothing() {
        let fx = Fixture::new("pass");
        let (clone, a) = fx.device("a", BRANCH);
        let data_dir = fx.root.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let mut record = crate::run_registry::sample_record("1a2b3c4d-0000-0000-0000-000000000000");
        record.cwd = a.clone();
        record.clone = Some(clone);
        record.branch = Some(BRANCH.to_string());
        record.base_branch = Some("main".to_string());
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&data_dir, record);
        let pusher = AutoPusher {
            data_dir: data_dir.clone(),
            watches: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
        };
        assert_eq!(registry_targets(&data_dir).len(), 1);

        std::fs::write(a.join("work.txt"), "committed\n").unwrap();
        git_ok(&a, &["add", "."]);
        git_ok(&a, &["commit", "-m", "agent commit"]);
        std::fs::write(a.join("lib.rs"), "fn a() { editing }\n").unwrap();
        pusher.pass(true);
        assert_eq!(fx.origin_rev(&format!("refs/heads/{BRANCH}")), None);
        assert!(fx.origin_rev(&wip_ref(BRANCH)).is_some());

        // A second commit reaches the hidden ref at the next 30 s check (no
        // snapshot due), as HEAD itself on a clean tree; never the branch.
        git_ok(&a, &["commit", "-a", "-m", "agent commit 2"]);
        pusher.pass(false);
        assert_eq!(fx.origin_rev(&format!("refs/heads/{BRANCH}")), None);
        assert_eq!(fx.origin_rev(&wip_ref(BRANCH)), Some(git_ok(&a, &["rev-parse", "HEAD"])));
        std::fs::write(a.join("lib.rs"), "fn a() { editing again }\n").unwrap();
        pusher.pass(true);

        let (b_clone, b) = fx.device("b", BRANCH);
        assert_eq!(restore_wip(&b_clone, &b, BRANCH, None), WipRestore::Restored { fast_forwarded: true });
        assert_eq!(std::fs::read_to_string(b.join("lib.rs")).unwrap(), "fn a() { editing again }\n");
        assert_eq!(git_ok(&b, &["log", "-1", "--format=%s"]), "agent commit 2");
    }

    #[test]
    fn a_snapshot_on_unpushed_commits_fast_forwards_then_restores() {
        let fx = Fixture::new("ff");
        let (_, a) = fx.device("a", BRANCH);
        std::fs::write(a.join("one.txt"), "1\n").unwrap();
        git_ok(&a, &["add", "."]);
        git_ok(&a, &["commit", "-m", "pushed"]);
        agent_push(&a);
        // The agent's next commit is not released yet: the snapshot
        // (parented on it) carries it.
        std::fs::write(a.join("two.txt"), "2\n").unwrap();
        git_ok(&a, &["add", "."]);
        git_ok(&a, &["commit", "-m", "not pushed"]);
        std::fs::write(a.join("lib.rs"), "fn a() { wip }\n").unwrap();
        assert!(matches!(push_wip(&target(&a), true), WipPush::Pushed { .. }));

        let (b_clone, b) = fx.device("b", BRANCH);
        assert_eq!(restore_wip(&b_clone, &b, BRANCH, None), WipRestore::Restored { fast_forwarded: true });
        assert_eq!(git_ok(&b, &["rev-parse", "HEAD"]), git_ok(&a, &["rev-parse", "HEAD"]));
        assert_eq!(std::fs::read_to_string(b.join("lib.rs")).unwrap(), "fn a() { wip }\n");
    }

    #[test]
    fn a_stale_or_conflicting_snapshot_is_never_applied() {
        let fx = Fixture::new("stale");
        let (_, a) = fx.device("a", BRANCH);
        std::fs::write(a.join("lib.rs"), "fn a() { old wip }\n").unwrap();
        assert!(matches!(sync(&target(&a), true).wip, Some(WipPush::Pushed { .. })));
        // Another device moved the branch on past the snapshot's parent.
        let (_, c) = fx.device("c", BRANCH);
        std::fs::write(c.join("c.txt"), "c\n").unwrap();
        git_ok(&c, &["add", "."]);
        git_ok(&c, &["commit", "-m", "c"]);
        agent_push(&c);

        let (b_clone, b) = fx.device("b", BRANCH);
        assert_eq!(restore_wip(&b_clone, &b, BRANCH, None), WipRestore::Stale);
        assert_eq!(git_ok(&b, &["status", "--porcelain"]), "");
        // A worktree with its own changes is never overwritten.
        std::fs::write(b.join("mine.txt"), "mine\n").unwrap();
        assert!(matches!(restore_wip(&b_clone, &b, BRANCH, None), WipRestore::Skipped(_)));
        // No snapshot at all.
        let (d_clone, d) = fx.device("d", "exp/session-ffffffff");
        assert_eq!(restore_wip(&d_clone, &d, "exp/session-ffffffff", None), WipRestore::None);
    }

    #[test]
    fn secret_paths_and_contents() {
        for path in [".env", "app/.env.local", "prod.env", "id_rsa", "keys/server.pem", ".npmrc", "credentials.json"] {
            assert!(secret_path(path), "{path}");
        }
        for path in [".env.example", "src/env.ts", "README.md", "id_card.rs", "keyboard.rs", "Cargo.lock"] {
            assert!(!secret_path(path), "{path}");
        }
        assert!(secret_content(b"-----BEGIN OPENSSH PRIVATE KEY-----\nabc"));
        assert!(secret_content(b"token = \"ghp_0123456789abcdefghijklmnopqrstuvwxyzAB\""));
        assert!(secret_content(b"AWS=AKIAIOSFODNN7EXAMPLE"));
        assert!(secret_content(b"key: expu_AbCdEfGhIjKlMnOpQrStUvWxYz012345"));
        // A bare prefix in source code is no token.
        assert!(!secret_content(b"if token.starts_with(\"ghp_\") || key.starts_with(\"expu_\") {"));
        assert!(!secret_content(b"const AKIA = 1"));
    }
}
