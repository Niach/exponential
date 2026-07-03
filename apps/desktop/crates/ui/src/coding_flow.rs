//! UI-side Start-coding orchestration (masterplan-v3 §7.1/§7.2/§7.7, EXP-4,
//! EXP-2a/b) — everything between the issue-detail header's **Start coding**
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
//!   §7 reasons (EXP-4: never a false "not connected", never an unexplained
//!   block). Click → [`launch`]: `prepare_launch` on the background executor
//!   → `spawn_prepared_with` into THIS window's bottom terminal dock.
//!
//! The relay-origin `start_session` path (§08) is the SAME sequence — its
//! control channel builds the same [`build_launch`] input and calls the same
//! `coding::prepare_launch`/`spawn_prepared_with`; only the `LaunchOrigin`
//! differs (§7.1: "there is no second, divergent remote-start
//! implementation").

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    notification::Notification,
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use gpui_component::dock::DockItem;
use sync::Store;
use terminal::{TabId, TerminalManager, TerminalManagerEvent};

use coding::{
    run_doctor, CodingDeps, DisabledReason, DoctorReport, IssueSeed, LaunchOrigin, LaunchOutcome,
    LaunchRequest, Prepared, Settings,
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

/// One locally running coding session (a `coding_sessions` row whose child
/// lives in one of OUR terminal docks).
pub struct LocalCodingSession {
    pub session_id: String,
    pub issue_id: String,
    pub tab: TabId,
    pub manager: WeakEntity<TerminalManager>,
}

/// Issue-keyed registry of local sessions. An entity (not a bare global) so
/// the header affordances can `cx.observe` it for the play↔stop flip.
#[derive(Default)]
pub struct LocalSessions {
    by_issue: HashMap<String, LocalCodingSession>,
    /// Keeps the per-session `TabClosed` watchers alive (keyed like
    /// `by_issue`; dropped with the entry).
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
    /// coding it (reverse of the issue-keyed map — the §8.5 banner resolves a
    /// dock tab back to its steer session).
    pub fn session_id_for_tab(&self, tab: TabId) -> Option<&str> {
        self.by_issue
            .values()
            .find(|session| session.tab == tab)
            .map(|session| session.session_id.as_str())
    }

    fn remove(sessions: &Entity<LocalSessions>, issue_id: &str, cx: &mut App) {
        sessions.update(cx, |this, cx| {
            this.by_issue.remove(issue_id);
            this.watchers.remove(issue_id);
            cx.notify();
        });
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
        let issue_id = session.issue_id.clone();
        let watcher = session.manager.upgrade().map(|manager| {
            let sessions = sessions.downgrade();
            let watch_issue = issue_id.clone();
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
                    LocalSessions::remove(&sessions, &watch_issue, cx);
                }
            })
        });
        sessions.update(cx, |this, cx| {
            if let Some(watcher) = watcher {
                this.watchers.insert(issue_id.clone(), watcher);
            }
            this.by_issue.insert(issue_id, session);
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

/// Everything `coding::prepare_launch` needs, assembled from the signed-in
/// app state. `None` when signed out or the issue isn't synced (both make
/// Start coding meaningless). Shared by the local button and — via the same
/// construction — the §08 relay `start_session` path.
pub fn build_launch(
    issue_id: &str,
    origin: LaunchOrigin,
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

/// Foreground half of the launch: spawn the prepared Claude tab into THIS
/// window's dock, register the local session (play→stop), and hook the exit
/// edge to clear it again. A spawn failure never strands the row —
/// `spawn_prepared_with` already ends it.
pub fn spawn_into_window(
    prepared: coding::PreparedLaunch,
    issue_id: String,
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

    let sessions = LocalSessions::global(cx);
    let notify_sessions = sessions.downgrade();
    let notify_issue = issue_id.clone();
    let exit_notify: coding::ExitNotify = Box::new(move |cx: &mut App| {
        if let Some(sessions) = notify_sessions.upgrade() {
            LocalSessions::remove(&sessions, &notify_issue, cx);
        }
    });

    match coding::spawn_prepared_with(prepared, &manager, cx, Arc::clone(&trpc), Some(exit_notify))
    {
        Ok(LaunchOutcome::Spawned { session_id, terminal_tab, .. }) => {
            // §08 steer publisher attach — tee this session's PTY out to the
            // relay for phone steering. Best-effort: a no-op when steer is
            // disabled/unreachable or the account is signed out. This is the
            // single hookup the §08 wiring owns (`ui::steer_wiring`).
            crate::steer_wiring::attach_publisher(
                &session_id,
                &issue_id,
                terminal_tab,
                &manager,
                cx,
            );
            LocalSessions::insert(
                &sessions,
                LocalCodingSession {
                    session_id,
                    issue_id,
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
    /// `Ready(None)` = no repository linked (the EXP-4 disabled state).
    Ready(Option<api::repositories::IssueRepository>),
    /// Transport failure — the button stays CLICKABLE (EXP-4: a transient
    /// network error must never falsely block; the launch re-checks anyway).
    Error(String),
}

/// The Start-coding button + stop affordance. One per issue-detail view;
/// `set_issue` follows navigation.
pub struct StartCodingControl {
    issue_id: Option<String>,
    probe: RepoProbe,
    probe_generation: u64,
    launching: bool,
    /// A launch-time `DisabledReason` (GithubAppMissing / SessionLimit /
    /// TokenDenied / doctor) — rendered with the exact §7 copy + a retry.
    runtime_disabled: Option<DisabledReason>,
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
            launching: false,
            runtime_disabled: None,
            _subscriptions: subscriptions,
        }
    }

    /// Point the control at an issue (navigation edge). Resets the probe and
    /// any launch-time disabled reason.
    pub fn set_issue(&mut self, issue_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.probe = RepoProbe::Idle;
        self.runtime_disabled = None;
        self.launching = false;
        cx.notify();
    }

    /// Kick the `repositories.forIssue` probe when idle (render-time, like
    /// the repositories pane — a hidden control never fetches). The button is
    /// driven by LIVE server state, never a cached local flag (EXP-4).
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

    /// Re-probe + clear a launch-time disabled reason (the tiny retry next to
    /// a disabled state — a SessionLimit or App install can resolve without
    /// navigating away).
    fn retry(&mut self, cx: &mut gpui::Context<Self>) {
        self.runtime_disabled = None;
        self.probe = RepoProbe::Idle;
        CodingHub::refresh_doctor(&CodingHub::global(cx), cx);
        cx.notify();
    }

    /// The click: §7.1 steps 1–6 on the background executor, then steps 7–8
    /// into this window's dock on the foreground.
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launching {
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let Some((request, deps)) = build_launch(&issue_id, LaunchOrigin::Local, cx) else {
            window.push_notification(
                Notification::warning("Sign in and wait for sync before starting a session."),
                cx,
            );
            return;
        };
        self.launching = true;
        self.runtime_disabled = None;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { coding::prepare_launch(&request, &deps) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.launching = false;
                match prepared {
                    Ok(Prepared::Ready(prepared)) => {
                        if let Err(message) = spawn_into_window(prepared, issue_id, window, cx) {
                            window.push_notification(Notification::error(message), cx);
                        }
                    }
                    Ok(Prepared::Disabled(reason)) => {
                        // EXP-4: explain, never crash — exact §7 copy.
                        window.push_notification(
                            Notification::warning(reason.message()),
                            cx,
                        );
                        this.runtime_disabled = Some(reason);
                    }
                    Err(err) => {
                        window.push_notification(
                            Notification::error(format!(
                                "Could not start the coding session: {err}"
                            )),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
        if let Some(reason) = &self.runtime_disabled {
            return Some(reason.message().into());
        }
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
            // EXP-4: a probe transport error never falsely blocks — the
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
        // Lazy kicks: the hub (doctor) exists once anything coding renders;
        // the probe follows the current issue.
        let _ = CodingHub::global(cx);
        self.ensure_probe(cx);

        // Local session running → "Coding…" + the play button becomes STOP.
        let local = LocalSessions::global(cx)
            .read(cx)
            .get(&issue_id)
            .is_some();
        if local {
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

        if self.launching {
            return Button::new("start-coding")
                .ghost()
                .xsmall()
                .label("Starting…")
                .loading(true)
                .disabled(true)
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
                // EXP-4: the disabled state ALWAYS explains itself — the
                // exact §7 copy rides the tooltip; retry re-probes.
                row = row.child(button.disabled(true).tooltip(reason));
                if self.runtime_disabled.is_some()
                    || matches!(self.probe, RepoProbe::Ready(None))
                {
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
                // A probe transport error stays clickable (EXP-4: never
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
