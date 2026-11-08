//! EXP-637 — auto-remove a RUN's worktree once the run is over.
//!
//! Decision 1: every repo-backed action/chat run gets its own worktree +
//! branch instead of writing to the trunk clone. That trades one problem
//! (agents leaving uncommitted work on master, autopull parked on
//! `SkippedDirty`) for another (a worktree per run piling up), so a finished
//! run cleans up after itself — but ONLY when it provably left nothing
//! behind:
//!
//! - tracked changes → keep (real work, and the ended strip warns about it);
//! - commits the base branch does not have → keep (unpushed work);
//! - the clone's launch gate is held by another launch → skip, the prune's
//!   own pass picks it up later;
//! - another LIVE run holds the branch or the worktree (FEED-53: a sibling
//!   reviewer on the same dir, a later run resumed into it) → skip.
//!
//! Untracked-only debris (build artifacts, the launcher's own seed files) is
//! removable — same rule the prune uses.

use std::path::{Path, PathBuf};

use crate::git_worktree::{run_git, validate_branch_arg};
use crate::prune::{worktree_dirty_state, DirtyState};
use crate::wip_save::{save_tracked_wip_commit, WipSave};

/// Everything the cleanup needs about one finished run — recorded by the
/// launcher on [`crate::launcher::PreparedLaunch`] and carried by the host
/// (desktop `LocalSessions`, CLI reaper) to the run's exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunCleanup {
    /// The clone the worktree hangs off (the `git worktree remove` cwd and
    /// the launch gate's key).
    pub clone: PathBuf,
    pub worktree: PathBuf,
    /// The run branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`).
    pub branch: String,
    /// What the branch was cut from — the "did anything land here" compare.
    pub base_branch: String,
}

/// What one [`remove_if_clean`] pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// Worktree removed, branch deleted.
    Removed,
    /// Tracked changes survive in the worktree — the run left work behind,
    /// which the ended strip reports as an amber warning (decision 7).
    KeptDirty(DirtyState),
    /// `n` commits the base branch does not have.
    KeptAhead(usize),
    /// Nothing was attempted; the reason is for logs, not the user.
    Skipped(SkipReason),
}

impl CleanupOutcome {
    /// Decision 7: did the run leave uncommitted changes behind?
    pub fn left_dirty(&self) -> bool {
        matches!(self, Self::KeptDirty(_))
    }
}

/// FEED-53 — what OTHER live runs hold right now, filled by the host at the
/// finished run's exit: its own live sessions (desktop `LocalSessions`, the
/// CLI's live-run list, the finished run already taken out) plus, through
/// [`Self::with_registry`], every run a live host process drives on the
/// shared data dir.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveHolders {
    pub branches: Vec<String>,
    pub worktrees: Vec<PathBuf>,
}

impl LiveHolders {
    /// Add the cwds of every run a live host process drives
    /// ([`crate::run_registry::live_host_runs`]), except `own_session`'s:
    /// the finished run's record may still carry its host's pid.
    pub fn with_registry(mut self, data_dir: &Path, own_session: &str) -> Self {
        self.worktrees.extend(
            crate::run_registry::live_host_runs(data_dir)
                .into_iter()
                .filter(|(session, _)| session != own_session)
                .map(|(_, cwd)| cwd),
        );
        self
    }

    /// Why `cleanup` must not be removed, if a live run holds it.
    fn holder_of(&self, cleanup: &RunCleanup) -> Option<String> {
        if self.branches.iter().any(|branch| *branch == cleanup.branch) {
            return Some(format!("branch {} is held by a live session", cleanup.branch));
        }
        let target = canonical(&cleanup.worktree);
        self.worktrees
            .iter()
            .any(|held| {
                let held = canonical(held);
                held == target || held.starts_with(&target)
            })
            .then(|| format!("worktree {} is held by a live session", cleanup.worktree.display()))
    }
}

/// Resolved when possible: git and the registry may spell one dir two ways
/// (a `/private` prefix on macOS temp dirs, a symlinked data dir).
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// FEED-53: another live run holds the branch or the worktree; the
    /// text says which.
    HeldByLiveRun(String),
    /// Another launch holds the clone's gate (EXP-478) — retry later.
    GateHeld,
    /// The worktree is already gone (a previous pass, a manual removal).
    Missing,
    /// A malformed branch/base never reaches git argv.
    InvalidBranch,
    /// `git worktree remove` / `branch -D` failed (locked, permissions, …).
    GitFailed(String),
}

/// Remove the run's worktree + branch iff it provably left nothing behind.
/// The run's registry record stays either way: a resume re-creates a
/// removed worktree on the recorded branch (`launcher::prepare_resume`), and the
/// record lives until the session history setting retires it (EXP-886).
///
/// FEED-53: never while another live run (`live`) holds the branch or the
/// worktree.
pub fn remove_if_clean(cleanup: &RunCleanup, live: &LiveHolders) -> CleanupOutcome {
    match crate::launch_gate::try_exclusive(&cleanup.clone, || remove_locked(cleanup, live)) {
        Some(outcome) => outcome,
        None => CleanupOutcome::Skipped(SkipReason::GateHeld),
    }
}

fn remove_locked(cleanup: &RunCleanup, live: &LiveHolders) -> CleanupOutcome {
    if !cleanup.worktree.is_dir() {
        return CleanupOutcome::Skipped(SkipReason::Missing);
    }
    if let Some(why) = live.holder_of(cleanup) {
        return CleanupOutcome::Skipped(SkipReason::HeldByLiveRun(why));
    }
    if validate_branch_arg(&cleanup.branch, "run cleanup").is_err()
        || validate_branch_arg(&cleanup.base_branch, "run cleanup base").is_err()
    {
        return CleanupOutcome::Skipped(SkipReason::InvalidBranch);
    }
    // EXP-1536 A: the HEAD the checks below vouch for; the removal refuses
    // once it moved.
    let Some(head) = crate::leftover::Verified::head_of(&cleanup.worktree) else {
        return CleanupOutcome::Skipped(SkipReason::GitFailed("HEAD unreadable".to_string()));
    };
    // Tracked work is sacred — a failing `git status` reads as dirty, so an
    // uninspectable worktree is never force-removed.
    let dirty = worktree_dirty_state(&cleanup.worktree);
    if dirty == DirtyState::TrackedChanges {
        return CleanupOutcome::KeptDirty(dirty);
    }
    // Anything committed on the run branch outlives the run: it is either
    // pushed (the PR keeps it) or local-only (deleting it would lose it).
    // Compared against the LOCAL base tip first, then origin's — a run whose
    // fetch failed must not read as "1000 commits ahead".
    let range = |base: &str| format!("{base}..{}", cleanup.branch);
    let ahead = count_commits(&cleanup.clone, &range(&format!("origin/{}", cleanup.base_branch)))
        .or_else(|| count_commits(&cleanup.clone, &range(&cleanup.base_branch)));
    match ahead {
        Some(0) => {}
        Some(n) => return CleanupOutcome::KeptAhead(n),
        // No comparable base at all: keep, the same conservative stance the
        // prune takes with `NoDefaultBranch`.
        None => {
            return CleanupOutcome::Skipped(SkipReason::GitFailed(
                "no comparable base branch".to_string(),
            ))
        }
    }
    // Untracked-only debris goes with the worktree. EXP-1536 A (#FEED-171):
    // refused when locked or changed since these checks (a clean tree that
    // grew untracked files included); build output first, `.git` last,
    // long-path safe; a failure leaves a valid worktree.
    let verified = crate::leftover::Verified { head, allow_untracked: dirty == DirtyState::UntrackedOnly };
    if let Err(err) = crate::leftover::remove_worktree(
        &cleanup.clone,
        &cleanup.worktree,
        &format!("git worktree remove ({})", cleanup.branch),
        &verified,
    ) {
        return CleanupOutcome::Skipped(SkipReason::GitFailed(err.detail));
    }
    // The branch carried nothing the base does not have — it goes too.
    let _ = run_git(
        Some(&cleanup.clone),
        &["branch", "-D", &cleanup.branch],
        None,
        &format!("git branch -D {}", cleanup.branch),
    );
    CleanupOutcome::Removed
}

/// NEW-5: the branch namespace a finished session's unpushed work is parked
/// under on `origin` (`exp/leftover-<id8>`).
pub const LEFTOVER_BRANCH_PREFIX: &str = "exp/leftover-";

/// How many `-<n>` suffixes [`finish_worktree`] tries when the leftover name
/// is taken by an unrelated history.
const LEFTOVER_SUFFIX_TRIES: usize = 9;

/// What one [`finish_worktree`] pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishOutcome {
    /// Worktree removed, local branch deleted. `leftover` names the
    /// `origin` branch the unpushed work (a WIP commit of tracked changes
    /// included) was pushed to — the caller posts it in the session's end
    /// message; `None` when everything was already on `origin`.
    Removed { leftover: Option<String> },
    /// No `origin` remote — nowhere to park work, so today's
    /// [`remove_if_clean`] rule applies: tracked changes keep the worktree…
    KeptDirty(DirtyState),
    /// … and so do commits the base does not have.
    KeptAhead(usize),
    /// Nothing removed; a failed WIP commit or push lands here as
    /// `GitFailed` and KEEPS the worktree (never lose work).
    Skipped(SkipReason),
}

impl From<CleanupOutcome> for FinishOutcome {
    fn from(outcome: CleanupOutcome) -> Self {
        match outcome {
            CleanupOutcome::Removed => Self::Removed { leftover: None },
            CleanupOutcome::KeptDirty(state) => Self::KeptDirty(state),
            CleanupOutcome::KeptAhead(n) => Self::KeptAhead(n),
            CleanupOutcome::Skipped(reason) => Self::Skipped(reason),
        }
    }
}

/// NEW-5 — the AUTOMATIC end of a session's worktree: nothing is kept on
/// disk once the session is over, and nothing is lost either.
///
/// - clean, and every commit already on `origin` (or the branch landed in
///   `origin/<base>`, e.g. squash-merged) → remove the worktree, delete the
///   local branch;
/// - otherwise commit the tracked changes as ONE WIP commit
///   ([`crate::wip_save::save_tracked_wip_commit`]; untracked debris is never
///   committed and goes with the worktree), push `HEAD` to
///   `origin/exp/leftover-<id8>`, then remove worktree + local branch and
///   name the leftover branch in the outcome;
/// - no `origin` remote → [`remove_if_clean`]'s keep-dirty/keep-ahead rule.
///
/// Leftover name collisions: the push is a PLAIN push (never forced). A
/// leftover branch that already carries this history (a retried pass, a
/// resumed session ending again on top of it) fast-forwards; one carrying
/// an unrelated history is never overwritten — the push moves on to
/// `exp/leftover-<id8>-2` … `-9`. Any other push failure (auth, network)
/// keeps the worktree and reports `Skipped(GitFailed)`.
///
/// The push uses the clone's own repo-local credential helper (EXP-73) —
/// callers refresh the token first when it may have expired.
///
/// Same shields as [`remove_if_clean`]: the clone's launch gate, live
/// holders, a missing worktree, malformed branch names.
pub fn finish_worktree(cleanup: &RunCleanup, live: &LiveHolders, session_id8: &str) -> FinishOutcome {
    finish_worktree_with(cleanup, live, session_id8, false)
}

/// EXP-1542 (#FEED-169): [`finish_worktree`] for a session that MOVED to
/// another device: its continuation there cuts from `origin/<branch>`, so
/// the work (the WIP commit included) is handed on under the session's OWN
/// branch name first ([`hand_off_head`], [`push_session_branch`]: a branch
/// origin already has gets it on `refs/exp-wip/<branch>`, never on the
/// branch: #EXP-1622), a landed branch is never pushed back, a `HEAD` off
/// the branch is parked. Only when origin's branch diverged does it fall
/// back to `exp/leftover-<id8>`, which the new device tries next; any other
/// push failure keeps the worktree.
pub fn finish_moved_worktree(cleanup: &RunCleanup, live: &LiveHolders, session_id8: &str) -> FinishOutcome {
    finish_worktree_with(cleanup, live, session_id8, true)
}

fn finish_worktree_with(
    cleanup: &RunCleanup,
    live: &LiveHolders,
    session_id8: &str,
    push_own: bool,
) -> FinishOutcome {
    let outcome = match crate::launch_gate::try_exclusive(&cleanup.clone, || {
        finish_locked(cleanup, live, session_id8, push_own)
    }) {
            Some(outcome) => outcome,
            None => FinishOutcome::Skipped(SkipReason::GateHeld),
        };
    // EXP-1375: a worktree that stays loses its regenerable build output now.
    crate::worktree_lifecycle::after_finish(&cleanup.worktree, &outcome);
    outcome
}

fn finish_locked(cleanup: &RunCleanup, live: &LiveHolders, session_id8: &str, push_own: bool) -> FinishOutcome {
    if !cleanup.worktree.is_dir() {
        return FinishOutcome::Skipped(SkipReason::Missing);
    }
    if let Some(why) = live.holder_of(cleanup) {
        return FinishOutcome::Skipped(SkipReason::HeldByLiveRun(why));
    }
    let leftover = format!("{LEFTOVER_BRANCH_PREFIX}{session_id8}");
    let id_ok = !session_id8.is_empty()
        && !session_id8.starts_with(['-', '.'])
        && session_id8.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !session_id8.contains("..");
    if !id_ok
        || validate_branch_arg(&cleanup.branch, "run finish").is_err()
        || validate_branch_arg(&cleanup.base_branch, "run finish base").is_err()
        || validate_branch_arg(&leftover, "run finish leftover").is_err()
    {
        return FinishOutcome::Skipped(SkipReason::InvalidBranch);
    }
    // Nowhere to park work: today's keep rule.
    if !has_origin(&cleanup.clone) {
        return remove_locked(cleanup, live).into();
    }
    // Tracked changes become one WIP commit on the worktree's HEAD. A
    // refused save (detached mid-rebase, a failing commit) keeps everything.
    if let WipSave::Failed(detail) = save_tracked_wip_commit(&cleanup.worktree) {
        return FinishOutcome::Skipped(SkipReason::GitFailed(detail));
    }
    // EXP-1536 A: the HEAD the push checks below vouch for; the removal
    // refuses once it moved.
    let Some(head) = crate::leftover::Verified::head_of(&cleanup.worktree) else {
        return FinishOutcome::Skipped(SkipReason::GitFailed("HEAD unreadable".to_string()));
    };
    // An uninspectable tree after the save is never removed.
    if worktree_dirty_state(&cleanup.worktree) == DirtyState::TrackedChanges {
        return FinishOutcome::Skipped(SkipReason::GitFailed(
            "tracked changes survived the WIP commit".to_string(),
        ));
    }
    let Some(unpushed) = unpushed_count(&cleanup.worktree) else {
        return FinishOutcome::Skipped(SkipReason::GitFailed(
            "could not count the unpushed commits".to_string(),
        ));
    };
    // #EXP-1622: an ending session's own branch is the agent's to push
    // (it may carry an open PR, whose CI a device push would restart):
    // unpushed work is parked on the leftover branch, never on it.
    let own = crate::auto_push::PushTarget {
        worktree: cleanup.worktree.clone(),
        branch: cleanup.branch.clone(),
        base_branch: Some(cleanup.base_branch.clone()),
    };
    // EXP-1542: a moved session's own branch carries the work on, HEAD
    // checked ([`hand_off_head`]: never a landed branch pushed back, HEAD
    // elsewhere parked).
    let leftover = if push_own {
        match hand_off_head(cleanup, &leftover) {
            Ok(HandOff::Parked(parked)) => Some(parked),
            Ok(_) => None,
            Err(detail) => return FinishOutcome::Skipped(SkipReason::GitFailed(detail)),
        }
    } else if unpushed == 0 || head_landed(cleanup) {
        None
    } else {
        match push_leftover(&cleanup.worktree, "HEAD", &leftover) {
            Ok(pushed) => Some(pushed),
            Err(detail) => return FinishOutcome::Skipped(SkipReason::GitFailed(detail)),
        }
    };
    // Everything is on a branch now: the uncommitted-work snapshot is spent.
    // #EXP-1622: a moved session's hand-off may have parked its work ON
    // that ref for the next device: it stays.
    if !push_own {
        if let Err(detail) = crate::auto_push::drop_wip(&own) {
            log::info!("run finish {}: the WIP snapshot stays on origin: {detail}", cleanup.branch);
        }
    }
    // Everything worth keeping is on `origin` now; untracked debris goes
    // with the worktree (EXP-1536 A: refused when locked or changed since;
    // content first, `.git` last).
    let verified = crate::leftover::Verified { head, allow_untracked: true };
    if let Err(err) = crate::leftover::remove_worktree(
        &cleanup.clone,
        &cleanup.worktree,
        &format!("git worktree remove ({})", cleanup.branch),
        &verified,
    ) {
        return FinishOutcome::Skipped(SkipReason::GitFailed(err.detail));
    }
    let _ = run_git(
        Some(&cleanup.clone),
        &["branch", "-D", &cleanup.branch],
        None,
        &format!("git branch -D {}", cleanup.branch),
    );
    FinishOutcome::Removed { leftover }
}

/// Whether `clone` has an `origin` remote configured.
fn has_origin(clone: &Path) -> bool {
    run_git(Some(clone), &["remote", "get-url", "origin"], None, "git remote get-url origin").is_ok()
}

/// Commits on `HEAD` no `origin/*` remote-tracking ref has. Stale tracking
/// refs only ever over-count (a leftover push of already-pushed work), the
/// safe direction.
fn unpushed_count(worktree: &Path) -> Option<usize> {
    run_git(
        Some(worktree),
        &["rev-list", "--count", "HEAD", "--not", "--remotes=origin"],
        None,
        "git rev-list --count (unpushed)",
    )
    .ok()
    .and_then(|out| out.trim().parse::<usize>().ok())
}

/// Whether the worktree's `HEAD` work is contained in `origin/<base>` (a
/// squash or cherry-pick counts).
fn head_landed(cleanup: &RunCleanup) -> bool {
    rev_landed(cleanup, "HEAD")
}

/// [`head_landed`] for any revision of the worktree (EXP-1542: the session
/// branch's own ref when `HEAD` is elsewhere).
fn rev_landed(cleanup: &RunCleanup, rev: &str) -> bool {
    let origin_base = format!("origin/{}", cleanup.base_branch);
    run_git(
        Some(&cleanup.clone),
        &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{origin_base}")],
        None,
        "git rev-parse origin base",
    )
    .is_ok()
        && rev_commit(&cleanup.worktree, rev).is_some_and(|commit| {
            crate::prune::branch_landed(&cleanup.clone, &commit, &origin_base, &Default::default())
        })
}

/// S-4: [`finish_worktree`] would take NOTHING off the machine — the tree is
/// clean (untracked files included) and every commit is pushed or landed.
/// The sweep removes an open chat's worktree only then.
pub(crate) fn nothing_to_park(cleanup: &RunCleanup) -> bool {
    worktree_dirty_state(&cleanup.worktree) == DirtyState::Clean
        && (unpushed_count(&cleanup.worktree) == Some(0) || head_landed(cleanup))
}

fn rev_commit(worktree: &Path, rev: &str) -> Option<String> {
    run_git(Some(worktree), &["rev-parse", "--verify", &format!("{rev}^{{commit}}")], None, "git rev-parse")
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|sha| !sha.is_empty())
}

/// Push `src` (`HEAD`, or a branch ref) to `origin` as `name`, then `name-2` … when an unrelated
/// history holds the name (a plain push, never forced). The pushed name, or
/// the failure detail.
fn push_leftover(worktree: &Path, src: &str, name: &str) -> Result<String, String> {
    let mut last = String::new();
    for attempt in 1..=LEFTOVER_SUFFIX_TRIES {
        let target = if attempt == 1 { name.to_string() } else { format!("{name}-{attempt}") };
        let refspec = format!("{src}:refs/heads/{target}");
        match run_git(
            Some(worktree),
            &["push", "--no-verify", "origin", &refspec],
            None,
            &format!("git push origin {target}"),
        ) {
            Ok(_) => return Ok(target),
            Err(err) if is_rejected_push(&err.detail) => last = err.detail,
            Err(err) => return Err(err.detail),
        }
    }
    Err(format!("every leftover branch name is taken: {last}"))
}

/// EXP-1542 (#FEED-169) / #EXP-1622: hand `src` (`HEAD`, or the session
/// branch's own ref) on to the next device under the session's branch name.
/// - origin has NO such branch (never pushed: no PR can exist): a plain
///   push creates it, as before;
/// - origin HAS it (it may carry an open PR, whose CI a device push would
///   restart, with commits the agent never released): the branch is the
///   agent's to push, so the work goes to `refs/exp-wip/<branch>` instead,
///   which the next device's prepare fast-forwards onto
///   ([`crate::auto_push::restore_wip`]); nothing to hand on when origin
///   already has it all;
/// - origin's branch diverged from `src`: refused, never overwritten (the
///   caller parks a leftover).
///
/// The failure detail is logged and returned.
fn push_session_branch(worktree: &Path, src: &str, branch: &str) -> Result<(), String> {
    validate_branch_arg(branch, "moved session branch").map_err(|err| err.detail)?;
    let refused = |detail: String| {
        log::warn!("moved session: {branch} not handed on, parking on a leftover branch: {detail}");
        detail
    };
    let remote = format!("refs/remotes/origin/{branch}");
    let on_origin = run_git(
        Some(worktree),
        &["fetch", "--quiet", "--no-tags", "origin", &format!("+refs/heads/{branch}:{remote}")],
        None,
        &format!("git fetch origin {branch}"),
    )
    .is_ok();
    if !on_origin {
        let refspec = format!("{src}:refs/heads/{branch}");
        return run_git(Some(worktree), &["push", "--no-verify", "origin", &refspec], None, &format!("git push origin {branch}"))
            .map(|_| ())
            .map_err(|err| refused(err.detail));
    }
    let Some(commit) = rev_commit(worktree, src) else {
        return Err(refused(format!("{src} unreadable")));
    };
    let ancestor = |a: &str, b: &str| {
        run_git(Some(worktree), &["merge-base", "--is-ancestor", a, b], None, "git merge-base --is-ancestor").is_ok()
    };
    // Origin's branch already holds it all: nothing to hand on.
    if ancestor(&commit, &remote) {
        return Ok(());
    }
    if !ancestor(&remote, &commit) {
        return Err(refused(format!("origin's {branch} diverged from {src}")));
    }
    let target = crate::auto_push::PushTarget {
        worktree: worktree.to_path_buf(),
        branch: branch.to_string(),
        base_branch: None,
    };
    crate::auto_push::push_wip_commit(&target, &commit).map_err(refused)
}

/// What a moved session's hand-off did ([`hand_off_moved_branch`],
/// [`finish_moved_worktree`]); [`HandOff::outcome`] is what the server
/// records (`sessions.handedOff`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandOff {
    /// The work is on `origin/<branch>` (pushed now, or already there).
    Pushed,
    /// (Some of) the work was parked on this branch: the own-branch push was
    /// refused, or `HEAD` was not on the session's branch.
    Parked(String),
    /// Nothing to hand on (no worktree and nothing parked, the work already
    /// landed, an unusable branch name): the continuation goes on without
    /// waiting.
    Nothing(String),
    /// Not now (the launch gate, a live holder, a failed push or WIP
    /// commit): the worktree stays as it was; the pickup tries again.
    Retry(String),
}

impl HandOff {
    /// `(outcome, leftover)` for `sessions.handedOff`; `None` = not done
    /// yet ([`HandOff::Retry`]): nothing is reported.
    pub fn outcome(&self) -> Option<(&'static str, Option<&str>)> {
        match self {
            Self::Pushed => Some(("pushed", None)),
            Self::Parked(branch) => Some(("parked", Some(branch.as_str()))),
            Self::Nothing(_) => Some(("none", None)),
            Self::Retry(_) => None,
        }
    }
}

/// EXP-1542: hand a moved session's committed work on (the launch gate
/// held, the WIP commit made). On the session's branch: a landed branch
/// (merged, maybe deleted on origin) is never pushed back; else `HEAD` goes
/// to `origin/<branch>`, or is parked on `leftover` when that is refused.
/// Elsewhere (another branch, detached): the session branch's OWN ref goes
/// to `origin/<branch>` (parked when refused) and `HEAD`'s unpushed commits
/// are parked too; nothing is lost. `Err` = a push failed outright.
fn hand_off_head(cleanup: &RunCleanup, leftover: &str) -> Result<HandOff, String> {
    let worktree = &cleanup.worktree;
    let branch = cleanup.branch.as_str();
    if crate::git_worktree::worktree_head(worktree).as_deref() == Some(branch) {
        if head_landed(cleanup) {
            return Ok(HandOff::Nothing("landed".to_string()));
        }
        if push_session_branch(worktree, "HEAD", branch).is_ok() {
            return Ok(HandOff::Pushed);
        }
        return push_leftover(worktree, "HEAD", leftover).map(HandOff::Parked);
    }
    log::warn!("moved session: the worktree is not on {branch}: pushing {branch} itself, parking HEAD");
    let own = format!("refs/heads/{branch}");
    let mut pushed = false;
    let mut parked: Option<String> = None;
    let own_exists =
        run_git(Some(worktree), &["rev-parse", "--verify", "--quiet", &own], None, "git rev-parse branch").is_ok();
    if own_exists && !rev_landed(cleanup, &own) {
        if push_session_branch(worktree, &own, branch).is_ok() {
            pushed = true;
        } else {
            parked = Some(push_leftover(worktree, &own, leftover)?);
        }
    }
    if unpushed_count(worktree) != Some(0) && !head_landed(cleanup) {
        let head = push_leftover(worktree, "HEAD", leftover)?;
        parked.get_or_insert(head);
    }
    Ok(match parked {
        Some(branch) => HandOff::Parked(branch),
        None if pushed => HandOff::Pushed,
        None => HandOff::Nothing("nothing to hand on".to_string()),
    })
}

/// EXP-1542 (#FEED-169): the hand-off of a session that moved to another
/// device while NO engine ran it here (an idle or ended session): its
/// tracked changes become one WIP commit and its work goes to the session's
/// own branch ([`hand_off_head`]), else `exp/leftover-<id8>`. The worktree
/// is never removed here: the nightly sweep
/// ([`crate::prune::sweep_stale_worktrees`]) finishes it. A worktree that
/// is already gone (the run's finish removed it) hands on what that finish
/// parked: the leftover branch when origin has it. Shielded like
/// [`finish_worktree`]: the clone's launch gate, live holders.
pub fn hand_off_moved_branch(cleanup: &RunCleanup, live: &LiveHolders, session_id8: &str) -> HandOff {
    let run = || {
        let leftover = format!("{LEFTOVER_BRANCH_PREFIX}{session_id8}");
        if session_id8.is_empty()
            || !session_id8.chars().all(|c| c.is_ascii_alphanumeric())
            || validate_branch_arg(&cleanup.branch, "hand-off").is_err()
        {
            return HandOff::Nothing("invalid branch".to_string());
        }
        if !cleanup.worktree.is_dir() {
            return if cleanup.clone.is_dir() && origin_has_branch(&cleanup.clone, &leftover) {
                HandOff::Parked(leftover)
            } else {
                HandOff::Nothing("no worktree".to_string())
            };
        }
        if let Some(why) = live.holder_of(cleanup) {
            return HandOff::Retry(why);
        }
        if let WipSave::Failed(detail) = save_tracked_wip_commit(&cleanup.worktree) {
            return HandOff::Retry(detail);
        }
        hand_off_head(cleanup, &leftover).unwrap_or_else(HandOff::Retry)
    };
    crate::launch_gate::try_exclusive(&cleanup.clone, run)
        .unwrap_or_else(|| HandOff::Retry("launch gate held".to_string()))
}

/// Whether `origin` has `refs/heads/<branch>` (`git ls-remote`).
fn origin_has_branch(clone: &Path, branch: &str) -> bool {
    run_git(
        Some(clone),
        &["ls-remote", "--exit-code", "--heads", "origin", &format!("refs/heads/{branch}")],
        None,
        "git ls-remote",
    )
    .is_ok()
}

/// EXP-1542: report a hand-off's terminal outcome for `move_id`
/// (`sessions.handedOff`). `true` = recorded (or nothing to record yet:
/// [`HandOff::Retry`] reports nothing and returns `false`).
pub fn report_hand_off(trpc: &api::trpc::TrpcClient, move_id: &str, outcome: &HandOff) -> bool {
    let Some((outcome, leftover)) = outcome.outcome() else {
        return false;
    };
    match api::session_device::handed_off(trpc, move_id, outcome, leftover) {
        Ok(()) => true,
        Err(err) => {
            log::warn!("hand-off of move {move_id} not reported: {err}");
            false
        }
    }
}

/// EXP-1542 (review): a host whose ONE run just exited (the foreground
/// `code`/`run`, which never runs the daemon's finish): a run marked moved
/// hands its worktree on now ([`hand_off_moved_branch`], kept) and reports
/// it when the move's record was known. Whatever is left unreported, the
/// daemon's pass does once this process is gone
/// (`coding::pickup::HandOffStep::Wait`). `None` = it never moved.
pub fn hand_off_on_exit(
    trpc: &api::trpc::TrpcClient,
    session_id: &str,
    cleanup: Option<&RunCleanup>,
) -> Option<HandOff> {
    let marked = take_moved(session_id)?;
    let cleanup = cleanup?;
    let outcome = hand_off_moved_branch(cleanup, &LiveHolders::default(), &session_id8(session_id));
    log::info!("session {session_id} moved away: handed off {} on exit: {outcome:?}", cleanup.branch);
    if let HandOff::Parked(parked) = &outcome {
        post_leftover_note(trpc, session_id, parked);
    }
    if let Some(move_id) = marked {
        report_hand_off(trpc, &move_id, &outcome);
    }
    Some(outcome)
}

/// EXP-1542: the sessions this process saw MOVE to another device while
/// their engine ran here ([`mark_moved`]), with the move's id when the
/// record had synced; their run's finish ([`finish_session_worktree`])
/// hands the branch on and reports it.
static MOVED: std::sync::Mutex<Vec<(String, Option<String>)>> = std::sync::Mutex::new(Vec::new());

/// EXP-1542: `session_id` moved away (`coding::pickup` saw it): its run's
/// finish hands the branch on (and reports `move_id`'s hand-off).
pub fn mark_moved(session_id: &str, move_id: Option<&str>) {
    let mut moved = MOVED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match moved.iter_mut().find(|(id, _)| id == session_id) {
        Some((_, known)) => {
            if known.is_none() {
                *known = move_id.map(str::to_string);
            }
        }
        None => moved.push((session_id.to_string(), move_id.map(str::to_string))),
    }
}

/// Whether `session_id` was [`mark_moved`] (with its move id), forgetting it.
pub fn take_moved(session_id: &str) -> Option<Option<String>> {
    let mut moved = MOVED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let at = moved.iter().position(|(id, _)| id == session_id)?;
    Some(moved.remove(at).1)
}

/// A push the remote REFUSED for the ref's existing history (as opposed to
/// auth, network, or a missing remote).
fn is_rejected_push(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    ["non-fast-forward", "fetch first", "[rejected]", "stale info"]
        .iter()
        .any(|needle| detail.contains(needle))
}

/// NEW-5: a session id's 8-character tag — its first 8 ASCII alphanumerics,
/// lowercased: the `<id8>` of `exp/session-<id8>` and
/// `exp/leftover-<id8>`.
pub fn session_id8(session_id: &str) -> String {
    session_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// NEW-5: the ONE line a session's conversation gets when its finished
/// worktree's unpushed work was parked on `branch`.
pub fn leftover_note(branch: &str) -> String {
    format!("Unpushed work was kept on branch {branch}.")
}

/// NEW-5 — a finished session's worktree end ([`finish_worktree`]), then,
/// when unpushed work was parked on a leftover branch, ONE line naming it in
/// the session's conversation (`sessions.reply`; posted even when the
/// session has ended — the chat shows it). Blocking (git + network): run
/// it off any UI thread. A lost post is logged, never retried (the branch
/// stands on `origin` either way).
pub fn finish_session_worktree(
    trpc: &api::trpc::TrpcClient,
    session_id: &str,
    cleanup: &RunCleanup,
    live: &LiveHolders,
) -> FinishOutcome {
    // EXP-1542: a moved session's branch goes to origin first, and the
    // move's record hears it (the pickup reports a move it had no id for).
    let moved = take_moved(session_id);
    let outcome = if moved.is_some() {
        finish_moved_worktree(cleanup, live, &session_id8(session_id))
    } else {
        finish_worktree(cleanup, live, &session_id8(session_id))
    };
    if let FinishOutcome::Removed { leftover: Some(branch) } = &outcome {
        post_leftover_note(trpc, session_id, branch);
    }
    if let (Some(Some(move_id)), FinishOutcome::Removed { leftover }) = (&moved, &outcome) {
        let handed = leftover.clone().map_or(HandOff::Pushed, HandOff::Parked);
        report_hand_off(trpc, move_id, &handed);
    }
    outcome
}

/// NEW-5: post [`leftover_note`] into `session_id`'s conversation —
/// best-effort, logged on failure. #EXP-1618: through `sessions.leftover`
/// (the server's line + an inbox row for the starter, so a leftover is
/// never silently orphaned); a server without it gets the old
/// `sessions.reply` line.
pub fn post_leftover_note(trpc: &api::trpc::TrpcClient, session_id: &str, branch: &str) {
    match api::session_device::leftover(trpc, session_id, branch) {
        Ok(()) => return,
        Err(err) => log::info!("run cleanup [{session_id}]: sessions.leftover failed ({err}); posting a reply"),
    }
    if let Err(err) = api::session_device::reply(
        trpc,
        session_id,
        &crate::sealing::seal_body_or_plain(session_id, &leftover_note(branch)),
        None,
    ) {
        log::warn!("run cleanup [{session_id}]: leftover note for {branch} not posted: {err}");
    }
}

/// `git rev-list --count <range>` — `None` when the range does not resolve.
/// Also the launcher's (EXP-834) "did this re-created worktree keep its
/// commits?" probe, which decides which reclaimed-workspace note a resume
/// hears; the two ask git the same question, so they share one caller.
pub(crate) fn count_commits(clone: &Path, range: &str) -> Option<usize> {
    run_git(
        Some(clone),
        &["rev-list", "--count", range],
        None,
        "git rev-list --count",
    )
    .ok()
    .and_then(|out| out.trim().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .expect("git");
        assert!(
            status.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-run-cleanup-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real clone with an `origin/main` remote-tracking ref and one run
    /// worktree on `exp/chat-1a2b3c4d`.
    fn fixture(tag: &str) -> (PathBuf, RunCleanup) {
        let root = temp_dir(tag);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "--initial-branch=main", "."]);

        let clone = root.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "--initial-branch=main", "."]);
        std::fs::write(clone.join("README.md"), "hello\n").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-m", "base"]);
        git(&clone, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&clone, &["push", "-u", "origin", "main"]);

        let worktree = root.join("wt");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-b",
                "exp/chat-1a2b3c4d",
                worktree.to_str().unwrap(),
                "origin/main",
            ],
        );
        let cleanup = RunCleanup {
            clone,
            worktree,
            branch: "exp/chat-1a2b3c4d".to_string(),
            base_branch: "main".to_string(),
        };
        (root, cleanup)
    }

    fn branch_exists(clone: &Path, branch: &str) -> bool {
        run_git(
            Some(clone),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
            None,
            "verify",
        )
        .is_ok()
    }

    #[test]
    fn a_clean_run_worktree_is_removed_with_its_branch() {
        let (root, cleanup) = fixture("clean");
        assert_eq!(remove_if_clean(&cleanup, &LiveHolders::default()), CleanupOutcome::Removed);
        assert!(!cleanup.worktree.exists());
        assert!(!branch_exists(&cleanup.clone, &cleanup.branch));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn untracked_debris_does_not_save_a_worktree() {
        let (root, cleanup) = fixture("untracked");
        std::fs::write(cleanup.worktree.join("scratch.log"), "noise").unwrap();
        assert_eq!(remove_if_clean(&cleanup, &LiveHolders::default()), CleanupOutcome::Removed);
        assert!(!cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_commit_on_the_run_branch_keeps_it() {
        let (root, cleanup) = fixture("ahead");
        std::fs::write(cleanup.worktree.join("work.txt"), "real work\n").unwrap();
        git(&cleanup.worktree, &["add", "."]);
        git(&cleanup.worktree, &["commit", "-m", "run work"]);
        assert_eq!(remove_if_clean(&cleanup, &LiveHolders::default()), CleanupOutcome::KeptAhead(1));
        assert!(cleanup.worktree.exists());
        assert!(branch_exists(&cleanup.clone, &cleanup.branch));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tracked_changes_keep_it_and_report_dirty() {
        let (root, cleanup) = fixture("dirty");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        let outcome = remove_if_clean(&cleanup, &LiveHolders::default());
        assert_eq!(outcome, CleanupOutcome::KeptDirty(DirtyState::TrackedChanges));
        assert!(outcome.left_dirty(), "the ended strip warns on this");
        assert!(cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_launch_gate_skips_the_pass() {
        let (root, cleanup) = fixture("gate");
        let hold = crate::launch_gate::hold(&cleanup.clone);
        assert_eq!(
            remove_if_clean(&cleanup, &LiveHolders::default()),
            CleanupOutcome::Skipped(SkipReason::GateHeld)
        );
        assert!(cleanup.worktree.exists());
        drop(hold);
        // ... and the next pass, once the launch released it, removes it.
        assert_eq!(remove_if_clean(&cleanup, &LiveHolders::default()), CleanupOutcome::Removed);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// FEED-53: a worktree or branch another LIVE run holds is never
    /// removed under it, and the outcome says why; once released, it goes.
    #[test]
    fn a_worktree_or_branch_held_by_a_live_run_is_kept() {
        let (root, cleanup) = fixture("held");
        let by_branch = LiveHolders {
            branches: vec![cleanup.branch.clone()],
            ..LiveHolders::default()
        };
        assert_eq!(
            remove_if_clean(&cleanup, &by_branch),
            CleanupOutcome::Skipped(SkipReason::HeldByLiveRun(format!(
                "branch {} is held by a live session",
                cleanup.branch
            )))
        );
        // A live run whose cwd is the worktree (or inside it), however the
        // path is spelled.
        let by_dir = LiveHolders {
            worktrees: vec![cleanup.worktree.join("sub")],
            ..LiveHolders::default()
        };
        std::fs::create_dir_all(cleanup.worktree.join("sub")).unwrap();
        let outcome = remove_if_clean(&cleanup, &by_dir);
        assert!(
            matches!(&outcome, CleanupOutcome::Skipped(SkipReason::HeldByLiveRun(why)) if why.starts_with("worktree ")),
            "{outcome:?}"
        );
        assert!(cleanup.worktree.exists());
        assert!(branch_exists(&cleanup.clone, &cleanup.branch));
        // An unrelated live run holds nothing of it.
        let other = LiveHolders {
            branches: vec!["exp/chat-ffffffff".to_string()],
            worktrees: vec![root.join("elsewhere")],
        };
        assert_eq!(remove_if_clean(&cleanup, &other), CleanupOutcome::Removed);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// FEED-53: the cross-process feed. A live host's record on the dir
    /// holds it; the finished run's OWN record does not.
    #[test]
    fn the_registry_feeds_live_holders_except_the_run_itself() {
        let (root, cleanup) = fixture("registry");
        let data_dir = root.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        for (session, cwd) in [("s-own", cleanup.worktree.clone()), ("s-other", root.join("x"))] {
            let mut record = crate::run_registry::sample_record(session);
            record.cwd = cwd;
            record.host_pid = Some(std::process::id());
            crate::run_registry::record(&data_dir, record);
        }
        let own = LiveHolders::default().with_registry(&data_dir, "s-own");
        assert_eq!(own.worktrees, vec![root.join("x")]);
        assert_eq!(remove_if_clean(&cleanup, &own), CleanupOutcome::Removed);
        let (root2, cleanup2) = fixture("registry2");
        let mut record = crate::run_registry::sample_record("s-sib");
        record.cwd = cleanup2.worktree.clone();
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&data_dir, record);
        let sibling = LiveHolders::default().with_registry(&data_dir, "s-own");
        assert!(matches!(
            remove_if_clean(&cleanup2, &sibling),
            CleanupOutcome::Skipped(SkipReason::HeldByLiveRun(_))
        ));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }

    #[test]
    fn an_already_removed_worktree_is_a_no_op() {
        let (root, cleanup) = fixture("missing");
        assert_eq!(remove_if_clean(&cleanup, &LiveHolders::default()), CleanupOutcome::Removed);
        assert_eq!(
            remove_if_clean(&cleanup, &LiveHolders::default()),
            CleanupOutcome::Skipped(SkipReason::Missing)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- NEW-5: finish_worktree ----

    fn origin_of(cleanup: &RunCleanup) -> PathBuf {
        cleanup.clone.parent().unwrap().join("origin.git")
    }

    fn origin_has(cleanup: &RunCleanup, branch: &str) -> bool {
        branch_exists(&origin_of(cleanup), branch)
    }

    fn origin_log(cleanup: &RunCleanup, branch: &str) -> String {
        run_git(Some(&origin_of(cleanup)), &["log", "-1", "--format=%s", branch], None, "log")
            .unwrap()
            .trim()
            .to_string()
    }

    fn commit_in(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
        git(dir, &["add", name]);
        git(dir, &["commit", "-q", "-m", &format!("add {name}")]);
    }

    #[test]
    fn finish_removes_a_clean_worktree_whose_commits_are_pushed() {
        let (root, cleanup) = fixture("finish-pushed");
        commit_in(&cleanup.worktree, "work.txt", "real work\n");
        git(&cleanup.worktree, &["push", "-q", "origin", "exp/chat-1a2b3c4d"]);
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(!cleanup.worktree.exists());
        assert!(!branch_exists(&cleanup.clone, &cleanup.branch));
        assert!(!origin_has(&cleanup, "exp/leftover-1a2b3c4d"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finish_removes_an_untouched_worktree_without_a_leftover() {
        let (root, cleanup) = fixture("finish-clean");
        std::fs::write(cleanup.worktree.join("scratch.log"), "noise").unwrap();
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(!cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #EXP-1618: origin's copy of the session's own branch moved on
    /// without this worktree (another device pushed, or the agent rewrote
    /// it): the finish can no longer fast-forward it, the leftover branch
    /// takes the work.
    fn diverge_own_branch(cleanup: &RunCleanup) {
        git(&cleanup.clone, &["checkout", "-q", "-b", "elsewhere", "origin/main"]);
        commit_in(&cleanup.clone, "elsewhere.txt", "another device\n");
        git(&cleanup.clone, &["push", "-q", "origin", &format!("elsewhere:refs/heads/{}", cleanup.branch)]);
        git(&cleanup.clone, &["checkout", "-q", "main"]);
    }

    /// #EXP-1622: a finish never pushes the session's OWN branch (it may
    /// carry an open PR): unpushed commits and the tracked WIP commit are
    /// parked on the leftover branch.
    #[test]
    fn finish_parks_dirty_work_and_never_pushes_the_sessions_own_branch() {
        let (root, cleanup) = fixture("finish-own");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d".to_string()) }
        );
        assert!(!cleanup.worktree.exists());
        assert!(!origin_has(&cleanup, &cleanup.branch));
        assert_eq!(origin_log(&cleanup, "exp/leftover-1a2b3c4d"), crate::wip_save::WIP_FINISH_COMMIT_MESSAGE);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// EXP-1542 (#FEED-169): a MOVED session's finish pushes its work (the
    /// WIP commit included) to its OWN branch, so the continuation on the
    /// new device cuts from `origin/<branch>`; no leftover branch.
    #[test]
    fn a_moved_finish_pushes_the_session_branch_itself() {
        let (root, cleanup) = fixture("finish-moved");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        assert_eq!(
            finish_moved_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(!cleanup.worktree.exists());
        assert!(origin_has(&cleanup, "exp/chat-1a2b3c4d"));
        assert!(!origin_has(&cleanup, "exp/leftover-1a2b3c4d"));
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), crate::wip_save::WIP_FINISH_COMMIT_MESSAGE);
        let files = run_git(
            Some(&origin_of(&cleanup)),
            &["ls-tree", "-r", "--name-only", "exp/chat-1a2b3c4d"],
            None,
            "ls-tree",
        )
        .unwrap();
        assert!(files.contains("local.txt"), "{files}");
        // EXP-1542 (review): an untouched (or merged) branch has landed:
        // never pushed back (a merged branch deleted on origin stays gone);
        // the continuation cuts from the base, which has it all.
        let (root2, untouched) = fixture("finish-moved-untouched");
        assert_eq!(
            finish_moved_worktree(&untouched, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(!origin_has(&untouched, "exp/chat-1a2b3c4d"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }

    /// EXP-1542: a refused own-branch push (origin's branch diverged) is
    /// never forced: the work is parked on `exp/leftover-<id8>` instead and
    /// origin's branch keeps its history.
    #[test]
    fn a_moved_finish_whose_branch_push_is_refused_parks_a_leftover() {
        let (root, cleanup) = fixture("finish-moved-refused");
        git(&cleanup.clone, &["branch", "other", "main"]);
        let other = root.join("other");
        git(&cleanup.clone, &["worktree", "add", other.to_str().unwrap(), "other"]);
        commit_in(&other, "theirs.txt", "theirs\n");
        git(&other, &["push", "-q", "origin", "other:refs/heads/exp/chat-1a2b3c4d"]);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        assert_eq!(
            finish_moved_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d".to_string()) }
        );
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), "add theirs.txt");
        assert_eq!(origin_log(&cleanup, "exp/leftover-1a2b3c4d"), "add local.txt");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// EXP-1542: a moved session with no engine here: its worktree's work
    /// is pushed to its own branch and the worktree KEPT (the sweep
    /// finishes it); the moved mark routes a run's finish the same way.
    #[test]
    fn a_hand_off_pushes_the_branch_and_keeps_the_worktree() {
        let (root, cleanup) = fixture("hand-off");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        assert_eq!(hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"), HandOff::Pushed);
        assert!(cleanup.worktree.join("local.txt").exists());
        assert!(branch_exists(&cleanup.clone, &cleanup.branch));
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), crate::wip_save::WIP_FINISH_COMMIT_MESSAGE);
        assert_eq!(HandOff::Pushed.outcome(), Some(("pushed", None)));
        // A live holder shields it: not now, retried (nothing reported).
        let held = LiveHolders { branches: vec![cleanup.branch.clone()], ..LiveHolders::default() };
        let shielded = hand_off_moved_branch(&cleanup, &held, "1a2b3c4d");
        assert!(matches!(shielded, HandOff::Retry(_)), "{shielded:?}");
        assert_eq!(shielded.outcome(), None);
        // The mark is taken once, with the move id it was given.
        mark_moved("sess-moved-1", None);
        mark_moved("sess-moved-1", Some("mv-1"));
        assert_eq!(take_moved("sess-moved-1"), Some(Some("mv-1".to_string())));
        assert_eq!(take_moved("sess-moved-1"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #EXP-1622: a moved session whose branch is on origin (an open PR's
    /// head) never pushes it: its unreleased commits go to the hidden
    /// `refs/exp-wip/<branch>`, origin's branch (and the PR's CI) stays as
    /// the agent left it, and the next device's prepare fast-forwards onto
    /// the parked work.
    #[test]
    fn a_moved_branch_with_a_pr_hands_off_on_the_wip_ref_never_the_branch() {
        let (root, cleanup) = fixture("hand-off-wip-ref");
        commit_in(&cleanup.worktree, "released.txt", "pushed by the agent\n");
        git(&cleanup.worktree, &["push", "-q", "origin", "HEAD:refs/heads/exp/chat-1a2b3c4d"]);
        commit_in(&cleanup.worktree, "local.txt", "not released\n");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        assert_eq!(hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"), HandOff::Pushed);
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), "add released.txt");
        let wip = crate::auto_push::wip_ref("exp/chat-1a2b3c4d");
        assert_eq!(origin_log(&cleanup, &wip), crate::wip_save::WIP_FINISH_COMMIT_MESSAGE);
        assert!(!origin_has(&cleanup, "exp/leftover-1a2b3c4d"));

        // The next device: a fresh clone cut from origin's branch.
        let next = root.join("next");
        git(&root, &["clone", "-q", origin_of(&cleanup).to_str().unwrap(), next.to_str().unwrap()]);
        git(&next, &["checkout", "-q", "-b", "exp/chat-1a2b3c4d", "origin/exp/chat-1a2b3c4d"]);
        assert_eq!(
            crate::auto_push::restore_wip(&next, &next, "exp/chat-1a2b3c4d", None),
            crate::auto_push::WipRestore::FastForwarded
        );
        assert!(next.join("local.txt").exists());
        assert_eq!(std::fs::read_to_string(next.join("README.md")).unwrap(), "edited\n");

        // A second hand-off with nothing new leaves origin's branch alone too.
        assert_eq!(hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"), HandOff::Pushed);
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), "add released.txt");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// EXP-1542 (review): the foreground `code`/`run` host hands a moved
    /// run's branch on as it exits (it never runs the daemon's finish) and
    /// reports the move; an unmoved run does nothing.
    #[test]
    fn a_foreground_host_hands_a_moved_run_off_on_exit() {
        let (root, cleanup) = fixture("hand-off-on-exit");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        let (base, requests) =
            crate::test_support::canned_server_recording(vec![(200, r#"{"result":{"data":{"outcome":"pushed"}}}"#.to_string())]);
        let trpc = api::trpc::TrpcClient::new(&base, std::sync::Arc::new(api::StaticToken("t".into())));
        assert_eq!(hand_off_on_exit(&trpc, "1a2b3c4d-exit", Some(&cleanup)), None);
        assert!(!origin_has(&cleanup, "exp/chat-1a2b3c4d"));
        mark_moved("1a2b3c4d-exit", Some("mv-9"));
        assert_eq!(hand_off_on_exit(&trpc, "1a2b3c4d-exit", Some(&cleanup)), Some(HandOff::Pushed));
        assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), "add local.txt");
        assert!(cleanup.worktree.exists());
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("sessions.handedOff"), "{}", requests[0]);
        assert!(requests[0].contains(r#"{"moveId":"mv-9","outcome":"pushed"}"#), "{}", requests[0]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// EXP-1542 (review): a worktree whose HEAD is NOT on the session's
    /// branch (the agent checked out another one) never pushes that HEAD
    /// as the session branch: the branch's OWN ref goes to origin and
    /// HEAD's commits are parked on the leftover branch; nothing is lost,
    /// in the finish and in the no-engine hand-off alike.
    #[test]
    fn a_moved_hand_off_off_its_branch_pushes_the_branch_ref_and_parks_head() {
        for finish in [false, true] {
            let (root, cleanup) = fixture(if finish { "moved-off-branch-finish" } else { "moved-off-branch" });
            commit_in(&cleanup.worktree, "mine.txt", "on the session branch\n");
            git(&cleanup.worktree, &["checkout", "-q", "-b", "side"]);
            commit_in(&cleanup.worktree, "side.txt", "on another branch\n");
            if finish {
                assert_eq!(
                    finish_moved_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
                    FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d".to_string()) }
                );
            } else {
                assert_eq!(
                    hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
                    HandOff::Parked("exp/leftover-1a2b3c4d".to_string())
                );
            }
            assert_eq!(origin_log(&cleanup, "exp/chat-1a2b3c4d"), "add mine.txt");
            assert_eq!(origin_log(&cleanup, "exp/leftover-1a2b3c4d"), "add side.txt");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// EXP-1542 (review): a landed branch (merged; origin deleted it) is
    /// never pushed back by a hand-off; a worktree the finish already
    /// removed hands on what that finish parked on origin, else nothing.
    #[test]
    fn a_hand_off_never_revives_a_landed_branch_and_reads_a_removed_worktrees_leftover() {
        let (root, cleanup) = fixture("hand-off-landed");
        commit_in(&cleanup.worktree, "done.txt", "merged\n");
        // The PR merged: main has the commit, the branch is gone on origin.
        git(&cleanup.worktree, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
        git(&cleanup.clone, &["fetch", "-q", "origin"]);
        assert!(matches!(
            hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            HandOff::Nothing(_)
        ));
        assert!(!origin_has(&cleanup, "exp/chat-1a2b3c4d"));
        // The finish removed the worktree (nothing parked): nothing to hand on.
        git(&cleanup.clone, &["worktree", "remove", "--force", cleanup.worktree.to_str().unwrap()]);
        assert!(matches!(
            hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            HandOff::Nothing(_)
        ));
        // … or it parked the work: that leftover is the hand-off.
        git(&cleanup.clone, &["push", "-q", "origin", "main:refs/heads/exp/leftover-1a2b3c4d"]);
        assert_eq!(
            hand_off_moved_branch(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            HandOff::Parked("exp/leftover-1a2b3c4d".to_string())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finish_saves_dirty_work_as_a_pushed_leftover_branch() {
        let (root, cleanup) = fixture("finish-dirty");
        diverge_own_branch(&cleanup);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        std::fs::write(cleanup.worktree.join("debris.log"), "noise").unwrap();
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d".to_string()) }
        );
        assert!(!cleanup.worktree.exists());
        assert!(!branch_exists(&cleanup.clone, &cleanup.branch));
        assert_eq!(
            origin_log(&cleanup, "exp/leftover-1a2b3c4d"),
            crate::wip_save::WIP_FINISH_COMMIT_MESSAGE
        );
        let files = run_git(
            Some(&origin_of(&cleanup)),
            &["ls-tree", "-r", "--name-only", "exp/leftover-1a2b3c4d"],
            None,
            "ls-tree",
        )
        .unwrap();
        assert!(files.contains("local.txt") && files.contains("README.md"), "{files}");
        assert!(!files.contains("debris.log"), "untracked debris is never committed: {files}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An unrelated history on the leftover name is never overwritten: the
    /// push moves on to `-2`.
    #[test]
    fn a_taken_leftover_name_gets_a_suffix() {
        let (root, cleanup) = fixture("finish-taken");
        git(&cleanup.clone, &["checkout", "-q", "-b", "other"]);
        commit_in(&cleanup.clone, "other.txt", "unrelated\n");
        git(&cleanup.clone, &["push", "-q", "origin", "other:refs/heads/exp/leftover-1a2b3c4d"]);
        git(&cleanup.clone, &["checkout", "-q", "main"]);
        diverge_own_branch(&cleanup);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d-2".to_string()) }
        );
        assert_eq!(origin_log(&cleanup, "exp/leftover-1a2b3c4d"), "add other.txt");
        assert_eq!(origin_log(&cleanup, "exp/leftover-1a2b3c4d-2"), "add local.txt");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_push_keeps_the_worktree() {
        let (root, cleanup) = fixture("finish-pushfail");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        let gone = root.join("no-such-origin.git");
        git(&cleanup.clone, &["remote", "set-url", "origin", gone.to_str().unwrap()]);
        let outcome = finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d");
        assert!(matches!(outcome, FinishOutcome::Skipped(SkipReason::GitFailed(_))), "{outcome:?}");
        assert!(cleanup.worktree.exists());
        assert!(branch_exists(&cleanup.clone, &cleanup.branch));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_origin_keeps_todays_keep_dirty_rule() {
        let (root, cleanup) = fixture("finish-noremote");
        git(&cleanup.clone, &["remote", "remove", "origin"]);
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::KeptDirty(DirtyState::TrackedChanges)
        );
        git(&cleanup.worktree, &["checkout", "--", "README.md"]);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::KeptAhead(1)
        );
        assert!(cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A branch whose work landed upstream under other commits (a
    /// squash/cherry-pick) needs no leftover even though its own commits
    /// were never pushed.
    #[test]
    fn landed_work_needs_no_leftover() {
        let (root, cleanup) = fixture("finish-landed");
        commit_in(&cleanup.worktree, "work.txt", "real work\n");
        let head = run_git(Some(&cleanup.worktree), &["rev-parse", "HEAD"], None, "head").unwrap();
        git(&cleanup.clone, &["cherry-pick", head.trim()]);
        git(&cleanup.clone, &["push", "-q", "origin", "main"]);
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(!origin_has(&cleanup, "exp/leftover-1a2b3c4d"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finish_respects_the_gate_live_holders_and_bad_names() {
        let (root, cleanup) = fixture("finish-shields");
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        let hold = crate::launch_gate::hold(&cleanup.clone);
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "1a2b3c4d"),
            FinishOutcome::Skipped(SkipReason::GateHeld)
        );
        drop(hold);
        let live = LiveHolders { branches: vec![cleanup.branch.clone()], ..LiveHolders::default() };
        assert!(matches!(
            finish_worktree(&cleanup, &live, "1a2b3c4d"),
            FinishOutcome::Skipped(SkipReason::HeldByLiveRun(_))
        ));
        assert_eq!(
            finish_worktree(&cleanup, &LiveHolders::default(), "bad name"),
            FinishOutcome::Skipped(SkipReason::InvalidBranch)
        );
        assert!(cleanup.worktree.exists());
        assert!(!origin_has(&cleanup, "exp/leftover-1a2b3c4d"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// NEW-5: a session's finish that parked work on a leftover branch posts
    /// ONE line naming it into the conversation; a clean finish posts none.
    #[test]
    fn a_session_finish_names_its_leftover_branch_once() {
        use std::sync::Arc;
        let session = "1A2B3C4D-1111-4111-8111-111111111111";
        assert_eq!(session_id8(session), "1a2b3c4d");
        let (root, cleanup) = fixture("finish-session-note");
        diverge_own_branch(&cleanup);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        let (base, requests) = crate::test_support::canned_server_recording(vec![(
            200,
            r#"{"result":{"data":{"txid":"1"}}}"#.to_string(),
        )]);
        let trpc = api::trpc::TrpcClient::new(&base, Arc::new(api::StaticToken("t".into())));
        assert_eq!(
            finish_session_worktree(&trpc, session, &cleanup, &LiveHolders::default()),
            FinishOutcome::Removed { leftover: Some("exp/leftover-1a2b3c4d".to_string()) }
        );
        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1, "{requests:?}");
        // #EXP-1618: the server's leftover mutation (line + inbox row).
        assert!(requests[0].starts_with("POST /api/trpc/sessions.leftover "), "{}", requests[0]);
        assert!(requests[0].contains("exp/leftover-1a2b3c4d"), "{}", requests[0]);
        let _ = std::fs::remove_dir_all(&root);

        // A server without `sessions.leftover`: the old reply line.
        let (root, cleanup) = fixture("finish-session-note-old");
        diverge_own_branch(&cleanup);
        commit_in(&cleanup.worktree, "local.txt", "unpushed\n");
        let (base, requests) = crate::test_support::canned_server_recording(vec![
            (404, r#"{"error":{"message":"No procedure found"}}"#.to_string()),
            (200, r#"{"result":{"data":{"messageId":"m-1"}}}"#.to_string()),
        ]);
        let trpc = api::trpc::TrpcClient::new(&base, Arc::new(api::StaticToken("t".into())));
        finish_session_worktree(&trpc, session, &cleanup, &LiveHolders::default());
        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "{requests:?}");
        assert!(requests[1].starts_with("POST /api/trpc/sessions.reply "), "{}", requests[1]);
        assert!(requests[1].contains("Unpushed work was kept on branch exp/leftover-1a2b3c4d."), "{}", requests[1]);
        let _ = std::fs::remove_dir_all(&root);

        // Everything already on origin: no line.
        let (root, cleanup) = fixture("finish-session-clean");
        let (base, requests) = crate::test_support::canned_server_recording(vec![]);
        let trpc = api::trpc::TrpcClient::new(&base, Arc::new(api::StaticToken("t".into())));
        assert_eq!(
            finish_session_worktree(&trpc, session, &cleanup, &LiveHolders::default()),
            FinishOutcome::Removed { leftover: None }
        );
        assert!(requests.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
