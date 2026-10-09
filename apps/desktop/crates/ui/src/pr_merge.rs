//! Shared two-click PR merge/close machinery (EXP-325).
//!
//! Four surfaces offer the same "Merge PR" affordance — the Reviews page's
//! rows, the PR diff header, the issue-detail properties sidebar, and the
//! terminal dock's
//! session tabs — and they must behave identically: first click arms
//! ("Confirm merge", ~5s auto-disarm), second click fires the server-side
//! GitHub-App squash merge, failures caption the row, success holds the
//! spinner until the Electric echo flips `pr_state`. The state lives in ONE
//! app-global entity (the [`crate::coding_flow::LocalSessions`] pattern), so
//! a merge started from ANY surface renders its phase and failure on every
//! other surface — a conflict hit from a session's Changes bar shows up in
//! the Reviews list exactly as if Merge had been clicked there.
//!
//! Keys share one namespace: an issue UUID for `issues.mergePr`,
//! [`close_pr_key`] (`close:<uuid>`) for `issues.closePr`,
//! [`session_merge_key`] (`session:<uuid>`) for a RUN's own chore PR
//! (`codingSessions.mergePr`, EXP-734) — the prefixes can never collide.
//! Error captions always key on the ROW (the issue id / session key), so a failed close renders under the same row
//! as a failed merge — the caption carries a [`FailedOp`] so the surfaces can
//! still tell the two apart (only a failed merge may open the fix-conflicts
//! composer).
//!
//! EXP-1233: a merge refused by a REAL conflict is not captioned at all on
//! the surfaces that wire [`two_click`]'s `on_failure`: the hook opens the
//! composer on the Fix merge conflicts builtin at once and answers `true`,
//! which drops the failure (the composer's card says why it is up). Every
//! other refusal captions its row until the `pr_state` echo, the next attempt
//! or an explicit [`MergeState::clear_error`] from a surface that refetched.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, AppContext as _, Entity, SharedString, Subscription};
use sync::Store;

use crate::queries;

/// Arm/in-flight key for an issue row's close-without-merge action (EXP-100).
/// The `close:` prefix can never collide with an issue UUID.
pub fn close_pr_key(issue_id: &str) -> String {
    format!("close:{issue_id}")
}

/// EXP-734: arm/in-flight key for a RUN's own chore PR (`codingSessions
/// .mergePr`). The `session:` prefix can never collide with an issue UUID,
/// or a `close:` key.
pub fn session_merge_key(session_id: &str) -> String {
    format!("session:{session_id}")
}

/// The server's user-facing failure message when there is one; everything
/// else gets [`api::ApiError::user_message`]'s plain sentence (EXP-533 — an
/// offline machine says so instead of leaking reqwest's
/// `error sending request for url …`).
pub fn user_message(err: api::ApiError) -> String {
    err.user_message()
}

/// Which op produced a failure caption. The fix-conflicts recovery run
/// rebases, force-pushes and then MERGES the pull request, so it may only be
/// offered after a failed MERGE — a user who asked to CLOSE a PR must never
/// be handed a button that merges it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedOp {
    Merge,
    Close,
}

/// One confirmable merge-shaped server call.
#[derive(Clone)]
pub enum MergeOp {
    /// `issues.mergePr` — squash-merge the issue's linked PR (a batch PR
    /// completes every linked issue). Echo-settled: the spinner holds until
    /// `pr_state` leaves `open`. Merge always closes (EXP-498): the server
    /// ends the issues' live coding sessions on every merge.
    ///
    /// EXP-1248: `stack_through` = the stack confirm's answer: `Some(id)`
    /// posts `issues.mergePr({issueId: id, mergeStack: true})` — ONE GitHub
    /// merge-async through `id` (it lands everything beneath it). `issue_id` stays the CLICKED row
    /// (key, spinner, failure caption): it always sits at or below `id`, so
    /// its own echo settles the spinner.
    MergeIssuePr {
        issue_id: String,
        stack_through: Option<String>,
    },
    /// `issues.closePr` — close the linked PR WITHOUT merging (EXP-100).
    /// Echo-settled like the merge.
    CloseIssuePr { issue_id: String },
    /// EXP-734: `codingSessions.mergePr` — the pull request an ACTION or CHAT
    /// run opened for itself (it links no issue, so no issue row can carry
    /// it). Echo-settled on the SESSION row: the spinner holds until the
    /// synced `pr_state` leaves `open`.
    MergeSessionPr { session_id: String },
}

impl MergeOp {
    /// The arm/in-flight key.
    fn key(&self) -> String {
        match self {
            MergeOp::MergeIssuePr { issue_id, .. } => issue_id.clone(),
            MergeOp::CloseIssuePr { issue_id } => close_pr_key(issue_id),
            MergeOp::MergeSessionPr { session_id } => session_merge_key(session_id),
        }
    }

    /// The ROW key failure captions render under (issue rows caption on the
    /// issue id whichever of merge/close failed).
    fn row_key(&self) -> String {
        match self {
            MergeOp::MergeIssuePr { issue_id, .. } | MergeOp::CloseIssuePr { issue_id } => {
                issue_id.clone()
            }
            MergeOp::MergeSessionPr { .. } => self.key(),
        }
    }

    /// Which action a failure of this op reports — the recovery affordance
    /// gates on it (a failed close must not offer a run that merges).
    fn failed_op(&self) -> FailedOp {
        match self {
            MergeOp::CloseIssuePr { .. } => FailedOp::Close,
            MergeOp::MergeIssuePr { .. } | MergeOp::MergeSessionPr { .. } => FailedOp::Merge,
        }
    }

    /// Keys whose in-flight call blocks this op — merge and close of the
    /// same issue row guard each other.
    fn guard_keys(&self) -> Vec<String> {
        match self {
            MergeOp::MergeIssuePr { issue_id, .. } | MergeOp::CloseIssuePr { issue_id } => {
                vec![issue_id.clone(), close_pr_key(issue_id)]
            }
            MergeOp::MergeSessionPr { .. } => vec![self.key()],
        }
    }

    fn describe(&self) -> String {
        match self {
            MergeOp::MergeIssuePr {
                issue_id,
                stack_through,
            } => match stack_through {
                Some(through) => format!("issues.mergePr({through}, stack, from {issue_id})"),
                None => format!("issues.mergePr({issue_id})"),
            },
            MergeOp::CloseIssuePr { issue_id } => format!("issues.closePr({issue_id})"),
            MergeOp::MergeSessionPr { session_id } => {
                format!("codingSessions.mergePr({session_id})")
            }
        }
    }

    /// `Ok(true)` = the pull request landed (or was already in); `Ok(false)`
    /// = GitHub's merge queue only took it (EXP-1165: `merged: false,
    /// queued: true`), so no echo is coming yet and the row stays.
    fn run(&self, trpc: &api::TrpcClient) -> Result<bool, api::ApiError> {
        match self {
            MergeOp::MergeIssuePr {
                issue_id,
                stack_through,
            } => match stack_through {
                Some(through) => api::issues::merge_pr(trpc, through, true).map(|r| r.landed()),
                None => api::issues::merge_pr(trpc, issue_id, false).map(|r| r.landed()),
            },
            MergeOp::CloseIssuePr { issue_id } => {
                api::issues::close_pr(trpc, issue_id).map(|_| true)
            }
            MergeOp::MergeSessionPr { session_id } => {
                api::coding_sessions::merge_pr(trpc, session_id).map(|r| r.landed())
            }
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
    /// issues-collection observer sees `pr_state` leave `open`.
    merging: HashSet<String>,
    /// The last failure — one caption at a time, cleared on the next
    /// confirmed attempt anywhere (the pre-EXP-325 reviews-rail semantic).
    error: Option<MergeFailure>,
    _subscriptions: Vec<Subscription>,
}

/// One merge/close failure, everything a surface needs to caption the row and
/// decide whether to offer the recovery run.
#[derive(Clone, Debug)]
pub struct MergeFailure {
    /// The ROW the caption renders under (an issue id or a session key).
    pub row_key: String,
    pub message: SharedString,
    /// Whether MERGE or CLOSE produced it — the recovery run ends in a merge,
    /// so only a merge failure may offer it.
    pub op: FailedOp,
    /// EXP-533: the server said this is a REAL content conflict (tRPC
    /// `CONFLICT` / HTTP 409). A failed merge with no internet, a stale base
    /// or an unconfigured GitHub App is NOT one, and "Fix merge conflicts"
    /// would send an agent to rebase a branch over a problem it cannot fix.
    pub conflict: bool,
}

/// [`two_click`]'s failure hook: runs once, right after the failure is
/// recorded, outside the state's own update (so it may open windows).
/// Answering `true` = the hook HANDLED the refusal (EXP-1233: it opened the
/// fix-conflicts composer), and the failure is dropped instead of captioned.
pub type OnMergeFailure = Box<dyn FnOnce(&MergeFailure, &mut App) -> bool>;

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
            let collections = Store::try_global(cx).map(|store| store.collections().clone());
            if let Some(collections) = collections {
                subscriptions.push(cx.observe(
                    &collections.issues,
                    |this: &mut MergeState, _, cx| {
                        this.prune_settled(cx);
                    },
                ));
                // EXP-734: a run's OWN chore PR settles on the SESSION row's
                // `pr_state` — no issue ever carries it, so without this
                // observer an action/chat merge would spin forever.
                subscriptions.push(cx.observe(
                    &collections.coding_sessions,
                    |this: &mut MergeState, _, cx| {
                        this.prune_settled(cx);
                    },
                ));
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

    /// This ROW key's failure, if the last one was its (an issue id or a
    /// session key).
    pub fn failure(&self, row_key: &str) -> Option<&MergeFailure> {
        self.error.as_ref().filter(|f| f.row_key == row_key)
    }

    /// The failure caption for a ROW key (an issue id or a session key).
    pub fn error(&self, row_key: &str) -> Option<SharedString> {
        self.failure(row_key).map(|f| f.message.clone())
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

    /// Drop the standing failure caption. A surface calls this when it
    /// REFETCHES the pull request's data (entering the Reviews screen,
    /// re-pointing the PR diff): the refusal was about the previous snapshot.
    pub fn clear_error(cx: &mut App) {
        let state = MergeState::global(cx);
        state.update(cx, |this, cx| {
            if this.error.take().is_some() {
                cx.notify();
            }
        });
    }

    /// Collect keys whose issue's PR is no longer open (the Electric echo
    /// after a merge/close — also covers a PR closed from GitHub itself).
    fn prune_settled(&mut self, cx: &mut gpui::Context<Self>) {
        let changed = {
            let Some(store) = Store::try_global(cx) else {
                return;
            };
            let issues = store.collections().issues.read(cx);
            let sessions = store.collections().coding_sessions.read(cx);
            let settled = |key: &str| -> bool {
                // EXP-734: a run's OWN chore PR lives on the SESSION row —
                // no issue carries it, so it settles on that row's `pr_state`.
                if let Some(session_id) = key.strip_prefix("session:") {
                    return match sessions.get(session_id) {
                        Some(session) => session.pr_state.as_deref() != Some("open"),
                        None => false,
                    };
                }
                let id = key.strip_prefix("close:").unwrap_or(key);
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
            if self.error.as_ref().is_some_and(|f| settled(&f.row_key)) {
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
    two_click(op, None, cx)
}

/// EXP-1248: the ONE stack confirm for merging `issue_id`'s pull request in
/// `mode` (`Stack` = the merge control, `Through` = a stack row's ghost
/// action). Reads the issue and its team's open-PR rows off the synced
/// collections; `None` (not in a linear open stack, or nothing synced)
/// merges plainly.
pub(crate) fn stack_merge_confirm_for(
    issue_id: &str,
    mode: domain::pr_stack::StackConfirmMode,
    cx: &App,
) -> Option<domain::pr_stack::StackMergeConfirm> {
    let store = Store::try_global(cx)?;
    let issue = store.collections().issues.read(cx).get(issue_id).cloned()?;
    let team_id = queries::issue_team_id(cx, issue_id)?;
    let issues = queries::review_issues(cx, &team_id);
    domain::pr_stack::stack_merge_confirm(&issue, &issues, mode)
}

/// EXP-1248: the ONE merge control's label — `Merge stack` on a member of a
/// linear open stack (contract `diffUi.mergeStack`), else `Merge PR`.
pub(crate) fn merge_label_for(issue_id: &str, cx: &App) -> &'static str {
    let stacked = Store::try_global(cx).and_then(|store| {
        let issue = store.collections().issues.read(cx).get(issue_id).cloned()?;
        let team_id = queries::issue_team_id(cx, issue_id)?;
        let issues = queries::review_issues(cx, &team_id);
        Some(domain::pr_stack::open_pr_shape(&issue, &issues) == domain::pr_stack::PrGraphShape::Stack)
    });
    if stacked == Some(true) {
        domain::pr_stack::MERGE_STACK_LABEL
    } else {
        domain::contract::DIFF_UI_MERGE_PR
    }
}

/// The confirm window's size: the one-line title band, a body of one or two
/// lines and the two-button footer.
const STACK_CONFIRM_WIDTH: f32 = 420.;
const STACK_CONFIRM_HEIGHT: f32 = 200.;

/// The op the confirm's primary fires: `issues.mergePr({issueId:
/// confirm.issue_id, mergeStack: true})`, keyed on the CLICKED issue (its
/// spinner and failure caption; it always lands, so its echo settles).
pub(crate) fn stack_confirm_op(
    clicked: &str,
    confirm: &domain::pr_stack::StackMergeConfirm,
) -> MergeOp {
    MergeOp::MergeIssuePr {
        issue_id: clicked.to_string(),
        stack_through: Some(confirm.issue_id.clone()),
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
    issue_id: &str,
    mode: domain::pr_stack::StackConfirmMode,
    window: &mut gpui::Window,
    cx: &mut App,
) -> bool {
    // An in-flight merge or close of this row: the caller's guard ignores it.
    let state = MergeState::global(cx);
    if [issue_id.to_string(), close_pr_key(issue_id)]
        .iter()
        .any(|key| state.read(cx).merging.contains(key))
    {
        return false;
    }
    let Some(confirm) = stack_merge_confirm_for(issue_id, mode, cx) else {
        return false;
    };
    MergeState::disarm(cx);
    let spec = stack_confirm_alert(issue_id, &confirm);
    crate::native_dialog::open_alert(window, cx, spec);
    true
}

/// The merge control's path ([`ask_stack_merge_mode`] in `Stack` mode).
pub(crate) fn ask_stack_merge(issue_id: &str, window: &mut gpui::Window, cx: &mut App) -> bool {
    ask_stack_merge_mode(issue_id, domain::pr_stack::StackConfirmMode::Stack, window, cx)
}

/// The shared two-click flow: first call arms (auto-disarm ~5s), second call
/// fires the op on the background executor. Failures land in the shared
/// error slot and then run `on_failure` ([`OnMergeFailure`]: answering
/// `true` drops the failure again — EXP-1233's conflict → composer);
/// successes hold the in-flight spinner until the Electric echo.
pub fn two_click(
    op: MergeOp,
    on_failure: Option<OnMergeFailure>,
    cx: &mut App,
) -> TwoClick {
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
            let failed = this.update(cx, |this, cx| {
                match result {
                    Ok(false) => {
                        // Queued on GitHub: nothing landed, no echo will
                        // settle it now; release the spinner, keep the row.
                        this.merging.remove(&key);
                        cx.notify();
                        None
                    }
                    // The issues-collection observer clears the key when
                    // the echo flips `pr_state`.
                    Ok(true) => None,
                    Err(err) => {
                        log::warn!("[ui] {} failed: {err}", call_op.describe());
                        this.merging.remove(&key);
                        // EXP-533: classify BEFORE the message is consumed —
                        // only a real 409 conflict may offer the recovery run.
                        let conflict = err.is_conflict();
                        let failure = MergeFailure {
                            row_key: call_op.row_key(),
                            message: SharedString::from(user_message(err)),
                            op: call_op.failed_op(),
                            conflict,
                        };
                        this.error = Some(failure.clone());
                        cx.notify();
                        Some(failure)
                    }
                }
            });
            // The hook runs OUTSIDE the state's update: it may open a window
            // (the fix-conflicts composer) or read this state itself.
            if let (Ok(Some(failure)), Some(on_failure)) = (failed, on_failure) {
                let _ = cx.update(|cx| {
                    if on_failure(&failure, cx) {
                        MergeState::global(cx).update(cx, |this, cx| {
                            if this.error.as_ref().is_some_and(|f| f.row_key == failure.row_key) {
                                this.error = None;
                                cx.notify();
                            }
                        });
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

    /// One namespace, collision-free key shapes; merge/close of the
    /// same issue guard each other and caption the same ROW.
    #[test]
    fn keys_share_one_collision_free_namespace() {
        assert_eq!(close_pr_key("issue-1"), "close:issue-1");
        let merge = MergeOp::MergeIssuePr {
            issue_id: "i1".to_string(),
            stack_through: None,
        };
        let close = MergeOp::CloseIssuePr {
            issue_id: "i1".to_string(),
        };
        // EXP-734: a run's own chore PR, keyed by SESSION id.
        let session = MergeOp::MergeSessionPr {
            session_id: "s1".to_string(),
        };
        assert_eq!(merge.key(), "i1");
        assert_eq!(close.key(), "close:i1");
        assert_eq!(session.key(), "session:s1");
        assert_eq!(merge.row_key(), "i1");
        assert_eq!(close.row_key(), "i1");
        // A session row captions under its own key — no issue exists to
        // caption on, and it must never land on an issue whose id it shares.
        assert_eq!(session.row_key(), "session:s1");
        assert_eq!(merge.guard_keys(), close.guard_keys());
        assert_eq!(session.guard_keys(), vec!["session:s1".to_string()]);
        // Every key shape stays distinct: none is a prefix-free collision of
        // another, and a bare uuid is never confused for a prefixed one.
        let keys = [merge.key(), close.key(), session.key()];
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len());
        assert_eq!(session_merge_key("s1"), "session:s1");
        assert_eq!(session.describe(), "codingSessions.mergePr(s1)");
        // …but the caption still says WHICH op failed: the fix run ends
        // in a merge, so a failed close must never be offered it.
        assert_eq!(merge.failed_op(), FailedOp::Merge);
        assert_eq!(close.failed_op(), FailedOp::Close);
        assert_eq!(session.failed_op(), FailedOp::Merge);
    }

    /// EXP-1248: the ONE confirm's primary posts `mergePr({issueId:
    /// confirm.issue_id, mergeStack: true})`, keyed on the clicked row.
    #[test]
    fn the_stack_confirm_maps_to_one_merge_pr_call() {
        let confirm = domain::pr_stack::StackMergeConfirm {
            title: domain::pr_stack::MERGE_STACK_LABEL.to_string(),
            landing: vec!["EXP-1".into(), "EXP-2".into(), "EXP-3".into()],
            stays_open: Vec::new(),
            body: String::new(),
            issue_id: "t".into(),
        };
        let op = stack_confirm_op("m", &confirm);
        assert_eq!(op.key(), "m");
        assert_eq!(op.describe(), "issues.mergePr(t, stack, from m)");
        let spec = stack_confirm_alert("m", &confirm);
        assert!(!spec.is_ok_disabled());
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

    /// EXP-533: a failure records WHICH op failed and whether it was a real
    /// 409 — the two facts `work_header::conflict_opens_composer` gates on —
    /// and only under its own row.
    #[test]
    fn a_failure_carries_its_op_and_conflict_under_its_row() {
        let failure = |conflict, op| MergeFailure {
            row_key: "i1".to_string(),
            message: SharedString::from("nope"),
            op,
            conflict,
        };
        let state = |error| MergeState {
            arm: None,
            arm_seq: 0,
            merging: HashSet::new(),
            error: Some(error),
            _subscriptions: Vec::new(),
        };
        let conflicted = state(failure(true, FailedOp::Merge));
        let recorded = conflicted.failure("i1").expect("its row");
        assert!(recorded.conflict);
        assert_eq!(recorded.op, FailedOp::Merge);
        assert_eq!(conflicted.error("i1").as_deref(), Some("nope"));
        // Another row's failure never leaks into this one.
        assert!(conflicted.failure("i2").is_none());
        // Offline / stale-base / no-GitHub-App merge failure: caption only.
        assert!(!state(failure(false, FailedOp::Merge)).failure("i1").unwrap().conflict);
        // A failed CLOSE says so — a run that merges is never offered for it.
        let closed = state(failure(true, FailedOp::Close));
        assert_eq!(closed.failure("i1").unwrap().op, FailedOp::Close);
    }
}
