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
//!   agent CLI — claude/codex — plus `git`, EXP-201) and re-runs whenever
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
//!   connected", never an unexplained block). Click → the Agent page
//!   composer (`crate::chat_screen`, EXP-825), which owns the
//!   model/effort/plan-mode choices and the prepare→spawn task.
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
    div, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;
use terminal::TerminalManager;

use coding::{
    run_doctor,
    run_registry::{RunKind, RunRecord},
    CodingDeps, DoctorReport, IssueSeed, LaunchOptions, LaunchOrigin, LaunchRequest, Settings,
};

use crate::controls::WebControl as _;
use crate::queries;
use crate::session::AuthContext;
use crate::session_bar::SessionBar;
use crate::icons::registry;

// ---------------------------------------------------------------------------
// CodingHub — settings + doctor (§7.7)
// ---------------------------------------------------------------------------

/// Doctor lifecycle: `NotRun` → `Running` → `Ready(report)`; `Running` keeps
/// the previous report visible (a re-check must not flash the pane empty).
#[derive(Default)]
pub struct DoctorState {
    pub report: Option<DoctorReport>,
    /// EXP-1196: THE readiness block for this device, built off the same
    /// run (`coding::device_doctor::current`: the report + the usage cache's
    /// re-login verdicts + the computer-use permissions). Lands with
    /// `report`; `device_readiness` renders it.
    pub device: Option<coding::device_doctor::DeviceDoctor>,
    pub running: bool,
    generation: u64,
}

/// App-global coding state (one per process — the settings file and the local
/// toolchain are per-install, not per-window).
pub struct CodingHub {
    pub settings: Settings,
    settings_path: PathBuf,
    pub doctor: DoctorState,
    /// EXP-484: this machine's OWN latest agent status (who each CLI is
    /// signed in as + its rate-limit windows), mirrored here from the
    /// device-sync beat so the per-tab toolbar and the own-device settings
    /// dialog read the numbers without a round trip through the row. `None`
    /// until the first beat lands. Never collected on the foreground — the
    /// collector blocks on keychain reads, an HTTPS GET and a `codex
    /// app-server` spawn.
    pub agent_status: Option<coding::agent_usage::AgentStatusPayload>,
}

struct CodingHubGlobal(Entity<CodingHub>);

impl gpui::Global for CodingHubGlobal {}

/// EXP-1158: `default_agent` is the LAST USED agent — the launcher stamps it
/// into settings.json, and no settings surface edits it — so a UI save takes
/// the file's value rather than a possibly older copy held in memory.
fn keep_last_used_agent(settings: &mut Settings, path: &std::path::Path) {
    if path.exists() {
        settings.default_agent = Settings::load(path).default_agent;
    }
}

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
            agent_status: None,
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
        let data_dir = coding_data_dir(cx);
        cx.spawn(async move |cx| {
            let (report, device) = cx
                .background_executor()
                .spawn(async move {
                    let report = run_doctor(&settings, &data_dir);
                    let device = coding::device_doctor::current(&settings, &data_dir, &report);
                    (report, device)
                })
                .await;
            let landed = hub.update(cx, |this, cx| {
                if this.doctor.generation != generation {
                    return false; // superseded
                }
                this.doctor.running = false;
                this.doctor.report = Some(report);
                this.doctor.device = Some(device);
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
                    // EXP-484: the agent-status collector's INPUT is this
                    // report, so the beat is nudged here — once the fresh
                    // one has landed. Nudging from the caller (a finished
                    // login, say) would race the probe and ship the old
                    // report's accounts.
                    crate::device_sync::beat_soon(cx);
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
        mut settings: Settings,
        cx: &mut App,
    ) -> Result<(), String> {
        let (result, settings_path) = hub.update(cx, |this, cx| {
            keep_last_used_agent(&mut settings, &this.settings_path);
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
    pub fn save_ui_prefs(hub: &Entity<CodingHub>, mut settings: Settings, cx: &mut App) {
        let result = hub.update(cx, |this, cx| {
            keep_last_used_agent(&mut settings, &this.settings_path);
            let result = settings.save(&this.settings_path);
            this.settings = settings;
            cx.notify();
            result
        });
        if let Err(err) = result {
            log::warn!("[ui] persisting the ui prefs failed: {err}");
        }
    }

    /// EXP-484: re-collect this machine's agent status (who each CLI is
    /// signed in as + its usage windows).
    ///
    /// The collector is BLOCKING (keychain, an HTTPS GET, a `codex
    /// app-server` spawn), so it only ever runs inside the device-sync beat
    /// on the background executor — and its INPUT is the doctor report. So
    /// this re-probes, and [`Self::refresh_doctor`]'s completion nudges the
    /// beat once the fresh report is in place: a nudge from here would race
    /// the probe and ship the stale accounts.
    pub fn refresh_agent_usage(hub: &Entity<CodingHub>, cx: &mut App) {
        Self::refresh_doctor(hub, cx);
    }

    /// EXP-484: this machine's own usage snapshot for `agent`, or `None`
    /// when the collector has not reported it (yet).
    pub fn own_agent_usage(&self, agent: coding::CodingAgent) -> Option<&coding::agent_usage::AgentUsage> {
        self.agent_status
            .as_ref()?
            .usage
            .get(agent.id())
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

/// WHERE a locally running session's agent actually lives (EXP-746).
///
/// Before ACP every local run was a PTY tab in a terminal dock, so the
/// bookkeeping carried a `TabId` + its manager outright. An ACP run has
/// neither: it is an in-process [`engine::EngineSession`] rendered by the
/// session screen. Everything that used to reach for the tab goes through
/// this instead, so the two hosts never grow parallel registries.
#[derive(Clone)]
pub struct LocalSessionHost {
    pub session: engine::EngineSession,
}

impl LocalSessionHost {
    /// "Stop this session."
    ///
    /// Killing the engine runs its own end sequence (D14): it ends the row
    /// and reports through `EngineHost::on_exit`, so the row never ghosts.
    pub fn stop(&self, cx: &mut App) {
        let _ = cx;
        self.session.kill("ended");
    }
}

/// One locally running coding session (a `coding_sessions` row whose agent
/// this process hosts — a PTY tab in one of our docks, or an in-process ACP
/// engine).
pub struct LocalCodingSession {
    pub session_id: String,
    pub subject: SessionSubject,
    /// The shared clone the session's worktree hangs off — releases its P9
    /// token-refresher hold when the session ends.
    pub clone: PathBuf,
    /// The branch the session's worktree is on (`exp/<IDENTIFIER>` /
    /// `exp/batch-<id8>`) — the EXP-102 sweep/delete guard key.
    pub branch: String,
    /// EXP-746: the PTY tab or the ACP engine hosting the agent.
    pub host: LocalSessionHost,
    /// The team action this run executes (`None` for issue/batch sessions).
    pub action_id: Option<String>,
    /// EXP-484: the agent CLI this session runs.
    pub agent: coding::CodingAgent,
    /// EXP-688: the session's worktree and the ref its "Latest changes" bar
    /// diffs against (`origin/<default branch>`; `None` for a repo-less
    /// scratch run — nothing to compare to).
    pub worktree: PathBuf,
    pub base_ref: Option<String>,
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

    /// EXP-849: the live local session with this `coding_sessions` row id,
    /// whatever it is keyed under — the one lookup that starts from a SESSION
    /// rather than from its subject (an account switch knows the row it is
    /// continuing, not whether it was keyed by issue, batch or action).
    pub fn session_for_id(&self, session_id: &str) -> Option<&LocalCodingSession> {
        self.all().find(|session| session.session_id == session_id)
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

    /// The live local session with this `coding_sessions` ROW id — the
    /// reverse of [`Self::session_for_tab`]. EXP-686: the automations run log
    /// resolves a synced live row back to the tab this process is hosting it
    /// in, so clicking the row can reveal that session.
    pub fn session_by_id(&self, session_id: &str) -> Option<&LocalCodingSession> {
        self.all().find(|session| session.session_id == session_id)
    }

    /// Every live local session's row id (issue + batch) — the EXP-105
    /// quit-time sweep input, the EXP-229 reconcile skip-set, and the
    /// sign-out sweep input.
    pub(crate) fn session_ids(&self) -> Vec<String> {
        self.all()
            .map(|session| session.session_id.clone())
            .collect()
    }

    /// EXP-746: every in-process ACP engine this app hosts — the app-quit
    /// sweep's kill list.
    pub(crate) fn acp_hosts(&self) -> Vec<engine::EngineSession> {
        self.all()
            .map(|session| session.host.session.clone())
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
        let (removed, live) = sessions.update(cx, |this, cx| {
            let entry = match subject {
                SessionSubject::Issue(id) => this.by_issue.remove(id),
                SessionSubject::Batch(id) => this.by_batch.remove(id),
                SessionSubject::Action(id) => this.by_action.remove(id),
            };
            cx.notify();
            // FEED-53: what the OTHER live sessions hold, taken with the
            // finished one already out, so its cleanup never removes a dir
            // or branch a sibling still runs on.
            let live = coding::run_cleanup::LiveHolders {
                branches: this.held_branches().map(str::to_string).collect(),
                worktrees: this.all().map(|session| session.worktree.clone()).collect(),
            };
            (entry, live)
        });
        // EXP-481: a session ending changes the inventory's busy flags.
        crate::device_sync::report_soon(cx);
        if let Some(entry) = removed {
            // EXP-637: reclaim the run's own worktree — but ONLY when it is
            // provably clean and carries no commits ([`coding::run_cleanup`]).
            // Background, because it shells out to git; a worktree left
            // dirty stays, and shows as dirty in Worktrees.
            // EXP-757: the run registry the LAUNCHER writes — never
            // `window_size::app_data_dir`, which is the window-layout dir
            // and a different folder on macOS/Windows/staging.
            let data_dir = coding_data_dir(cx);
            if let Some(cleanup) = entry.run_cleanup.clone() {
                let session_id = entry.session_id.clone();
                cx.spawn(async move |cx| {
                    let verdict = cx
                        .background_executor()
                        .spawn({
                            let cleanup = cleanup.clone();
                            let data_dir = data_dir.clone();
                            let session_id = session_id.clone();
                            async move {
                                // Plus every run a live host drives on the
                                // shared data dir (the CLI daemon, another
                                // desktop).
                                let live = live.with_registry(&data_dir, &session_id);
                                coding::remove_if_clean(&cleanup, &live)
                            }
                        })
                        .await;
                    let _ = cx.update(|cx| {
                        // The record STAYS on a removal: a resume re-creates
                        // the worktree on the recorded branch (the prune
                        // reclaims it again once the work lands), and the
                        // registry's TTL retires the record.
                        log::info!(
                            "run cleanup [{session_id}] on {}: {verdict:?}",
                            cleanup.branch
                        );
                        // EXP-481: the worktree inventory changed.
                        crate::device_sync::report_soon(cx);
                    });
                })
                .detach();
            } else if coding::scratch::is_scratch_dir(&data_dir, &entry.worktree) {
                // EXP-764: a repo-less run is purged WHOLE with the run —
                // scratch dir, claude trust entries, run
                // record, steer journal. Nothing of it is resumable.
                let session_id = entry.session_id.clone();
                let worktree = entry.worktree.clone();
                // Stamped HERE, not in the spawned task: a resume that
                // re-enters the dir while this sits in the queue keeps it.
                let requested_at = std::time::SystemTime::now();
                cx.background_executor()
                    .spawn(async move {
                        let purged = coding::scratch::purge(
                            &data_dir,
                            &session_id,
                            &worktree,
                            requested_at,
                        );
                        if purged {
                            steer::remove_journal(&data_dir, &session_id);
                        }
                        log::info!(
                            "scratch purge [{session_id}] {}: purged={purged}",
                            worktree.display()
                        );
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

    /// Track a freshly spawned session.
    fn insert(
        sessions: &Entity<LocalSessions>,
        session: LocalCodingSession,
        _trpc: Arc<api::TrpcClient>,
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
        // EXP-481: a session registering changes the inventory's busy flags.
        crate::device_sync::report_soon(cx);
        // EXP-1158: and a person's start just moved the last used login +
        // agent (the launcher's stamp) — beat now so the row carries them.
        crate::device_sync::beat_soon(cx);
        sessions.update(cx, |this, cx| {
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
    }
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

/// EXP-758: the floor under one engine's share of the quit budget. Mirrors
/// the CLI daemon's shutdown sweep (`daemon.rs`): even a budget that is
/// already spent still gives each ACP child the moment it needs to die, or
/// ⌘Q leaves `codex app-server` and its tool subprocesses behind exactly as
/// it did before the wait existed.
const QUIT_ENGINE_MIN_WAIT: std::time::Duration = std::time::Duration::from_millis(50);

/// EXP-758: how long the quit hook waits for ONE engine, given the shared
/// `deadline` and the clock reading `now`. Pure so the budget arithmetic is
/// testable without a window or a live engine.
///
/// The deadline is shared across engines rather than divided by their count:
/// the kills went out together, so the waits overlap in wall-clock terms and
/// the first engine to die hands the rest of the budget to the next one.
fn quit_engine_wait(
    deadline: std::time::Instant,
    now: std::time::Instant,
) -> std::time::Duration {
    deadline
        .saturating_duration_since(now)
        .max(QUIT_ENGINE_MIN_WAIT)
}

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
        // EXP-746: an ACP run's agent is OUR child, not a PTY's — nothing
        // SIGHUPs it when we go. Kill every engine first so the child dies
        // with us and the end sequence has the quit window to land in; the
        // row end below is the same idempotent backstop the retired PTY
        // path used.
        // (Closing a WINDOW deliberately does not do this: an ACP run is not
        // window-bound — that is the point of moving it off the dock.)
        let engines: Vec<engine::EngineSession> = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).acp_hosts())
            .unwrap_or_default();
        for session in &engines {
            session.kill("ended");
        }
        let deadline = std::time::Instant::now() + QUIT_END_TIMEOUT;
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
        // EXP-758: a `kill` is only a message to the engine loop. The ACP
        // child dies in the engine thread's `ChildGuard` drop, which our exit
        // never waited for, so ⌘Q with a live codex run orphaned `codex
        // app-server` and every tool subprocess under it (only claude carries
        // the reaper's `claude-hooks` anchor). Wait each engine out on the SAME
        // deadline the row ends drain against; the network calls above are
        // already running on their own threads, so the two waits overlap.
        for session in &engines {
            let wait = quit_engine_wait(deadline, std::time::Instant::now());
            if session.wait_timeout(wait).is_none() {
                log::warn!("[ui] quit: an acp engine did not exit within {wait:?}");
            }
        }
        drain_pending_ends(deadline);
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
        // EXP-758: …and the ACP children the waits above could not collect
        // (a wedged agent that outlived its budget). `reap` only selects on
        // the claude hook marker, so a codex/external child is invisible
        // to it; `reap_recorded` works off the run registry's recorded pids.
        let reaped = coding::reaper::reap_recorded(&data_dir);
        if reaped > 0 {
            log::info!("[ui] quit: reaped {reaped} orphaned acp child process(es)");
        }
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
// Window plumbing — this window's TerminalManager (the session bar, EXP-769)
// ---------------------------------------------------------------------------

/// Resolve THIS window's terminal manager — the [`SessionBar`]'s, looked up
/// in its per-window registry. `None` on non-shell windows (login) — the
/// caller surfaces an error.
pub fn window_terminal_manager(window: &Window, cx: &App) -> Option<Entity<TerminalManager>> {
    let bar = window_session_bar(window, cx)?;
    Some(bar.read(cx).manager().clone())
}

/// THIS window's session bar — the manager's owner, for callers that need
/// its own launch paths (EXP-369: the settings pane's per-worktree agent
/// shell, the agent-login tab) rather than just its tab store.
pub fn window_session_bar(window: &Window, cx: &App) -> Option<Entity<SessionBar>> {
    crate::session_bar::host_for_window(window, cx)
}

/// EXP-484: ANY window that owns a session bar, preferring the active one.
///
/// The device-settings dialog and the alert windows are windows of their own
/// with no bar at all, so a login started from one cannot resolve its
/// terminals through `navigation::on_active_window` — it would land on the
/// dialog and silently do nothing. Probing each window instead is also the
/// re-entrancy guard: a window already inside an `update` (the caller's own)
/// answers `Err` and is skipped, so callers DEFER before asking.
pub fn any_terminal_dock(cx: &mut App) -> Option<gpui::AnyWindowHandle> {
    let mut candidates: Vec<gpui::AnyWindowHandle> = Vec::new();
    if let Some(active) = cx.active_window() {
        candidates.push(active);
    }
    candidates.extend(cx.windows());
    for handle in candidates {
        let has_bar = handle
            .update(cx, |_, window, cx| window_session_bar(window, cx).is_some())
            .unwrap_or(false);
        if has_bar {
            return Some(handle);
        }
    }
    None
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
/// (both make Start coding meaningless). Shared by the Agent page composer
/// and — via the same construction — the §08 relay `start_session` path
/// (which passes settings-default `options` with plan mode forced OFF).
///
/// EXP-662: `resume_prompt` only seeds the RESUME prompt (and clamps plan
/// mode) — an exact resume goes through [`build_resume_deps`] +
/// `PrepareRequest::ResumeRun` instead, off a `run_registry` record.
/// `prompt` (EXP-825): the composer's free text — the prompt's
/// additional-instructions section.
pub fn build_launch(
    issue_id: &str,
    origin: LaunchOrigin,
    options: LaunchOptions,
    resume_prompt: bool,
    prompt: Option<String>,
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
        // EXP-712: the issue's board decides the worktree base + PR target.
        board_id: Some(issue.board_id.clone()),
        issue_identifier: issue.identifier.clone(),
        issue_status: issue.status,
        device_label: coding::default_device_label(),
        origin,
        options,
        resume_prompt,
        prompt,
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
        // EXP-746: the engine runs its session on the steer runtime, so no
        // runtime means no ACP host — the transport decision is made HERE,
        // at prepare time, because an Acp-prepared launch has no TUI argv to
        // fall back to at spawn time.
        acp_available: crate::steer_wiring::runtime(cx).is_some(),
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
        // EXP-746: see `build_launch` above.
        acp_available: crate::steer_wiring::runtime(cx).is_some(),
        data_dir,
    })
}

/// EXP-637: does the run registry hold a resumable record for `session_id`?
/// Callable from a render pass (`&App`). A repo-backed run stays resumable
/// after the prune reclaimed its worktree — the resume re-creates it.
///
/// EXP-764: a repo-less run answers `false` even while its scratch dir
/// stands. The dir, the record and the journal are purged with the run the
/// moment it ends, and [`LocalSessions::remove`] hands that purge to a
/// background thread — so the ended screen's first paint can still see the
/// dir and would cache a Resume whose only possible outcome is "no local
/// record for this run".
pub fn run_is_resumable_ref(session_id: &str, cx: &App) -> bool {
    coding::run_registry::get(&coding_data_dir(cx), session_id)
        .is_some_and(|record| record.clone.is_some() && record.resumable())
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
    resume_blocker_for(record, false, cx)
}

/// [`resume_blocker`], told whether this resume is a CONTINUATION of the
/// record's own live run (EXP-849: an account switch).
///
/// A continuation exempts the record's own session and everything already
/// chained off it (`queries::resume_chain`) from the cross-device probe: the
/// live row it is about to replace is the same piece of work, so counting it
/// would make a switch impossible while the run it switches is still live.
/// Another machine's session on the issue still refuses.
pub fn resume_blocker_for(
    record: &RunRecord,
    continuation: bool,
    cx: &mut App,
) -> Option<String> {
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
                "Already coding {identifier}. Stop that run first."
            ));
        }
        let except = continuation.then_some(record.session_id.as_str());
        if let Some(device) =
            queries::live_session_device_for_issue_except(cx, issue_id, now, except)
        {
            return Some(format!(
                "{identifier} already has a live run on {device} (one run per issue)."
            ));
        }
    }
    None
}

struct DesktopEngineHost {
    exits: flume::Sender<engine::EngineExit>,
    /// FEED-63/67: where a stopped run's uncommitted work is saved.
    worktree: PathBuf,
}

impl engine::EngineHost for DesktopEngineHost {
    fn on_exit(&self, exit: engine::EngineExit) {
        // FEED-63/67: a run a PERSON stopped keeps its uncommitted work as a
        // local WIP commit on its branch (never pushed). Here, off the gpui
        // foreground (the engine thread), and BEFORE the exit edge reaches
        // the drain, whose run cleanup must see the saved commit.
        engine::save_stopped_run_work(&exit, &self.worktree);
        let _ = self.exits.send(exit);
    }
}

/// Start a prepared launch on the in-process ACP engine and open its session
/// screen — unless an agent started it ([`should_open_session`], EXP-1088).
///
/// The ORDER below is the whole function: every step is an invariant some
/// earlier bug bought.
///
/// 1. `launch_hold` comes out first (EXP-478) — anything that consumes
///    `prepared` afterwards would drop the gate pre-spawn.
/// 2. The kill watch is registered BEFORE `engine::start` (D14) so no
///    `ended` edge can land in the gap between the row existing and the
///    engine reading its feed.
/// 3. The exit drain is spawned before the start for the same reason.
/// 4. The hold is dropped only after `LocalSessions` registered the session,
///    so the auto-prune never sees an unheld worktree.
///
/// EXP-773: the ONE spawn path. A coding session has no tab, so a window
/// without a terminal dock is a perfectly good host. Every caller — the
/// Agent page composer, the three relay start handlers,
/// `action_run::resume_run`, the chat launch — funnels through here.
pub fn spawn_into_window(
    mut prepared: coding::PreparedLaunch,
    subject: SessionSubject,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    // EXP-478, before anything can consume `prepared`.
    let launch_hold = prepared.launch_hold.take();

    // The bookkeeping snapshot, exactly the PTY arm's (the engine takes
    // `prepared` whole).
    let session_id = prepared.session_id.clone();
    let clone = prepared.clone.clone();
    let repository_id = prepared.repository_id.clone();
    let branch = prepared.branch.clone();
    let worktree = prepared.worktree.clone();
    let base_ref = prepared.base_ref.clone();
    let agent = prepared.agent;
    let action_id = prepared.action_id.clone();
    let started_reason = prepared.heartbeat_scope.started_reason.clone();
    let started_reason_for_nav = started_reason.clone();
    let started_by_id = prepared.heartbeat_scope.started_by_id.clone();
    let run_cleanup = prepared.run_cleanup.clone();
    // EXP-792: the team MCP servers this run starts WITHOUT, shown once the
    // engine is up (a launch that fails shows its own error instead).
    let mcp_warnings = std::mem::take(&mut prepared.mcp_warnings);

    let Some(trpc) = queries::trpc_client(cx) else {
        return Err("Not signed in.".to_string());
    };
    let trpc = Arc::new(trpc);
    let Some(account) = queries::active_account(cx) else {
        return Err("Not signed in.".to_string());
    };
    // `prepare` only resolves the Acp transport when `acp_available` said a
    // runtime exists, so this is a signed-out-mid-launch degenerate case.
    let Some(runtime) = crate::steer_wiring::runtime(cx) else {
        end_row_best_effort(&trpc, &session_id);
        return Err("The steering runtime is not available.".to_string());
    };

    let sessions = LocalSessions::global(cx);
    let (exit_tx, exit_rx) = flume::bounded::<engine::EngineExit>(1);
    // The exit edge lands on the engine's thread; this is its foreground half
    // (EXP-283's ordering, verbatim: unwatch before our own `ended` flip can
    // sync back and read as a remote kill).
    {
        let sessions = sessions.downgrade();
        let subject = subject.clone();
        let session_id = session_id.clone();
        cx.spawn(async move |cx| {
            let Ok(exit) = exit_rx.recv_async().await else {
                return;
            };
            let _ = cx.update(|cx| {
                if let Some(error) = exit.error.as_deref() {
                    log::warn!("[ui] acp session {session_id} failed: {error}");
                } else {
                    log::info!("[ui] acp session {session_id} ended ({})", exit.outcome);
                }
                crate::steer_wiring::unwatch_kill(&session_id, cx);
                if let Some(sessions) = sessions.upgrade() {
                    LocalSessions::remove(&sessions, &subject, cx);
                }
                // The tab STAYS: an ended run is a read-only transcript.
                // EXP-758: with the failure carried in, so a run that died on
                // its handshake is not an empty tab that only says "ended".
                crate::session_screen::mark_ended(&session_id, exit.error.clone(), cx);
            });
        })
        .detach();
    }

    // D14: registered with our `cx` before the engine exists, so an `ended`
    // row that lands during the handshake is not lost.
    let kill = crate::steer_wiring::register_kill_feed(&session_id, cx);
    let start = engine::EngineStart {
        prepared,
        trpc: Arc::clone(&trpc),
        runtime,
        data_dir: coding_data_dir(cx),
        account_id: account.id,
        own_user_id: Some(account.user_id),
        // REV2-17 + the agent's MCP bearer.
        personal_key: crate::steer_wiring::personal_key(cx),
        issue_id: match &subject {
            SessionSubject::Issue(issue_id) => Some(issue_id.clone()),
            SessionSubject::Batch(_) | SessionSubject::Action(_) => None,
        },
        foreign_host: crate::steer_wiring::foreign_host(started_by_id.as_deref(), cx),
        // EXP-1005: the desktop runs the account-rotation beat.
        rotation_host: true,
        publish: true,
        kill,
        // The session screen reads the feed through `EngineSession::subscribe`
        // — a view can attach late (the tab reopened, the screen rebuilt) and
        // needs the backlog replayed, which a push sink cannot give it. The
        // sink is still handed over because its PRESENCE is what tells the
        // engine this host renders locally at all (the daemon passes `None`).
        local_sink: Some(Arc::new(|_| {})),
    };

    let host = Arc::new(DesktopEngineHost {
        exits: exit_tx,
        worktree: worktree.clone(),
    });
    let session = match engine::start(start, host) {
        Ok(session) => session,
        Err(err) => {
            // The row is already created (prepare made it), so it must not
            // ghost — the PTY arm's failure path, on a thread because it is a
            // blocking HTTPS call.
            crate::steer_wiring::unwatch_kill(&session_id, cx);
            end_row_best_effort(&trpc, &session_id);
            // `launch_hold` releases via RAII with this return.
            return Err(format!("Could not start the coding session: {err}"));
        }
    };

    // P9: keep the clone's embedded token fresh for the session's life
    // (released via `LocalSessions::remove` on the exit edge).
    if let Some(repository_id) = &repository_id {
        TokenRefreshers::retain(&clone, repository_id, cx);
    }
    LocalSessions::insert(
        &sessions,
        LocalCodingSession {
            session_id: session_id.clone(),
            subject,
            clone,
            branch,
            host: LocalSessionHost { session },
            action_id,
            agent,
            worktree,
            base_ref,
            started_reason,
            run_cleanup,
        },
        trpc,
        cx,
    );
    // EXP-478: only now (see the PTY arm).
    drop(launch_hold);
    if should_open_session(started_reason_for_nav.as_deref()) {
        crate::session_screen::open_session(&session_id, window, cx);
    }
    // EXP-792: a picked server the run starts without is not a silent
    // degradation: ONE toast names every skipped server (the launcher's
    // resolver already logged each line).
    if !mcp_warnings.is_empty() {
        crate::toast::warning(mcp_warnings.join("\n"), window, cx);
    }
    Ok(())
}

/// EXP-1088: whether a freshly spawned run takes the window over. A run
/// another coding session started (`agent`, MCP `sessions_start`) must NOT
/// navigate: the person
/// may be mid-edit elsewhere, and the run already appears in the sidebar via
/// its synced row, nested under its parent. Everything a person asked for
/// (the composer, a phone/web relay start, a chat, an automation) still
/// opens.
///
/// A person's LOCAL Resume never trips this even on an agent-started run:
/// the launcher recomputes the reason from `LaunchOrigin::Local` (always
/// `None`), and EXP-906's reason inheritance happens server-side, never in
/// `prepared.heartbeat_scope`. Only a relay frame (`agent`) can put that
/// reason here.
pub(crate) fn should_open_session(started_reason: Option<&str>) -> bool {
    started_reason != Some("agent")
}

// ---------------------------------------------------------------------------
// EXP-818: following a REMOTE start into its run
// ---------------------------------------------------------------------------

/// How long a remote start waits for the run's row to sync before giving up on
/// opening it. The frame is already on the machine by then — the toast said so
/// — so this is only about whether we navigate for the user.
const REMOTE_OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// How often the wait re-reads the synced rows. Electric notifies sooner than
/// this on a live shape; a poll keeps the watch out of the entity graph (the
/// launcher is a plain `&mut App` path).
const REMOTE_OPEN_TICK: std::time::Duration = std::time::Duration::from_millis(250);

/// EXP-818: which freshly synced row a remote start is waiting for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemoteRunSubject {
    /// An issue start — the row names the issue.
    Issue(String),
    /// A resume (`resumeSessionId`): the new row links back through
    /// `resumed_from_id`.
    Resume(String),
    /// A batch, action or chat start: nothing on the row identifies it beyond
    /// the machine it runs on, so any NEW live row there is the one.
    Any,
}

impl RemoteRunSubject {
    /// The subject a `steer.startSession` payload is waiting for: a resume
    /// links back by `resumed_from_id`, a single-issue start names its issue,
    /// and a batch / action / chat start names nothing the row carries.
    pub(crate) fn of(input: &api::steer::StartSessionInput) -> Self {
        if let Some(resumed_from) = input.resume_session_id.clone() {
            return RemoteRunSubject::Resume(resumed_from);
        }
        match input.issue_id.clone() {
            Some(issue_id) => RemoteRunSubject::Issue(issue_id),
            None => RemoteRunSubject::Any,
        }
    }

    /// Whether `row` (already known to be new, live and on the target machine)
    /// is the run this start asked for.
    fn matches(&self, row: &domain::rows::CodingSession) -> bool {
        match self {
            RemoteRunSubject::Issue(issue_id) => row.issue_id.as_deref() == Some(issue_id.as_str()),
            RemoteRunSubject::Resume(from) => {
                row.resumed_from_id.as_deref() == Some(from.as_str())
            }
            RemoteRunSubject::Any => true,
        }
    }
}

/// EXP-818: follow a remote start INTO the run, exactly as a local start lands
/// in its session screen.
///
/// `steer.startSession` only confirms the frame reached the machine; the
/// `coding_sessions` row is written by THAT machine and arrives over Electric a
/// moment later, which is why this is a watch and not a return value. The ids
/// already on screen are snapshotted first, so an unrelated run that happens to
/// be on the same machine can never be mistaken for this one; the newest
/// matching row wins, and nothing happens at all if none arrives within
/// [`REMOTE_OPEN_WAIT`] (the toast already told the user where the run went).
pub(crate) fn follow_remote_start(
    device_id: String,
    subject: RemoteRunSubject,
    window: &Window,
    cx: &mut App,
) {
    let known: HashSet<String> = session_ids(cx);
    // EXP-1075: the run lands on the team the launch was made from. The
    // predicate is TOLERANT — a row whose `team_id` has not decoded is never
    // dropped, because losing the run we just started is worse than following
    // a row that turns out to live elsewhere.
    let team = crate::navigation::active_team_id(&crate::navigation::nav_for_window(window, cx), cx);
    // EXP-1037: the run opens SECONDS from now, in a window that still
    // exists then — a composer DIALOG has closed itself by that point, so the
    // wait is pinned to the window that opened it.
    let handle = crate::navigation::deferred_open_window(window, cx);
    cx.spawn(async move |cx| {
        let deadline = std::time::Instant::now() + REMOTE_OPEN_WAIT;
        loop {
            cx.background_executor().timer(REMOTE_OPEN_TICK).await;
            if let Some(session_id) =
                cx.update(|cx| fresh_remote_run(&known, &device_id, &subject, team.as_deref(), cx))
            {
                // The window may have closed while we waited; there is nothing
                // else to do with the run then (its tab opens on the next
                // visit to the Agent page).
                let _ = handle.update(cx, |_, window, cx| {
                    crate::session_screen::open_session(&session_id, window, cx);
                });
                return;
            }
            if std::time::Instant::now() >= deadline {
                return;
            }
        }
    })
    .detach();
}

/// Every synced `coding_sessions` id right now — the "already there" baseline
/// of [`follow_remote_start`].
fn session_ids(cx: &App) -> HashSet<String> {
    let Some(store) = Store::try_global(cx) else {
        return HashSet::new();
    };
    store
        .collections()
        .coding_sessions
        .read(cx)
        .iter()
        .map(|row| row.id.clone())
        .collect()
}

/// The newest row that is NEW (not in `known`), live, hosted on `device_id`,
/// on `team` and matching `subject`.
///
/// EXP-1075: `team` is the team the start was made from — the run opens there,
/// where its rail row is. The check is TOLERANT: a row whose `team_id` is
/// `None` (not decoded yet) still matches, because the alternative is silently
/// losing the run the user just started.
fn fresh_remote_run(
    known: &HashSet<String>,
    device_id: &str,
    subject: &RemoteRunSubject,
    team: Option<&str>,
    cx: &App,
) -> Option<String> {
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let rows = collections.coding_sessions.read(cx);
    rows.iter()
        .filter(|row| !known.contains(&row.id))
        .filter(|row| row.device_id.as_deref() == Some(device_id))
        .filter(|row| row.status.as_deref() != Some("ended"))
        .filter(|row| match team {
            Some(team) => row.team_id.is_none() || row.team_id.as_deref() == Some(team),
            None => true,
        })
        .filter(|row| subject.matches(row))
        .max_by(|a, b| {
            a.started_at
                .as_deref()
                .unwrap_or_default()
                .cmp(b.started_at.as_deref().unwrap_or_default())
        })
        .map(|row| row.id.clone())
}

/// End `session_id`'s row off the foreground, best effort. Both arms' failure
/// paths use it: `prepare` already created the row, so a start that never
/// happened must not leave "coding now" ghosting on every client.
fn end_row_best_effort(trpc: &Arc<api::TrpcClient>, session_id: &str) {
    let trpc = Arc::clone(trpc);
    let session_id = session_id.to_string();
    std::thread::spawn(move || {
        coding::end_session_best_effort(&trpc, &session_id);
    });
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
    /// EXP-760: stand down from the PRIMARY paint to the glass pill. Set by
    /// the issue header while a PR is open, where Merge is the action the
    /// user came for and two white pills side by side would name neither.
    demoted: bool,
    /// EXP-1121: the board's `repository_id` the current probe answers for —
    /// a repo linked (or unlinked) under the open issue re-probes.
    probed_repo_id: Option<String>,
    /// EXP-1121: the team's GitHub facts, read while the board has no repo.
    github: crate::coding_readiness::GithubProbe,
    github_generation: u64,
    /// EXP-1121: the "Ready to code?" popover is up — it keeps rendering
    /// (rows ticking) even once every step is met.
    readiness_open: bool,
    /// The repository row's inline picker is showing.
    picker_open: bool,
    /// Its search field (created on first open — it needs a window).
    repo_query: Option<Entity<InputState>>,
    /// `boards.setRepository` in flight / its refusal.
    linking: bool,
    link_error: Option<SharedString>,
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
        // EXP-1121: the readiness rows tick live off the synced board (a repo
        // linked anywhere) and devices rows.
        let boards = Store::global(cx).collections().boards.clone();
        let devices = Store::global(cx).collections().devices.clone();
        let subscriptions = vec![
            cx.observe(&hub, |_, _, cx| cx.notify()),
            cx.observe(&sessions, |_, _, cx| cx.notify()),
            cx.observe(&synced_sessions, |_, _, cx| cx.notify()),
            cx.observe(&boards, |_, _, cx| cx.notify()),
            cx.observe(&devices, |_, _, cx| cx.notify()),
            // SLOP-7/25: the GitHub step's fixes finish in the browser — the
            // two hand-backs (`oauth-return?linked=github`, `github-connected`)
            // re-read the facts so the row ticks in place.
            cx.observe_global::<crate::oauth::SignInLinkOutcome>(|this, cx| {
                if matches!(
                    crate::github_connect::github_link_outcome(cx),
                    Some(Ok(()))
                ) {
                    this.refresh_github(cx);
                }
            }),
            cx.observe_global::<crate::github_connect::GithubConnectSignal>(|this, cx| {
                let connected = cx
                    .try_global::<crate::github_connect::GithubConnectSignal>()
                    .is_some_and(|signal| {
                        matches!(
                            signal.outcome,
                            Some(crate::github_connect::GithubConnectOutcome::Connected)
                        )
                    });
                if connected {
                    this.refresh_github(cx);
                }
            }),
        ];
        Self {
            issue_id: None,
            probe: RepoProbe::Idle,
            probe_generation: 0,
            demoted: false,
            probed_repo_id: None,
            github: crate::coding_readiness::GithubProbe::Idle,
            github_generation: 0,
            readiness_open: false,
            picker_open: false,
            repo_query: None,
            linking: false,
            link_error: None,
            _subscriptions: subscriptions,
        }
    }

    /// See [`Self::demoted`]. Repaints only on a real change — this is
    /// called from the header's render.
    pub fn set_demoted(&mut self, demoted: bool, cx: &mut gpui::Context<Self>) {
        if self.demoted != demoted {
            self.demoted = demoted;
            cx.notify();
        }
    }

    /// Point the control at an issue (navigation edge). Resets the probe.
    pub fn set_issue(&mut self, issue_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.probe = RepoProbe::Idle;
        self.picker_open = false;
        self.link_error = None;
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
        self.probed_repo_id = issue_board(&issue_id, cx).and_then(|board| board.repository_id);
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

    /// The click: open the Agent page composer with this issue picked
    /// (EXP-825 — it owns the model/effort/plan-mode choices AND the
    /// prepare→spawn task).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        crate::navigation::navigate_to_chat(
            window,
            cx,
            crate::navigation::ChatSeed::issues(vec![issue_id]),
        );
    }

    /// Whether the control renders anything at all: an issue is set. The
    /// issue header gates its whole agent row on this so an empty control
    /// never leaves an orphaned row.
    ///
    /// EXP-1121: a repo-less board no longer hides Start coding — it renders
    /// NOT READY (the dashed amber capsule + caption) and its click opens the
    /// "Ready to code?" checklist. The desktop can always run on itself and
    /// needs no relay, so for a member (anyone who sees the issue) it always
    /// renders.
    pub fn is_visible(&self, _cx: &App) -> bool {
        self.issue_id.is_some()
    }

    /// EXP-1121: the board the checklist is about, off the synced rows.
    fn readiness_subject(&self, cx: &App) -> Option<crate::coding_readiness::ReadinessSubject> {
        let board = issue_board(self.issue_id.as_deref()?, cx)?;
        let team_name = Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(&board.team_id)
            .map(|team| team.name.clone())
            .unwrap_or_default();
        Some(crate::coding_readiness::ReadinessSubject {
            team_id: board.team_id.clone(),
            team_name,
            board_id: board.id.clone(),
            board_name: board.name.clone(),
            board_slug: board.slug.clone().unwrap_or_default(),
            repository_id: board.repository_id.clone(),
        })
    }

    /// EXP-1121: the readiness model right now (`None` = the board has not
    /// synced — the plain launcher renders, never hide on a sync race).
    fn readiness(
        &self,
        local_device_label: &str,
        cx: &App,
    ) -> Option<(
        domain::coding_readiness::CodingReadiness,
        crate::coding_readiness::ReadinessSubject,
    )> {
        let subject = self.readiness_subject(cx)?;
        let probed = match &self.probe {
            RepoProbe::Ready(Some(repo))
                if subject.repository_id.as_deref() == Some(repo.repository_id.as_str()) =>
            {
                Some(repo.full_name.as_str())
            }
            _ => None,
        };
        let github = if subject.repository_id.is_none() {
            self.github.facts_for(&subject.team_id)
        } else {
            None
        };
        let input = crate::coding_readiness::readiness_input(
            &subject,
            probed,
            github,
            local_device_label,
            chrono::Utc::now().timestamp_millis(),
        );
        Some((domain::coding_readiness::coding_readiness(&input), subject))
    }

    /// EXP-1121: read the team's GitHub facts. `keep` = a refresh behind
    /// facts already on screen (the popover opening) — they stay up while it
    /// runs, so the trigger never flips to the inert loading state under an
    /// open popover.
    fn fetch_github(&mut self, team_id: String, keep: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        if !(keep && self.github.facts_for(&team_id).is_some()) {
            self.github = crate::coding_readiness::GithubProbe::Loading {
                team_id: team_id.clone(),
            };
        }
        self.github_generation += 1;
        let generation = self.github_generation;
        cx.spawn(async move |this, cx| {
            let fetch_team = team_id.clone();
            let facts = cx
                .background_executor()
                .spawn(async move {
                    crate::coding_readiness::fetch_github_facts(&trpc, &fetch_team)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.github_generation != generation {
                    return;
                }
                this.github = crate::coding_readiness::GithubProbe::Ready { team_id, facts };
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read the GitHub facts behind the ones on screen (the picker's
    /// retry, the popover opening — a connect finishes in the browser).
    pub(crate) fn refresh_github(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(subject) = self.readiness_subject(cx) {
            self.fetch_github(subject.team_id, true, cx);
        }
    }

    /// The popover opened or closed.
    fn set_readiness_open(&mut self, open: bool, cx: &mut gpui::Context<Self>) {
        if self.readiness_open == open {
            return;
        }
        self.readiness_open = open;
        if open {
            if self
                .readiness_subject(cx)
                .is_some_and(|subject| subject.repository_id.is_none())
            {
                self.refresh_github(cx);
            }
        } else {
            self.picker_open = false;
            self.link_error = None;
        }
        cx.notify();
    }

    /// "Choose repository": the inline picker, search focused.
    pub(crate) fn open_repo_picker(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let query = match &self.repo_query {
            Some(query) => query.clone(),
            None => {
                let query = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(domain::coding_readiness::copy::PICKER_SEARCH)
                });
                self._subscriptions.push(cx.observe(&query, |_, _, cx| cx.notify()));
                self.repo_query = Some(query.clone());
                query
            }
        };
        use gpui::Focusable as _;
        query.update(cx, |input, cx| input.set_value("", window, cx));
        query.read(cx).focus_handle(cx).focus(window, cx);
        self.picker_open = true;
        self.link_error = None;
        cx.notify();
    }

    /// A picked repository: `boards.setRepository` (the Board settings
    /// pane's call). Success needs nothing — the synced board row's echo
    /// ticks the row and re-probes; a refusal shows under the row.
    pub(crate) fn link_repository(&mut self, repository_id: String, cx: &mut gpui::Context<Self>) {
        let Some(subject) = self.readiness_subject(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.linking = true;
        self.link_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::boards::boards_set_repository(
                        &trpc,
                        &subject.board_id,
                        Some(&repository_id),
                        None,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.linking = false;
                match result {
                    Ok(_) => this.picker_open = false,
                    Err(err) => {
                        log::warn!("[ui] readiness: boards.setRepository failed: {err}");
                        this.link_error = Some(err.user_message().into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The popover footer's Start coding — the control's own launch.
    pub(crate) fn launch_from_readiness(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.readiness_open = false;
        self.launch(window, cx);
    }

    /// Everything the popover draws, read fresh on each of its renders.
    pub(crate) fn popover_facts(
        control: &Entity<Self>,
        cx: &mut App,
    ) -> Option<crate::coding_readiness::PopoverFacts> {
        let local = local_device_label(cx);
        let this = control.read(cx);
        let (readiness, subject) = this.readiness(&local, cx)?;
        let repos = this
            .github
            .facts_for(&subject.team_id)
            .map(|facts| facts.repos.clone());
        let github = this.github.facts_for(&subject.team_id).cloned();
        Some(crate::coding_readiness::PopoverFacts {
            readiness,
            subject,
            picker_open: this.picker_open,
            linking: this.linking,
            link_error: this.link_error.clone(),
            github,
            repos,
            query: this.repo_query.clone(),
            launch_blocked: this.disabled_reason(cx),
        })
    }

    /// EXP-1121: the not-ready (or open) render — the trigger wrapped in the
    /// "Ready to code?" popover.
    fn render_readiness(
        &mut self,
        readiness: &domain::coding_readiness::CodingReadiness,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let control = cx.entity();
        let open_control = control.clone();
        gpui_component::popover::Popover::new("start-coding-readiness-popover")
            .anchor(gpui::Anchor::TopRight)
            .p_0()
            .on_open_change(move |open, _window, cx| {
                let open = *open;
                open_control.update(cx, |this, cx| this.set_readiness_open(open, cx));
            })
            .trigger(crate::coding_readiness::ReadinessTrigger::new(
                readiness.caption.clone(),
            ))
            .content(move |state, window, cx| {
                crate::coding_readiness::render_popover(&control, state, window, cx)
            })
            .into_any_element()
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
                // EXP-1196: the tooltip IS the failing readiness row
                // (`Git · Not installed`, `Claude Code · Signed out`) when
                // the block has one.
                let row_reason = || {
                    hub.doctor
                        .device
                        .as_ref()
                        .and_then(|doctor| crate::device_readiness::no_agent_row(doctor, true))
                        .map(|row| SharedString::from(crate::device_readiness::summary(&row)))
                };
                if !report.git.ok {
                    return Some(row_reason().unwrap_or_else(|| {
                        report
                            .git
                            .error
                            .clone()
                            .unwrap_or_else(|| "git is not available".to_string())
                            .into()
                    }));
                }
                if !report.any_agent_ok() {
                    return Some(row_reason().unwrap_or_else(|| {
                        if report.unauthed_agents().is_empty() {
                            NO_AGENT_COPY.into()
                        } else {
                            NO_AGENT_SIGNED_IN_COPY.into()
                        }
                    }));
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

/// EXP-1121: THIS machine's name for the readiness device row — its synced
/// row's label, else "This device" (the heartbeat may not have landed).
fn local_device_label(cx: &mut App) -> String {
    const FALLBACK: &str = "This device";
    // The device id derives from the data dir; no auth context (headless
    // tests) = no id to look up.
    if !cx.has_global::<AuthContext>() {
        return FALLBACK.to_string();
    }
    let own = queries::own_device_id(cx);
    Store::try_global(cx)
        .and_then(|store| {
            store
                .collections()
                .devices
                .read(cx)
                .iter()
                .find(|row| row.device_id.as_deref() == Some(own.as_str()))
                .and_then(|row| row.label.clone())
        })
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.to_string())
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
    "No coding agent CLI found (claude or codex). Install one in Settings → Tools.";

/// EXP-409 variant: agents ARE installed, but every one of them is signed
/// out — the fix is a login, not an install.
pub(crate) const NO_AGENT_SIGNED_IN_COPY: &str =
    "No coding agent is signed in. Sign in to claude or codex (see Settings → Tools).";

/// `Some(reason)` when the doctor has REPORTED and no agent CLI is usable —
/// the shared gate for every Start-coding entry point (EXP-367: buttons
/// disable with this tooltip, never hide). `None` while the probe is still
/// running (never falsely block on a race) or before anything coding exists.
pub(crate) fn no_agent_reason(cx: &App) -> Option<SharedString> {
    let hub = CodingHub::global_ref(cx)?;
    // Borrowed, not cloned: list screens ask per render (EXP-832).
    let hub = hub.read(cx);
    let report = hub.doctor.report.as_ref()?;
    if report.any_agent_ok() {
        return None;
    }
    // EXP-1196: the failing readiness row as the tooltip, when the block has
    // one with a fix in reach (every agent simply missing keeps the copy).
    if let Some(row) = hub.doctor.device.as_ref().and_then(|doctor| {
        crate::device_readiness::no_agent_row(doctor, true)
            .filter(|row| row.key != coding::device_doctor::KEY_GIT)
    }) {
        return Some(crate::device_readiness::summary(&row).into());
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
        if self.issue_id.is_none() {
            return div().into_any_element();
        }
        // Lazy kicks: the hub (doctor) exists once anything coding renders;
        // the probe follows the current issue.
        let _ = CodingHub::global(cx);
        // EXP-1121: a repo linked/unlinked under the open issue re-probes.
        if !matches!(self.probe, RepoProbe::Idle) {
            let current = self
                .issue_id
                .as_deref()
                .and_then(|id| issue_board(id, cx))
                .and_then(|board| board.repository_id);
            if current != self.probed_repo_id {
                self.probe = RepoProbe::Idle;
            }
        }
        self.ensure_probe(cx);

        // EXP-1121: the readiness checklist. A repo-less board reads the
        // team's GitHub facts first (the caption names the FIRST missing
        // step); loading = inert; not ready (or the popover up) = the dashed
        // amber trigger + popover; ready = the launcher below.
        if let Some(subject) = self.readiness_subject(cx) {
            if subject.repository_id.is_none() && !self.github.covers(&subject.team_id) {
                self.fetch_github(subject.team_id.clone(), false, cx);
            }
        }
        let local = local_device_label(cx);
        if let Some((readiness, _)) = self.readiness(&local, cx) {
            if self.readiness_open || (!readiness.loading && !readiness.ready) {
                return self.render_readiness(&readiness, cx);
            }
            if readiness.loading {
                return crate::surface::glass_pill_button(
                    "start-coding",
                    crate::surface::PillSize::Sm,
                    cx,
                )
                .icon(
                    Icon::new(registry::ACTION_RUN)
                        .with_size(px(crate::surface::PillSize::Sm.glyph()))
                        .text_color(cx.theme().muted_foreground),
                )
                .label(domain::coding_readiness::copy::START)
                .disabled(true)
                .into_any_element();
            }
        }

        // EXP-877: this control is ONLY the launcher. A live run of mine puts
        // Stop in this slot and an ended resumable one Resume — the header
        // derives that from the run state (`work_header::coding_action`)
        // and never asks this control while it does.

        // EXP-417: the primary action of the header's agent row — a solid
        // content-sized button; the repo-less retry sits beside it as a
        // compact icon.
        // EXP-698: it sits INSIDE the property tray now, so it wears the ONE
        // capsule at the tray's own `Sm` rung (24px, same as the chips beside
        // it) in the primary paint — the single emphasised pill of the header.
        // EXP-760: demoted (a PR is open) it wears the plain glass pill
        // instead — Merge takes the white one.
        let disabled = self.disabled_reason(cx);
        let mut row = h_flex().gap_1().items_center();
        let glyph_color = if disabled.is_some() || self.demoted {
            cx.theme().muted_foreground
        } else {
            cx.theme().primary_foreground
        };
        let button = if self.demoted {
            crate::surface::glass_pill_button("start-coding", crate::surface::PillSize::Sm, cx)
        } else {
            crate::surface::glass_pill_button_primary(
                "start-coding",
                crate::surface::PillSize::Sm,
            )
        }
        // The solid variant carries the emphasis now — a green glyph on
        // the primary fill only muddies it.
        .icon(
            Icon::new(registry::ACTION_RUN)
                .with_size(px(crate::surface::PillSize::Sm.glyph()))
                .text_color(glyph_color),
        )
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

/// FEED-47/57 — the device holding a LIVE batch run that covers `issue_id`
/// (`batch_issue_ids`), off the synced rows. The per-issue probe
/// ([`queries::live_session_device_for_issue`]) only reads `issue_id`, and a
/// batch row has none, so an issue start beside a live batch on the same
/// issue passed it and put a second agent on the work.
pub(crate) fn live_batch_device_covering<'a>(
    sessions: impl Iterator<Item = &'a domain::rows::CodingSession>,
    devices: impl Iterator<Item = &'a domain::rows::DeviceRow>,
    issue_id: &str,
    now_epoch: i64,
) -> Option<String> {
    let session = sessions
        .filter(|session| session.issue_id.is_none())
        .filter(|session| {
            domain::batch_run::parse_batch_issue_ids(session.batch_issue_ids.as_ref())
                .iter()
                .any(|id| id == issue_id)
        })
        .find(|session| queries::coding_session_is_live(session, now_epoch))?;
    Some(
        queries::session_device_presentation(session, devices, now_epoch * 1_000)
            .label
            .unwrap_or_else(|| "another device".to_string()),
    )
}

/// [`live_batch_device_covering`] against the global store.
pub(crate) fn live_batch_device_for_issue(cx: &App, issue_id: &str, now_epoch: i64) -> Option<String> {
    let collections = Store::global(cx).collections();
    let sessions = collections.coding_sessions.read(cx);
    let devices = collections.devices.read(cx);
    live_batch_device_covering(sessions.iter(), devices.iter(), issue_id, now_epoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1088: agent-started runs never steal the window;
    /// person-started ones (incl. automations and a local resume, whose
    /// reason is `None`) still open.
    #[test]
    fn only_person_started_runs_navigate() {
        assert!(should_open_session(None));
        assert!(should_open_session(Some("schedule")));
        assert!(should_open_session(Some("event")));
        assert!(!should_open_session(Some("agent")));
    }

    /// FEED-47/57: a live batch covering the issue holds it; an ended
    /// batch, an issue row and a batch over other issues do not.
    #[test]
    fn a_live_batch_covering_the_issue_holds_it() {
        let now = chrono::Utc::now();
        let fresh = now.to_rfc3339();
        let row = |id: &str, status: &str, issue: Option<&str>, batch: &[&str]| {
            serde_json::from_value::<domain::rows::CodingSession>(serde_json::json!({
                "id": id, "team_id": "t-1", "status": status,
                "issue_id": issue, "batch_issue_ids": batch,
                "device_label": "Studio", "updated_at": fresh,
            }))
            .unwrap()
        };
        let rows = vec![
            row("s-ended", "ended", None, &["i-1"]),
            row("s-issue", "running", Some("i-9"), &[]),
            row("s-other", "running", None, &["i-2", "i-3"]),
        ];
        let epoch = now.timestamp();
        assert_eq!(live_batch_device_covering(rows.iter(), std::iter::empty(), "i-1", epoch), None);
        let mut rows = rows;
        rows.push(row("s-live", "running", None, &["i-4", "i-1"]));
        assert_eq!(
            live_batch_device_covering(rows.iter(), std::iter::empty(), "i-1", epoch),
            Some("Studio".to_string())
        );
    }

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

    /// EXP-758: ⌘Q waits for the ACP engines it just killed, on ONE shared
    /// budget, and never skips a wait outright, or the codex children the
    /// wait exists for are orphaned exactly as before.
    #[test]
    fn each_engine_waits_on_the_shared_quit_deadline() {
        let now = std::time::Instant::now();
        let deadline = now + QUIT_END_TIMEOUT;
        // The first engine gets the whole budget.
        assert_eq!(quit_engine_wait(deadline, now), QUIT_END_TIMEOUT);
        // A later one gets what is left of it, not a fresh copy.
        let spent = now + std::time::Duration::from_millis(1_500);
        assert_eq!(
            quit_engine_wait(deadline, spent),
            std::time::Duration::from_millis(500)
        );
        // An exhausted budget still leaves the floor.
        assert_eq!(
            quit_engine_wait(deadline, deadline),
            QUIT_ENGINE_MIN_WAIT
        );
        assert_eq!(
            quit_engine_wait(deadline, deadline + std::time::Duration::from_secs(10)),
            QUIT_ENGINE_MIN_WAIT
        );
        // …and a remainder under the floor is raised to it.
        let nearly_spent = deadline - std::time::Duration::from_millis(5);
        assert_eq!(
            quit_engine_wait(deadline, nearly_spent),
            QUIT_ENGINE_MIN_WAIT
        );
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

    // ── EXP-818: following a remote start into its run ────────────────────

    /// The subject a remote start waits for: a resume by `resumed_from_id`, a
    /// single issue by its id, everything else by "any new row on that
    /// machine" — and the row matcher agrees with each.
    #[test]
    fn a_remote_start_waits_for_its_own_row() {
        use api::steer::StartSessionInput;
        let row = |issue_id: Option<&str>, resumed_from: Option<&str>| {
            domain::rows::CodingSession {
                id: "s1".to_string(),
                issue_id: issue_id.map(str::to_string),
                resumed_from_id: resumed_from.map(str::to_string),
                ..sample_session()
            }
        };
        let resume = StartSessionInput {
            resume_session_id: Some("old".to_string()),
            device_id: "dev".to_string(),
            ..Default::default()
        };
        assert_eq!(
            RemoteRunSubject::of(&resume),
            RemoteRunSubject::Resume("old".to_string())
        );
        assert!(RemoteRunSubject::of(&resume).matches(&row(None, Some("old"))));
        assert!(!RemoteRunSubject::of(&resume).matches(&row(None, Some("other"))));

        let issue = StartSessionInput {
            issue_id: Some("i1".to_string()),
            device_id: "dev".to_string(),
            ..Default::default()
        };
        assert_eq!(
            RemoteRunSubject::of(&issue),
            RemoteRunSubject::Issue("i1".to_string())
        );
        assert!(RemoteRunSubject::of(&issue).matches(&row(Some("i1"), None)));
        assert!(!RemoteRunSubject::of(&issue).matches(&row(Some("i2"), None)));

        // A batch / action / chat start: the row says nothing we could match,
        // so any new row on the target machine is it.
        let batch = StartSessionInput {
            issue_ids: Some(vec!["i1".to_string(), "i2".to_string()]),
            device_id: "dev".to_string(),
            ..Default::default()
        };
        assert_eq!(RemoteRunSubject::of(&batch), RemoteRunSubject::Any);
        assert!(RemoteRunSubject::of(&batch).matches(&row(None, None)));
    }

    /// A bare row the subject matcher can vary one field of.
    fn sample_session() -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({ "id": "s1" }))
            .expect("a row needs nothing but an id")
    }
}
