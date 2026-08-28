//! UI-side Start-coding orchestration (masterplan-v3 §7.1/§7.2/§7.7) —
//! everything between the issue-detail header's **Start coding**
//! affordance and the `coding` crate's one launch entry point.
//!
//! Three shared pieces live here:
//!
//! - [`CodingHub`] — the app-global coding state: the §7.7 settings
//!   (per-agent path/model/effort + repos root / branch prefix,
//!   file-persisted, per-install) and the tooling-doctor report. The settings
//!   pane edits through it; the Start-coding button and the launcher read
//!   from it. The doctor runs on the background executor (it probes every
//!   agent CLI — claude/codex/pi — plus `git`, EXP-201) and re-runs whenever
//!   the settings change.
//! - [`LocalSessions`] — the sessions THIS process launched (issue →
//!   `{session_id, tab, manager}`). Drives the §7.5 play↔stop flip: while an
//!   issue has a local session, the header shows a "Coding…" indicator and a
//!   **stop** affordance (kill the child → the exit hook fires the idempotent
//!   `codingSessions.end`). A manually closed tab is also caught
//!   (`TabClosed`) and ends the row best-effort — the "coding now" badge must
//!   never ghost. EXP-105 extends that rule to the two teardown paths that
//!   fire neither the exit hook nor `TabClosed`: window close (a manager
//!   release observer per session) and app quit ([`install_quit_hook`]).
//! - [`StartCodingControl`] — the header affordance itself. Enabled iff
//!   `repositories.forIssue` resolves non-null AND the doctor is green
//!   (EXP-201: `git` plus ANY usable agent enables the affordance; a launch
//!   additionally gates on the SELECTED agent via `first_failure_for`);
//!   disabled states carry the EXACT §7 reasons (never a false "not
//!   connected", never an unexplained block). Click → the shared
//!   Start-coding dialog
//!   (`crate::start_coding_dialog`), which owns the model/effort/plan-mode
//!   choices and the prepare→spawn task.
//!
//! The relay-origin `start_session` path (§08) is the SAME sequence — its
//! control channel builds the same [`build_launch`] input and calls the same
//! `coding::prepare`/`spawn_prepared_with`; only the `LaunchOrigin`
//! differs (§7.1: "there is no second, divergent remote-start
//! implementation").

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Disableable as _, Icon,
};
use gpui_component::dock::DockItem;
use sync::Store;
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent};

use coding::{
    run_doctor,
    run_registry::{RunKind, RunRecord},
    CodingDeps, DoctorReport, IssueSeed, LaunchOptions, LaunchOrigin, LaunchOutcome, LaunchRequest,
    Settings,
};

use crate::controls::WebControl as _;
use crate::queries;
use crate::session::AuthContext;
use crate::terminal_dock::TerminalDockPanel;
use crate::shell::Shell;
use crate::icons::registry;

// ---------------------------------------------------------------------------
// CodingHub — settings + doctor (§7.7)
// ---------------------------------------------------------------------------

/// Doctor lifecycle: `NotRun` → `Running` → `Ready(report)`; `Running` keeps
/// the previous report visible (a re-check must not flash the pane empty).
#[derive(Default)]
pub struct DoctorState {
    pub report: Option<DoctorReport>,
    pub running: bool,
    generation: u64,
}

/// App-global coding state (one per process — the settings file and the local
/// toolchain are per-install, not per-window).
pub struct CodingHub {
    pub settings: Settings,
    settings_path: PathBuf,
    pub doctor: DoctorState,
}

struct CodingHubGlobal(Entity<CodingHub>);

impl gpui::Global for CodingHubGlobal {}

impl CodingHub {
    /// The hub, created lazily on first access. Creation loads the persisted
    /// settings and kicks the FIRST doctor run — the §7.7 onboarding rule
    /// ("run the doctor automatically with clear errors BEFORE Start coding
    /// is usable") holds no matter which surface touches coding first.
    pub fn global(cx: &mut App) -> Entity<CodingHub> {
        if let Some(global) = cx.try_global::<CodingHubGlobal>() {
            return global.0.clone();
        }
        let data_dir = cx
            .try_global::<AuthContext>()
            .map(|auth| auth.data_dir.clone())
            .unwrap_or_else(api::default_data_dir);
        let settings_path = Settings::default_path(&data_dir);
        let hub = cx.new(|_| CodingHub {
            settings: Settings::load(&settings_path),
            settings_path,
            doctor: DoctorState::default(),
        });
        cx.set_global(CodingHubGlobal(hub.clone()));
        Self::refresh_doctor(&hub, cx);
        hub
    }

    /// Re-run the tooling doctor on the background executor (it spawns two
    /// `--version` children — never on the foreground). Generation-guarded so
    /// a stale run can't clobber a newer one after a settings change.
    pub fn refresh_doctor(hub: &Entity<CodingHub>, cx: &mut App) {
        let (settings, generation) = hub.update(cx, |this, cx| {
            this.doctor.running = true;
            this.doctor.generation += 1;
            cx.notify();
            (this.settings.clone(), this.doctor.generation)
        });
        let hub = hub.clone();
        cx.spawn(async move |cx| {
            let report = cx
                .background_executor()
                .spawn(async move { run_doctor(&settings) })
                .await;
            let landed = hub.update(cx, |this, cx| {
                if this.doctor.generation != generation {
                    return false; // superseded
                }
                this.doctor.running = false;
                this.doctor.report = Some(report);
                cx.notify();
                true
            });
            if landed {
                // EXP-485: a changed advertisement (agents, sign-in state,
                // per-agent defaults) re-posts the devices ROW — the online
                // frame carries none of it any more, so only a flip of the
                // dial decision itself (EXP-367: the last agent CLI vanishing,
                // or the first one appearing) touches the socket.
                let _ = cx.update(|cx| {
                    crate::steer_wiring::refresh_device_advertisement(cx);
                });
            }
        })
        .detach();
    }

    /// Persist + apply new settings (the §7.7 pane's save path), then re-run
    /// the doctor against the new claude path. Returns the save error for the
    /// pane's inline notice; the in-memory settings are updated either way so
    /// the launcher and the file never silently diverge from the UI.
    pub fn save_settings(
        hub: &Entity<CodingHub>,
        settings: Settings,
        cx: &mut App,
    ) -> Result<(), String> {
        let (result, settings_path) = hub.update(cx, |this, cx| {
            let result = settings
                .save(&this.settings_path)
                .map_err(|err| format!("Could not save settings: {err}"));
            this.settings = settings.clone();
            cx.notify();
            (result, this.settings_path.clone())
        });
        // EXP-481: launch defaults are server-authoritative — a local edit
        // pushes up (fingerprint-gated inside; offline queues the dirty
        // marker for the beat's retry). Best-effort on the background
        // executor; paths/prefs-only saves no-op.
        if result.is_ok() {
            if let Some(trpc) = crate::queries::trpc_client(cx) {
                let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
                let device_id = steer::persistent_device_id(&data_dir);
                cx.background_executor()
                    .spawn(async move {
                        crate::device_sync::push_local_defaults_if_changed(
                            trpc,
                            settings_path,
                            data_dir,
                            device_id,
                            settings,
                        );
                    })
                    .detach();
            }
        }
        Self::refresh_doctor(hub, cx);
        result
    }

    /// EXP-551: persist a purely LOCAL ui preference (the emoji recents;
    /// EXP-638 the OS-notification switch) into the same per-install file — WITHOUT the doctor re-run
    /// and the launch-defaults push [`save_settings`] does. Neither field is
    /// a launcher knob, and an emoji pick must not spawn `--version` probes.
    ///
    /// [`save_settings`]: Self::save_settings
    pub fn save_ui_prefs(hub: &Entity<CodingHub>, settings: Settings, cx: &mut App) {
        let result = hub.update(cx, |this, cx| {
            let result = settings.save(&this.settings_path);
            this.settings = settings;
            cx.notify();
            result
        });
        if let Err(err) = result {
            log::warn!("[ui] persisting the ui prefs failed: {err}");
        }
    }

    /// The §7.1-step-1 gate half the button ANDs in: git green + at least
    /// one usable agent CLI (EXP-201 — the dialog gates the SELECTED agent).
    pub fn doctor_ok(&self) -> bool {
        self.doctor
            .report
            .as_ref()
            .is_some_and(|report| report.git.ok && report.any_agent_ok())
    }
}

// ---------------------------------------------------------------------------
// LocalSessions — the sessions THIS process launched (§7.5 play↔stop)
// ---------------------------------------------------------------------------

/// What a local coding session works on: one issue (§7.1), a multi-issue
/// batch (one session per batch run, keyed by its batch id), or an action
/// run (EXP-253 — keyed by its `coding_sessions` ROW id, not the action id:
/// concurrent runs of the same action are allowed, so the action id is not
/// unique per session and would let one run's exit tear down another's
/// bookkeeping).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionSubject {
    Issue(String),
    Batch(String),
    Action(String),
}

/// One locally running coding session (a `coding_sessions` row whose child
/// lives in one of OUR terminal docks).
pub struct LocalCodingSession {
    pub session_id: String,
    pub subject: SessionSubject,
    /// The shared clone the session's worktree hangs off — releases its P9
    /// token-refresher hold when the session ends.
    pub clone: PathBuf,
    /// The branch the session's worktree is on (`exp/<IDENTIFIER>` /
    /// `exp/batch-<id8>`) — the EXP-102 sweep/delete guard key.
    pub branch: String,
    pub tab: TabId,
    pub manager: WeakEntity<TerminalManager>,
    /// The team action this run executes (`None` for issue/batch sessions).
    pub action_id: Option<String>,
    /// EXP-637: `schedule`/`event` when an AUTOMATION started this run —
    /// nobody is watching its tab, so an agent-declared end closes it
    /// instead of leaving an ended strip. `None` = a person started it.
    pub started_reason: Option<String>,
    /// EXP-637: the run's own worktree, auto-removed at exit when it is
    /// clean and carries no commits. `None` for issue/batch sessions and for
    /// runs with no worktree of their own.
    pub run_cleanup: Option<coding::RunCleanup>,
}

/// Subject-keyed registry of local sessions. An entity (not a bare global) so
/// the header affordances can `cx.observe` it for the play↔stop flip.
#[derive(Default)]
pub struct LocalSessions {
    by_issue: HashMap<String, LocalCodingSession>,
    /// Multi-issue batch sessions, keyed by batch id.
    by_batch: HashMap<String, LocalCodingSession>,
    /// Action runs (EXP-253), keyed by SESSION row id (concurrent runs of
    /// one action are allowed — the action id is not unique per session).
    by_action: HashMap<String, LocalCodingSession>,
    /// Keeps the per-session watchers (`TabClosed` + manager release) alive
    /// (keyed by session id; dropped with the entry).
    watchers: HashMap<String, Vec<Subscription>>,
}

struct LocalSessionsGlobal(Entity<LocalSessions>);

impl gpui::Global for LocalSessionsGlobal {}

impl LocalSessions {
    pub fn global(cx: &mut App) -> Entity<LocalSessions> {
        if let Some(global) = cx.try_global::<LocalSessionsGlobal>() {
            return global.0.clone();
        }
        let sessions = cx.new(|_| LocalSessions::default());
        cx.set_global(LocalSessionsGlobal(sessions.clone()));
        sessions
    }

    /// Immutable accessor — `None` before the first coding session materializes
    /// the global. Used by the §8.5 banner (render has no `&mut App`).
    pub fn global_ref(cx: &App) -> Option<Entity<LocalSessions>> {
        cx.try_global::<LocalSessionsGlobal>().map(|g| g.0.clone())
    }

    pub fn get(&self, issue_id: &str) -> Option<&LocalCodingSession> {
        self.by_issue.get(issue_id)
    }

    /// EVERY live local session (issue, batch, or action) whose worktree is
    /// on `branch`. Trunk/scratch action runs carry an empty branch and never
    /// match. The fix-conflicts launch uses this to END the stale sessions
    /// still holding the PR branch (a session stays alive through in_review)
    /// before the run rebases that worktree.
    ///
    /// All of them, never "the first found": one branch is routinely held by
    /// TWO sessions at once — a live fix-conflicts run sits in `by_action`
    /// while the issue's own session (whose play/resume affordance keys off
    /// `by_issue`) can be resumed onto the very same branch/worktree. A
    /// single arbitrary match made the "Fixing…" park flicker off and let the
    /// launcher close one holder while a second agent kept rebasing and
    /// force-pushing the same tree.
    pub fn sessions_on_branch<'a>(
        &'a self,
        branch: &'a str,
    ) -> impl Iterator<Item = &'a LocalCodingSession> + 'a {
        self.all().filter(move |session| holds_branch(&session.branch, branch))
    }

    /// Every live local session, whatever its subject.
    fn all(&self) -> impl Iterator<Item = &LocalCodingSession> {
        self.by_issue
            .values()
            .chain(self.by_batch.values())
            .chain(self.by_action.values())
    }

    /// Every branch a live local session holds (issue, batch AND action
    /// runs) — the prune keep-set feed (EXP-465): synced rows can't name
    /// batch/action branches, only this process knows them. Trunk/scratch
    /// runs carry an empty branch and are skipped.
    pub fn held_branches(&self) -> impl Iterator<Item = &str> {
        self.all()
            .map(|session| session.branch.as_str())
            .filter(|branch| !branch.is_empty())
    }

    /// ONE live local session holding `branch`, arbitrarily chosen when
    /// several do. Only for callers that ask "is this branch held at all"
    /// (the EXP-102 clone sweep/delete guard) — anything that ACTS on the
    /// holder (ending it, or classifying it) must use
    /// [`Self::sessions_on_branch`] and handle all of them.
    pub fn session_on_branch(&self, branch: &str) -> Option<&LocalCodingSession> {
        self.all().find(|session| holds_branch(&session.branch, branch))
    }

    /// Whether a live fix-conflicts run (EXP-259) is already working
    /// `branch` — the ONLY case the "Fix conflicts" buttons park as
    /// "Fixing…". Any other session holding the branch (its own coding
    /// session, still alive after a plain Merge failed) is ended by the
    /// fix-run launch itself, so those buttons stay clickable. ANY holder
    /// being a fix run parks the button: a co-held branch must not unpark it
    /// just because the issue session happened to be iterated first.
    pub fn is_branch_fixing(&self, branch: &str) -> bool {
        self.sessions_on_branch(branch)
            .any(|session| is_fix_conflicts_run(session.action_id.as_deref()))
    }

    /// The coding session id whose terminal tab is `tab`, if this process is
    /// coding it (reverse of the subject-keyed maps — the §8.5 banner resolves
    /// a dock tab back to its steer session).
    pub fn session_id_for_tab(&self, tab: TabId) -> Option<&str> {
        self.all()
            .find(|session| session.tab == tab)
            .map(|session| session.session_id.as_str())
    }

    /// The subject whose terminal tab is `tab` — the terminal dock resolves
    /// an issue-session tab back to its issue for the EXP-325 issue-styled
    /// chip (status glyph + identifier + synced title).
    pub fn subject_for_tab(&self, tab: TabId) -> Option<&SessionSubject> {
        self.session_for_tab(tab).map(|session| &session.subject)
    }

    /// The live local session whose terminal tab is `tab` — the terminal
    /// dock's merge affordance needs both the subject AND the branch
    /// (EXP-498: a batch tab's merge target resolves by branch match).
    pub fn session_for_tab(&self, tab: TabId) -> Option<&LocalCodingSession> {
        self.all().find(|session| session.tab == tab)
    }

    /// Every live local session's row id (issue + batch) — the EXP-105
    /// quit-time sweep input, the EXP-229 reconcile skip-set, and the
    /// sign-out sweep input.
    pub(crate) fn session_ids(&self) -> Vec<String> {
        self.all()
            .map(|session| session.session_id.clone())
            .collect()
    }

    /// The ACTIONS with a live local run (EXP-530) — the automation host's
    /// defer set: a trigger never launches a second run of an action this
    /// process is already running. Keyed by action id (not session id), and
    /// deliberately over ALL subjects: an issue/batch session carries no
    /// action id, so only real action runs land in the set.
    pub(crate) fn live_action_ids(&self) -> HashSet<String> {
        self.all()
            .filter_map(|session| session.action_id.clone())
            .collect()
    }

    /// Drop the session for `subject` and release its P9 token-refresher
    /// hold. Both exit paths (child-exit notify + `TabClosed` watcher) land
    /// here; the second one finds the entry already gone, so the refresher is
    /// released exactly once.
    fn remove(sessions: &Entity<LocalSessions>, subject: &SessionSubject, cx: &mut App) {
        let removed = sessions.update(cx, |this, cx| {
            let entry = match subject {
                SessionSubject::Issue(id) => this.by_issue.remove(id),
                SessionSubject::Batch(id) => this.by_batch.remove(id),
                SessionSubject::Action(id) => this.by_action.remove(id),
            };
            if let Some(entry) = &entry {
                this.watchers.remove(&entry.session_id);
            }
            cx.notify();
            entry
        });
        // EXP-481: a session ending changes the inventory's busy flags.
        crate::device_sync::report_soon(cx);
        if let Some(entry) = removed {
            // EXP-637: reclaim the run's own worktree — but ONLY when it is
            // provably clean and carries no commits ([`coding::run_cleanup`]).
            // Background, because it shells out to git; a worktree left
            // dirty stays, and shows as dirty in Worktrees.
            if let Some(cleanup) = entry.run_cleanup.clone() {
                let session_id = entry.session_id.clone();
                let data_dir = crate::window_size::app_data_dir();
                cx.spawn(async move |cx| {
                    let verdict = cx
                        .background_executor()
                        .spawn({
                            let cleanup = cleanup.clone();
                            async move { coding::remove_if_clean(&cleanup) }
                        })
                        .await;
                    let _ = cx.update(|cx| {
                        if matches!(verdict, coding::CleanupOutcome::Removed) {
                            // The workspace is gone: nothing left to resume,
                            // so the record goes too.
                            if let Some(dir) = &data_dir {
                                coding::run_registry::remove(dir, &session_id);
                            }
                        }
                        log::info!(
                            "run cleanup [{session_id}] on {}: {verdict:?}",
                            cleanup.branch
                        );
                        // EXP-481: the worktree inventory changed.
                        crate::device_sync::report_soon(cx);
                    });
                })
                .detach();
            }
            TokenRefreshers::release(&entry.clone, cx);
            // EXP-640: the crash-recovery registry entry is deliberately NOT
            // dropped here — the session's end path is running (or already
            // ran), and only its RESOLVED outcome removes the entry (the
            // coding crate's session-end observer, `session_registry`). An
            // end this build can no longer land (the server 426-gated it
            // mid-deploy, a dead network) thus stays recorded for the next
            // launch's reconcile instead of ghosting "coding now" for the
            // server sweep's 2h window.
        }
    }

    /// Track a freshly spawned session. Also watches the manager for a
    /// manual `TabClosed` on our tab: closing a running Claude tab kills the
    /// child without its exit hook ever firing (the tab's subscription dies
    /// with it), so the watcher ends the row best-effort here — the synced
    /// "coding now" badge must never ghost (§7.1 step 8's intent). The same
    /// reasoning covers the manager entity's RELEASE (EXP-105): closing the
    /// window while the app keeps running (macOS) tears the dock down with
    /// no `TabClosed` and no exit hook — the PTY closing SIGHUPs the child,
    /// so ending the row there is badge-only, never a kill of live work.
    fn insert(
        sessions: &Entity<LocalSessions>,
        session: LocalCodingSession,
        trpc: Arc<api::TrpcClient>,
        cx: &mut App,
    ) {
        // EXP-229: persist the row id so a crash / forced logout / failed
        // quit-time end can be reconciled (ended) on the next launch —
        // in-memory tracking alone strands the row `running` for the server
        // sweep's full 2h window. Dialog, relay remote-start, and batch
        // launches all funnel through here (`spawn_into_window`).
        if let (Some(auth), Some(account)) = (
            cx.try_global::<AuthContext>().cloned(),
            crate::queries::active_account(cx),
        ) {
            crate::session_registry::record(&auth.data_dir, &session.session_id, &account.id);
        }
        let subject = session.subject.clone();
        let session_key = session.session_id.clone();
        let mut watchers: Vec<Subscription> = Vec::new();
        if let Some(manager) = session.manager.upgrade() {
            {
                let sessions = sessions.downgrade();
                let watch_subject = subject.clone();
                let watch_tab = session.tab;
                let session_id = session.session_id.clone();
                let trpc = Arc::clone(&trpc);
                watchers.push(cx.subscribe(
                    &manager,
                    move |_, event: &TerminalManagerEvent, cx| {
                        if *event != TerminalManagerEvent::TabClosed(watch_tab) {
                            return;
                        }
                        // End the row off the foreground (idempotent
                        // server-side — a normal exit already ended it
                        // before the close).
                        spawn_tracked_end(Arc::clone(&trpc), session_id.clone());
                        // EXP-283: detach the steer side (stop publisher +
                        // emitter, unwatch the kill-watch) so our own end's
                        // synced `ended` flip can't read back as a remote
                        // kill. No-op when the kill path already detached.
                        // The bye outcome is `ended`, NOT `killed`: closing a
                        // tab is a normal end, and remote viewers label the
                        // outcome verbatim (only `ended` renders detail-less).
                        crate::steer_wiring::detach_publisher(
                            &session_id,
                            Some("ended".to_string()),
                            cx,
                        );
                        if let Some(sessions) = sessions.upgrade() {
                            LocalSessions::remove(&sessions, &watch_subject, cx);
                        }
                    },
                ));
            }
            {
                let sessions = sessions.downgrade();
                let watch_subject = subject.clone();
                let session_id = session.session_id.clone();
                let trpc = Arc::clone(&trpc);
                watchers.push(cx.observe_release(&manager, move |_, cx| {
                    // A normal end already removed the entry (and with it
                    // this subscription) — reaching here means the window
                    // died around a live session. Idempotent server-side;
                    // tracked so a release-cascade-then-quit still waits.
                    spawn_tracked_end(trpc, session_id.clone());
                    // EXP-283: same detach as the exit/close edges — the end
                    // above flips the row, and the kill-watch must not read
                    // our own flip back as a remote kill. `ended`, not
                    // `killed`: a released window is a normal end for viewers.
                    crate::steer_wiring::detach_publisher(
                        &session_id,
                        Some("ended".to_string()),
                        cx,
                    );
                    if let Some(sessions) = sessions.upgrade() {
                        LocalSessions::remove(&sessions, &watch_subject, cx);
                    }
                }));
            }
        }
        // EXP-498: merge always closes, but a batch session's row can't be
        // ended server-side (issue_id NULL, no batch↔PR linkage) — so the
        // desktop closes it itself when its branch's synced issues reach a
        // MERGED PR (merged from web/mobile/GitHub; this tab's own merge
        // button already closes locally before the sync echo). Closing the
        // tab reuses the full teardown: the TabClosed watcher above ends the
        // row and removes the entry, which drops this subscription — fire-
        // once by construction. Issue sessions need none of this: their
        // server-side →ended flip lands via the kill-watch.
        //
        // EXP-637 (decision 6): NOT when the session merged its own PR. The
        // server flips such a row back to `running` instead of ending it
        // (server-only `merged_own_pr`), so a live `running` row on a merged
        // branch IS this session having merged it — and it goes on working
        // until it calls `exponential_sessions_end`. An externally merged
        // batch is parked `in_review` (its own `pr_open` put it there) or
        // already `ended`, so the close still fires for it.
        let batch_close = (matches!(subject, SessionSubject::Batch(_))
            && !session.branch.is_empty())
        .then(|| {
            (
                session.branch.clone(),
                session.tab,
                session.manager.clone(),
                session.session_id.clone(),
            )
        });
        if let Some((branch, tab, manager, session_id)) = batch_close.clone() {
            if let Some(store) = Store::try_global(cx) {
                let issues = store.collections().issues.clone();
                let sessions_collection = store.collections().coding_sessions.clone();
                watchers.push(cx.observe(&issues, move |issues, cx| {
                    if !branch_pr_merged(&branch, issues.read(cx).iter()) {
                        return;
                    }
                    if session_merged_its_own_pr(&sessions_collection, &session_id, cx) {
                        return;
                    }
                    if let Some(manager) = manager.upgrade() {
                        manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
                    }
                }));
            }
        }
        // EXP-481: a session registering changes the inventory's busy flags.
        crate::device_sync::report_soon(cx);
        sessions.update(cx, |this, cx| {
            if !watchers.is_empty() {
                this.watchers.insert(session_key, watchers);
            }
            match &subject {
                SessionSubject::Issue(id) => {
                    this.by_issue.insert(id.clone(), session);
                }
                SessionSubject::Batch(id) => {
                    this.by_batch.insert(id.clone(), session);
                }
                SessionSubject::Action(id) => {
                    this.by_action.insert(id.clone(), session);
                }
            }
            cx.notify();
        });
        // A batch registered late against an already-merged PR (resume onto
        // a merged branch) never sees another issues notify — run the same
        // check once, AFTER the entry exists so the close's teardown finds
        // and removes it.
        if let Some((branch, tab, manager, session_id)) = batch_close {
            let merged = Store::try_global(cx).is_some_and(|store| {
                branch_pr_merged(&branch, store.collections().issues.read(cx).iter())
                    && !session_merged_its_own_pr(
                        &store.collections().coding_sessions,
                        &session_id,
                        cx,
                    )
            });
            if merged {
                if let Some(manager) = manager.upgrade() {
                    manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
                }
            }
        }
    }
}

/// EXP-637 (decision 6): did THIS session merge the PR that just landed?
/// A merge normally ends every live session on the PR — except the one that
/// called `exponential_pr_merge` itself, which the server spares and flips
/// back to `running`. So a still-`running` own row on a merged branch means
/// "the agent merged its own PR and is still working"; anything else
/// (`in_review`, `ended`, a row that never synced) means the merge came from
/// somewhere else and the tab should close.
fn session_merged_its_own_pr(
    sessions: &Entity<sync::collections::Collection<domain::rows::CodingSession>>,
    session_id: &str,
    cx: &App,
) -> bool {
    sessions
        .read(cx)
        .get(session_id)
        .and_then(|row| row.status.as_deref())
        .is_some_and(|status| status == domain::contract::CODING_SESSION_STATUS_RUNNING)
}

/// Does a session's worktree sit on `branch`? An EMPTY branch never matches
/// anything: trunk/scratch action runs record no branch, and neither does a
/// branch-less query — matching those to each other would make every scratch
/// run look like a holder of every other scratch run's tree.
fn holds_branch(session_branch: &str, branch: &str) -> bool {
    !session_branch.is_empty() && session_branch == branch
}

/// Is this session a fix-conflicts run (EXP-259)? `action_id` is the
/// session's action id — `None` for issue/batch sessions.
pub(crate) fn is_fix_conflicts_run(action_id: Option<&str>) -> bool {
    action_id == Some(api::actions::BUILTIN_FIX_CONFLICTS_ID)
}

/// Does any synced issue on `branch` carry a MERGED PR? The batch self-close
/// predicate (EXP-498): a batch session's issues all share its branch, so one
/// merged sibling means the batch PR merged and the session must end. An
/// empty branch never matches (trunk/scratch runs record no branch), and an
/// issue with no branch never matches either.
fn branch_pr_merged<'a>(
    branch: &str,
    issues: impl Iterator<Item = &'a domain::rows::Issue>,
) -> bool {
    !branch.is_empty()
        && issues.into_iter().any(|issue| {
            issue.branch.as_deref() == Some(branch)
                && issue.pr_state.as_deref() == Some("merged")
        })
}

/// One live session's claim on a branch, reduced to what a fix-conflicts
/// launch decides on. Generic over the close handle so the multiplicity rules
/// ([`plan_branch_takeover`]) stay unit-testable without gpui entities.
pub(crate) struct BranchClaim<H> {
    pub is_fix_run: bool,
    pub handle: H,
}

/// What a fix-conflicts launch must do with the sessions holding its branch.
pub(crate) enum BranchTakeover<H> {
    /// A fix run is ALREADY working the branch — refuse the duplicate rather
    /// than put two agents on one worktree rebasing + force-pushing.
    Refuse,
    /// Close EVERY holder (never just the first — an issue session and a
    /// batch session can both sit on one branch), then launch.
    Close(Vec<H>),
}

/// The takeover plan for the sessions currently holding a fix run's branch:
/// refuse if ANY of them is itself a fix run, otherwise hand back ALL of them
/// to close.
pub(crate) fn plan_branch_takeover<H>(
    claims: impl IntoIterator<Item = BranchClaim<H>>,
) -> BranchTakeover<H> {
    let claims: Vec<BranchClaim<H>> = claims.into_iter().collect();
    if claims.iter().any(|claim| claim.is_fix_run) {
        return BranchTakeover::Refuse;
    }
    BranchTakeover::Close(claims.into_iter().map(|claim| claim.handle).collect())
}

/// The local session for `issue_id`, if this process is coding it right now.
pub fn local_session_for<'a>(
    sessions: &'a LocalSessions,
    issue_id: &str,
) -> Option<&'a LocalCodingSession> {
    sessions.get(issue_id)
}

/// In-flight best-effort `codingSessions.end` calls (count + wakeup). The
/// watcher threads are fire-and-forget in steady state, but on a non-macOS
/// last-window-close quit the release CASCADE runs before the quit
/// observers — the release watchers have already emptied [`LocalSessions`]
/// and spawned their end threads by the time the quit hook fires, and those
/// threads would race process exit. The hook drains this counter instead of
/// (only) sweeping the registry.
static PENDING_ENDS: (std::sync::Mutex<usize>, std::sync::Condvar) =
    (std::sync::Mutex::new(0), std::sync::Condvar::new());

/// Fire `codingSessions.end` for `session_id` on a plain thread, tracked in
/// [`PENDING_ENDS`] so the quit hook can wait for it (idempotent
/// server-side; errors are swallowed — the server sweep is the backstop).
fn spawn_tracked_end(trpc: Arc<api::TrpcClient>, session_id: String) {
    {
        let (count, _) = &PENDING_ENDS;
        *count.lock().unwrap_or_else(|poison| poison.into_inner()) += 1;
    }
    std::thread::spawn(move || {
        coding::end_session_best_effort(&trpc, &session_id);
        let (count, wake) = &PENDING_ENDS;
        *count.lock().unwrap_or_else(|poison| poison.into_inner()) -= 1;
        wake.notify_all();
    });
}

/// Block until every tracked end resolved or `deadline` passed.
fn drain_pending_ends(deadline: std::time::Instant) {
    let (count, wake) = &PENDING_ENDS;
    let mut pending = count.lock().unwrap_or_else(|poison| poison.into_inner());
    while *pending > 0 {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return;
        };
        pending = wake
            .wait_timeout(pending, remaining)
            .unwrap_or_else(|poison| poison.into_inner())
            .0;
    }
}

/// How long the quit hook waits for the best-effort `codingSessions.end`
/// calls before letting the quit proceed (a dead network must never wedge
/// ⌘Q; the server staleness sweep remains the backstop).
const QUIT_END_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// EXP-105: end every coding_sessions row THIS process launched when the app
/// quits (⌘Q, last-window close on non-macOS). The per-session exit hook and
/// watchers die with their entities during teardown — the claude child gets
/// SIGHUP from its PTY closing, but nothing would end the synced row, so the
/// "coding now" badge ghosted on every client until the server staleness
/// sweep. Installed once from `ui::init`.
///
/// The waiting happens IN the observer body, not the returned future: gpui's
/// `shutdown` only grants quit futures `SHUTDOWN_TIMEOUT` (200ms) — not
/// enough for HTTPS round trips — while the body runs synchronously before
/// that clock starts. Sessions still registered end here; sessions the
/// pre-quit release cascade already handed to their release watchers are
/// covered by draining [`PENDING_ENDS`] within the same deadline.
pub fn install_quit_hook(cx: &mut App) {
    cx.on_app_quit(|cx| {
        let session_ids: Vec<String> = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).session_ids())
            .unwrap_or_default();
        let trpc = (!session_ids.is_empty())
            .then(|| queries::trpc_client(cx).map(Arc::new))
            .flatten();
        if let Some(trpc) = trpc {
            for session_id in session_ids {
                spawn_tracked_end(Arc::clone(&trpc), session_id);
            }
        }
        drain_pending_ends(std::time::Instant::now() + QUIT_END_TIMEOUT);
        // EXP-300: kill agent processes that escaped their PTY. `claude`
        // spawns a daemon that `setsid`s away, so killing the PTY child does
        // not reach it; it survives our exit, stays in our macOS COALITION,
        // and keeps our Launch Services registration alive as
        // `exited-with-subordinates`. The next launch is then delivered to
        // the dead instance as a re-open — the "app does nothing until you
        // launch it twice" report. Verified: killing the survivors releases
        // the registration immediately.
        let data_dir = cx
            .try_global::<AuthContext>()
            .map(|auth| auth.data_dir.clone())
            .unwrap_or_else(api::default_data_dir);
        coding::reaper::reap(&data_dir);
        async {}
    })
    .detach();
}

// ---------------------------------------------------------------------------
// TokenRefreshers — per-clone installation-token keep-alive (EXP-56 P9)
// ---------------------------------------------------------------------------

struct RefresherEntry {
    /// Live sessions sharing this clone (single-issue + batch runs on the
    /// same repo share one loop).
    count: usize,
    cancel: Arc<AtomicBool>,
}

/// Process-global, ref-counted per-CLONE token refreshers: while any local
/// session runs on a clone, a background loop keeps the clone's ambient git
/// credentials fresh ([`coding::refresh_clone_token`] — cached-or-fresh mint
/// + downgrade-guarded credential-file install, EXP-73) so `git push` keeps
/// working past the ≤1 h token TTL — for the main worktree AND every
/// subagent worktree (the credential file lives in the shared `.git`). The
/// cadence is derived from each token's REAL expiry
/// ([`coding::next_refresh_delay`]); the old fixed 40-minute loop could
/// outlive a cache-served token. `retain` after every successful spawn;
/// `release` rides [`LocalSessions::remove`] (both exit paths, exactly once).
/// The CLI/daemon runs the gpui-free counterpart,
/// `coding::token_refresh_host` (EXP-447).
#[derive(Default)]
pub struct TokenRefreshers {
    by_clone: HashMap<PathBuf, RefresherEntry>,
}

impl gpui::Global for TokenRefreshers {}

impl TokenRefreshers {
    /// Hold a refresh loop for `clone` (starting one on the first hold).
    pub fn retain(clone: &Path, repository_id: &str, cx: &mut App) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return; // signed out mid-spawn — the session itself is doomed anyway
        };
        {
            let refreshers = cx.default_global::<TokenRefreshers>();
            if let Some(entry) = refreshers.by_clone.get_mut(clone) {
                entry.count += 1;
                return;
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        cx.default_global::<TokenRefreshers>().by_clone.insert(
            clone.to_path_buf(),
            RefresherEntry {
                count: 1,
                cancel: cancel.clone(),
            },
        );

        let trpc = Arc::new(trpc);
        let clone = clone.to_path_buf();
        let repository_id = repository_id.to_string();
        cx.spawn(async move |cx| {
            // Refresh first, then sleep the expiry-derived delay: the
            // launcher just seeded the token cache, so the first pass is a
            // cache hit + idempotent credential install — and it hands us
            // the real expiry to schedule from.
            loop {
                let trpc = Arc::clone(&trpc);
                let refresh_clone = clone.clone();
                let refresh_repo = repository_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        coding::refresh_clone_token(&trpc, &refresh_repo, &refresh_clone)
                    })
                    .await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let delay = match result {
                    Ok(minted) => coding::next_refresh_delay(
                        minted.expires_at.as_deref(),
                        std::time::SystemTime::now(),
                    ),
                    Err(err) => {
                        // A persistent failure eventually surfaces as a
                        // visible push 401 in the tab (GIT_TERMINAL_PROMPT=0
                        // — never a hidden prompt); keep retrying meanwhile.
                        log::warn!(
                            "[ui] clone token refresh failed for {}: {err} — retrying in {}s",
                            clone.display(),
                            coding::TOKEN_REFRESH_RETRY.as_secs()
                        );
                        coding::TOKEN_REFRESH_RETRY
                    }
                };
                cx.background_executor().timer(delay).await;
                if cancel.load(Ordering::SeqCst) {
                    break; // released while sleeping — a stale refresh is useless
                }
            }
        })
        .detach();
    }

    /// Drop one hold; the loop is cancelled when the last holder releases.
    pub fn release(clone: &Path, cx: &mut App) {
        let refreshers = cx.default_global::<TokenRefreshers>();
        if let Some(entry) = refreshers.by_clone.get_mut(clone) {
            entry.count = entry.count.saturating_sub(1);
            if entry.count == 0 {
                entry.cancel.store(true, Ordering::SeqCst);
                refreshers.by_clone.remove(clone);
            }
        }
    }
}

/// The synced board row backing `issue_id`, if both are in the collections.
/// Used by the header affordance to decide whether Start coding even applies
/// (a repo-less board never codes) and by the §P7 activity gating.
pub(crate) fn issue_board(issue_id: &str, cx: &App) -> Option<domain::rows::Board> {
    let store = Store::global(cx);
    let board_id = store
        .collections()
        .issues
        .read(cx)
        .get(issue_id)
        .map(|issue| issue.board_id.clone())?;
    store
        .collections()
        .boards
        .read(cx)
        .get(&board_id)
        .cloned()
}

/// EXP-288: the user's terminal-shell override for new `+` shell tabs, read
/// live from the coding settings at each spawn (the terminal crate can't see
/// the coding crate, so every `open_shell` caller threads this through).
/// `None` = auto (the terminal crate's platform `default_shell`).
pub(crate) fn terminal_shell_override(cx: &mut App) -> Option<String> {
    let hub = CodingHub::global(cx);
    hub.read(cx).settings.terminal_shell.clone()
}

// ---------------------------------------------------------------------------
// Window plumbing — this window's TerminalManager (§06 dock)
// ---------------------------------------------------------------------------

/// Resolve THIS window's bottom terminal dock manager: `Root` → [`Shell`]
/// → `DockArea` → bottom `Dock` → the registered [`TerminalDockPanel`].
/// `None` on non-shell windows (login) — the caller surfaces an error.
pub fn window_terminal_manager(window: &Window, cx: &App) -> Option<Entity<TerminalManager>> {
    let panel = window_terminal_dock(window, cx)?;
    Some(panel.read(cx).manager().clone())
}

/// THIS window's terminal dock PANEL — the manager's owner, for callers that
/// need the panel's own launch paths (EXP-369: the settings pane's
/// per-worktree agent shell) rather than just its tab store.
pub fn window_terminal_dock(window: &Window, cx: &App) -> Option<Entity<TerminalDockPanel>> {
    let root = window.root::<gpui_component::Root>().flatten()?;
    let team = root
        .read(cx)
        .view()
        .clone()
        .downcast::<Shell>()
        .ok()?;
    let dock_area = team.read(cx).dock_area().clone();
    let bottom = dock_area.read(cx).bottom_dock()?.clone();
    find_terminal_dock(bottom.read(cx).panel())
}

/// Walk a `DockItem` tree for the terminal dock panel (the bottom dock is a
/// single `Tabs` today, but a user-rearranged layout may nest it in splits).
pub(crate) fn find_terminal_dock(item: &DockItem) -> Option<Entity<TerminalDockPanel>> {
    match item {
        DockItem::Tabs { items, .. } => items
            .iter()
            .find_map(|panel| panel.view().downcast::<TerminalDockPanel>().ok()),
        DockItem::Panel { view, .. } => view.view().downcast::<TerminalDockPanel>().ok(),
        DockItem::Split { items, .. } => items.iter().find_map(find_terminal_dock),
        // Tiles never host the terminal dock (team layout never creates
        // them); skipping is safe — the caller degrades to an error surface.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Launch orchestration (§7.1 — the ONE sequence, UI side)
// ---------------------------------------------------------------------------

/// Where this install keeps its per-account state — the token store, the
/// device id and (EXP-662) the run registry every resume reads. The signed-in
/// [`AuthContext`] is authoritative; the default only covers the pre-session
/// window.
pub fn coding_data_dir(cx: &App) -> PathBuf {
    cx.try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .unwrap_or_else(api::default_data_dir)
}

/// Everything `coding::prepare` needs for an ISSUE launch, assembled from the
/// signed-in app state. `None` when signed out or the issue isn't synced
/// (both make Start coding meaningless). Shared by the Start-coding dialog
/// and — via the same construction — the §08 relay `start_session` path
/// (which passes settings-default `options` with plan mode forced OFF).
///
/// EXP-662: `resume_prompt` only seeds the RESUME prompt (and clamps plan
/// mode) — an exact resume goes through [`build_resume_deps`] +
/// `PrepareRequest::ResumeRun` instead, off a `run_registry` record.
pub fn build_launch(
    issue_id: &str,
    origin: LaunchOrigin,
    options: LaunchOptions,
    resume_prompt: bool,
    cx: &mut App,
) -> Option<(LaunchRequest, CodingDeps)> {
    let account = queries::active_account(cx)?;
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let data_dir = coding_data_dir(cx);
    let hub = CodingHub::global(cx);
    let settings = hub.read(cx).settings.clone();

    let issue = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .get(issue_id)
        .cloned()?;
    // Snapshot the PROMPT.md seed now — the IssueSeedFn runs on a background
    // thread where the collections are unreachable (§7.1 step 5).
    let seed = IssueSeed {
        title: issue.title.clone(),
        description: issue.description.clone(),
    };
    let request = LaunchRequest {
        issue_id: issue.id.clone(),
        issue_identifier: issue.identifier.clone(),
        issue_status: issue.status,
        device_label: coding::default_device_label(),
        origin,
        options,
        resume_prompt,
    };
    let deps = CodingDeps {
        trpc,
        token_store: Arc::new(api::token_store::TokenStore::new(data_dir.clone())),
        account_id: account.id,
        settings,
        issue_seed: Arc::new(move |_| Some(seed.clone())),
        worktrees: Arc::new(coding::GitWorktrees),
        codex_sessions_root: None,
        claude_projects_root: None,
        device_id: Some(steer::persistent_device_id(&data_dir)),
        data_dir,
    };
    Some((request, deps))
}

/// [`CodingDeps`] for a BATCH launch — the same assembly as [`build_launch`]
/// minus the issue lookup: the dialog snapshots every issue's
/// title/description into the [`coding::BatchLaunchRequest`] itself, so the
/// seed fn is inert. `None` when signed out.
pub fn build_batch_deps(cx: &mut App) -> Option<CodingDeps> {
    let account = queries::active_account(cx)?;
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let data_dir = coding_data_dir(cx);
    let hub = CodingHub::global(cx);
    let settings = hub.read(cx).settings.clone();
    Some(CodingDeps {
        trpc,
        token_store: Arc::new(api::token_store::TokenStore::new(data_dir.clone())),
        account_id: account.id,
        settings,
        issue_seed: Arc::new(|_| None),
        worktrees: Arc::new(coding::GitWorktrees),
        codex_sessions_root: None,
        claude_projects_root: None,
        device_id: Some(steer::persistent_device_id(&data_dir)),
        data_dir,
    })
}

/// EXP-637: does the run registry still hold a resumable workspace for
/// `session_id`? Callable from a render pass (`&App`).
pub fn run_is_resumable_ref(session_id: &str, _cx: &App) -> bool {
    run_is_resumable_now(session_id)
}

fn run_is_resumable_now(session_id: &str) -> bool {
    let Some(data_dir) = crate::window_size::app_data_dir() else {
        return false;
    };
    coding::run_registry::get(&data_dir, session_id)
        .is_some_and(|record| record.resumable())
}

/// [`build_batch_deps`]'s action sibling (EXP-253): the same assembly — the
/// action's name/body ride the [`coding::ActionLaunchRequest`] itself, so
/// the issue-seed fn is inert. `None` when signed out.
pub fn build_action_deps(cx: &mut App) -> Option<CodingDeps> {
    build_batch_deps(cx)
}

/// EXP-662: [`CodingDeps`] for RESUMING `record` — [`build_batch_deps`] plus
/// the issue seed an ISSUE record's fallback prompt needs (the launcher asks
/// for `record.issue_id` when no native transcript survived). Snapshotted in
/// the foreground like [`build_launch`] does: the seed fn runs on a
/// background thread, where the collections are unreachable. `None` when
/// signed out; a missing/unsynced issue just leaves the seed empty (the
/// prompt falls back to the identifier).
pub fn build_resume_deps(record: &RunRecord, cx: &mut App) -> Option<CodingDeps> {
    let mut deps = build_batch_deps(cx)?;
    if let Some(issue_id) = record.issue_id.clone() {
        let seed = Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(&issue_id)
            .map(|issue| IssueSeed {
                title: issue.title.clone(),
                description: issue.description.clone(),
            });
        deps.issue_seed = Arc::new(move |asked| {
            if asked == issue_id {
                seed.clone()
            } else {
                None
            }
        });
    }
    Some(deps)
}

/// EXP-662: which [`LocalSessions`] key a resumed run registers under. An
/// issue/batch resume must land on the SUBJECT (so the header's Coding…/Stop
/// flip and the one-session-per-issue guard see it), not on the new row id —
/// only action/chat runs key by session.
pub fn resume_subject(record: &RunRecord, new_session_id: String) -> SessionSubject {
    match record.kind {
        RunKind::Issue => match &record.issue_id {
            Some(issue_id) => SessionSubject::Issue(issue_id.clone()),
            None => SessionSubject::Action(new_session_id),
        },
        RunKind::Batch => match &record.batch_id {
            Some(batch_id) => SessionSubject::Batch(batch_id.clone()),
            None => SessionSubject::Action(new_session_id),
        },
        _ => SessionSubject::Action(new_session_id),
    }
}

/// EXP-662: why `record` cannot be resumed right now; `None` = go ahead.
/// The one-session-per-issue rule (EXP-202/REV2-24) applies to a resume
/// exactly as it applies to a fresh start — the resumed agent lands back in
/// the same `exp/<ID>` worktree, so a second one would orphan the first.
/// Checked against BOTH this process ([`LocalSessions`]) and the live synced
/// rows (any other device), for the issue itself or every member of a batch.
/// Action and chat records own their own branch and are never blocked.
pub fn resume_blocker(record: &RunRecord, cx: &mut App) -> Option<String> {
    let subjects: Vec<(&str, &str)> = match record.kind {
        RunKind::Issue => {
            let issue_id = record.issue_id.as_deref()?;
            vec![(
                issue_id,
                record.issue_identifier.as_deref().unwrap_or(issue_id),
            )]
        }
        RunKind::Batch => record
            .issues
            .iter()
            .map(|issue| (issue.issue_id.as_str(), issue.identifier.as_str()))
            .collect(),
        _ => return None,
    };
    let sessions = LocalSessions::global(cx);
    let now = chrono::Utc::now().timestamp();
    for (issue_id, identifier) in subjects {
        // The record's OWN session never blocks its resume: the ended strip
        // closes the finished tab and calls straight through, but the close's
        // `TabClosed` (which drops the registration) only lands on the next
        // effect flush — a self-conflict would refuse every strip Resume.
        if sessions
            .read(cx)
            .get(issue_id)
            .is_some_and(|session| session.session_id != record.session_id)
        {
            return Some(format!(
                "Already coding {identifier}. Stop that session first."
            ));
        }
        if let Some(device) = queries::live_session_device_for_issue(cx, issue_id, now) {
            return Some(format!(
                "{identifier} already has a live session on {device} (only one session per issue)."
            ));
        }
    }
    None
}

/// Foreground half of the launch: spawn the prepared Claude tab into THIS
/// window's dock, register the local session (play→stop), and hook the exit
/// edge to clear it again. Shared by the single-issue and batch paths — only
/// the [`SessionSubject`] differs. A spawn failure never strands the row —
/// `spawn_prepared_with` already ends it.
pub fn spawn_into_window(
    mut prepared: coding::PreparedLaunch,
    subject: SessionSubject,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let Some(manager) = window_terminal_manager(window, cx) else {
        // No dock in this window — end the already-started row so the
        // "coding now" badge doesn't ghost (§7.1 step 6 created it).
        if let Some(trpc) = queries::trpc_client(cx) {
            let session_id = prepared.session_id.clone();
            std::thread::spawn(move || {
                coding::end_session_best_effort(&trpc, &session_id);
            });
        }
        return Err("No terminal dock in this window.".to_string());
    };
    let Some(trpc) = queries::trpc_client(cx) else {
        return Err("Not signed in.".to_string());
    };
    let trpc = Arc::new(trpc);

    // The P9 refresher inputs, snapshotted before the spawn consumes them.
    let clone = prepared.clone.clone();
    let repository_id = prepared.repository_id.clone();
    // EXP-478: taken out BEFORE `spawn_prepared_with` — its `..` destructure
    // would drop the hold pre-spawn, reopening the prune window it guards.
    let launch_hold = prepared.launch_hold.take();
    // The emitter's per-session facts (EXP-275 posture, EXP-383 agent,
    // EXP-443 identities, EXP-432 requester), snapshotted the same way.
    let steer_info = crate::steer_wiring::SteerSessionInfo {
        bypass_permissions: prepared.bypass_permissions,
        plan_mode: prepared.plan_mode,
        agent: prepared.agent,
        claude_session_id: prepared.claude_session_id.clone(),
        codex_originator: prepared.codex_originator.clone(),
        codex_resume_id: prepared.codex_resume_id.clone(),
        started_by_id: prepared.heartbeat_scope.started_by_id.clone(),
    };
    // Action identity for the registry's exit announcement (EXP-257).
    let action_id = match &prepared.tab_kind {
        TabKind::Action(id) => Some(id.clone()),
        _ => None,
    };
    // EXP-637: automation attribution (decides an agent-declared end's tab
    // policy) and the run worktree to reclaim at exit.
    let started_reason = prepared.heartbeat_scope.started_reason.clone();
    let run_cleanup = prepared.run_cleanup.clone();

    let sessions = LocalSessions::global(cx);
    let notify_sessions = sessions.downgrade();
    let notify_subject = subject.clone();
    let notify_session_id = prepared.session_id.clone();
    let exit_notify: coding::ExitNotify = Box::new(move |exit, cx: &mut App| {
        // EXP-283: detach the steer side FIRST — the exit hook just spawned
        // the `codingSessions.end` thread, and the kill-watch must be
        // unregistered before our own `ended` flip syncs back via Electric,
        // or it reads as a remote kill and closes the exited tab (which must
        // stay open with the exit strip, §7.5). Also stops the publisher with
        // the spec'd `exit:<code>` bye and halts the activity emitter.
        crate::steer_wiring::detach_publisher(
            &notify_session_id,
            Some(format!("exit:{}", exit.code)),
            cx,
        );
        if let Some(sessions) = notify_sessions.upgrade() {
            LocalSessions::remove(&sessions, &notify_subject, cx);
        }
    });

    match coding::spawn_prepared_with(prepared, &manager, cx, Arc::clone(&trpc), Some(exit_notify))
    {
        Ok(LaunchOutcome::Spawned { session_id, terminal_tab, worktree, branch }) => {
            // §08 steer publisher attach — tee this session's PTY out to the
            // relay for phone steering. Best-effort: a no-op when steer is
            // disabled/unreachable or the account is signed out. This is the
            // single hookup the §08 wiring owns (`ui::steer_wiring`). The
            // worktree rides along for the §P7 scrubbed activity emitter
            // (members-only activity channel).
            crate::steer_wiring::attach_publisher(
                &session_id,
                &subject,
                terminal_tab,
                &manager,
                worktree,
                steer_info,
                cx,
            );
            // P9: keep the clone's embedded token fresh for the session's
            // life (released via LocalSessions::remove on either exit path).
            // Repo-less action runs have no clone and nothing to refresh.
            if let Some(repository_id) = &repository_id {
                TokenRefreshers::retain(&clone, repository_id, cx);
            }
            LocalSessions::insert(
                &sessions,
                LocalCodingSession {
                    session_id,
                    subject,
                    clone,
                    branch,
                    tab: terminal_tab,
                    manager: manager.downgrade(),
                    action_id,
                    started_reason,
                    run_cleanup,
                },
                trpc,
                cx,
            );
            // EXP-478: released only now that the session is registered —
            // every prune policy derived from here on carries the branch in
            // `held_branches`; the gate covered the gap since before the
            // worktree existed. Failure arms release via RAII instead.
            drop(launch_hold);
            Ok(())
        }
        Ok(LaunchOutcome::Disabled { reason }) => Err(reason.message()),
        Err(err) => Err(format!("Could not start the coding session: {err}")),
    }
}

// ---------------------------------------------------------------------------
// StartCodingControl — the issue-detail header affordance (§7.1 / §4.2)
// ---------------------------------------------------------------------------

/// `repositories.forIssue` probe state for the current issue.
enum RepoProbe {
    Idle,
    Loading,
    /// `Ready(None)` = no repository linked (the disabled state).
    Ready(Option<api::repositories::IssueRepository>),
    /// Transport failure — the button stays CLICKABLE (a transient
    /// network error must never falsely block; the launch re-checks anyway).
    Error(String),
}

/// The Start-coding button + stop affordance. One per issue-detail view;
/// `set_issue` follows navigation.
pub struct StartCodingControl {
    issue_id: Option<String>,
    probe: RepoProbe,
    probe_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingControl {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // The hub (settings + doctor) and the local-session registry drive
        // the enabled state — re-render whenever either moves. The synced
        // coding_sessions collection drives the running↔in_review tone
        // (EXP-194: the server flips the row when the agent's PR opens).
        let hub = CodingHub::global(cx);
        let sessions = LocalSessions::global(cx);
        let synced_sessions = Store::global(cx).collections().coding_sessions.clone();
        let subscriptions = vec![
            cx.observe(&hub, |_, _, cx| cx.notify()),
            cx.observe(&sessions, |_, _, cx| cx.notify()),
            cx.observe(&synced_sessions, |_, _, cx| cx.notify()),
        ];
        Self {
            issue_id: None,
            probe: RepoProbe::Idle,
            probe_generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Point the control at an issue (navigation edge). Resets the probe.
    pub fn set_issue(&mut self, issue_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.probe = RepoProbe::Idle;
        cx.notify();
    }

    /// Kick the `repositories.forIssue` probe when idle (render-time, like
    /// the repositories pane — a hidden control never fetches). The button is
    /// driven by LIVE server state, never a cached local flag.
    fn ensure_probe(&mut self, cx: &mut gpui::Context<Self>) {
        if !matches!(self.probe, RepoProbe::Idle) {
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.probe = RepoProbe::Loading;
        self.probe_generation += 1;
        let generation = self.probe_generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::for_issue(&trpc, &issue_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.probe_generation != generation {
                    return; // navigated away mid-flight
                }
                this.probe = match result {
                    Ok(repo) => RepoProbe::Ready(repo),
                    Err(err) => RepoProbe::Error(err.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The probe's resolved repository, if it landed. The issue detail's
    /// actions menu gates worktree maintenance (Update from main, EXP-179)
    /// on it — the same live server state that drives the button.
    pub fn resolved_repo(&self) -> Option<&api::repositories::IssueRepository> {
        match &self.probe {
            RepoProbe::Ready(Some(repo)) => Some(repo),
            _ => None,
        }
    }

    /// Re-probe (the tiny retry next to the repo-less disabled state — a repo
    /// link or App install can resolve without navigating away).
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        self.probe = RepoProbe::Idle;
        CodingHub::refresh_doctor(&CodingHub::global(cx), cx);
        cx.notify();
    }

    /// The click: open the shared Start-coding dialog (it owns the
    /// model/effort/plan-mode choices AND the prepare→spawn task).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        crate::start_coding_dialog::open_for_issue(window, cx, issue_id);
    }

    /// The stop affordance (§7.5), behind a confirm (EXP-268 — destructive
    /// native actions confirm first): close this issue's terminal tab
    /// entirely. `close_tab` kills the child and joins the PTY threads; the
    /// `TabClosed` watcher then fires the idempotent `codingSessions.end`
    /// and clears the registry.
    fn stop(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let spec = crate::native_dialog::AlertSpec::new(
            "Stop this coding session?",
            "The agent stops immediately and the terminal tab closes. \
             Uncommitted work in the worktree is kept.",
            "Stop session",
        )
        .on_ok(move |_, cx| {
            let sessions = LocalSessions::global(cx);
            let handle = sessions.read(cx).get(&issue_id).and_then(|session| {
                session
                    .manager
                    .upgrade()
                    .map(|manager| (manager, session.tab))
            });
            if let Some((manager, tab)) = handle {
                manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
            }
            true
        });
        crate::native_dialog::open_alert(window, cx, spec);
    }

    /// Whether the control renders anything at all: an issue is set AND its
    /// board is repo-backed (or not yet synced — never hide on a sync race).
    /// The issue header gates its whole agent row on this so an empty control
    /// never leaves an orphaned row.
    pub fn is_visible(&self, cx: &App) -> bool {
        let Some(issue_id) = self.issue_id.as_deref() else {
            return false;
        };
        match issue_board(issue_id, cx) {
            Some(board) => board.repository_id.is_some(),
            None => true,
        }
    }

    /// The disabled reason right now, `None` when the button may launch
    /// (repo non-null AND doctor green — `git` plus ANY usable agent;
    /// the launch path re-gates on the SELECTED agent).
    fn disabled_reason(&self, cx: &App) -> Option<SharedString> {
        let hub = CodingHub::global_ref(cx)?;
        let hub = hub.read(cx);
        match hub.doctor.report.as_ref() {
            None => return Some("Checking local tools…".into()),
            // EXP-201: the affordance needs git + at least ONE agent; the
            // dialog gates the specific agent the user selects.
            Some(report) => {
                if !report.git.ok {
                    return Some(
                        report
                            .git
                            .error
                            .clone()
                            .unwrap_or_else(|| "git is not available".to_string())
                            .into(),
                    );
                }
                if !report.any_agent_ok() {
                    return Some(if report.unauthed_agents().is_empty() {
                        NO_AGENT_COPY.into()
                    } else {
                        NO_AGENT_SIGNED_IN_COPY.into()
                    });
                }
            }
        }
        match &self.probe {
            RepoProbe::Idle | RepoProbe::Loading => Some("Checking linked repository…".into()),
            // §7.1's exact helper copy for the repo-less state.
            RepoProbe::Ready(None) => {
                Some("Link a repository to this board in team settings.".into())
            }
            RepoProbe::Ready(Some(_)) => None,
            // A probe transport error never falsely blocks — the
            // launch re-resolves the repo server-side anyway.
            RepoProbe::Error(_) => None,
        }
    }
}

impl CodingHub {
    /// Read-only global lookup (render paths that must not create the hub).
    pub(crate) fn global_ref(cx: &App) -> Option<Entity<CodingHub>> {
        cx.try_global::<CodingHubGlobal>().map(|g| g.0.clone())
    }
}

/// EXP-367: the ONE disabled-reason copy for every Start-coding affordance
/// when no agent CLI is installed (git may still be fine — coding just has
/// nothing to launch).
pub(crate) const NO_AGENT_COPY: &str =
    "No coding agent CLI found (claude, codex, or pi). Install one in Settings → Tools.";

/// EXP-409 variant: agents ARE installed, but every one of them is signed
/// out — the fix is a login, not an install.
pub(crate) const NO_AGENT_SIGNED_IN_COPY: &str =
    "No coding agent is signed in. Sign in to claude, codex, or pi (see Settings → Tools).";

/// `Some(reason)` when the doctor has REPORTED and no agent CLI is usable —
/// the shared gate for every Start-coding entry point (EXP-367: buttons
/// disable with this tooltip, never hide). `None` while the probe is still
/// running (never falsely block on a race) or before anything coding exists.
pub(crate) fn no_agent_reason(cx: &App) -> Option<SharedString> {
    let hub = CodingHub::global_ref(cx)?;
    let report = hub.read(cx).doctor.report.clone()?;
    if report.any_agent_ok() {
        return None;
    }
    // Installed-but-signed-out (EXP-409) reads as "sign in", not "install".
    if !report.unauthed_agents().is_empty() {
        return Some(NO_AGENT_SIGNED_IN_COPY.into());
    }
    Some(NO_AGENT_COPY.into())
}

impl Render for StartCodingControl {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Coding gates purely on repository presence: a repo-less board
        // shows NO Start-coding affordance, while any repo-backed board
        // keeps the button. Hidden here before the probe so it never fetches
        // (`is_visible` is the same gate the panel's group label uses).
        if !self.is_visible(cx) {
            return div().into_any_element();
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return div().into_any_element();
        };
        // Lazy kicks: the hub (doctor) exists once anything coding renders;
        // the probe follows the current issue.
        let _ = CodingHub::global(cx);
        self.ensure_probe(cx);

        // Local session running → "Coding…" + the play button becomes STOP.
        let running = LocalSessions::global(cx).read(cx).get(&issue_id).is_some();
        if running {
            // The terminal is still alive during review — the synced row's
            // parked states (EXP-194/EXP-214) only change the tone/label;
            // Stop stays either way. Review green, done blue once the PR
            // merges, needs-input amber while the agent waits on a picker.
            let now = chrono::Utc::now().timestamp();
            let collections = Store::global(cx).collections();
            let session = collections
                .coding_sessions
                .read(cx)
                .iter()
                .find(|session| {
                    session.issue_id.as_deref() == Some(issue_id.as_str())
                        && queries::coding_session_is_live(session, now)
                })
                .cloned();
            let pr_state = collections
                .issues
                .read(cx)
                .get(issue_id.as_str())
                .and_then(|issue| issue.pr_state.clone());
            let display = session
                .as_ref()
                .map(|session| queries::coding_session_display(session, pr_state.as_deref()))
                .unwrap_or(queries::CodingSessionDisplay::Running);
            let (tone, label) = match display {
                queries::CodingSessionDisplay::NeedsInput => {
                    (theme::tokens::YELLOW, "Needs input…")
                }
                queries::CodingSessionDisplay::Review => (theme::tokens::GREEN, "In review…"),
                queries::CodingSessionDisplay::Done => (theme::tokens::BLUE, "Done"),
                queries::CodingSessionDisplay::Running => (theme::tokens::GREEN, "Coding…"),
            };
            // EXP-417: content-sized status dot + label beside Stop — the
            // control is one item in the header's wrapping agent row now, not
            // a sidebar column.
            return h_flex()
                .gap_2()
                .items_center()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(div().size_1p5().rounded_full().bg(tone.to_hsla()))
                        .child(label),
                )
                .child(
                    Button::new("stop-coding")
                        .outline()
                        .web_sm()
                        .icon(Icon::new(registry::CODING_STOP).text_color(cx.theme().danger))
                        .label("Stop")
                        .tooltip("Stop the coding session and close its terminal")
                        .on_click(cx.listener(|this, _, window, cx| this.stop(window, cx))),
                )
                .into_any_element();
        }

        // EXP-417: the primary action of the header's agent row — a solid
        // content-sized button; the repo-less retry sits beside it as a
        // compact icon.
        let disabled = self.disabled_reason(cx);
        let mut row = h_flex().gap_1().items_center();
        let button = Button::new("start-coding")
            .primary()
            .web_sm()
            // The solid variant carries the emphasis now — a green glyph on
            // the primary fill only muddies it.
            .icon(Icon::new(registry::ACTION_RUN).text_color(if disabled.is_some() {
                cx.theme().muted_foreground
            } else {
                cx.theme().primary_foreground
            }))
            .label("Start coding");
        match disabled {
            Some(reason) => {
                // The disabled state ALWAYS explains itself — the
                // exact §7 copy rides the tooltip; retry re-probes.
                row = row.child(button.disabled(true).tooltip(reason));
                if matches!(self.probe, RepoProbe::Ready(None)) {
                    row = row.child(
                        Button::new("start-coding-retry")
                            .ghost()
                            .web_icon_xs()
                            .icon(
                                Icon::new(registry::UI_UNDO)
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .tooltip("Re-check repository and tools")
                            .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                    );
                }
            }
            None => {
                // A probe transport error stays clickable (never
                // falsely block) but says so — the launch re-resolves the
                // repo server-side and surfaces the real failure.
                let tooltip: SharedString = match &self.probe {
                    RepoProbe::Error(err) => format!(
                        "Couldn't check the linked repository ({err}). Starting will retry."
                    )
                    .into(),
                    _ => "Clone the linked repository and start an agent on this issue".into(),
                };
                row = row.child(
                    button
                        .tooltip(tooltip)
                        .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
                );
            }
        }
        row.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the claim list a branch's holders reduce to (`true` = that
    /// holder IS a fix-conflicts run), labelled so a failure names the holder.
    fn claims(holders: &[(&'static str, bool)]) -> Vec<BranchClaim<&'static str>> {
        holders
            .iter()
            .map(|(label, is_fix_run)| BranchClaim {
                is_fix_run: *is_fix_run,
                handle: *label,
            })
            .collect()
    }

    fn closed(takeover: BranchTakeover<&'static str>) -> Vec<&'static str> {
        match takeover {
            BranchTakeover::Close(handles) => handles,
            BranchTakeover::Refuse => panic!("expected a takeover, got a refusal"),
        }
    }

    /// Trunk/scratch action runs record no branch — they must never read as
    /// holders (of each other, or of a branch-less query).
    #[test]
    fn a_branchless_session_holds_nothing() {
        assert!(holds_branch("exp/EXP-1", "exp/EXP-1"));
        assert!(!holds_branch("exp/EXP-1", "exp/EXP-2"));
        assert!(!holds_branch("", ""), "two scratch runs share no branch");
        assert!(!holds_branch("", "exp/EXP-1"));
        assert!(!holds_branch("exp/EXP-1", ""));
    }

    /// EXP-498: the batch self-close predicate — only a MERGED PR on the
    /// session's own branch closes the batch tab.
    #[test]
    fn branch_pr_merged_matches_only_merged_prs_on_the_branch() {
        let issue = |branch: Option<&str>, pr_state: Option<&str>| -> domain::rows::Issue {
            serde_json::from_value(serde_json::json!({
                "id": "i-1", "board_id": "b-1", "number": 1,
                "identifier": "EXP-1", "title": "t", "status": "in_review",
                "branch": branch, "pr_state": pr_state,
            }))
            .unwrap()
        };
        let merged = issue(Some("exp/batch-a1b2c3d4"), Some("merged"));
        let open = issue(Some("exp/batch-a1b2c3d4"), Some("open"));
        let other_branch = issue(Some("exp/EXP-1"), Some("merged"));
        let branchless = issue(None, Some("merged"));
        assert!(branch_pr_merged("exp/batch-a1b2c3d4", [&merged].into_iter()));
        // One merged sibling suffices, whatever else shares the branch.
        assert!(branch_pr_merged(
            "exp/batch-a1b2c3d4",
            [&open, &merged].into_iter()
        ));
        assert!(!branch_pr_merged("exp/batch-a1b2c3d4", [&open].into_iter()));
        assert!(!branch_pr_merged(
            "exp/batch-a1b2c3d4",
            [&other_branch, &branchless].into_iter()
        ));
        // Trunk/scratch runs record no branch — never match anything.
        assert!(!branch_pr_merged("", [&merged, &branchless].into_iter()));
    }

    /// EXP-662: a resumed run registers under its SUBJECT, so the header's
    /// Coding…/Stop flip and the one-session-per-issue guards see it — the
    /// new row id keys only action/chat runs.
    #[test]
    fn a_resumed_issue_or_batch_registers_under_its_subject() {
        let record = |extra: serde_json::Value| -> RunRecord {
            let mut value = serde_json::json!({
                "sessionId": "old-row", "accountId": "acct-1", "agent": "claude",
                "cwd": "/tmp/wt", "recordedAt": 1,
            });
            for (key, field) in extra.as_object().expect("object") {
                value[key] = field.clone();
            }
            serde_json::from_value(value).expect("record")
        };
        assert_eq!(
            resume_subject(
                &record(serde_json::json!({ "kind": "issue", "issueId": "i-1" })),
                "new-row".to_string()
            ),
            SessionSubject::Issue("i-1".to_string())
        );
        assert_eq!(
            resume_subject(
                &record(serde_json::json!({ "kind": "batch", "batchId": "a1b2c3d4" })),
                "new-row".to_string()
            ),
            SessionSubject::Batch("a1b2c3d4".to_string())
        );
        assert_eq!(
            resume_subject(
                &record(serde_json::json!({ "kind": "chat" })),
                "new-row".to_string()
            ),
            SessionSubject::Action("new-row".to_string())
        );
        // A subject-less issue record (a downgrade/upgrade artifact) degrades
        // to the row key rather than panicking or keying on an empty string.
        assert_eq!(
            resume_subject(
                &record(serde_json::json!({ "kind": "issue" })),
                "new-row".to_string()
            ),
            SessionSubject::Action("new-row".to_string())
        );
    }

    #[test]
    fn only_the_fix_conflicts_builtin_counts_as_a_fix_run() {
        assert!(is_fix_conflicts_run(Some(
            api::actions::BUILTIN_FIX_CONFLICTS_ID
        )));
        assert!(!is_fix_conflicts_run(None), "issue/batch session");
        assert!(!is_fix_conflicts_run(Some("some-team-action")));
    }

    /// The launch proceeds (and closes nothing) when nobody holds the branch.
    #[test]
    fn an_unheld_branch_launches_with_nothing_to_close() {
        assert!(closed(plan_branch_takeover(claims(&[]))).is_empty());
    }

    /// The regression: a branch is co-held by the issue's own session AND a
    /// live fix run, and the fix run is NOT the first holder found (map
    /// iteration order is arbitrary, and `by_issue` is chained first). The
    /// duplicate must be refused whatever the order.
    #[test]
    fn any_live_fix_run_refuses_the_duplicate_whatever_the_order() {
        for holders in [
            vec![("issue-session", false), ("fix-run", true)],
            vec![("fix-run", true), ("issue-session", false)],
            vec![("fix-run", true)],
        ] {
            assert!(
                matches!(
                    plan_branch_takeover(claims(&holders)),
                    BranchTakeover::Refuse
                ),
                "a live fix run must refuse the duplicate: {holders:?}"
            );
        }
    }

    /// The other half: EVERY non-fix holder is closed before the launch —
    /// closing only the first left a second PTY sitting on the worktree the
    /// run is about to rebase and force-push.
    #[test]
    fn every_non_fix_holder_is_closed_before_the_launch() {
        let holders = [("issue-session", false), ("batch-session", false)];
        assert_eq!(
            closed(plan_branch_takeover(claims(&holders))),
            vec!["issue-session", "batch-session"],
        );
    }
}
