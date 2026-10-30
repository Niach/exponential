//! Landed-work pruning of session worktrees and stale coding branches
//! (EXP-465, superseding the EXP-76 pr_state-only rule).
//!
//! The old prune asked one question — "does a synced issue with
//! `pr_state == merged` map to this branch?" — which missed everything the
//! tracker no longer tells us about: deleted issues, issues closed without a
//! PR, abandoned batch branches. This engine asks GIT whether a branch's work
//! has **landed** in the repo's effective default branch, and lets the caller
//! layer the issue-state facts on top via [`PrunePolicy`]:
//!
//! * `keep` — branches that must never be touched (unfinished issues, live
//!   sessions, busy terminal tabs). Callers err toward keeping.
//! * `merged` — branches whose issue's PR is merged: nominated even outside
//!   every prefix, but still deleted only when git confirms the tip landed —
//!   merged sessions stay live (EXP-358) and post-merge follow-up commits
//!   must never dangle on a `branch -D`.
//! * `finished` — branches whose issue is done/cancelled/duplicate: pruned
//!   only when git agrees the work landed.
//! * everything else prefix-matched (`exp/…`, `exp/batch-…`): the tracker has
//!   no opinion (issue deleted, branch never linked) — git truth alone
//!   decides.
//!
//! **Landed** = the branch is an ancestor of `origin/<default>`, OR every
//! commit is patch-equivalent to an upstream one (`git cherry`, catches
//! rebases), OR the branch's WHOLE diff squashed into one probe commit is
//! patch-equivalent (`commit-tree` + `cherry` — the load-bearing check for
//! squash-merge workflows, where a multi-commit branch lands as one commit
//! and neither ancestry nor per-commit cherry can see it).
//!
//! Dirty handling (EXP-465 decision): untracked-only leftovers do NOT protect
//! a landed worktree — they are exactly the debris (compose overrides, stray
//! agent files) that used to park worktrees forever — so those removals pass
//! `--force`. Modified or staged TRACKED files always protect the worktree
//! and are reported. A landed branch is deleted together with its worktree
//! (nothing unpushed can be lost once the work is in the default branch), and
//! the optional branch-only pass sweeps prefix branches whose worktree is
//! already gone.
//!
//! All git here is local and tokenless (`run_git` with `url: None`) — pruning
//! never touches the network; callers fetch on their own cadence.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::git_worktree::{list_worktrees, run_git, validate_branch_arg};

/// EXP-478: worktrees younger than this never prune — covers launches the
/// in-process launch gate cannot see (the CLI daemon shares clone paths from
/// its own process) and the stale-policy window (a launch that starts AND
/// finishes entirely inside one prune interval leaves the gate free while
/// the pass's snapshot sets still lack its branch).
pub const LAUNCH_GRACE: Duration = Duration::from_secs(15 * 60);

/// What the caller knows (issue state, live sessions) and wants (branch
/// sweep), fed into [`prune_landed`]. All sets hold full branch names
/// (`exp/EXP-42`).
#[derive(Clone, Debug, Default)]
pub struct PrunePolicy {
    /// Branch namespaces this engine may touch at all (typically the user's
    /// branch prefix plus the hardcoded `exp/batch-` — batch branches ignore
    /// the settings prefix). Branches outside every prefix are only ever
    /// candidates via `merged`/`finished` (an explicit issue mapping).
    pub prefixes: Vec<String>,
    /// The server-resolved effective default branch, when known. Never a
    /// fabricated `main` — [`effective_default_branch`] falls back to the
    /// clone's own `origin/HEAD`, then its checked-out branch.
    pub default_branch: Option<String>,
    /// Never touch these (unfinished issues, live sessions).
    pub keep: HashSet<String>,
    /// Paths currently in use (running terminal tab cwds): any worktree that
    /// is — or contains — one of these is kept, whatever its branch says.
    pub busy_paths: Vec<std::path::PathBuf>,
    /// Issue `pr_state == merged` — a prune candidate regardless of prefix,
    /// but git must still confirm the tip landed (EXP-358: merged sessions
    /// stay live and collect post-merge follow-up commits).
    pub merged: HashSet<String>,
    /// Issue finished (done/cancelled/duplicate) — prune iff git agrees.
    pub finished: HashSet<String>,
    /// Also delete landed prefix branches that have no worktree anymore.
    pub delete_stale_branches: bool,
    /// EXP-637: the app data dir holding `runs.json`. When set, every RUN
    /// branch this install recorded for the clone is nominated like a
    /// `finished` issue — the run cleanup normally removes those worktrees
    /// at exit, and this is the backstop for the ones it could not (a held
    /// gate, a crash). Nomination is not permission: git still has to
    /// confirm the branch landed, and dirty/ahead worktrees survive.
    pub run_registry_dir: Option<std::path::PathBuf>,
}

/// Why a nominated worktree survived the pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Modified or staged tracked files — real work, never force-removed.
    TrackedChanges,
    /// The branch has commits the default branch does not contain.
    NotLanded,
    /// `git worktree remove` failed (locked, permissions, …).
    RemoveFailed(String),
    /// No default branch could be resolved, so landed checks were impossible.
    NoDefaultBranch,
    /// Younger than [`LAUNCH_GRACE`] — likely a launch in flight (EXP-478).
    RecentlyCreated,
}

impl SkipReason {
    /// Short human-readable reason for result lines and logs.
    pub fn describe(&self, default_branch: Option<&str>) -> String {
        match self {
            Self::TrackedChanges => "uncommitted changes".to_string(),
            Self::NotLanded => match default_branch {
                Some(branch) => format!("not merged into {branch}"),
                None => "not merged".to_string(),
            },
            Self::RemoveFailed(detail) => detail.clone(),
            Self::NoDefaultBranch => "no default branch to compare against".to_string(),
            Self::RecentlyCreated => "recently created".to_string(),
        }
    }
}

/// Outcome of one [`prune_landed`] pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Branches whose worktrees were removed (branch deleted too).
    pub removed_worktrees: Vec<String>,
    /// Every branch deleted — worktree-attached and branch-only sweeps.
    pub deleted_branches: Vec<String>,
    /// Nominated worktrees that survived, with the reason.
    pub skipped: Vec<(String, SkipReason)>,
    /// The default branch the landed checks compared against, for messages.
    pub default_branch: Option<String>,
    /// EXP-478: the pass never ran — a coding launch held the clone's gate.
    /// The auto-sync cadence retries on its own; the settings button reports.
    pub blocked_by_launch: bool,
}

impl PruneReport {
    /// Nothing removed, nothing to report on.
    pub fn is_empty(&self) -> bool {
        self.removed_worktrees.is_empty()
            && self.deleted_branches.is_empty()
            && self.skipped.is_empty()
    }
}

/// A worktree's `git status --porcelain`, split the way the prune cares:
/// untracked-only debris is removable, tracked changes are sacred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    UntrackedOnly,
    TrackedChanges,
}

/// EXP-478: age via the worktree's `.git` link file (written by `git
/// worktree add`, refreshed by `create_worktree`'s reuse paths — "when did a
/// launch last claim this worktree"). Unreadable → NOT young (the keep/busy/
/// gate shields still apply); a future mtime reads as young.
fn worktree_is_young(worktree: &Path, now: SystemTime) -> bool {
    std::fs::metadata(worktree.join(".git"))
        .and_then(|meta| meta.modified())
        .ok()
        .is_some_and(|mtime| now.duration_since(mtime).map_or(true, |age| age < LAUNCH_GRACE))
}

/// Classify a worktree's status. A failing `git status` counts as
/// [`DirtyState::TrackedChanges`] — never prune what we couldn't inspect.
pub fn worktree_dirty_state(worktree: &Path) -> DirtyState {
    // EXP-766: `--untracked-files=all` overrides a repo/global
    // `status.showUntrackedFiles=no`, which would otherwise report a
    // worktree full of untracked work as CLEAN — and clean is the one state
    // that gets removed without `--force`.
    let Ok(output) = run_git(
        Some(worktree),
        &["status", "--porcelain", "--untracked-files=all"],
        None,
        "git status",
    ) else {
        return DirtyState::TrackedChanges;
    };
    let mut untracked_only = true;
    let mut any = false;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        any = true;
        if !line.starts_with("?? ") {
            untracked_only = false;
        }
    }
    match (any, untracked_only) {
        (false, _) => DirtyState::Clean,
        (true, true) => DirtyState::UntrackedOnly,
        (true, false) => DirtyState::TrackedChanges,
    }
}

/// Resolve the branch the landed checks compare against: the server-supplied
/// effective default when its `origin/<b>` ref exists locally, else the
/// clone's `origin/HEAD` symref, else the trunk's checked-out branch. `None`
/// when nothing verifies — never a fabricated `main`/`master`.
/// S-4: the branches of OPEN (not ended) sessions, split by what the
/// worktree cleanup may do with them now that nothing ends an idle chat.
/// `settled` = the session's whole tree is idle (nothing working, nobody
/// asked) and it has pull requests, every one merged: its worktree may go,
/// but only when clean (never parked, never with uncommitted changes).
/// `keep` = every other open session's branch: never touched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenBranches {
    pub keep: HashSet<String>,
    pub settled: HashSet<String>,
}

/// [`OpenBranches`] over the synced `sessions` and `pull_requests` rows.
pub fn open_branches<'a>(
    sessions: impl IntoIterator<Item = &'a domain::rows::SessionRow>,
    pulls: impl IntoIterator<Item = &'a domain::rows::PullRequestRow>,
) -> OpenBranches {
    use domain::session_tree::{session_state, SessionState};
    let sessions: Vec<&domain::rows::SessionRow> = sessions.into_iter().collect();
    let mut busy_roots: HashSet<&str> = HashSet::new();
    for session in &sessions {
        if matches!(session_state(session), SessionState::Working | SessionState::NeedsYou) {
            busy_roots.insert(session.root_id_or_self());
        }
    }
    // root id → (any PR, every PR merged).
    let mut prs: std::collections::HashMap<&str, (bool, bool)> = std::collections::HashMap::new();
    for pr in pulls {
        let Some(root) = pr.root_session_id.as_deref().or(pr.session_id.as_deref()) else {
            continue;
        };
        let entry = prs.entry(root).or_insert((false, true));
        entry.0 = true;
        entry.1 &= pr.is_merged();
    }
    let mut open = OpenBranches::default();
    for session in sessions {
        if session.is_ended() {
            continue;
        }
        let Some(branch) = session.branch.clone().filter(|branch| !branch.is_empty()) else {
            continue;
        };
        let root = session.root_id_or_self();
        let settled = !busy_roots.contains(root) && prs.get(root).is_some_and(|&(any, merged)| any && merged);
        if settled {
            open.settled.insert(branch);
        } else {
            open.keep.insert(branch);
        }
    }
    // A branch two open sessions share is kept if either needs it.
    open.settled.retain(|branch| !open.keep.contains(branch));
    open
}

pub fn effective_default_branch(clone: &Path, server_default: Option<&str>) -> Option<String> {
    let verified = |branch: &str| -> bool {
        validate_branch_arg(branch, "prune default branch").is_ok()
            && run_git(
                Some(clone),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/origin/{branch}"),
                ],
                None,
                "git rev-parse origin default",
            )
            .is_ok()
    };
    if let Some(branch) = server_default.filter(|branch| !branch.is_empty()) {
        if verified(branch) {
            return Some(branch.to_string());
        }
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["symbolic-ref", "refs/remotes/origin/HEAD"],
        None,
        "git symbolic-ref origin/HEAD",
    ) {
        if let Some(branch) = output.trim().strip_prefix("refs/remotes/origin/") {
            if verified(branch) {
                return Some(branch.to_string());
            }
        }
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["symbolic-ref", "--short", "HEAD"],
        None,
        "git symbolic-ref HEAD",
    ) {
        let branch = output.trim();
        if !branch.is_empty() && verified(branch) {
            return Some(branch.to_string());
        }
    }
    None
}

/// Branches already reachable from `origin_ref`, in ONE git call — the fast
/// path of the landed check across an arbitrary branch count.
fn ancestor_branches(clone: &Path, origin_ref: &str) -> HashSet<String> {
    run_git(
        Some(clone),
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(refname:short)",
            "--merged",
            origin_ref,
        ],
        None,
        "git for-each-ref --merged",
    )
    .map(|output| output.lines().map(|line| line.trim().to_string()).collect())
    .unwrap_or_default()
}

/// Whether `branch`'s work is contained in `origin_ref`. Three probes, cheap
/// to expensive: ancestry (pre-batched), per-commit patch equivalence
/// (`git cherry`, catches rebases), then the squash probe — synthesize ONE
/// commit carrying the branch's whole diff over the merge-base and cherry
/// THAT, which is the only shape a squash-merged multi-commit branch matches.
/// The probe writes a dangling commit object; gc reclaims it.
pub(crate) fn branch_landed(
    clone: &Path,
    branch: &str,
    origin_ref: &str,
    ancestors: &HashSet<String>,
) -> bool {
    if ancestors.contains(branch) {
        return true;
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["cherry", origin_ref, branch],
        None,
        "git cherry",
    ) {
        if output.lines().all(|line| !line.starts_with('+')) {
            return true;
        }
    }
    let Ok(base) = run_git(
        Some(clone),
        &["merge-base", origin_ref, branch],
        None,
        "git merge-base",
    ) else {
        return false;
    };
    let base = base.trim().to_string();
    let tree = |rev: &str| {
        run_git(
            Some(clone),
            &["rev-parse", &format!("{rev}^{{tree}}")],
            None,
            "git rev-parse tree",
        )
        .map(|output| output.trim().to_string())
    };
    let (Ok(branch_tree), Ok(base_tree)) = (tree(branch), tree(&base)) else {
        return false;
    };
    if branch_tree == base_tree {
        // The branch nets to zero diff over its base — nothing to land.
        return true;
    }
    // Identity via -c: the probe must work without global git config, and
    // commit-tree refuses to run identity-less.
    let Ok(probe) = run_git(
        Some(clone),
        &[
            "-c",
            "user.name=exp-prune",
            "-c",
            "user.email=prune@exponential.local",
            "commit-tree",
            &branch_tree,
            "-p",
            &base,
            "-m",
            "exp-prune landed probe",
        ],
        None,
        "git commit-tree",
    ) else {
        return false;
    };
    run_git(
        Some(clone),
        &["cherry", origin_ref, probe.trim(), &base],
        None,
        "git cherry (squash probe)",
    )
    .map(|output| {
        let mut lines = output.lines().filter(|line| !line.trim().is_empty());
        matches!(lines.next(), Some(line) if line.starts_with('-')) && lines.next().is_none()
    })
    .unwrap_or(false)
}

/// Remove every landed session worktree of `clone` (+ its branch), then
/// optionally sweep landed prefix branches that lost their worktree long ago.
/// Blocking (spawned gits) — callers run it on a background executor.
///
/// EXP-478: the whole pass runs inside the clone's launch gate — a launch in
/// flight (worktree created, session not yet registered) aborts the pass
/// entirely, and no launch can begin while the pass runs. The policy the
/// caller derived is a pre-fetch snapshot, so it cannot protect a worktree
/// born after it.
pub fn prune_landed(clone: &Path, policy: &PrunePolicy) -> PruneReport {
    match crate::launch_gate::try_exclusive(clone, || prune_landed_locked(clone, policy)) {
        Some(report) => report,
        None => PruneReport {
            blocked_by_launch: true,
            ..Default::default()
        },
    }
}

fn prune_landed_locked(clone: &Path, policy: &PrunePolicy) -> PruneReport {
    let mut report = PruneReport::default();
    let Ok(entries) = list_worktrees(clone) else {
        return report;
    };
    // EXP-637: this install's own recorded run branches for THIS clone,
    // nominated exactly like `finished` (git still decides).
    let recorded_runs: HashSet<String> = policy
        .run_registry_dir
        .as_deref()
        .map(|dir| {
            crate::run_registry::branches_for_clone(dir, clone)
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    // EXP-766: the cwds of runs a LIVE host process is still driving.
    // `policy.busy_paths` is process-local (this app's terminal tabs), so
    // without this a worktree the CLI daemon — or a sibling desktop on the
    // shared data dir (REV-20) — is coding in reads as idle debris.
    let live_run_cwds: Vec<std::path::PathBuf> = policy
        .run_registry_dir
        .as_deref()
        .map(crate::run_registry::live_host_cwds)
        .unwrap_or_default();
    let default_branch = effective_default_branch(clone, policy.default_branch.as_deref());
    report.default_branch = default_branch.clone();
    let origin_ref = default_branch.as_deref().map(|branch| format!("origin/{branch}"));
    let ancestors = origin_ref
        .as_deref()
        .map(|origin_ref| ancestor_branches(clone, origin_ref))
        .unwrap_or_default();

    let prefix_matched = |branch: &str| {
        policy
            .prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && branch.starts_with(prefix.as_str()))
    };
    // A busy tab (or a live run's cwd) anywhere under a worktree parks it.
    // Both sides canonicalized when possible — git reports resolved paths,
    // tab cwds may be symlinked.
    let busy = |worktree: &Path| {
        let canonical = worktree.canonicalize().ok();
        policy.busy_paths.iter().chain(&live_run_cwds).any(|path| {
            let path = path.canonicalize().unwrap_or_else(|_| path.clone());
            path.starts_with(worktree)
                || canonical
                    .as_deref()
                    .is_some_and(|worktree| path.starts_with(worktree))
        })
    };
    // Returns None when the branch is no candidate at all (silent skip),
    // Some(Err(reason)) when nominated but not removable. `merged` and
    // `finished` nominate beyond the prefixes, but NOTHING bypasses the git
    // agreement: a merged issue's session stays live (EXP-358) and its
    // branch collects post-merge follow-up commits `branch -D` would dangle.
    let landed_verdict = |branch: &str| -> Option<Result<(), SkipReason>> {
        if !policy.merged.contains(branch)
            && !policy.finished.contains(branch)
            && !recorded_runs.contains(branch)
            && !prefix_matched(branch)
        {
            return None;
        }
        let Some(origin_ref) = origin_ref.as_deref() else {
            return Some(Err(SkipReason::NoDefaultBranch));
        };
        if branch_landed(clone, branch, origin_ref, &ancestors) {
            Some(Ok(()))
        } else {
            Some(Err(SkipReason::NotLanded))
        }
    };

    // ---- worktree pass (the first entry is always the main working tree) ----
    for entry in entries.iter().skip(1) {
        if entry.path == *clone {
            continue;
        }
        let Some(branch) = &entry.branch else {
            continue; // detached — no branch to reason about
        };
        if validate_branch_arg(branch, "prune worktree").is_err()
            || policy.keep.contains(branch)
            || Some(branch.as_str()) == default_branch.as_deref()
            || busy(&entry.path)
        {
            continue;
        }
        match landed_verdict(branch) {
            None => continue,
            Some(Err(reason)) => report.skipped.push((branch.clone(), reason)),
            Some(Ok(())) => {
                let path = entry.path.to_string_lossy().into_owned();
                let args: &[&str] = match worktree_dirty_state(&entry.path) {
                    DirtyState::TrackedChanges => {
                        report
                            .skipped
                            .push((branch.clone(), SkipReason::TrackedChanges));
                        continue;
                    }
                    DirtyState::Clean => &["worktree", "remove", &path],
                    // Untracked-only debris goes with the worktree — git
                    // refuses untracked files without --force.
                    DirtyState::UntrackedOnly => &["worktree", "remove", "--force", &path],
                };
                // EXP-478: last shield before removal — checked only when the
                // removal is otherwise imminent, so a registered session never
                // shows up here (`keep`/`busy` shield it first) and dirty
                // worktrees keep their more informative `TrackedChanges`.
                if worktree_is_young(&entry.path, SystemTime::now()) {
                    report
                        .skipped
                        .push((branch.clone(), SkipReason::RecentlyCreated));
                    continue;
                }
                match run_git(Some(clone), args, None, &format!("git worktree remove ({branch})"))
                {
                    Ok(_) => {
                        report.removed_worktrees.push(branch.clone());
                        // Landed per git — the branch carries nothing
                        // unpushed anymore, so it goes too.
                        if run_git(
                            Some(clone),
                            &["branch", "-D", branch],
                            None,
                            &format!("git branch -D {branch}"),
                        )
                        .is_ok()
                        {
                            report.deleted_branches.push(branch.clone());
                        }
                    }
                    Err(err) => report
                        .skipped
                        .push((branch.clone(), SkipReason::RemoveFailed(err.detail))),
                }
            }
        }
    }

    // ---- branch-only sweep: landed prefix branches without a worktree ----
    // No launch grace here (branch-only — nothing to stamp), and none needed:
    // an in-process launch is excluded by the gate, and a cross-process
    // `worktree add -b` creates branch+checkout atomically, so even against
    // the stale `checked_out` snapshot the `-D` below fails harmlessly (git
    // refuses to delete a branch checked out anywhere, per live state).
    if policy.delete_stale_branches {
        let checked_out: HashSet<String> = entries
            .iter()
            .filter_map(|entry| entry.branch.clone())
            .collect();
        let branches = run_git(
            Some(clone),
            &["for-each-ref", "refs/heads", "--format=%(refname:short)"],
            None,
            "git for-each-ref",
        )
        .map(|output| output.lines().map(|line| line.trim().to_string()).collect())
        .unwrap_or_else(|_| Vec::<String>::new());
        for branch in branches {
            if branch.is_empty()
                || validate_branch_arg(&branch, "prune branch").is_err()
                || !prefix_matched(&branch)
                || checked_out.contains(&branch)
                || policy.keep.contains(&branch)
                || Some(branch.as_str()) == default_branch.as_deref()
                || report.deleted_branches.contains(&branch)
            {
                continue;
            }
            // Same rule as the worktree pass: even `merged` branches need
            // git-confirmed containment before `-D`.
            let landed = origin_ref
                .as_deref()
                .is_some_and(|origin_ref| branch_landed(clone, &branch, origin_ref, &ancestors));
            if !landed {
                continue; // unlanded stale branches are kept, silently
            }
            if run_git(
                Some(clone),
                &["branch", "-D", &branch],
                None,
                &format!("git branch -D {branch}"),
            )
            .is_ok()
            {
                report.deleted_branches.push(branch);
            }
        }
    }

    report
}

/// NEW-5: how long a finished session's worktree may sit idle before the
/// nightly sweep finishes it.
pub const SWEEP_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// What [`sweep_stale_worktrees`] may touch.
#[derive(Clone, Debug)]
pub struct SweepPolicy {
    /// A worktree whose last activity is younger than this stays.
    pub max_age: Duration,
    /// The host's own live sessions (desktop `LocalSessions`, the CLI's live
    /// runs); every live host process on the data dir is added on top.
    pub live: crate::run_cleanup::LiveHolders,
    /// The server-resolved default branch — the landed check's base for a
    /// worktree no run record names (see [`effective_default_branch`]).
    pub default_branch: Option<String>,
    /// S-4: the open sessions' branches ([`open_branches`]): a `keep` one is
    /// never swept, a `settled` one only while it has no tracked changes.
    pub open: OpenBranches,
}

impl Default for SweepPolicy {
    fn default() -> Self {
        Self {
            max_age: SWEEP_MAX_AGE,
            live: Default::default(),
            default_branch: None,
            open: OpenBranches::default(),
        }
    }
}

/// One worktree the sweep put through [`crate::run_cleanup::finish_worktree`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SweptWorktree {
    pub path: std::path::PathBuf,
    pub branch: String,
    /// The newest run record on the worktree, when there is one — the
    /// session a leftover branch belongs to.
    pub session_id: Option<String>,
    pub outcome: crate::run_cleanup::FinishOutcome,
}

/// Outcome of one [`sweep_stale_worktrees`] pass. Worktrees the sweep did
/// not consider (live, young, detached, outside the worktrees dir) are not
/// listed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub finished: Vec<SweptWorktree>,
}

impl SweepReport {
    /// The `origin` branches unpushed work was parked on, with their session.
    pub fn leftovers(&self) -> Vec<(Option<&str>, &str)> {
        self.finished
            .iter()
            .filter_map(|swept| match &swept.outcome {
                crate::run_cleanup::FinishOutcome::Removed { leftover: Some(branch) } => {
                    Some((swept.session_id.as_deref(), branch.as_str()))
                }
                _ => None,
            })
            .collect()
    }
}

/// NEW-5 — the nightly backstop of the automatic worktree cleanup: every
/// session worktree under `<clone>.worktrees/` whose session is over goes
/// through the SAME [`crate::run_cleanup::finish_worktree`] procedure (clean
/// and pushed → removed; otherwise WIP commit + `exp/leftover-<id8>` push +
/// removed; no `origin` → kept dirty/ahead).
///
/// "Ours" (EXP-1473) = a run record on `data_dir` names the worktree, or its
/// branch is one of the account's open sessions (`policy.open`). Any other
/// worktree belongs to another install sharing the repos root (its own data
/// dir, another account) and is never touched: its sessions are not ours to
/// judge over.
///
/// "Over" = none of the records naming the worktree has a live host process
/// (`hostPid` cleared at the run's end, or dead).
/// "Stale" = its last activity — the newer of the `.git` link file's mtime
/// (refreshed whenever a launch claims the worktree) and the newest record's
/// `recordedAt` — is older than `policy.max_age`. A live run's dir is never
/// touched: `policy.live` plus every live host on `data_dir` shield it, and
/// `finish_worktree` takes the clone's launch gate per worktree.
///
/// The leftover id is the newest record's session id (first 8 chars); an
/// open session's record-less worktree uses its sanitized branch
/// (`exp-EXP-42`). Blocking
/// (spawned gits, a push per leftover) — run it on a background executor.
pub fn sweep_stale_worktrees(
    clone: &Path,
    data_dir: &Path,
    now: SystemTime,
    policy: &SweepPolicy,
) -> SweepReport {
    use crate::run_cleanup::{finish_worktree, FinishOutcome, RunCleanup, SkipReason as FinishSkip};
    let mut report = SweepReport::default();
    let Ok(entries) = list_worktrees(clone) else {
        return report;
    };
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let root = canonical(&crate::git_worktree::worktrees_dir(clone));
    let mut live = policy.live.clone();
    live.worktrees.extend(crate::run_registry::live_host_cwds(data_dir));
    let records = crate::run_registry::all(data_dir);
    for entry in entries.iter().skip(1) {
        let path = canonical(&entry.path);
        if !path.starts_with(&root) || path == root {
            continue;
        }
        let Some(branch) = entry.branch.clone() else {
            continue; // detached: no branch to reason about
        };
        // S-4: an open chat's worktree is what its next message resumes in:
        // it goes only once its tree is idle with every PR merged (the
        // settled check below the base branch: nothing to park).
        if policy.open.keep.contains(&branch) {
            continue;
        }
        let open_chat = policy.open.settled.contains(&branch);
        let mut mine: Vec<&crate::run_registry::RunRecord> = records
            .iter()
            .filter(|record| canonical(&record.cwd) == path)
            .collect();
        if mine.is_empty() && !open_chat {
            continue; // EXP-1473: another install's worktree
        }
        if mine
            .iter()
            .any(|record| record.host_pid.is_some_and(crate::process::is_alive))
        {
            continue; // the session is still running
        }
        mine.sort_by_key(|record| record.recorded_at);
        let newest = mine.last().copied();
        let link_mtime = std::fs::metadata(entry.path.join(".git"))
            .and_then(|meta| meta.modified())
            .ok();
        let recorded = newest.map(|record| {
            std::time::UNIX_EPOCH + Duration::from_secs(record.recorded_at)
        });
        let last_activity = link_mtime.into_iter().chain(recorded).max();
        // Unreadable activity reads as fresh: never sweep what we cannot date.
        let stale = last_activity.is_some_and(|last| {
            now.duration_since(last).is_ok_and(|age| age >= policy.max_age)
        });
        if !stale {
            continue;
        }
        let base_branch = newest
            .and_then(|record| record.base_branch.clone())
            .or_else(|| effective_default_branch(clone, policy.default_branch.as_deref()));
        // S-4: a settled open chat's worktree goes only when the finish would
        // park nothing — no change (untracked files included), no commit
        // that is neither pushed nor landed. Its work stays where it is.
        if open_chat
            && !base_branch.as_ref().is_some_and(|base_branch| {
                crate::run_cleanup::nothing_to_park(&RunCleanup {
                    clone: clone.to_path_buf(),
                    worktree: entry.path.clone(),
                    branch: branch.clone(),
                    base_branch: base_branch.clone(),
                })
            })
        {
            continue;
        }
        let session_id = newest.map(|record| record.session_id.clone());
        let id8 = match &session_id {
            Some(id) => id.chars().take(8).collect(),
            None => crate::git_worktree::sanitize_branch_for_path(&branch),
        };
        let outcome = match base_branch {
            Some(base_branch) => finish_worktree(
                &RunCleanup {
                    clone: clone.to_path_buf(),
                    worktree: entry.path.clone(),
                    branch: branch.clone(),
                    base_branch,
                },
                &live,
                &id8,
            ),
            None => FinishOutcome::Skipped(FinishSkip::GitFailed(
                "no base branch to compare against".to_string(),
            )),
        };
        report.finished.push(SweptWorktree {
            path: entry.path.clone(),
            branch,
            session_id,
            outcome,
        });
    }
    report
}

/// NEW-5: the nightly cadence — [`sweep_stale_worktrees_if_due`] runs a
/// clone's sweep at most once per this in one process.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether `clone` is due for its nightly sweep at `now` (`last` = the
/// clone → last pass map); a due clone is stamped. Pure.
fn sweep_due(
    last: &mut std::collections::HashMap<std::path::PathBuf, std::time::Instant>,
    clone: &Path,
    now: std::time::Instant,
) -> bool {
    if last
        .get(clone)
        .is_some_and(|at| now.saturating_duration_since(*at) < SWEEP_INTERVAL)
    {
        return false;
    }
    last.insert(clone.to_path_buf(), now);
    true
}

/// [`sweep_stale_worktrees`] at most once per [`SWEEP_INTERVAL`] per clone
/// in this process (`None` = not due) — the desktop's AutoSync pass and the
/// CLI daemon's loop both call it on their own cadence; the first call after
/// a start sweeps. Blocking: run it off any UI thread.
pub fn sweep_stale_worktrees_if_due(
    clone: &Path,
    data_dir: &Path,
    now: SystemTime,
    policy: &SweepPolicy,
) -> Option<SweepReport> {
    static LAST: std::sync::Mutex<
        Option<std::collections::HashMap<std::path::PathBuf, std::time::Instant>>,
    > = std::sync::Mutex::new(None);
    {
        let mut guard = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !sweep_due(guard.get_or_insert_with(Default::default), clone, std::time::Instant::now()) {
            return None;
        }
    }
    let report = sweep_stale_worktrees(clone, data_dir, now, policy);
    // EXP-1375: what stayed is listed in the doctor; `git worktree prune` and
    // the stale lock cleanup run with the removal.
    crate::worktree_lifecycle::record_sweep(data_dir, clone, &report, now);
    crate::worktree_lifecycle::maintain_clone(clone, now);
    Some(report)
}

/// NEW-5: one sweep pass's log line + the ONE conversation line per parked
/// leftover branch whose session is known
/// ([`crate::run_cleanup::post_leftover_note`]). `trpc` = `None` (signed
/// out) posts nothing. Blocking (network).
pub fn settle_sweep_report(
    trpc: Option<&api::trpc::TrpcClient>,
    label: &str,
    report: &SweepReport,
) {
    if report.finished.is_empty() {
        return;
    }
    log::info!("stale-worktree sweep {label}: {:?}", report.finished);
    let Some(trpc) = trpc else { return };
    for (session_id, branch) in report.leftovers() {
        if let Some(session_id) = session_id {
            crate::run_cleanup::post_leftover_note(trpc, session_id, branch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// NEW-5: the nightly cadence — once per clone per interval.
    #[test]
    fn the_stale_sweep_runs_once_a_day_per_clone() {
        let mut last = std::collections::HashMap::new();
        let t0 = std::time::Instant::now();
        let a = std::path::Path::new("/repos/a");
        let b = std::path::Path::new("/repos/b");
        assert!(super::sweep_due(&mut last, a, t0));
        assert!(!super::sweep_due(&mut last, a, t0 + std::time::Duration::from_secs(60)));
        assert!(super::sweep_due(&mut last, b, t0 + std::time::Duration::from_secs(60)));
        assert!(super::sweep_due(&mut last, a, t0 + super::SWEEP_INTERVAL));
    }

    use crate::git_worktree::{
        create_worktree, sanitize_branch_for_path, worktrees_dir, TokenUrl,
    };
    use std::fs;
    use std::path::PathBuf;
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
        path.push(format!("exp-coding-prune-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A local `main`-defaulted origin plus its clone (file://, no network).
    fn seed(dir: &Path) -> (PathBuf, PathBuf) {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        let clone = dir.join("clone");
        git(dir, &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        (origin, clone)
    }

    fn worktree(clone: &Path, branch: &str) -> PathBuf {
        create_worktree(clone, branch, "origin/main", &TokenUrl::new("acme/web", "ghs_dead"))
            .unwrap()
    }

    /// EXP-478: age a worktree past the launch grace so removal assertions
    /// exercise the landed logic, not the grace shield.
    fn backdate(worktree: &Path) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(worktree.join(".git"))
            .unwrap();
        file.set_modified(SystemTime::now() - 2 * LAUNCH_GRACE).unwrap();
    }

    fn commit_file(worktree: &Path, name: &str, content: &str, message: &str) {
        fs::write(worktree.join(name), content).unwrap();
        git(worktree, &["add", "."]);
        git(worktree, &["commit", "--quiet", "-m", message]);
    }

    fn policy(prefixes: &[&str]) -> PrunePolicy {
        PrunePolicy {
            prefixes: prefixes.iter().map(|prefix| prefix.to_string()).collect(),
            ..PrunePolicy::default()
        }
    }

    fn branch_exists(clone: &Path, branch: &str) -> bool {
        run_git(
            Some(clone),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
            None,
            "rev-parse",
        )
        .is_ok()
    }

    #[test]
    fn squash_merged_branch_is_landed_and_pruned() {
        let dir = temp_dir("squash");
        let (origin, clone) = seed(&dir.0);
        // Two commits on the branch …
        let wt = worktree(&clone, "exp/EXP-1");
        commit_file(&wt, "a.txt", "one\n", "part 1");
        commit_file(&wt, "b.txt", "two\n", "part 2");
        // … land on origin/main as ONE squashed commit (the repo's real
        // merge style): same combined content, unrelated history.
        fs::write(origin.join("a.txt"), "one\n").unwrap();
        fs::write(origin.join("b.txt"), "two\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "EXP-1: squashed"]);
        git(&clone, &["fetch", "--quiet", "origin"]);
        backdate(&wt);

        // Ancestry can NOT see a squash merge — the probe must.
        let out = Command::new("git")
            .args(["merge-base", "--is-ancestor", "exp/EXP-1", "origin/main"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert!(!out.status.success(), "fixture must not be a plain merge");

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-1".to_string()]);
        assert_eq!(report.deleted_branches, vec!["exp/EXP-1".to_string()]);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/EXP-1"));
    }

    #[test]
    fn unlanded_branch_survives_and_reports_not_landed() {
        let dir = temp_dir("unlanded");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-2");
        commit_file(&wt, "wip.txt", "unmerged\n", "real work");

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-2".to_string(), SkipReason::NotLanded)]
        );
        assert_eq!(
            report.skipped[0].1.describe(report.default_branch.as_deref()),
            "not merged into main"
        );
        assert!(wt.exists());
        assert!(branch_exists(&clone, "exp/EXP-2"));
    }

    #[test]
    fn keep_protects_a_trivially_landed_worktree() {
        // A fresh session worktree is 0 commits ahead — trivially "landed".
        // The keep set (live sessions, open issues) must shield it.
        let dir = temp_dir("keep");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-3");
        backdate(&wt);

        let mut kept = policy(&["exp/"]);
        kept.keep.insert("exp/EXP-3".to_string());
        let report = prune_landed(&clone, &kept);
        assert!(report.is_empty(), "{report:?}");
        assert!(wt.exists());

        // Without the keep it is debris and goes.
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-3".to_string()]);
        assert!(!wt.exists());
    }

    #[test]
    fn untracked_only_is_removed_tracked_changes_survive() {
        let dir = temp_dir("dirty");
        let (_origin, clone) = seed(&dir.0);
        // Both landed (0 commits ahead); one holds untracked debris, one a
        // tracked modification.
        let debris = worktree(&clone, "exp/EXP-4");
        fs::write(debris.join("docker-compose.override.yaml"), "ports: []\n").unwrap();
        backdate(&debris);
        // `real` stays FRESH on purpose: `TrackedChanges` must win the skip
        // report over the EXP-478 grace (the grace runs only when a removal
        // is otherwise imminent).
        let real = worktree(&clone, "exp/EXP-5");
        fs::write(real.join("README.md"), "edited\n").unwrap();

        assert_eq!(worktree_dirty_state(&debris), DirtyState::UntrackedOnly);
        assert_eq!(worktree_dirty_state(&real), DirtyState::TrackedChanges);

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-4".to_string()]);
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-5".to_string(), SkipReason::TrackedChanges)]
        );
        assert!(!debris.exists(), "untracked debris must not protect");
        assert!(real.exists(), "tracked changes must protect");

        // Staged counts as tracked too.
        git(&real, &["add", "."]);
        assert_eq!(worktree_dirty_state(&real), DirtyState::TrackedChanges);
    }

    #[test]
    fn merged_branch_with_unpushed_commits_survives() {
        // EXP-358: a merged session stays live and users commit follow-ups
        // on its branch. The tracker's `merged` fact nominates the branch,
        // but only git-confirmed containment may delete — an unpushed
        // post-merge commit keeps branch AND worktree for a later pass.
        let dir = temp_dir("merged");
        let (origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-6");
        commit_file(&wt, "follow-up.txt", "x\n", "post-merge follow-up");
        backdate(&wt);

        let mut merged = policy(&["exp/"]);
        merged.merged.insert("exp/EXP-6".to_string());
        let report = prune_landed(&clone, &merged);
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-6".to_string(), SkipReason::NotLanded)]
        );
        assert!(wt.exists());
        assert!(branch_exists(&clone, "exp/EXP-6"));

        // Once the follow-up lands upstream (squashed) and is fetched, git
        // agrees and the merged branch finally goes.
        fs::write(origin.join("follow-up.txt"), "x\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "EXP-6: squashed"]);
        git(&clone, &["fetch", "--quiet", "origin"]);
        let report = prune_landed(&clone, &merged);
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-6".to_string()]);
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/EXP-6"));
    }

    #[test]
    fn branch_only_sweep_deletes_landed_prefix_branches() {
        let dir = temp_dir("sweep");
        let (_origin, clone) = seed(&dir.0);
        // Landed (at origin/main) without worktrees: prefix + batch → go;
        // non-prefix → stays. Unlanded prefix branch → stays, silently.
        git(&clone, &["branch", "exp/OLD-1", "origin/main"]);
        git(&clone, &["branch", "exp/batch-1b81d4c7", "origin/main"]);
        git(&clone, &["branch", "feature/keep-me", "origin/main"]);
        let wt = worktree(&clone, "exp/OLD-2");
        commit_file(&wt, "wip.txt", "unmerged\n", "work");
        git(&clone, &["worktree", "remove", "--force", wt.to_str().unwrap()]);

        let mut sweeping = policy(&["exp/"]);
        sweeping.delete_stale_branches = true;
        // Tracker-merged but locally unlanded (unpushed follow-up): the
        // sweep must not `-D` it on the tracker's word alone.
        sweeping.merged.insert("exp/OLD-2".to_string());
        let report = prune_landed(&clone, &sweeping);
        let mut deleted = report.deleted_branches.clone();
        deleted.sort();
        assert_eq!(deleted, vec!["exp/OLD-1".to_string(), "exp/batch-1b81d4c7".to_string()]);
        assert!(!branch_exists(&clone, "exp/OLD-1"));
        assert!(!branch_exists(&clone, "exp/batch-1b81d4c7"));
        assert!(
            branch_exists(&clone, "exp/OLD-2"),
            "unlanded branch survives, even tracker-merged"
        );
        assert!(branch_exists(&clone, "feature/keep-me"), "non-prefix survives");
        assert!(branch_exists(&clone, "main"), "default branch survives");

        // Without the flag nothing branch-only happens.
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.deleted_branches.is_empty());
        assert!(branch_exists(&clone, "exp/OLD-2"));
    }

    #[test]
    fn effective_default_branch_resolution_chain() {
        let dir = temp_dir("default");
        let (_origin, clone) = seed(&dir.0);
        // Server value wins when its origin ref exists.
        assert_eq!(
            effective_default_branch(&clone, Some("main")),
            Some("main".to_string())
        );
        // A bogus/hostile server value falls back to origin/HEAD; a
        // flag-shaped one must never reach argv.
        assert_eq!(
            effective_default_branch(&clone, Some("nope")),
            Some("main".to_string())
        );
        assert_eq!(
            effective_default_branch(&clone, Some("--force")),
            Some("main".to_string())
        );
        assert_eq!(effective_default_branch(&clone, None), Some("main".to_string()));
        // origin/HEAD gone → the checked-out branch still verifies.
        git(&clone, &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"]);
        assert_eq!(effective_default_branch(&clone, None), Some("main".to_string()));
    }

    #[test]
    fn no_default_branch_reports_instead_of_guessing() {
        // A repo with NO origin remote at all: nothing verifies, nothing is
        // fabricated — every candidate reports, nothing is pruned.
        let dir = temp_dir("nodefault");
        let root = dir.0.join("repo");
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "-b", "trunk"]);
        fs::write(root.join("README.md"), "seed\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "seed"]);
        assert_eq!(effective_default_branch(&root, Some("trunk")), None);

        git(&root, &["branch", "exp/EXP-7"]);
        let path = worktrees_dir(&root).join(sanitize_branch_for_path("exp/EXP-7"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        git(&root, &["worktree", "add", path.to_str().unwrap(), "exp/EXP-7"]);

        let report = prune_landed(&root, &policy(&["exp/"]));
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-7".to_string(), SkipReason::NoDefaultBranch)]
        );

        // `merged` no longer bypasses the check: with no default branch
        // there is no containment proof, so it keeps too (fail-safe).
        let mut merged = policy(&["exp/"]);
        merged.merged.insert("exp/EXP-7".to_string());
        let report = prune_landed(&root, &merged);
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-7".to_string(), SkipReason::NoDefaultBranch)]
        );
    }

    #[test]
    fn busy_path_parks_its_worktree() {
        // A running terminal tab cd'ed anywhere under a worktree keeps it,
        // whatever the branch's landed state says.
        let dir = temp_dir("busy");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-9");
        let mut parked = policy(&["exp/"]);
        // A real dir (tab cwds exist) so canonicalization works on macOS,
        // where git reports /private/tmp for a /tmp-composed worktree path.
        fs::create_dir_all(wt.join("some/subdir")).unwrap();
        parked.busy_paths.push(wt.join("some/subdir"));
        let report = prune_landed(&clone, &parked);
        assert!(report.is_empty(), "{report:?}");
        assert!(wt.exists());
    }

    #[test]
    fn clean_worktree_states() {
        let dir = temp_dir("state");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-8");
        assert_eq!(worktree_dirty_state(&wt), DirtyState::Clean);
        // A missing dir fails `git status` → fail-safe TrackedChanges.
        assert_eq!(
            worktree_dirty_state(&dir.0.join("missing")),
            DirtyState::TrackedChanges
        );
    }

    /// EXP-478: a just-created 0-ahead worktree is exactly what a mid-launch
    /// session looks like to a stale policy — the grace must shield it even
    /// with EMPTY protection sets, and stop once it ages out.
    #[test]
    fn fresh_worktree_survives_the_launch_grace() {
        let dir = temp_dir("grace");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-20");

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-20".to_string(), SkipReason::RecentlyCreated)]
        );
        assert!(wt.exists());
        assert!(branch_exists(&clone, "exp/EXP-20"));

        backdate(&wt);
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-20".to_string()]);
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/EXP-20"));
    }

    /// EXP-478: reuse (a relaunch on a merged issue, fix-conflicts) refreshes
    /// the stamp — an OLD worktree a launch just reclaimed is young again.
    #[test]
    fn reused_worktree_regains_the_grace() {
        let dir = temp_dir("reuse");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-22");
        backdate(&wt);
        // The reuse path of `create_worktree` — same call a relaunch makes.
        let reused = worktree(&clone, "exp/EXP-22");
        assert_eq!(reused, wt);

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-22".to_string(), SkipReason::RecentlyCreated)]
        );
        assert!(wt.exists());
    }

    /// EXP-766: a run whose HOST process is still alive parks its worktree,
    /// even though nothing in THIS process knows about it — that is exactly
    /// the CLI daemon (or a sibling desktop) coding on the shared data dir.
    #[test]
    fn a_live_runs_worktree_is_parked() {
        let dir = temp_dir("livehost");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-30");
        backdate(&wt);
        let data_dir = dir.0.join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let mut record = crate::run_registry::sample_record("run-live");
        record.cwd = wt.clone();
        record.clone = None;
        record.branch = None;
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&data_dir, record.clone());

        let mut watched = policy(&["exp/"]);
        watched.run_registry_dir = Some(data_dir.clone());
        let report = prune_landed(&clone, &watched);
        assert!(report.is_empty(), "{report:?}");
        assert!(wt.exists(), "a live host's worktree must survive");

        // Same record, a pid nobody holds: nothing parks it anymore.
        let mut dead = record;
        dead.host_pid = Some(u32::MAX - 1);
        crate::run_registry::record(&data_dir, dead);
        let report = prune_landed(&clone, &watched);
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-30".to_string()]);
        assert!(!wt.exists());
    }

    /// EXP-766: `status.showUntrackedFiles=no` in the repo config used to make
    /// a worktree full of untracked work look CLEAN — and clean is the state
    /// that is removed without `--force`.
    #[test]
    fn untracked_files_are_seen_through_a_status_config() {
        let dir = temp_dir("hidden");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-31");
        git(&wt, &["config", "status.showUntrackedFiles", "no"]);
        fs::write(wt.join("agent-notes.md"), "work in progress\n").unwrap();
        backdate(&wt);

        assert_eq!(worktree_dirty_state(&wt), DirtyState::UntrackedOnly);

        // And the removal still succeeds, because it now passes --force.
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-31".to_string()]);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert!(!wt.exists());
    }

    /// EXP-478: a live launch hold parks the WHOLE pass — worktree removals
    /// and the branch sweep — and the pass reports it ran into the gate.
    #[test]
    fn launch_hold_blocks_the_whole_pass() {
        let dir = temp_dir("gate");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-21");
        backdate(&wt);
        git(&clone, &["branch", "exp/OLD-9", "origin/main"]);
        let mut sweeping = policy(&["exp/"]);
        sweeping.delete_stale_branches = true;

        let hold = crate::launch_gate::hold(&clone);
        let report = prune_landed(&clone, &sweeping);
        assert!(report.blocked_by_launch, "{report:?}");
        assert!(report.removed_worktrees.is_empty());
        assert!(report.deleted_branches.is_empty());
        assert!(wt.exists());
        assert!(branch_exists(&clone, "exp/OLD-9"));

        drop(hold);
        let report = prune_landed(&clone, &sweeping);
        assert!(!report.blocked_by_launch);
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-21".to_string()]);
        assert!(report.deleted_branches.contains(&"exp/OLD-9".to_string()));
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/OLD-9"));
    }

    // ---- NEW-5: the nightly sweep ----

    fn age_by(worktree: &Path, by: Duration) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(worktree.join(".git"))
            .unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    fn origin_branch_exists(origin: &Path, branch: &str) -> bool {
        branch_exists(origin, branch)
    }

    /// The age rule: only worktrees idle past `max_age` (by the `.git` link
    /// AND the newest record) are finished; the stale one's unpushed work
    /// lands on a leftover branch, named after its session's record.
    #[test]
    fn the_sweep_finishes_only_stale_worktrees() {
        let dir = temp_dir("sweep-age");
        let (origin, clone) = seed(&dir.0);
        let data_dir = dir.0.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let day = Duration::from_secs(24 * 60 * 60);

        // Stale, recorded (ended: no host pid), unpushed work.
        let stale = worktree(&clone, "exp/session-aaaa1111");
        commit_file(&stale, "work.txt", "real work\n", "work");
        age_by(&stale, 2 * day);
        let mut record = crate::run_registry::sample_record("aaaa1111-0000-0000-0000-000000000000");
        record.cwd = stale.clone();
        record.clone = Some(clone.clone());
        record.branch = Some("exp/session-aaaa1111".to_string());
        record.host_pid = None;
        record.recorded_at = crate::run_registry::now_secs() - 3 * day.as_secs();
        crate::run_registry::record(&data_dir, record);

        // Old `.git` link, but its record was written an hour ago: fresh.
        let recent = worktree(&clone, "exp/session-bbbb2222");
        age_by(&recent, 2 * day);
        let mut record = crate::run_registry::sample_record("bbbb2222-0000-0000-0000-000000000000");
        record.cwd = recent.clone();
        record.host_pid = None;
        record.recorded_at = crate::run_registry::now_secs() - 3600;
        crate::run_registry::record(&data_dir, record);

        // Brand new, recorded.
        let young = worktree(&clone, "exp/EXP-60");
        let mut record = crate::run_registry::sample_record("eeee5555-0000-0000-0000-000000000000");
        record.cwd = young.clone();
        record.host_pid = None;
        record.recorded_at = crate::run_registry::now_secs() - 3600;
        crate::run_registry::record(&data_dir, record);

        // Stale, no record, but the branch of an open (settled) session:
        // finished, its id from the branch.
        let orphan = worktree(&clone, "exp/EXP-61");
        age_by(&orphan, 2 * day);

        // EXP-1473: stale, no record and no open session of ours: another
        // install's worktree (its own data dir or account), never touched.
        let foreign = worktree(&clone, "exp/EXP-62");
        commit_file(&foreign, "foreign.txt", "their work\n", "foreign");
        age_by(&foreign, 2 * day);

        let policy = SweepPolicy {
            open: OpenBranches { settled: HashSet::from(["exp/EXP-61".to_string()]), ..OpenBranches::default() },
            ..SweepPolicy::default()
        };
        let report = sweep_stale_worktrees(&clone, &data_dir, SystemTime::now(), &policy);
        let mut branches: Vec<&str> = report.finished.iter().map(|swept| swept.branch.as_str()).collect();
        branches.sort();
        assert_eq!(branches, vec!["exp/EXP-61", "exp/session-aaaa1111"], "{report:?}");
        assert_eq!(
            report.leftovers(),
            vec![(Some("aaaa1111-0000-0000-0000-000000000000"), "exp/leftover-aaaa1111")]
        );
        assert!(!stale.exists() && !orphan.exists());
        assert!(recent.exists() && young.exists() && foreign.exists());
        assert!(origin_branch_exists(&origin, "exp/leftover-aaaa1111"));
        assert!(!origin_branch_exists(&origin, "exp/leftover-exp-EXP-62"));
        assert!(!branch_exists(&clone, "exp/session-aaaa1111"));

        // A shorter policy reaches the hour-old one too (clean: no leftover).
        let short = SweepPolicy { max_age: Duration::from_secs(60), ..SweepPolicy::default() };
        age_by(&young, 2 * day);
        let report = sweep_stale_worktrees(&clone, &data_dir, SystemTime::now(), &short);
        assert_eq!(report.finished.len(), 2, "{report:?}");
        assert!(report.leftovers().is_empty());
        assert!(!recent.exists() && !young.exists());
    }

    /// S-4: an open chat's worktree survives the sweep unless its tree is
    /// idle with every PR merged — and even then never with uncommitted
    /// changes (nothing parked for a live chat).
    #[test]
    fn the_sweep_spares_open_chats_until_settled_and_clean() {
        let dir = temp_dir("sweep-open");
        let (origin, clone) = seed(&dir.0);
        let data_dir = dir.0.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let old = Duration::from_secs(3 * 24 * 60 * 60);
        let kept = worktree(&clone, "exp/session-open1");
        let dirty = worktree(&clone, "exp/session-open2");
        let clean = worktree(&clone, "exp/session-open3");
        let untracked = worktree(&clone, "exp/session-open4");
        let unpushed = worktree(&clone, "exp/session-open5");
        fs::write(dirty.join("README.md"), "edited\n").unwrap();
        fs::write(untracked.join("notes.txt"), "scratch\n").unwrap();
        fs::write(unpushed.join("work.txt"), "local work\n").unwrap();
        git(&unpushed, &["add", "."]);
        git(&unpushed, &["commit", "--quiet", "-m", "local"]);
        for path in [&kept, &dirty, &clean, &untracked, &unpushed] {
            age_by(path, old);
        }
        let policy = SweepPolicy {
            open: OpenBranches {
                keep: HashSet::from(["exp/session-open1".to_string()]),
                settled: HashSet::from([
                    "exp/session-open2".to_string(),
                    "exp/session-open3".to_string(),
                    "exp/session-open4".to_string(),
                    "exp/session-open5".to_string(),
                ]),
            },
            ..SweepPolicy::default()
        };
        let report = sweep_stale_worktrees(&clone, &data_dir, SystemTime::now(), &policy);
        let branches: Vec<&str> = report.finished.iter().map(|swept| swept.branch.as_str()).collect();
        assert_eq!(branches, vec!["exp/session-open3"], "{report:?}");
        assert!(report.leftovers().is_empty());
        assert!(kept.exists() && dirty.exists() && !clean.exists());
        assert!(untracked.join("notes.txt").exists(), "untracked files stay with the open chat");
        assert!(unpushed.exists(), "an unpushed commit stays with the open chat");
        // Its work landing upstream (a squash merge) frees it.
        fs::write(origin.join("work.txt"), "local work\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "squashed"]);
        git(&clone, &["fetch", "-q", "origin"]);
        let report = sweep_stale_worktrees(&clone, &data_dir, SystemTime::now(), &policy);
        let branches: Vec<&str> = report.finished.iter().map(|swept| swept.branch.as_str()).collect();
        assert_eq!(branches, vec!["exp/session-open5"], "{report:?}");
        assert!(report.leftovers().is_empty());
        assert!(!unpushed.exists());
    }

    /// S-4: a tree is settled only when nothing in it works or asks and it
    /// has PRs, every one merged; ended sessions name no branch.
    #[test]
    fn open_branches_splits_settled_trees() {
        use domain::rows::{PullRequestRow, SessionRow};
        let row = |id: &str, root: Option<&str>, status: &str, branch: &str| SessionRow {
            id: id.to_string(),
            root_id: root.map(str::to_string),
            status: Some(status.to_string()),
            branch: Some(branch.to_string()),
            ..SessionRow::default()
        };
        let pr = |root: &str, state: &str| -> PullRequestRow {
            serde_json::from_value(serde_json::json!({
                "id": format!("pr-{root}-{state}"),
                "root_session_id": root,
                "state": state,
            }))
            .unwrap()
        };
        let mut busy_child = row("b1", Some("b"), "running", "exp/b1");
        busy_child.busy = true;
        let sessions = vec![
            row("a", None, "idle", "exp/a"),
            row("b", None, "idle", "exp/b"),
            busy_child,
            row("c", None, "idle", "exp/c"),
            row("d", None, "idle", "exp/d"),
            row("e", None, "ended", "exp/e"),
        ];
        let pulls = vec![pr("a", "merged"), pr("b", "merged"), pr("c", "merged"), pr("c", "open")];
        let open = open_branches(&sessions, &pulls);
        assert_eq!(open.settled, HashSet::from(["exp/a".to_string()]));
        assert_eq!(
            open.keep,
            HashSet::from(["exp/b".to_string(), "exp/b1".to_string(), "exp/c".to_string(), "exp/d".to_string()])
        );
    }

    /// A live run's worktree is never swept, however old — by its record's
    /// live host or by the host's own live holders.
    #[test]
    fn the_sweep_never_touches_a_live_run() {
        let dir = temp_dir("sweep-live");
        let (_origin, clone) = seed(&dir.0);
        let data_dir = dir.0.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let old = Duration::from_secs(3 * 24 * 60 * 60);

        let running = worktree(&clone, "exp/session-cccc3333");
        age_by(&running, old);
        let mut record = crate::run_registry::sample_record("cccc3333");
        record.cwd = running.clone();
        record.host_pid = Some(std::process::id());
        record.recorded_at = 0;
        crate::run_registry::record(&data_dir, record);

        let held = worktree(&clone, "exp/session-dddd4444");
        age_by(&held, old);
        let mut record = crate::run_registry::sample_record("dddd4444");
        record.cwd = held.clone();
        record.host_pid = None;
        record.recorded_at = 0;
        crate::run_registry::record(&data_dir, record);
        let policy = SweepPolicy {
            live: crate::run_cleanup::LiveHolders {
                branches: vec!["exp/session-dddd4444".to_string()],
                ..Default::default()
            },
            ..SweepPolicy::default()
        };

        let report = sweep_stale_worktrees(&clone, &data_dir, SystemTime::now(), &policy);
        assert!(running.exists() && held.exists());
        assert_eq!(report.finished.len(), 1, "{report:?}");
        assert!(matches!(
            report.finished[0].outcome,
            crate::run_cleanup::FinishOutcome::Skipped(crate::run_cleanup::SkipReason::HeldByLiveRun(_))
        ));
    }
}
