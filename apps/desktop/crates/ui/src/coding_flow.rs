//! UI-side coding orchestration (masterplan-v3 §7.1/§7.2/§7.7) — the app
//! state every coding surface reads, and the host half of a session this
//! machine runs.
//!
//! Three shared pieces live here:
//!
//! - [`CodingHub`] — the app-global coding state: the §7.7 settings
//!   (per-agent path/model/effort + repos root / branch prefix,
//!   file-persisted, per-install) and the tooling-doctor report. The settings
//!   pane edits through it; the Start button and the launcher read from it.
//!   The doctor runs on the background executor (it probes every agent CLI —
//!   claude/codex — plus `git`, EXP-201) and re-runs whenever the settings
//!   change.
//! - [`LocalSessions`] — the sessions THIS process hosts, keyed by their
//!   `sessions` row id (EXP-1294). [`host_prepared`] registers one when the
//!   pickup ([`crate::session_pickup`]) launches a queued row; the engine's
//!   exit edge drops it. EXP-105: app quit ends every hosted session
//!   ([`install_quit_hook`]).
//! - [`StartCodingControl`] — the issue header's ONE Start button (EXP-1294).
//!   Enabled iff `repositories.forIssue` resolves non-null AND the doctor is
//!   green; disabled states carry the EXACT §7 reasons. Click → the composer
//!   dialog seeded with the issue (`crate::chat_screen`), whose send is ONE
//!   `sessions.send`.
//!
//! EXP-1294: there is no local launch path any more. Every start is a
//! `sessions.send` row; the hosting device picks it up from its shapes
//! (`coding::pickup`) and [`host_prepared`] is the engine start plus its
//! registration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{
    div, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex,
    input::InputState,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;
use terminal::TerminalManager;

use coding::{run_doctor, CodingDeps, DoctorReport, Settings};

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
        let fetch_settings = settings.clone();
        let fetch_dir = data_dir.clone();
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
                // EXP-1232: a stored Codex login with no managed build in
                // place → fetch it now; the row reads Downloading… until the
                // re-run below lands.
                if coding::managed_codex::should_fetch(&fetch_settings, &fetch_dir) {
                    let _ = cx.update(|cx| start_codex_fetch(&hub, fetch_dir.clone(), cx));
                }
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
// LocalSessions — the sessions THIS process hosts
// ---------------------------------------------------------------------------

/// WHERE a locally hosted session's agent actually lives (EXP-746): an
/// in-process [`engine::EngineSession`] rendered by the session screen.
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

/// One session this process hosts (a `sessions` row whose agent runs in
/// our in-process ACP engine).
pub struct LocalSession {
    pub session_id: String,
    /// EXP-1294: the issues linked to the session's root when it launched —
    /// the one-session-per-issue guard and the header's live state read it.
    pub issue_ids: Vec<String>,
    /// The shared clone the session's worktree hangs off — releases its P9
    /// token-refresher hold when the session ends.
    pub clone: PathBuf,
    /// The branch the session's worktree is on — the EXP-102 sweep/delete
    /// guard key. Empty for a scratch run.
    pub branch: String,
    /// EXP-746: the ACP engine hosting the agent.
    pub host: LocalSessionHost,
    /// EXP-484: the agent CLI this session runs.
    pub agent: coding::CodingAgent,
    /// EXP-688: the session's worktree.
    pub worktree: PathBuf,
    /// EXP-637: the run's own worktree, auto-removed at exit when it is
    /// clean and carries no commits.
    pub run_cleanup: Option<coding::RunCleanup>,
}

/// Session-keyed registry of local sessions. An entity (not a bare global)
/// so the header affordances can `cx.observe` it.
#[derive(Default)]
pub struct LocalSessions {
    by_session: HashMap<String, LocalSession>,
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

    /// The live local session working on `issue_id` (its root links it).
    pub fn get(&self, issue_id: &str) -> Option<&LocalSession> {
        self.all()
            .find(|session| session.issue_ids.iter().any(|id| id == issue_id))
    }

    /// EXP-849: the live local session with this `sessions` row id.
    pub fn session_for_id(&self, session_id: &str) -> Option<&LocalSession> {
        self.by_session.get(session_id)
    }

    /// Every live local session.
    fn all(&self) -> impl Iterator<Item = &LocalSession> {
        self.by_session.values()
    }

    /// Every branch a live local session holds — the prune keep-set feed
    /// (EXP-465). Scratch runs carry an empty branch and are skipped.
    pub fn held_branches(&self) -> impl Iterator<Item = &str> {
        self.all()
            .map(|session| session.branch.as_str())
            .filter(|branch| !branch.is_empty())
    }

    /// ONE live local session holding `branch`, arbitrarily chosen when
    /// several do. Only for callers that ask "is this branch held at all"
    /// (the EXP-102 clone sweep/delete guard).
    pub fn session_on_branch(&self, branch: &str) -> Option<&LocalSession> {
        self.all().find(|session| holds_branch(&session.branch, branch))
    }

    /// The live local session with this `sessions` row id (EXP-686).
    pub fn session_by_id(&self, session_id: &str) -> Option<&LocalSession> {
        self.by_session.get(session_id)
    }

    /// Every live local session's row id — the EXP-105 quit-time sweep input,
    /// the EXP-229 reconcile skip-set, and the sign-out sweep input.
    pub(crate) fn session_ids(&self) -> Vec<String> {
        self.by_session.keys().cloned().collect()
    }

    /// EXP-746: every in-process ACP engine this app hosts — the app-quit
    /// sweep's kill list.
    pub(crate) fn acp_hosts(&self) -> Vec<engine::EngineSession> {
        self.all()
            .map(|session| session.host.session.clone())
            .collect()
    }

    /// Drop `session_id` and release its P9 token-refresher hold. The engine's
    /// exit edge lands here exactly once.
    ///
    /// EXP-1292: a SUSPENDED run (an account switch re-spawns the same row
    /// right after) is no finished run: its entry goes, but its worktree or
    /// scratch dir is what the resume re-enters, so nothing is reclaimed —
    /// and an entry the resume already re-registered (a live engine) stays.
    fn remove(sessions: &Entity<LocalSessions>, session_id: &str, suspended: bool, cx: &mut App) {
        let (removed, live) = sessions.update(cx, |this, cx| {
            let relaunched = this
                .by_session
                .get(session_id)
                .is_some_and(|entry| !entry.host.session.is_done());
            let entry = if relaunched { None } else { this.by_session.remove(session_id) };
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
            // EXP-637 → NEW-5: the run's own worktree ends
            // ([`coding::finish_session_worktree`]): removed, its unpushed
            // work parked on `origin/exp/leftover-<id8>` and named in the
            // session's conversation. Background (git + network).
            let data_dir = coding_data_dir(cx);
            if suspended {
                log::info!("[ui] session {session_id} suspended: its workspace stays for the resume");
            } else if let Some(cleanup) = entry.run_cleanup.clone() {
                let session_id = entry.session_id.clone();
                let trpc = crate::queries::trpc_client(cx);
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
                                match &trpc {
                                    Some(trpc) => coding::finish_session_worktree(
                                        trpc,
                                        &session_id,
                                        &cleanup,
                                        &live,
                                    ),
                                    // Signed out meanwhile: no line to post.
                                    None => coding::finish_worktree(
                                        &cleanup,
                                        &live,
                                        &coding::session_id8(&session_id),
                                    ),
                                }
                            }
                        })
                        .await;
                    let _ = cx.update(|cx| {
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
                // scratch dir, claude trust entries, run record, steer
                // journal.
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
            // dropped here — only the session's RESOLVED end removes it
            // (`session_registry`).
        }
    }

    /// Track a freshly spawned session.
    fn insert(sessions: &Entity<LocalSessions>, session: LocalSession, cx: &mut App) {
        // EXP-229: persist the row id so a crash / forced logout / failed
        // quit-time end can be reconciled on the next launch.
        if let (Some(auth), Some(account)) = (
            cx.try_global::<AuthContext>().cloned(),
            crate::queries::active_account(cx),
        ) {
            crate::session_registry::record(&auth.data_dir, &session.session_id, &account.id);
        }
        // EXP-481: a session registering changes the inventory's busy flags.
        crate::device_sync::report_soon(cx);
        // EXP-1158: and a start just moved the last used login + agent (the
        // launcher's stamp) — beat now so the row carries them.
        crate::device_sync::beat_soon(cx);
        sessions.update(cx, |this, cx| {
            this.by_session.insert(session.session_id.clone(), session);
            cx.notify();
        });
    }
}

/// Does a session's worktree sit on `branch`? An EMPTY branch never matches
/// anything: scratch runs record no branch, and neither does a branch-less
/// query.
fn holds_branch(session_branch: &str, branch: &str) -> bool {
    !session_branch.is_empty() && session_branch == branch
}

/// How long the quit hook waits for the engines' suspend reports before
/// letting the quit proceed (a dead network must never wedge ⌘Q; the server
/// watchdog's offline flag and the next start's re-claim are the backstop).
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

/// EXP-105 → FEED-134: SUSPEND every session THIS process hosts when the
/// app quits (⌘Q, last-window close on non-macOS) — the CLI daemon's stop
/// rule (`daemon.rs`): a quit is not a person ending the work. Each engine
/// is killed with [`engine::SUSPEND_OUTCOME`], so its end sequence reports
/// the row `idle` (not busy) instead of ending it and keeps its worktree or
/// scratch dir and run record; the next message resumes the SAME row from
/// that record, exactly as after a crash (NEW-5's re-claim covers an
/// engine whose idle report never landed). Installed once from `ui::init`.
///
/// The waiting happens IN the observer body, not the returned future: gpui's
/// `shutdown` only grants quit futures `SHUTDOWN_TIMEOUT` (200ms) — not
/// enough for HTTPS round trips — while the body runs synchronously before
/// that clock starts.
pub fn install_quit_hook(cx: &mut App) {
    cx.on_app_quit(|cx| {
        // EXP-746: an ACP run's agent is OUR child, not a PTY's — nothing
        // SIGHUPs it when we go. Suspend every engine first so the child dies
        // with us and the end sequence has the quit window to land in.
        // (Closing a WINDOW deliberately does not do this: an ACP run is not
        // window-bound — that is the point of moving it off the dock.)
        let engines: Vec<engine::EngineSession> = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).acp_hosts())
            .unwrap_or_default();
        for session in &engines {
            session.kill(engine::SUSPEND_OUTCOME);
        }
        let deadline = std::time::Instant::now() + QUIT_END_TIMEOUT;
        // EXP-758: a `kill` is only a message to the engine loop. The ACP
        // child dies in the engine thread's `ChildGuard` drop, which our exit
        // never waited for, so ⌘Q with a live codex run orphaned `codex
        // app-server` and every tool subprocess under it (only claude carries
        // the reaper's `claude-hooks` anchor). Wait each engine out on the SAME
        // deadline, which also covers the idle reports the end sequences send.
        for session in &engines {
            let wait = quit_engine_wait(deadline, std::time::Instant::now());
            if session.wait_timeout(wait).is_none() {
                log::warn!("[ui] quit: an acp engine did not exit within {wait:?}");
            }
        }
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
    /// Live sessions sharing this clone (every session on the same repo
    /// shares one loop).
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
/// EXP-1232: fetch the managed Codex build off the UI thread (at most one
/// per process), then re-run the doctor so its row moves on. Automatic
/// work, so a failure is quiet (FEED-79): the row's `Download failed` ·
/// Update says it.
pub fn start_codex_fetch(hub: &Entity<CodingHub>, data_dir: PathBuf, cx: &mut App) {
    let hub = hub.clone();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { coding::managed_codex::ensure_once(&data_dir) })
            .await;
        let Some(result) = result else {
            return; // another fetch in this process is already running
        };
        let _ = cx.update(|cx| {
            if let Err(message) = result {
                log::warn!("[coding] managed Codex download failed: {message}");
            }
            CodingHub::refresh_doctor(&hub, cx);
        });
    })
    .detach();
}

/// NEW-5: what this app's live local sessions hold — the nightly
/// stale-worktree sweep's shield (every live host on the data dir is added
/// by the sweep itself).
pub(crate) fn live_holders(cx: &App) -> coding::LiveHolders {
    let Some(sessions) = LocalSessions::global_ref(cx) else {
        return coding::LiveHolders::default();
    };
    let sessions = sessions.read(cx);
    coding::LiveHolders {
        branches: sessions.held_branches().map(str::to_string).collect(),
        worktrees: sessions.all().map(|session| session.worktree.clone()).collect(),
    }
}

pub fn coding_data_dir(cx: &App) -> PathBuf {
    cx.try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .unwrap_or_else(api::default_data_dir)
}

/// Everything `coding::pickup` needs to prepare a session on this machine,
/// assembled from the signed-in app state. `None` when signed out.
pub fn build_coding_deps(cx: &mut App) -> Option<CodingDeps> {
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
        worktrees: Arc::new(coding::GitWorktrees),
        codex_sessions_root: None,
        claude_projects_root: None,
        device_id: Some(steer::persistent_device_id(&data_dir)),
        // EXP-746: the engine runs its session on the steer runtime, so no
        // runtime means no ACP host — decided at prepare time.
        acp_available: crate::steer_wiring::runtime(cx).is_some(),
        data_dir,
    })
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

/// EXP-1294 — host a prepared session on the in-process ACP engine and
/// register it: THE spawn behind the pickup's `LaunchFn`
/// ([`crate::session_pickup`]). It never touches a window: whoever started
/// the session already navigated to it (the composer lands on its root).
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
pub(crate) fn host_prepared(
    mut prepared: coding::PreparedLaunch,
    cx: &mut App,
) -> Result<engine::EngineSession, String> {
    // EXP-478, before anything can consume `prepared`.
    let launch_hold = prepared.launch_hold.take();

    // The bookkeeping snapshot (the engine takes `prepared` whole).
    let session_id = prepared.session_id.clone();
    let clone = prepared.clone.clone();
    let repository_id = prepared.repository_id.clone();
    let branch = prepared.branch.clone();
    let worktree = prepared.worktree.clone();
    let agent = prepared.agent;
    // EXP-444: the row's starter (a teammate's start on this shared machine).
    let started_by_id = crate::session_data::session(&session_id, cx).and_then(|row| row.user_id);
    // EXP-1292: a team agent's ROOT never ends — its engine idles the row on
    // exit. `prepare` recorded the kind in the run record a moment ago.
    let persistent = coding::run_registry::get(&coding_data_dir(cx), &session_id)
        .is_some_and(|record| record.kind.is_persistent());
    let run_cleanup = prepared.run_cleanup.clone();
    // EXP-792: the team MCP servers this run starts WITHOUT — logged; the
    // run's own transcript carries the warning.
    for warning in std::mem::take(&mut prepared.mcp_warnings) {
        log::warn!("[ui] session {session_id}: {warning}");
    }
    // The issues the session's root links (the one-session-per-issue guard).
    let issue_ids: Vec<String> = crate::session_data::root_of(&session_id, cx)
        .map(|root| {
            crate::session_data::issues_of_root(&root.id, cx)
                .into_iter()
                .map(|issue| issue.id)
                .collect()
        })
        .unwrap_or_default();

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
                // EXP-1252: a run released to another device keeps its
                // workspace too (the move packs it; its cleanup removes it).
                let suspended = engine::keeps_workspace(&exit.outcome);
                crate::steer_wiring::unwatch_kill(&session_id, cx);
                if let Some(sessions) = sessions.upgrade() {
                    LocalSessions::remove(&sessions, &session_id, suspended, cx);
                }
                // EXP-758: with the failure carried in, so a run that died on
                // its handshake is not an empty view that only says "ended".
                // A suspended run (an account switch) continues at once.
                if !suspended {
                    crate::session_screen::mark_ended(&session_id, exit.error.clone(), cx);
                }
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
        persistent,
        foreign_host: crate::steer_wiring::foreign_host(started_by_id.as_deref(), cx),
        // EXP-1005: the desktop runs the account-rotation beat.
        rotation_host: true,
        publish: true,
        kill,
        // The session screen reads the feed through `EngineSession::subscribe`;
        // the sink's PRESENCE tells the engine this host renders locally.
        local_sink: Some(Arc::new(|_| {})),
    };

    let host = Arc::new(DesktopEngineHost {
        exits: exit_tx,
        worktree: worktree.clone(),
    });
    let session = match engine::start(start, host) {
        Ok(session) => session,
        Err(err) => {
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
        LocalSession {
            session_id: session_id.clone(),
            issue_ids,
            clone,
            branch,
            host: LocalSessionHost {
                session: session.clone(),
            },
            agent,
            worktree,
            run_cleanup,
        },
        cx,
    );
    // EXP-478: only now.
    drop(launch_hold);
    Ok(session)
}

/// End `session_id`'s row off the foreground, best effort: `prepare` already
/// claimed the row, so a start that never happened must not leave it live.
fn end_row_best_effort(trpc: &Arc<api::TrpcClient>, session_id: &str) {
    let trpc = Arc::clone(trpc);
    let session_id = session_id.to_string();
    std::thread::spawn(move || {
        if let Err(err) = api::sessions::end(&trpc, &session_id) {
            log::debug!("[ui] sessions.end for {session_id} failed: {err}");
        }
    });
}
// ---------------------------------------------------------------------------
// StartCodingControl — the issue-detail header affordance (§7.1 / §4.2)
// ---------------------------------------------------------------------------

/// EXP-1294: the issue header's ONE Start button's label (the draft's
/// white capsule with the play glyph).
pub(crate) const START_LABEL: &str = "Start";

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
    /// EXP-1121: the board's `repository_id` the current probe answers for —
    /// a repo linked (or unlinked) under the open issue re-probes.
    probed_repo_id: Option<String>,
    /// EXP-1121: the team's GitHub facts, read while the board has no repo.
    github: crate::coding_readiness::GithubProbe,
    github_generation: u64,
    /// EXP-1121: the "Ready to code?" popover is up — it keeps rendering
    /// (rows ticking) even once every step is met.
    readiness_open: bool,
    /// FEED-79: the GitHub read in flight is the picker's Retry press (its
    /// failure toasts); the read on popover open fails quietly.
    github_retry_pressed: bool,
    /// The repository row's inline picker is showing.
    picker_open: bool,
    /// Its search field (created on first open — it needs a window).
    repo_query: Option<Entity<InputState>>,
    /// `boards.setRepository` in flight (a refusal is a toast, FEED-79).
    linking: bool,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingControl {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // The hub (settings + doctor) drives the enabled state — re-render
        // whenever it moves.
        let hub = CodingHub::global(cx);
        // EXP-1121: the readiness rows tick live off the synced board (a repo
        // linked anywhere) and devices rows.
        let boards = Store::global(cx).collections().boards.clone();
        let devices = Store::global(cx).collections().devices.clone();
        let subscriptions = vec![
            cx.observe(&hub, |_, _, cx| cx.notify()),
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
            probed_repo_id: None,
            github: crate::coding_readiness::GithubProbe::Idle,
            github_generation: 0,
            readiness_open: false,
            github_retry_pressed: false,
            picker_open: false,
            repo_query: None,
            linking: false,
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
        self.picker_open = false;
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
                // FEED-79: the picker's list failed — its empty state +
                // Retry; only a Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.github_retry_pressed);
                if facts.repos.is_none() {
                    crate::action_error::failed_load("the repositories", attended, None, cx);
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

    /// The picker's Retry: the same read, attended.
    pub(crate) fn retry_github(&mut self, cx: &mut gpui::Context<Self>) {
        self.github_retry_pressed = true;
        self.refresh_github(cx);
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
        cx.notify();
        let retry_repo = repository_id.clone();
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
                        let control = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "change",
                            "the board's repository",
                            crate::action_error::action_error_description(&err),
                            move |_, cx| {
                                let repository_id = retry_repo.clone();
                                let _ = control.update(cx, |this, cx| this.link_repository(repository_id, cx));
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The popover footer's Start — the control's own launch.
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
                let size = crate::work_header::header_action_size(false);
                return crate::surface::glass_pill_button("start-coding", size, cx)
                .icon(
                    Icon::new(registry::ACTION_RUN)
                        .with_size(px(size.glyph()))
                        .text_color(cx.theme().muted_foreground),
                )
                .label(START_LABEL)
                .disabled(true)
                .into_any_element();
            }
        }

        // EXP-1294: this control is ONLY the launcher — no Stop or Resume
        // ever takes its slot (a session is steered from its own view).

        // EXP-417: the primary action of the header's agent row — a solid
        // content-sized button; the repo-less retry sits beside it as a
        // compact icon.
        // EXP-1294: it sits in the header bar, before pin + ⋯, at the bar's
        // rung in the primary paint — the single emphasised pill of the header.
        let size = crate::work_header::header_action_size(false);
        // EXP-1294: Start never blocks on THIS machine's tools — it opens
        // the composer, whose device pick + readiness row gate the start
        // on the machine that will run it. A local problem only explains
        // itself in the tooltip.
        let tooltip: SharedString = match (self.disabled_reason(cx), &self.probe) {
            (Some(reason), _) => reason,
            (None, RepoProbe::Error(err)) => {
                format!("Couldn't check the linked repository ({err}). Starting will retry.").into()
            }
            (None, _) => "Start an agent on this issue".into(),
        };
        let row = h_flex().gap_1().items_center().child(
            crate::surface::glass_pill_button_primary("start-coding", size)
                .icon(
                    Icon::new(registry::ACTION_RUN)
                        .with_size(px(size.glyph()))
                        .text_color(cx.theme().primary_foreground),
                )
                .label(START_LABEL)
                .tooltip(tooltip)
                .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
        );
        row.into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(quit_engine_wait(deadline, deadline), QUIT_ENGINE_MIN_WAIT);
        assert_eq!(
            quit_engine_wait(deadline, deadline + std::time::Duration::from_secs(10)),
            QUIT_ENGINE_MIN_WAIT
        );
        // …and a remainder under the floor is raised to it.
        let nearly_spent = deadline - std::time::Duration::from_millis(5);
        assert_eq!(quit_engine_wait(deadline, nearly_spent), QUIT_ENGINE_MIN_WAIT);
    }

    /// Scratch runs record no branch — they must never read as holders (of
    /// each other, or of a branch-less query).
    #[test]
    fn a_branchless_session_holds_nothing() {
        assert!(holds_branch("exp/EXP-1", "exp/EXP-1"));
        assert!(!holds_branch("exp/EXP-1", "exp/EXP-2"));
        assert!(!holds_branch("", ""), "two scratch runs share no branch");
        assert!(!holds_branch("", "exp/EXP-1"));
        assert!(!holds_branch("exp/EXP-1", ""));
    }
}
