//! UI-side Start-coding orchestration (masterplan-v3 §7.1/§7.2/§7.7) —
//! everything between the issue-detail header's **Start coding**
//! affordance and the `coding` crate's one launch entry point.
//!
//! Three shared pieces live here:
//!
//! - [`CodingHub`] — the app-global coding state: the §7.7 settings
//!   (claude path / repos root / branch prefix, file-persisted, per-install)
//!   and the tooling-doctor report. The settings pane edits through it; the
//!   Start-coding button and the launcher read from it. The doctor runs on
//!   the background executor (it spawns `claude --version` / `git --version`)
//!   and re-runs whenever the settings change.
//! - [`LocalSessions`] — the sessions THIS process launched (issue →
//!   `{session_id, tab, manager}`). Drives the §7.5 play↔stop flip: while an
//!   issue has a local session, the header shows a "Coding…" indicator and a
//!   **stop** affordance (kill the child → the exit hook fires the idempotent
//!   `codingSessions.end`). A manually closed tab is also caught
//!   (`TabClosed`) and ends the row best-effort — the "coding now" badge must
//!   never ghost.
//! - [`StartCodingControl`] — the header affordance itself. Enabled iff
//!   `repositories.forIssue` resolves non-null AND the doctor is green
//!   (BOTH `claude` and `git`, §7.1 step 1); disabled states carry the EXACT
//!   §7 reasons (never a false "not connected", never an unexplained
//!   block). Click → the shared Start-coding dialog
//!   (`crate::start_coding_dialog`), which owns the model/effort/plan-mode
//!   choices and the prepare→spawn task.
//!
//! The relay-origin `start_session` path (§08) is the SAME sequence — its
//! control channel builds the same [`build_launch`] input and calls the same
//! `coding::prepare`/`spawn_prepared_with`; only the `LaunchOrigin`
//! differs (§7.1: "there is no second, divergent remote-start
//! implementation").

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_component::dock::DockItem;
use sync::Store;
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::{
    run_doctor, CodingDeps, DoctorReport, IssueSeed, LaunchOptions, LaunchOrigin, LaunchOutcome,
    LaunchRequest, Settings,
};

use crate::queries;
use crate::session::AuthContext;
use crate::terminal_dock::TerminalDockPanel;
use crate::workspace::Workspace;

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
            hub.update(cx, |this, cx| {
                if this.doctor.generation != generation {
                    return; // superseded
                }
                this.doctor.running = false;
                this.doctor.report = Some(report);
                cx.notify();
            });
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
        let result = hub.update(cx, |this, cx| {
            let result = settings
                .save(&this.settings_path)
                .map_err(|err| format!("Could not save settings: {err}"));
            this.settings = settings;
            cx.notify();
            result
        });
        Self::refresh_doctor(hub, cx);
        result
    }

    /// The §7.1-step-1 gate half the button ANDs in: both tools green.
    pub fn doctor_ok(&self) -> bool {
        self.doctor.report.as_ref().is_some_and(DoctorReport::ok)
    }
}

// ---------------------------------------------------------------------------
// LocalSessions — the sessions THIS process launched (§7.5 play↔stop)
// ---------------------------------------------------------------------------

/// What a local coding session works on: one issue (§7.1) or a multi-issue
/// batch (one session per batch run, keyed by its batch id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionSubject {
    Issue(String),
    Batch(String),
}

/// One locally running coding session (a `coding_sessions` row whose child
/// lives in one of OUR terminal docks).
pub struct LocalCodingSession {
    pub session_id: String,
    pub subject: SessionSubject,
    /// The shared clone the session's worktree hangs off — releases its P9
    /// token-refresher hold when the session ends.
    pub clone: PathBuf,
    pub tab: TabId,
    pub manager: WeakEntity<TerminalManager>,
}

/// Subject-keyed registry of local sessions. An entity (not a bare global) so
/// the header affordances can `cx.observe` it for the play↔stop flip.
#[derive(Default)]
pub struct LocalSessions {
    by_issue: HashMap<String, LocalCodingSession>,
    /// Multi-issue batch sessions, keyed by batch id.
    by_batch: HashMap<String, LocalCodingSession>,
    /// Keeps the per-session `TabClosed` watchers alive (keyed by session id;
    /// dropped with the entry).
    watchers: HashMap<String, Subscription>,
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

    /// The coding session id whose terminal tab is `tab`, if this process is
    /// coding it (reverse of the subject-keyed maps — the §8.5 banner resolves
    /// a dock tab back to its steer session).
    pub fn session_id_for_tab(&self, tab: TabId) -> Option<&str> {
        self.by_issue
            .values()
            .chain(self.by_batch.values())
            .find(|session| session.tab == tab)
            .map(|session| session.session_id.as_str())
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
            };
            if let Some(entry) = &entry {
                this.watchers.remove(&entry.session_id);
            }
            cx.notify();
            entry
        });
        if let Some(entry) = removed {
            TokenRefreshers::release(&entry.clone, cx);
        }
    }

    /// Track a freshly spawned session. Also watches the manager for a
    /// manual `TabClosed` on our tab: closing a running Claude tab kills the
    /// child without its exit hook ever firing (the tab's subscription dies
    /// with it), so the watcher ends the row best-effort here — the synced
    /// "coding now" badge must never ghost (§7.1 step 8's intent).
    fn insert(
        sessions: &Entity<LocalSessions>,
        session: LocalCodingSession,
        trpc: Arc<api::TrpcClient>,
        cx: &mut App,
    ) {
        let subject = session.subject.clone();
        let session_key = session.session_id.clone();
        let watcher = session.manager.upgrade().map(|manager| {
            let sessions = sessions.downgrade();
            let watch_subject = subject.clone();
            let watch_tab = session.tab;
            let session_id = session.session_id.clone();
            cx.subscribe(&manager, move |_, event: &TerminalManagerEvent, cx| {
                if *event != TerminalManagerEvent::TabClosed(watch_tab) {
                    return;
                }
                // End the row off the foreground (idempotent server-side —
                // a normal exit already ended it before the close).
                let trpc = Arc::clone(&trpc);
                let session_id = session_id.clone();
                std::thread::spawn(move || {
                    coding::end_session_best_effort(&trpc, &session_id);
                });
                if let Some(sessions) = sessions.upgrade() {
                    LocalSessions::remove(&sessions, &watch_subject, cx);
                }
            })
        });
        sessions.update(cx, |this, cx| {
            if let Some(watcher) = watcher {
                this.watchers.insert(session_key, watcher);
            }
            match &subject {
                SessionSubject::Issue(id) => {
                    this.by_issue.insert(id.clone(), session);
                }
                SessionSubject::Batch(id) => {
                    this.by_batch.insert(id.clone(), session);
                }
            }
            cx.notify();
        });
    }
}

/// The local session for `issue_id`, if this process is coding it right now.
pub fn local_session_for<'a>(
    sessions: &'a LocalSessions,
    issue_id: &str,
) -> Option<&'a LocalCodingSession> {
    sessions.get(issue_id)
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

/// The synced project row backing `issue_id`, if both are in the collections.
/// Used by the header affordance to decide whether Start coding even applies
/// (a repo-less non-dev board never codes) and by the §P7 activity gating.
pub(crate) fn issue_project(issue_id: &str, cx: &App) -> Option<domain::rows::Project> {
    let store = Store::global(cx);
    let project_id = store
        .collections()
        .issues
        .read(cx)
        .get(issue_id)
        .map(|issue| issue.project_id.clone())?;
    store
        .collections()
        .projects
        .read(cx)
        .get(&project_id)
        .cloned()
}

// ---------------------------------------------------------------------------
// Window plumbing — this window's TerminalManager (§06 dock)
// ---------------------------------------------------------------------------

/// Resolve THIS window's bottom terminal dock manager: `Root` → [`Workspace`]
/// → `DockArea` → bottom `Dock` → the registered [`TerminalDockPanel`].
/// `None` on non-workspace windows (login) — the caller surfaces an error.
pub fn window_terminal_manager(window: &Window, cx: &App) -> Option<Entity<TerminalManager>> {
    let root = window.root::<gpui_component::Root>().flatten()?;
    let workspace = root
        .read(cx)
        .view()
        .clone()
        .downcast::<Workspace>()
        .ok()?;
    let dock_area = workspace.read(cx).dock_area().clone();
    let bottom = dock_area.read(cx).bottom_dock()?.clone();
    let panel = find_terminal_dock(bottom.read(cx).panel())?;
    Some(panel.read(cx).manager().clone())
}

/// Walk a `DockItem` tree for the terminal dock panel (the bottom dock is a
/// single `Tabs` today, but a user-rearranged layout may nest it in splits).
fn find_terminal_dock(item: &DockItem) -> Option<Entity<TerminalDockPanel>> {
    match item {
        DockItem::Tabs { items, .. } => items
            .iter()
            .find_map(|panel| panel.view().downcast::<TerminalDockPanel>().ok()),
        DockItem::Panel { view, .. } => view.view().downcast::<TerminalDockPanel>().ok(),
        DockItem::Split { items, .. } => items.iter().find_map(find_terminal_dock),
        // Tiles never host the terminal dock (workspace layout never creates
        // them); skipping is safe — the caller degrades to an error surface.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Launch orchestration (§7.1 — the ONE sequence, UI side)
// ---------------------------------------------------------------------------

/// Everything `coding::prepare` needs for an ISSUE launch, assembled from the
/// signed-in app state. `None` when signed out or the issue isn't synced
/// (both make Start coding meaningless). Shared by the Start-coding dialog
/// and — via the same construction — the §08 relay `start_session` path
/// (which passes settings-default `options` with plan mode forced OFF).
pub fn build_launch(
    issue_id: &str,
    origin: LaunchOrigin,
    options: LaunchOptions,
    cx: &mut App,
) -> Option<(LaunchRequest, CodingDeps)> {
    let account = queries::active_account(cx)?;
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let data_dir = cx
        .try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .unwrap_or_else(api::default_data_dir);
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
        device_label: coding::default_device_label(),
        origin,
        options,
    };
    let deps = CodingDeps {
        trpc,
        token_store: Arc::new(api::token_store::TokenStore::new(data_dir)),
        account_id: account.id,
        settings,
        issue_seed: Arc::new(move |_| Some(seed.clone())),
        worktrees: Arc::new(coding::GitWorktrees),
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
    let data_dir = cx
        .try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .unwrap_or_else(api::default_data_dir);
    let hub = CodingHub::global(cx);
    let settings = hub.read(cx).settings.clone();
    Some(CodingDeps {
        trpc,
        token_store: Arc::new(api::token_store::TokenStore::new(data_dir)),
        account_id: account.id,
        settings,
        issue_seed: Arc::new(|_| None),
        worktrees: Arc::new(coding::GitWorktrees),
    })
}

/// Foreground half of the launch: spawn the prepared Claude tab into THIS
/// window's dock, register the local session (play→stop), and hook the exit
/// edge to clear it again. Shared by the single-issue and batch paths — only
/// the [`SessionSubject`] differs. A spawn failure never strands the row —
/// `spawn_prepared_with` already ends it.
pub fn spawn_into_window(
    prepared: coding::PreparedLaunch,
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

    let sessions = LocalSessions::global(cx);
    let notify_sessions = sessions.downgrade();
    let notify_subject = subject.clone();
    let exit_notify: coding::ExitNotify = Box::new(move |cx: &mut App| {
        if let Some(sessions) = notify_sessions.upgrade() {
            LocalSessions::remove(&sessions, &notify_subject, cx);
        }
    });

    match coding::spawn_prepared_with(prepared, &manager, cx, Arc::clone(&trpc), Some(exit_notify))
    {
        Ok(LaunchOutcome::Spawned { session_id, terminal_tab, worktree, .. }) => {
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
                cx,
            );
            // P9: keep the clone's embedded token fresh for the session's
            // life (released via LocalSessions::remove on either exit path).
            TokenRefreshers::retain(&clone, &repository_id, cx);
            LocalSessions::insert(
                &sessions,
                LocalCodingSession {
                    session_id,
                    subject,
                    clone,
                    tab: terminal_tab,
                    manager: manager.downgrade(),
                },
                trpc,
                cx,
            );
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
        // the enabled state — re-render whenever either moves.
        let hub = CodingHub::global(cx);
        let sessions = LocalSessions::global(cx);
        let subscriptions = vec![
            cx.observe(&hub, |_, _, cx| cx.notify()),
            cx.observe(&sessions, |_, _, cx| cx.notify()),
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

    /// The stop affordance (§7.5): kill this issue's local child; the exit
    /// hook then fires the idempotent `codingSessions.end` and clears the
    /// registry — stop is a kill, the bookkeeping rides the exit edge.
    fn stop(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.as_deref() else {
            return;
        };
        let sessions = LocalSessions::global(cx);
        let handle = sessions.read(cx).get(issue_id).and_then(|session| {
            session
                .manager
                .upgrade()
                .map(|manager| (manager, session.tab))
        });
        let Some((manager, tab)) = handle else {
            return;
        };
        if let Some(tab) = manager.read(cx).tab(tab) {
            tab.view.read(cx).session().borrow().kill();
        }
    }

    /// The disabled reason right now, `None` when the button may launch
    /// (§7.1 step 1: repo non-null AND doctor green — BOTH tools).
    fn disabled_reason(&self, cx: &App) -> Option<SharedString> {
        let hub = CodingHub::global_ref(cx)?;
        let hub = hub.read(cx);
        match hub.doctor.report.as_ref() {
            None => return Some("Checking local tools…".into()),
            Some(report) => {
                if let Some(failed) = report.first_failure() {
                    // The §7.7 actionable copy ("claude not found on PATH —
                    // set an absolute path" / "git not found on PATH").
                    return Some(
                        failed
                            .error
                            .clone()
                            .unwrap_or_else(|| format!("{} is not available", failed.tool))
                            .into(),
                    );
                }
            }
        }
        match &self.probe {
            RepoProbe::Idle | RepoProbe::Loading => Some("Checking linked repository…".into()),
            // §7.1's exact helper copy for the repo-less state.
            RepoProbe::Ready(None) => {
                Some("Link a repository to this project in workspace settings.".into())
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
    fn global_ref(cx: &App) -> Option<Entity<CodingHub>> {
        cx.try_global::<CodingHubGlobal>().map(|g| g.0.clone())
    }
}

impl Render for StartCodingControl {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(issue_id) = self.issue_id.clone() else {
            return div().into_any_element();
        };
        // A repo-less non-dev board (a task/feedback project that never runs
        // coding sessions) shows NO Start-coding affordance — not even the
        // disabled "link a repository" nudge, which only makes sense for dev
        // projects (they REQUIRE a repo). A non-dev board WITH a repo (the
        // dogfood feedback board) keeps the button: coding gates on repo
        // presence, not type. Hidden here before the probe so it never fetches.
        let project = issue_project(&issue_id, cx);
        if let Some(project) = &project {
            if !project.is_dev() && project.repository_id.is_none() {
                return div().into_any_element();
            }
        }
        // Lazy kicks: the hub (doctor) exists once anything coding renders;
        // the probe follows the current issue.
        let _ = CodingHub::global(cx);
        self.ensure_probe(cx);

        // Local session running → "Coding…" + the play button becomes STOP.
        let running = LocalSessions::global(cx).read(cx).get(&issue_id).is_some();
        if running {
            return h_flex()
                .flex_shrink_0()
                .gap_1()
                .items_center()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            div()
                                .size_1p5()
                                .rounded_full()
                                .bg(theme::tokens::GREEN.to_hsla()),
                        )
                        .child("Coding…"),
                )
                .child(
                    Button::new("stop-coding")
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(IconName::CircleX).text_color(cx.theme().danger))
                        .label("Stop")
                        .tooltip("Stop the coding session (ends it for every client)")
                        .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
                )
                .into_any_element();
        }

        let disabled = self.disabled_reason(cx);
        let mut row = h_flex().flex_shrink_0().gap_0p5().items_center();
        let button = Button::new("start-coding")
            .ghost()
            .xsmall()
            .icon(Icon::new(IconName::Play).text_color(if disabled.is_some() {
                cx.theme().muted_foreground
            } else {
                theme::tokens::GREEN.to_hsla()
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
                            .xsmall()
                            .icon(
                                Icon::new(IconName::Undo2)
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
                        "Couldn't check the linked repository ({err}) — starting will retry."
                    )
                    .into(),
                    _ => "Clone the linked repository and start Claude on this issue".into(),
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
