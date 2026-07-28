//! Shared two-click PR merge/close machinery (EXP-325).
//!
//! Three surfaces offer the same "Merge PR" affordance — the Reviews rail
//! rows, the issue-detail properties sidebar, and the terminal dock's
//! session tabs — and they must behave identically: first click arms
//! ("Confirm merge", ~5s auto-disarm), second click fires the server-side
//! GitHub-App squash merge, failures caption the row, success holds the
//! spinner until the Electric echo flips `pr_state`. The state lives in ONE
//! app-global entity (the [`crate::coding_flow::LocalSessions`] pattern), so
//! a merge started from ANY surface renders its phase and failure on every
//! other surface — a conflict hit from a terminal tab shows up in the
//! Reviews list exactly as if Merge had been clicked there.
//!
//! Keys share one namespace: an issue UUID for `issues.mergePr`,
//! [`close_pr_key`] (`close:<uuid>`) for `issues.closePr`, and
//! [`pull_merge_key`] (`<repo-uuid>#<number>`) for unlinked pulls — the
//! prefixes/`#` can never collide. Error captions always key on the ROW
//! (the issue id / pull key), so a failed close renders under the same row
//! as a failed merge — the caption carries a [`FailedOp`] so the surfaces can
//! still tell the two apart (only a failed merge offers "Fix conflicts").

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, AppContext as _, Entity, SharedString, Subscription};
use sync::Store;

use crate::queries;

/// Merge-state key for an unlinked pull. Shares the namespace with issue
/// UUID keys — `repo-uuid#number` can never collide with those.
pub fn pull_merge_key(repository_id: &str, number: u64) -> String {
    format!("{repository_id}#{number}")
}

/// Arm/in-flight key for an issue row's close-without-merge action (EXP-100).
/// The `close:` prefix can never collide with an issue UUID or a pull key.
pub fn close_pr_key(issue_id: &str) -> String {
    format!("close:{issue_id}")
}

/// The server's user-facing failure message when there is one;
/// transport-level errors keep the full rendering.
pub fn user_message(err: api::ApiError) -> String {
    match err {
        api::ApiError::Http { message, .. } => message,
        other => other.to_string(),
    }
}

/// Which op produced a failure caption. The "Fix conflicts" recovery run
/// rebases, force-pushes and then MERGES the pull request, so it may only be
/// offered after a failed MERGE — a user who asked to CLOSE a PR must never
/// be handed a button that merges it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedOp {
    Merge,
    Close,
}

/// One confirmable merge-shaped server call.
pub enum MergeOp {
    /// `issues.mergePr` — squash-merge the issue's linked PR (a batch PR
    /// completes every linked issue). Echo-settled: the spinner holds until
    /// `pr_state` leaves `open`.
    MergeIssuePr { issue_id: String },
    /// `issues.closePr` — close the linked PR WITHOUT merging (EXP-100).
    /// Echo-settled like the merge.
    CloseIssuePr { issue_id: String },
    /// `repositories.mergePull` — an issue-unlinked PR. No Electric echo:
    /// completion clears in-flight immediately and the caller's `on_success`
    /// drops the row from its local state.
    MergePull {
        repository_id: String,
        number: u64,
    },
}

impl MergeOp {
    /// The arm/in-flight key.
    fn key(&self) -> String {
        match self {
            MergeOp::MergeIssuePr { issue_id } => issue_id.clone(),
            MergeOp::CloseIssuePr { issue_id } => close_pr_key(issue_id),
            MergeOp::MergePull {
                repository_id,
                number,
            } => pull_merge_key(repository_id, *number),
        }
    }

    /// The ROW key failure captions render under (issue rows caption on the
    /// issue id whichever of merge/close failed).
    fn row_key(&self) -> String {
        match self {
            MergeOp::MergeIssuePr { issue_id } | MergeOp::CloseIssuePr { issue_id } => {
                issue_id.clone()
            }
            MergeOp::MergePull { .. } => self.key(),
        }
    }

    /// Which action a failure of this op reports — the recovery affordance
    /// gates on it (a failed close must not offer a run that merges).
    fn failed_op(&self) -> FailedOp {
        match self {
            MergeOp::CloseIssuePr { .. } => FailedOp::Close,
            MergeOp::MergeIssuePr { .. } | MergeOp::MergePull { .. } => FailedOp::Merge,
        }
    }

    /// Keys whose in-flight call blocks this op — merge and close of the
    /// same issue row guard each other.
    fn guard_keys(&self) -> Vec<String> {
        match self {
            MergeOp::MergeIssuePr { issue_id } | MergeOp::CloseIssuePr { issue_id } => {
                vec![issue_id.clone(), close_pr_key(issue_id)]
            }
            MergeOp::MergePull { .. } => vec![self.key()],
        }
    }

    /// Whether success is settled by the Electric echo (issue rows) rather
    /// than immediately on completion (unlinked pulls).
    fn echo_settled(&self) -> bool {
        !matches!(self, MergeOp::MergePull { .. })
    }

    fn describe(&self) -> String {
        match self {
            MergeOp::MergeIssuePr { issue_id } => format!("issues.mergePr({issue_id})"),
            MergeOp::CloseIssuePr { issue_id } => format!("issues.closePr({issue_id})"),
            MergeOp::MergePull {
                repository_id,
                number,
            } => format!("repositories.mergePull({repository_id}#{number})"),
        }
    }

    fn run(&self, trpc: &api::TrpcClient) -> Result<(), api::ApiError> {
        match self {
            MergeOp::MergeIssuePr { issue_id } => {
                api::issues::merge_pr(trpc, issue_id).map(|_| ())
            }
            MergeOp::CloseIssuePr { issue_id } => {
                api::issues::close_pr(trpc, issue_id).map(|_| ())
            }
            MergeOp::MergePull {
                repository_id,
                number,
            } => api::repositories::merge_pull(trpc, repository_id, *number).map(|_| ()),
        }
    }
}

/// The app-global two-click merge state. Surfaces read [`Self::armed`] /
/// [`Self::merging`] / [`Self::error`] and observe the entity for repaints;
/// all writes go through [`two_click`] / [`disarm`].
pub struct MergeState {
    /// The armed row's key — any other click or ~5s of inactivity disarms.
    arm: Option<String>,
    /// Bumped on every arm/disarm — a stale disarm timer checks it before
    /// clearing so it never cancels a newer arm.
    arm_seq: u64,
    /// Keys with an in-flight call. Echo-settled ops keep the key until the
    /// issues-collection observer sees `pr_state` leave `open`; pull ops
    /// clear on completion.
    merging: HashSet<String>,
    /// The last failure, `(row_key, message, failed_op)` — one caption at a
    /// time, cleared on the next confirmed attempt anywhere (the pre-EXP-325
    /// reviews-rail semantic). `failed_op` says whether merge or close
    /// produced it, so only a merge failure offers the recovery run.
    error: Option<(String, SharedString, FailedOp)>,
    _subscriptions: Vec<Subscription>,
}

struct MergeStateGlobal(Entity<MergeState>);

impl gpui::Global for MergeStateGlobal {}

impl MergeState {
    pub fn global(cx: &mut App) -> Entity<MergeState> {
        if let Some(global) = cx.try_global::<MergeStateGlobal>() {
            return global.0.clone();
        }
        let state = cx.new(|cx| {
            let mut subscriptions = Vec::new();
            // Echo settlement: a merged/closed PR flips `pr_state` away from
            // `open` — collect that issue's lingering "Merging…"/arm/error.
            // (`try_global`: headless view tests run without a sync store.)
            let issues = Store::try_global(cx).map(|store| store.collections().issues.clone());
            if let Some(issues) = issues {
                subscriptions.push(cx.observe(&issues, |this: &mut MergeState, _, cx| {
                    this.prune_settled(cx);
                }));
            }
            MergeState {
                arm: None,
                arm_seq: 0,
                merging: HashSet::new(),
                error: None,
                _subscriptions: subscriptions,
            }
        });
        cx.set_global(MergeStateGlobal(state.clone()));
        state
    }

    pub fn armed(&self, key: &str) -> bool {
        self.arm.as_deref() == Some(key)
    }

    pub fn merging(&self, key: &str) -> bool {
        self.merging.contains(key)
    }

    /// The failure caption for a ROW key (an issue id or a pull key).
    pub fn error(&self, row_key: &str) -> Option<SharedString> {
        self.error
            .as_ref()
            .filter(|(key, _, _)| key == row_key)
            .map(|(_, message, _)| message.clone())
    }

    /// Which action produced this row's caption — the "Fix conflicts"
    /// recovery run is offered on [`FailedOp::Merge`] only.
    pub fn failed_op(&self, row_key: &str) -> Option<FailedOp> {
        self.error
            .as_ref()
            .filter(|(key, _, _)| key == row_key)
            .map(|(_, _, op)| *op)
    }

    /// First click of the two-click confirm: arm `key` and start the ~5s
    /// seq-guarded auto-disarm timer.
    fn arm_key(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        self.arm = Some(key);
        self.arm_seq += 1;
        let seq = self.arm_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                if this.arm_seq == seq && this.arm.is_some() {
                    this.arm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Drop any pending arm — row/background clicks call this so a stray
    /// armed confirm never lingers.
    pub fn disarm(cx: &mut App) {
        let state = MergeState::global(cx);
        state.update(cx, |this, cx| {
            if this.arm.take().is_some() {
                this.arm_seq += 1;
                cx.notify();
            }
        });
    }

    /// Drop transient state for PULL keys (`repo#n`) no longer in `live` —
    /// the Reviews render calls this over the fetched pull list (a pull
    /// merged elsewhere has no Electric echo to settle it). Issue keys are
    /// echo-settled by the collection observer and stay untouched here.
    pub fn retain_pull_keys(&mut self, live: &HashSet<String>, cx: &mut gpui::Context<Self>) {
        let dead = |key: &str| key.contains('#') && !live.contains(key);
        let mut changed = false;
        let before = self.merging.len();
        self.merging.retain(|key| !dead(key));
        changed |= self.merging.len() != before;
        if self.arm.as_deref().is_some_and(dead) {
            self.arm = None;
            self.arm_seq += 1;
            changed = true;
        }
        if self.error.as_ref().is_some_and(|(key, _, _)| dead(key)) {
            self.error = None;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Collect keys whose issue's PR is no longer open (the Electric echo
    /// after a merge/close — also covers a PR closed from GitHub itself).
    fn prune_settled(&mut self, cx: &mut gpui::Context<Self>) {
        let changed = {
            let Some(store) = Store::try_global(cx) else {
                return;
            };
            let issues = store.collections().issues.read(cx);
            let settled = |key: &str| -> bool {
                let id = key.strip_prefix("close:").unwrap_or(key);
                if id.contains('#') {
                    // Pull keys are not issue-keyed — `retain_pull_keys` owns them.
                    return false;
                }
                match issues.get(id) {
                    Some(issue) => issue.pr_state.as_deref() != Some("open"),
                    // Unknown row (unsynced/out of scope) — leave it alone.
                    None => false,
                }
            };
            let mut changed = false;
            let before = self.merging.len();
            self.merging.retain(|key| !settled(key));
            changed |= self.merging.len() != before;
            if self.arm.as_deref().is_some_and(settled) {
                self.arm = None;
                self.arm_seq += 1;
                changed = true;
            }
            if self.error.as_ref().is_some_and(|(key, _, _)| settled(key)) {
                self.error = None;
                changed = true;
            }
            changed
        };
        if changed {
            cx.notify();
        }
    }
}

/// The shared two-click flow: first call arms (auto-disarm ~5s), second call
/// fires the op on the background executor. Failures land in the shared
/// error slot (and run `on_failure` — the terminal dock jumps to the Reviews
/// tool with it); echo-settled successes hold the in-flight spinner until
/// the Electric echo, pull successes clear it and run `on_success`.
pub fn two_click(
    op: MergeOp,
    on_failure: Option<Box<dyn FnOnce(&mut App)>>,
    on_success: Option<Box<dyn FnOnce(&mut App)>>,
    cx: &mut App,
) {
    let state = MergeState::global(cx);
    let key = op.key();
    // Ignore while any guarded call on this row is already in flight.
    if op
        .guard_keys()
        .iter()
        .any(|guard| state.read(cx).merging.contains(guard))
    {
        return;
    }
    if state.read(cx).arm.as_deref() != Some(key.as_str()) {
        state.update(cx, |this, cx| this.arm_key(key, cx));
        return;
    }

    // Confirmed — fire the server-side call.
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] {} skipped: no active account", op.describe());
        return;
    };
    state.update(cx, |this, cx| {
        this.arm = None;
        this.arm_seq += 1;
        this.error = None;
        this.merging.insert(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_op = Arc::new(op);
            let bg_op = call_op.clone();
            let result = cx
                .background_executor()
                .spawn(async move { bg_op.run(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        if call_op.echo_settled() {
                            // The issues-collection observer clears the key
                            // when the echo flips `pr_state`.
                        } else {
                            this.merging.remove(&key);
                            cx.notify();
                            if let Some(on_success) = on_success {
                                on_success(cx);
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!("[ui] {} failed: {err}", call_op.describe());
                        this.merging.remove(&key);
                        this.error = Some((
                            call_op.row_key(),
                            SharedString::from(user_message(err)),
                            call_op.failed_op(),
                        ));
                        cx.notify();
                        if let Some(on_failure) = on_failure {
                            on_failure(cx);
                        }
                    }
                }
            });
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One namespace, three collision-free key shapes; merge/close of the
    /// same issue guard each other and caption the same ROW.
    #[test]
    fn keys_share_one_collision_free_namespace() {
        assert_eq!(pull_merge_key("repo-1", 7), "repo-1#7");
        assert_eq!(close_pr_key("issue-1"), "close:issue-1");
        let merge = MergeOp::MergeIssuePr {
            issue_id: "i1".to_string(),
        };
        let close = MergeOp::CloseIssuePr {
            issue_id: "i1".to_string(),
        };
        let pull = MergeOp::MergePull {
            repository_id: "r1".to_string(),
            number: 3,
        };
        assert_eq!(merge.key(), "i1");
        assert_eq!(close.key(), "close:i1");
        assert_eq!(pull.key(), "r1#3");
        assert_eq!(merge.row_key(), "i1");
        assert_eq!(close.row_key(), "i1");
        assert_eq!(pull.row_key(), "r1#3");
        assert_eq!(merge.guard_keys(), close.guard_keys());
        assert_eq!(pull.guard_keys(), vec!["r1#3".to_string()]);
        // Issue ops settle on the Electric echo; pulls settle immediately.
        assert!(merge.echo_settled());
        assert!(close.echo_settled());
        assert!(!pull.echo_settled());
        // …but the caption still says WHICH op failed: "Fix conflicts" ends
        // in a merge, so a failed close must never be offered it.
        assert_eq!(merge.failed_op(), FailedOp::Merge);
        assert_eq!(close.failed_op(), FailedOp::Close);
        assert_eq!(pull.failed_op(), FailedOp::Merge);
    }

    #[test]
    fn user_message_prefers_the_servers_message() {
        let err = api::ApiError::Http {
            status: 409,
            message: "Pull request is not mergeable".to_string(),
        };
        assert_eq!(user_message(err), "Pull request is not mergeable");
        // Transport-level errors keep their full rendering.
        assert!(!user_message(api::ApiError::Unauthorized).is_empty());
    }
}
