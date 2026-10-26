//! Shared two-click PR merge/close machinery (EXP-325).
//!
//! Every "Merge PR" affordance — the issue tray, the Guide's PR rows, the
//! PR diff header, the session header — behaves identically: first click
//! arms ("Confirm merge", ~5s auto-disarm), second click fires the
//! server-side GitHub-App squash merge, failures toast, success holds the
//! spinner until the Electric echo flips the `pull_requests` row's `state`.
//! The state lives in ONE app-global entity, so a merge started from ANY
//! surface renders its phase on every other surface.
//!
//! EXP-1294: every op names a PULL REQUEST (`pullRequests.merge` /
//! `pullRequests.close`). Keys share one namespace: the pull request id for
//! a merge, [`close_pr_key`] (`close:<id>`) for a close. A refusal is the
//! error toast (no recovery run); EXP-1391: the server also posts a refused
//! merge into the thread's root session, which takes it over. EXP-1378: the
//! check gate's refusal (428) offers `Merge anyway` (`force`) instead of
//! Retry.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, AppContext as _, Entity, SharedString, Subscription};
use sync::Store;

use crate::queries;

/// Arm/in-flight key for closing a pull request without merging (EXP-100).
/// The `close:` prefix can never collide with a pull request id.
pub fn close_pr_key(pull_request_id: &str) -> String {
    format!("close:{pull_request_id}")
}

/// The server's user-facing failure message when there is one; everything
/// else gets [`api::ApiError::user_message`]'s plain sentence (EXP-533 — an
/// offline machine says so instead of leaking reqwest's
/// `error sending request for url …`).
pub fn user_message(err: api::ApiError) -> String {
    err.user_message()
}

/// Which op produced a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedOp {
    Merge,
    Close,
}

/// One confirmable merge-shaped server call (EXP-1294: keyed by pull
/// request).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOp {
    /// `pullRequests.merge` — squash-merge the pull request. Echo-settled:
    /// the spinner holds until its `state` leaves `open`. Merging never ends
    /// sessions.
    ///
    /// EXP-1248: `stack_through` = the stack confirm's answer: `Some(id)`
    /// posts `pullRequests.merge({pullRequestId: id, stack: true})` — ONE
    /// GitHub merge-async through `id` (it lands everything beneath it).
    /// `pull_request_id` stays the CLICKED row (key, spinner, caption): it
    /// always sits at or below `id`, so its own echo settles the spinner.
    ///
    /// EXP-1378: `force` = `Merge anyway` past the check gate.
    Merge {
        pull_request_id: String,
        stack_through: Option<String>,
        force: bool,
    },
    /// `pullRequests.close` — close WITHOUT merging (EXP-100). Echo-settled
    /// like the merge.
    Close { pull_request_id: String },
}

impl MergeOp {
    /// A plain merge of `pull_request_id`.
    pub fn merge(pull_request_id: impl Into<String>) -> Self {
        MergeOp::Merge {
            pull_request_id: pull_request_id.into(),
            stack_through: None,
            force: false,
        }
    }

    /// The same merge past the check gate (`force: true`); a close stays
    /// itself.
    fn forced(&self) -> Self {
        match self {
            MergeOp::Merge {
                pull_request_id,
                stack_through,
                ..
            } => MergeOp::Merge {
                pull_request_id: pull_request_id.clone(),
                stack_through: stack_through.clone(),
                force: true,
            },
            close => close.clone(),
        }
    }

    /// The arm/in-flight key.
    fn key(&self) -> String {
        match self {
            MergeOp::Merge {
                pull_request_id, ..
            } => pull_request_id.clone(),
            MergeOp::Close { pull_request_id } => close_pr_key(pull_request_id),
        }
    }

    fn failed_op(&self) -> FailedOp {
        match self {
            MergeOp::Close { .. } => FailedOp::Close,
            MergeOp::Merge { .. } => FailedOp::Merge,
        }
    }

    /// Keys whose in-flight call blocks this op — merge and close of the
    /// same pull request guard each other.
    fn guard_keys(&self) -> Vec<String> {
        let id = match self {
            MergeOp::Merge {
                pull_request_id, ..
            }
            | MergeOp::Close { pull_request_id } => pull_request_id.clone(),
        };
        vec![close_pr_key(&id), id]
    }

    fn describe(&self) -> String {
        match self {
            MergeOp::Merge {
                pull_request_id,
                stack_through,
                force,
            } => {
                let force = if *force { ", force" } else { "" };
                match stack_through {
                    Some(through) => format!(
                        "pullRequests.merge({through}, stack, from {pull_request_id}{force})"
                    ),
                    None => format!("pullRequests.merge({pull_request_id}{force})"),
                }
            }
            MergeOp::Close { pull_request_id } => format!("pullRequests.close({pull_request_id})"),
        }
    }

    /// `Ok(true)` = the pull request landed (or was already in); `Ok(false)`
    /// = GitHub's merge queue only took it (EXP-1165: `merged: false,
    /// queued: true`), so no echo is coming yet and the row stays.
    fn run(&self, trpc: &api::TrpcClient) -> Result<bool, api::ApiError> {
        match self {
            MergeOp::Merge {
                pull_request_id,
                stack_through,
                force,
            } => {
                let (id, stack) = match stack_through {
                    Some(through) => (through.as_str(), true),
                    None => (pull_request_id.as_str(), false),
                };
                let out = api::sessions::pr_merge(trpc, id, stack, *force)?;
                match out.error {
                    Some(message) if !out.merged && !out.queued => {
                        Err(api::ApiError::Http { status: 400, message })
                    }
                    _ => Ok(out.merged && !out.queued),
                }
            }
            MergeOp::Close { pull_request_id } => {
                api::sessions::pr_close(trpc, pull_request_id).map(|_| true)
            }
        }
    }
}

/// The app-global two-click merge state. Surfaces read [`Self::armed`] /
/// [`Self::merging`] and observe the entity for repaints;
/// all writes go through [`two_click`] / [`disarm`].
pub struct MergeState {
    /// The armed row's key — any other click or ~5s of inactivity disarms.
    arm: Option<String>,
    /// Bumped on every arm/disarm — a stale disarm timer checks it before
    /// clearing so it never cancels a newer arm.
    arm_seq: u64,
    /// Keys with an in-flight call. Echo-settled ops keep the key until the
    /// pull-requests observer sees `state` leave `open`.
    merging: HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

/// One merge/close failure: what its toast says.
#[derive(Clone, Debug)]
pub struct MergeFailure {
    pub message: SharedString,
    /// Whether MERGE or CLOSE produced it (the toast's verb).
    pub op: FailedOp,
    /// EXP-1378: the check gate refused (tRPC `PRECONDITION_REQUIRED` /
    /// HTTP 428: required checks running or red) — its toast offers
    /// `Merge anyway` instead of Retry.
    pub checks: bool,
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
            // Echo settlement: a merged/closed PR flips its `state` away
            // from `open` — collect its lingering "Merging…"/arm.
            // (`try_global`: headless view tests run without a sync store.)
            let collections = Store::try_global(cx).map(|store| store.collections().clone());
            if let Some(collections) = collections {
                subscriptions.push(cx.observe(
                    &collections.pull_requests,
                    |this: &mut MergeState, _, cx| {
                        this.prune_settled(cx);
                    },
                ));
            }
            MergeState {
                arm: None,
                arm_seq: 0,
                merging: HashSet::new(),
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

    /// Collect keys whose pull request is no longer open (the Electric echo
    /// after a merge/close — also covers a PR closed from GitHub itself).
    fn prune_settled(&mut self, cx: &mut gpui::Context<Self>) {
        let changed = {
            let Some(store) = Store::try_global(cx) else {
                return;
            };
            let pulls = store.collections().pull_requests.read(cx);
            let settled = |key: &str| -> bool {
                let id = key.strip_prefix("close:").unwrap_or(key);
                // Unknown row (unsynced/out of scope) — leave it alone.
                pulls.get(id).is_some_and(|pull| !pull.is_open())
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
            changed
        };
        if changed {
            cx.notify();
        }
    }
}

/// What a [`two_click`] call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwoClick {
    /// A guarded call was already in flight (or no account) — nothing fired.
    Ignored,
    /// First click — armed the ~5s confirm.
    Armed,
    /// Second click — the server-side call fired.
    Fired,
}

/// Fire an op that was CONFIRMED elsewhere (the stack merge dialog is the
/// confirm): arms and fires in one go.
pub fn fire_confirmed(op: MergeOp, cx: &mut App) -> TwoClick {
    let key = op.key();
    MergeState::global(cx).update(cx, |this, cx| this.arm_key(key, cx));
    two_click(op, cx)
}

/// EXP-1248: the ONE stack confirm for merging `pull_request_id` in `mode`
/// (`Stack` = the merge control, `Through` = a stack row's ghost action).
/// Reads its team's pull requests off the synced collections; `None` (not in
/// a linear open stack, or nothing synced) merges plainly.
pub(crate) fn stack_merge_confirm_for(
    pull_request_id: &str,
    mode: domain::pr_stack::StackConfirmMode,
    cx: &App,
) -> Option<domain::pr_stack::StackMergeConfirm> {
    let snapshot = team_snapshot_of(pull_request_id, cx)?;
    let pull = snapshot.pull(pull_request_id)?;
    snapshot.with_set(|set| domain::pr_stack::stack_merge_confirm(pull, set, mode))
}

/// The [`crate::session_data::PrSnapshot`] of `pull_request_id`'s team.
fn team_snapshot_of(pull_request_id: &str, cx: &App) -> Option<crate::session_data::PrSnapshot> {
    let store = Store::try_global(cx)?;
    let team_id = store
        .collections()
        .pull_requests
        .read(cx)
        .get(pull_request_id)?
        .team_id
        .clone()?;
    Some(crate::session_data::team_pr_snapshot(&team_id, cx))
}

/// EXP-1248: the ONE merge control's label — `Merge stack` on a member of a
/// linear open stack (contract `diffUi.mergeStack`), else `Merge PR`.
pub(crate) fn merge_label_for(pull_request_id: &str, cx: &App) -> &'static str {
    let stacked = team_snapshot_of(pull_request_id, cx).is_some_and(|snapshot| {
        snapshot.pull(pull_request_id).is_some_and(|pull| {
            snapshot.with_set(|set| {
                domain::pr_stack::open_pr_shape(pull, set) == domain::pr_stack::PrGraphShape::Stack
            })
        })
    });
    if stacked {
        domain::pr_stack::MERGE_STACK_LABEL
    } else {
        domain::contract::DIFF_UI_MERGE_PR
    }
}

/// The confirm window's size: the one-line title band, a body of one or two
/// lines and the two-button footer.
const STACK_CONFIRM_WIDTH: f32 = 420.;
const STACK_CONFIRM_HEIGHT: f32 = 200.;

/// The op the confirm's primary fires: `pullRequests.merge({pullRequestId:
/// confirm.pull_request_id, stack: true})`, keyed on the CLICKED pull
/// request (its spinner and caption; it always lands, so its echo settles).
pub(crate) fn stack_confirm_op(
    clicked: &str,
    confirm: &domain::pr_stack::StackMergeConfirm,
) -> MergeOp {
    MergeOp::Merge {
        pull_request_id: clicked.to_string(),
        stack_through: Some(confirm.pull_request_id.clone()),
        force: false,
    }
}

/// The failed toast's thing: `the stack` for a stack merge, else `the PR`
/// (`errors.mergeRefusal`).
fn merge_failure_thing(op: &MergeOp) -> &'static str {
    match op {
        MergeOp::Merge {
            stack_through: Some(_),
            ..
        } => "the stack",
        _ => "the PR",
    }
}

/// EXP-1248: the ONE confirm (it replaced the 3-way dialog): the title is
/// the primary button too (`Merge stack` / `Merge through here`), the body
/// lists what lands and what stays open, Cancel beside it.
pub(crate) fn stack_confirm_alert(
    clicked: &str,
    confirm: &domain::pr_stack::StackMergeConfirm,
) -> crate::native_dialog::AlertSpec {
    let op = stack_confirm_op(clicked, confirm);
    crate::native_dialog::AlertSpec::new(confirm.title.clone(), confirm.body.clone(), confirm.title.clone())
        .ok_icon(crate::icons::registry::PR_MERGED)
        .width(gpui::px(STACK_CONFIRM_WIDTH))
        .height(gpui::px(STACK_CONFIRM_HEIGHT))
        .on_ok(move |_, cx| {
            fire_confirmed(op.clone(), cx);
            true
        })
}

/// EXP-1248: a stack merge asks its ONE confirm. Returns `true` when the
/// confirm took the click (it IS the confirm, so its answer fires at once);
/// `false` = not in a linear open stack: the caller runs its plain two-click
/// merge (or, for `Through`, nothing).
pub(crate) fn ask_stack_merge_mode(
    pull_request_id: &str,
    mode: domain::pr_stack::StackConfirmMode,
    window: &mut gpui::Window,
    cx: &mut App,
) -> bool {
    // An in-flight merge or close of this row: the caller's guard ignores it.
    let state = MergeState::global(cx);
    if MergeOp::merge(pull_request_id)
        .guard_keys()
        .iter()
        .any(|key| state.read(cx).merging.contains(key))
    {
        return false;
    }
    let Some(confirm) = stack_merge_confirm_for(pull_request_id, mode, cx) else {
        return false;
    };
    MergeState::disarm(cx);
    let spec = stack_confirm_alert(pull_request_id, &confirm);
    crate::native_dialog::open_alert(window, cx, spec);
    true
}

/// The merge control's path ([`ask_stack_merge_mode`] in `Stack` mode).
pub(crate) fn ask_stack_merge(
    pull_request_id: &str,
    window: &mut gpui::Window,
    cx: &mut App,
) -> bool {
    ask_stack_merge_mode(pull_request_id, domain::pr_stack::StackConfirmMode::Stack, window, cx)
}

/// The shared two-click flow: first call arms (auto-disarm ~5s), second call
/// fires the op on the background executor. Failures toast; successes hold the in-flight spinner until the
/// Electric echo.
pub fn two_click(op: MergeOp, cx: &mut App) -> TwoClick {
    let state = MergeState::global(cx);
    let key = op.key();
    // Ignore while any guarded call on this row is already in flight.
    if op
        .guard_keys()
        .iter()
        .any(|guard| state.read(cx).merging.contains(guard))
    {
        return TwoClick::Ignored;
    }
    if state.read(cx).arm.as_deref() != Some(key.as_str()) {
        state.update(cx, |this, cx| this.arm_key(key, cx));
        return TwoClick::Armed;
    }

    // Confirmed — fire the server-side call.
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] {} skipped: no active account", op.describe());
        return TwoClick::Ignored;
    };
    state.update(cx, |this, cx| {
        this.arm = None;
        this.arm_seq += 1;
        this.merging.insert(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_op = Arc::new(op);
            let bg_op = call_op.clone();
            let retry_op = call_op.clone();
            let result = cx
                .background_executor()
                .spawn(async move { bg_op.run(&trpc) })
                .await;
            let failed = this.update(cx, |this, cx| {
                match result {
                    Ok(false) => {
                        // Queued on GitHub: nothing landed, no echo will
                        // settle it now; release the spinner, keep the row.
                        this.merging.remove(&key);
                        cx.notify();
                        None
                    }
                    // The pull-requests observer clears the key when the
                    // echo flips `state`.
                    Ok(true) => None,
                    Err(err) => {
                        log::warn!("[ui] {} failed: {err}", call_op.describe());
                        this.merging.remove(&key);
                        // Classify BEFORE the message is consumed.
                        let checks = err.is_precondition_required();
                        let failure = MergeFailure {
                            message: SharedString::from(user_message(err)),
                            op: call_op.failed_op(),
                            checks,
                        };
                        cx.notify();
                        Some(failure)
                    }
                }
            });
            if let Ok(Some(failure)) = failed {
                let _ = cx.update(|cx| {
                    // FEED-79: a refusal is the error toast. A merge/close
                    // re-runs safely (a settled PR just refuses); the check
                    // gate's refusal offers `Merge anyway` (the same merge
                    // with `force`) instead (EXP-1378).
                    let verb = match failure.op {
                        FailedOp::Merge => "merge",
                        FailedOp::Close => "close",
                    };
                    let thing = merge_failure_thing(&retry_op);
                    let description = Some(failure.message.to_string());
                    if failure.checks {
                        let forced = retry_op.forced();
                        crate::action_error::failed_action_with(
                            verb,
                            thing,
                            description,
                            crate::feedback::MERGE_ANYWAY,
                            move |_, cx| {
                                fire_confirmed(forced.clone(), cx);
                            },
                            cx,
                        );
                    } else {
                        crate::action_error::failed_action_with_retry(
                            verb,
                            thing,
                            description,
                            move |_, cx| {
                                fire_confirmed((*retry_op).clone(), cx);
                            },
                            cx,
                        );
                    }
                });
            }
        })
        .detach();
    });
    TwoClick::Fired
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One namespace, collision-free key shapes; merge/close of the same
    /// pull request guard each other and caption the same ROW.
    #[test]
    fn keys_share_one_collision_free_namespace() {
        assert_eq!(close_pr_key("pr-1"), "close:pr-1");
        let merge = MergeOp::merge("pr-1");
        let close = MergeOp::Close {
            pull_request_id: "pr-1".to_string(),
        };
        assert_eq!(merge.key(), "pr-1");
        assert_eq!(close.key(), "close:pr-1");
        assert_eq!(merge.guard_keys(), close.guard_keys());
        assert_eq!(merge.describe(), "pullRequests.merge(pr-1)");
        assert_eq!(close.describe(), "pullRequests.close(pr-1)");
        assert_eq!(merge.failed_op(), FailedOp::Merge);
        assert_eq!(close.failed_op(), FailedOp::Close);
    }

    /// EXP-1248: the ONE confirm's primary posts `pullRequests.merge(
    /// {pullRequestId: confirm.pull_request_id, stack: true})`, keyed on the
    /// clicked row.
    #[test]
    fn the_stack_confirm_maps_to_one_merge_call() {
        let confirm = domain::pr_stack::StackMergeConfirm {
            title: domain::pr_stack::MERGE_STACK_LABEL.to_string(),
            landing: vec!["EXP-1".into(), "EXP-2".into(), "EXP-3".into()],
            stays_open: Vec::new(),
            body: String::new(),
            pull_request_id: "t".into(),
        };
        let op = stack_confirm_op("m", &confirm);
        assert_eq!(op.key(), "m");
        assert_eq!(op.describe(), "pullRequests.merge(t, stack, from m)");
        let spec = stack_confirm_alert("m", &confirm);
        assert!(!spec.is_ok_disabled());
        assert_eq!(merge_failure_thing(&op), "the stack");
        assert_eq!(merge_failure_thing(&MergeOp::merge("m")), "the PR");
    }

    /// EXP-1378: `Merge anyway` re-fires the SAME merge (key, stack) with
    /// `force`; a close has no force.
    #[test]
    fn merge_anyway_is_the_same_merge_forced() {
        let plain = MergeOp::merge("pr-1").forced();
        assert_eq!(plain.key(), "pr-1");
        assert_eq!(plain.describe(), "pullRequests.merge(pr-1, force)");
        let confirm = domain::pr_stack::StackMergeConfirm {
            title: domain::pr_stack::MERGE_STACK_LABEL.to_string(),
            landing: Vec::new(),
            stays_open: Vec::new(),
            body: String::new(),
            pull_request_id: "t".into(),
        };
        let stack = stack_confirm_op("m", &confirm).forced();
        assert_eq!(stack.key(), "m");
        assert_eq!(stack.describe(), "pullRequests.merge(t, stack, from m, force)");
        let close = MergeOp::Close {
            pull_request_id: "pr-1".to_string(),
        };
        assert_eq!(close.forced(), close);
    }

    #[test]
    fn user_message_prefers_the_servers_message() {
        let err = api::ApiError::Http {
            status: 409,
            message: "Pull request is not mergeable".to_string(),
        };
        assert_eq!(user_message(err), "Pull request is not mergeable");
        // EXP-533: an offline merge attempt reads as offline, never as
        // reqwest's internals.
        let offline = api::ApiError::Transport {
            message: "error sending request for url (https://app.exponential.at)".to_string(),
            offline: true,
        };
        assert_eq!(user_message(offline), api::OFFLINE_MESSAGE);
        assert!(!user_message(api::ApiError::Unauthorized).is_empty());
    }
}
