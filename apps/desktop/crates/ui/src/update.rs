//! In-app update check + self-update pipeline (masterplan v5 §11.2, EXP-22).
//!
//! The app reads the instance's advert (`/api/version` → `desktop.latest`,
//! EXP-1761) at launch and then every [`RECHECK_INTERVAL`] while running (non-blocking, offline-safe) — when a
//! published `desktop-v*` release is newer than the compiled version it
//! flips a global that the [`crate::shell::Shell`] shell renders as a
//! dismissible banner. The old launch-only, daily-debounced check meant a
//! long-running IDE (or one sharing its stamp file with dev runs) effectively
//! NEVER reported an update (EXP-68) — with near-daily releases the banner
//! could stay silent forever.
//!
//! When the running install can self-update (see [`updater::capability`]) AND
//! a newer release carries the asset this platform needs (EXP-1693: the newest
//! such release, not just the newest release), the banner's button runs
//! the full pipeline in-app: download (with progress) → SHA256SUMS verify →
//! swap → "Restart to update" (gpui `set_restart_path` + `restart`). In every
//! other case — dev builds, unwritable installs, no newer release with our asset
//! (which is also the macOS signing gate: unsigned releases ship a `.zip`,
//! not the `.dmg`) — the button falls back to opening the release page, i.e.
//! the original banner-only behavior. Failures mid-pipeline degrade the same
//! way. The check itself stays best-effort: every failure path (offline,
//! rate-limited, malformed JSON) is swallowed so it can never take the shell
//! down or block startup.
//!
//! The **staging** channel skips the check (and therefore the updater)
//! entirely: staging builds track `next.exponential.at` and are hand-deployed,
//! so a production release tag must never nag a staging user (mirrors the
//! login `CLOUD_INSTANCE` split).

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use gpui::{App, AppContext as _, Entity, Global};
use serde::Deserialize;

/// GitHub's release list: the FALLBACK only (EXP-1761: the unauthenticated API
/// allows 60 calls/h per IP, shared by every app on the machine), for when the
/// advert is unreachable or unset, or names a release without our asset
/// (EXP-1693: `releases/latest` is one release, which
/// may lack this platform's asset, and it can point at any tag family). 30
/// releases hold about ten desktop ones across the repo's release trains, at a
/// third of the 100-release payload. `EXP_UPDATE_API` overrides it at runtime so
/// the whole pipeline can be driven against a fixture release list (dev/testing
/// only; release builds never set it).
const RELEASES_API: &str = "https://api.github.com/repos/Niach/exponential/releases?per_page=30";
/// Human release page — the banner's fallback link when a release omits its
/// own `html_url`.
const RELEASES_PAGE: &str = "https://github.com/Niach/exponential/releases/latest";
/// Only desktop releases (`desktop-v1.2.3`) drive the desktop banner — the repo
/// also ships web `v*` and `android-v*` tags whose versions are unrelated.
const DESKTOP_TAG_PREFIX: &str = "desktop-v";
/// Re-check cadence while the app runs. One advert read (plus three HEADs on
/// plain download URLs when a release is newer) per interval — an IDE left open for days still learns
/// about new releases, which the old launch-only check never did (EXP-68).
/// EXP-1489: 30 minutes, so a release reaches running apps promptly.
const RECHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Re-fetch cadence while the app sits BLOCKED (EXP-104) without an
/// installable release. The server's min-version gate can run ahead of the
/// GitHub release (426s start while the release assets are still building or
/// uploading), and the check itself can transiently fail — a blocked user must
/// not stare at the browser fallback for a whole [`RECHECK_INTERVAL`]
/// (EXP-316). The advert and the download URLs carry no API rate limit.
const BLOCKED_RETRY_INTERVAL: Duration = Duration::from_secs(60);
/// GitHub rejects API calls without a User-Agent.
const USER_AGENT: &str = "exp-desktop-update-check";
/// EXP-1489: how often the auto-restart watcher reads the gate.
const RESTART_WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// Whether this build channel checks for updates at all. Production does;
/// staging is skipped (see the module doc). A runtime `const` rather than a
/// `#[cfg]` on the whole path so every helper stays referenced (no dead-code
/// warnings on staging builds).
#[cfg(not(feature = "staging"))]
const CHANNEL_CHECKS_UPDATES: bool = true;
#[cfg(feature = "staging")]
const CHANNEL_CHECKS_UPDATES: bool = false;

/// The version this binary was compiled at — the shared
/// `domain::client_version` value (EXP-262 extracted it so the About screen
/// shows the same number the update check compares). Local/dev builds fall
/// back to the static `0.1.0` team placeholder, which simply means the
/// comparison always reports an update available off a real release —
/// harmless in dev.
fn current_version() -> &'static str {
    domain::client_version::current_version()
}

fn releases_api() -> String {
    std::env::var("EXP_UPDATE_API").unwrap_or_else(|_| RELEASES_API.to_string())
}

/// The human release page — the blocking view's browser fallback when no
/// release/plan is known (staging, dev, or an assetless release).
pub fn releases_page_url() -> &'static str {
    RELEASES_PAGE
}

/// Why this install can only offer the browser link, one sentence fragment
/// for the blocking view to surface (EXP-316) — `None` when the install can
/// self-update (a missing plan is then about the RELEASE, not the install).
pub fn self_update_unavailable_reason() -> Option<String> {
    if !CHANNEL_CHECKS_UPDATES {
        return Some(
            "staging builds don't self-update (deployed by hand)".to_string(),
        );
    }
    updater::self_update_unavailable_reason()
}

/// A newer release the user can download or install.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// The dotted version (tag with the `desktop-v` prefix stripped).
    pub version: String,
    /// The release page to open in the browser (the universal fallback).
    pub url: String,
    /// The in-app install plan — `None` means banner-only (dev build,
    /// unwritable install, or the release doesn't carry our asset).
    pub plan: Option<UpdatePlan>,
}

/// Everything the background pipeline needs, resolved against the CHECKED
/// release's own asset list — never `releases/latest/download/…`, so a release
/// published between check and click can't be half-applied, and the existing
/// `is_newer` gate doubles as downgrade protection.
#[derive(Clone, Debug)]
pub struct UpdatePlan {
    pub asset_name: String,
    pub asset_url: String,
    pub sums_url: String,
    /// The release's `SHA256SUMS.txt.minisig` — `None` when unsigned, which a
    /// build with a pinned feed key refuses in the pipeline (EXP-1430).
    pub sig_url: Option<String>,
    pub strategy: updater::Strategy,
}

/// Where the update sits in its lifecycle. One shot, no queue — a fresh check
/// only ever runs at launch, so at most one release is in flight per session.
#[derive(Clone, Debug, Default)]
pub enum UpdatePhase {
    /// Newer release known; waiting for the user to click.
    #[default]
    Available,
    /// Pipeline running: bytes received / expected (when known).
    Downloading { received: u64, total: Option<u64> },
    /// Verify + swap in progress (fast; no cancel).
    Installing,
    /// Swapped on disk — relaunch via `restart_path` picks up the new version.
    /// Dismissible: the next manual launch runs the new version anyway.
    ReadyToRestart { restart_path: Option<PathBuf> },
    /// Pipeline error; the banner offers retry + the browser fallback.
    Failed { message: String },
}

/// Shell-observed update state. Lives behind a gpui global so the check task
/// (which only has an `App`) and the per-window `Shell` render both reach
/// the same entity.
#[derive(Default)]
pub struct UpdateState {
    available: Option<UpdateInfo>,
    phase: UpdatePhase,
    dismissed: bool,
    /// The server rejected this build (HTTP 426 min-version gate, EXP-104).
    /// While set, the shell renders a FULL-WINDOW blocking "Update required"
    /// view instead of the board (and the dismissible banner is suppressed).
    /// Unlike a normal update it cannot be dismissed — the app is unusable
    /// until it updates.
    blocked: bool,
    /// EXP-1489: since when the swapped build waits for its restart (the
    /// [`updater::restart::MAX_WAIT`] clock).
    ready_since: Option<Instant>,
}

impl UpdateState {
    /// The banner to show, or `None` when there is no update or the user
    /// dismissed it this session.
    pub fn banner(&self) -> Option<(&UpdateInfo, &UpdatePhase)> {
        if self.dismissed {
            None
        } else {
            self.available.as_ref().map(|info| (info, &self.phase))
        }
    }

    /// Whether the app is gated behind the EXP-104 blocking "Update required"
    /// view (the server 426'd this build).
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }

    /// The known newer release, IGNORING a session dismissal — the blocking
    /// view drives its buttons off this directly (a required update can't be
    /// dismissed).
    pub fn available(&self) -> Option<&UpdateInfo> {
        self.available.as_ref()
    }

    /// The current pipeline phase (for the blocking view's progression).
    pub fn phase(&self) -> &UpdatePhase {
        &self.phase
    }

    /// Dismiss the banner for the rest of this session (not persisted — a fresh
    /// launch that still sees a newer release shows it again). A LATER release
    /// found by the periodic re-check un-dismisses (see [`Self::offer`]).
    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    /// Record a newer release found by the check loop. No-op while the
    /// pipeline is running or already done — a re-check must never clobber an
    /// in-flight download/install or a pending restart. A strictly newer
    /// version than the one currently offered resets a session dismissal
    /// (the user said no to THAT release, not to all future ones).
    fn offer(&mut self, info: UpdateInfo, cx: &mut gpui::Context<Self>) {
        if matches!(
            self.phase,
            UpdatePhase::Downloading { .. } | UpdatePhase::Installing | UpdatePhase::ReadyToRestart { .. }
        ) {
            return;
        }
        match &self.available {
            Some(current) if current.version == info.version => {
                // Same release re-confirmed — keep phase (a Failed banner
                // keeps its retry affordance) and the dismissal. But adopt a
                // newly buildable install plan (EXP-316): the first fetch can
                // land while the release's assets are still uploading, and a
                // later re-fetch is the only way "Download update" upgrades to
                // the in-app "Update now" pipeline.
                if current.plan.is_none() && info.plan.is_some() {
                    self.available = Some(info);
                }
            }
            _ => {
                self.dismissed = false;
                self.available = Some(info);
                self.phase = UpdatePhase::Available;
            }
        }
        cx.notify();
    }

    /// Get-or-create the global entity.
    pub fn global(cx: &mut App) -> Entity<UpdateState> {
        if let Some(global) = cx.try_global::<UpdateGlobal>() {
            return global.0.clone();
        }
        let entity = cx.new(|_| UpdateState::default());
        cx.set_global(UpdateGlobal(entity.clone()));
        entity
    }

    /// Immutable accessor for `render` (no `&mut App`); `None` before the check
    /// wired the global.
    pub fn global_ref(cx: &App) -> Option<Entity<UpdateState>> {
        cx.try_global::<UpdateGlobal>().map(|g| g.0.clone())
    }

    /// Gate the app into the blocking "Update required" state (EXP-104 — the
    /// sync/tRPC layer saw a 426). Idempotent: repeat 426s (many shape
    /// threads) collapse to one transition. While blocked without an
    /// INSTALLABLE release it fetches immediately and then keeps re-fetching
    /// every [`BLOCKED_RETRY_INTERVAL`] — bypassing the 4h loop — until the
    /// blocking view can offer the in-app "Update now" pipeline. One shot was
    /// not enough (EXP-316): the min-version gate regularly runs AHEAD of the
    /// published release (426s begin while the release assets are still
    /// building/uploading), and a transiently failed fetch left Linux users on
    /// the browser-link fallback for up to 4h. On the staging channel (which
    /// publishes no assets and never checks) fetching is skipped and the view
    /// degrades to the browser-link fallback; banner-only installs (dev
    /// builds, unwritable dirs) stop after the first successful fetch — no
    /// re-fetch can improve on the browser link there.
    pub fn set_blocked(cx: &mut App) {
        let model = Self::global(cx);
        let need_fetch = model.update(cx, |state, cx| {
            if state.blocked {
                return false;
            }
            state.blocked = true;
            cx.notify();
            // Poll only while no installable release is known and the channel
            // checks (staging has no published assets to install).
            CHANNEL_CHECKS_UPDATES
                && model_needs_plan(state)
        });
        auto_install(cx);
        if !need_fetch {
            return;
        }
        cx.spawn(async move |cx| {
            // Bounded (~30min of 1/min polls) — the 4h check loop keeps
            // running in parallel and re-offers on its own cadence anyway.
            for _ in 0..30 {
                let result = cx
                    .background_executor()
                    .spawn(async move { fetch_latest() })
                    .await;
                if let Ok(Some(info)) = result {
                    let _ = cx.update(|cx| {
                        model.update(cx, |state, cx| state.offer(info, cx));
                        auto_install(cx);
                    });
                }
                let needs_plan = cx.update(|cx| model_needs_plan(model.read(cx)));
                // A capability that can't self-update never yields a plan —
                // further fetches would poll GitHub for nothing.
                if !needs_plan
                    || !matches!(updater::capability(), updater::Capability::SelfUpdate(_))
                {
                    break;
                }
                cx.background_executor().timer(BLOCKED_RETRY_INTERVAL).await;
            }
        })
        .detach();
    }
}

struct UpdateGlobal(Entity<UpdateState>);
impl Global for UpdateGlobal {}

/// EXP-1489: the desktop installs like the CLI daemon does — a release this
/// install can swap itself to is downloaded at once (no click), and the
/// restart follows by [`watch_restart`]'s rule. Banner-only installs (dev
/// builds, unwritable dirs, no asset) keep the banner.
fn auto_install(cx: &mut App) {
    let Some(model) = UpdateState::global_ref(cx) else {
        return;
    };
    let installable = {
        let state = model.read(cx);
        matches!(state.phase, UpdatePhase::Available)
            && state.available.as_ref().is_some_and(|info| info.plan.is_some())
    };
    if installable && matches!(updater::capability(), updater::Capability::SelfUpdate(_)) {
        log::info!("[ui] update: installing the new release in the background");
        start_update(cx);
    }
}

/// EXP-1489: the restart of a swapped build, by the CLI daemon's rule
/// ([`updater::restart::restart_gate`]): once every session this app hosts
/// pauses between turns, at most [`updater::restart::MAX_WAIT`], and at
/// once when the server refuses this build. A person at the app (a focused
/// window) is never restarted under unless the build is refused: the
/// banner's "Restart to update" is theirs. The quit hook suspends every
/// run; one cut mid-turn reads `restarted` and the next launch continues
/// it by itself.
fn watch_restart(cx: &mut App, model: Entity<UpdateState>) {
    cx.spawn(async move |cx| {
        let mut last_busy = Instant::now();
        loop {
            cx.background_executor().timer(RESTART_WATCH_INTERVAL).await;
            let restart = cx.update(|cx| {
                let engines: Vec<engine::EngineSession> = crate::coding_flow::LocalSessions::global_ref(cx)
                    .map(|sessions| sessions.read(cx).acp_hosts())
                    .unwrap_or_default();
                let live: Vec<_> = engines.iter().filter(|engine| !engine.is_done()).collect();
                let now = Instant::now();
                // EXP-1675: a background task (a build, a test run) is work
                // in flight too, not a pause between turns.
                if live.iter().any(|engine| engine.agent_busy()) {
                    last_busy = now;
                }
                let state = model.read(cx);
                let (UpdatePhase::ReadyToRestart { restart_path }, Some(ready_since)) =
                    (&state.phase, state.ready_since)
                else {
                    return None;
                };
                let paused = (!live.is_empty()).then(|| Some(now.saturating_duration_since(last_busy)));
                let gate = updater::restart::restart_gate(
                    paused,
                    now.saturating_duration_since(ready_since),
                    state.blocked,
                );
                let attended = cx.active_window().is_some();
                (gate == updater::restart::RestartGate::Now && (state.blocked || !attended))
                    .then(|| restart_path.clone())
            });
            if let Some(restart_path) = restart {
                log::info!("[ui] update: restarting on the new version");
                cx.update(|cx| relaunch(restart_path, cx));
                return;
            }
        }
    })
    .detach();
}

/// Quit and come back on `restart_path` (`None` = this exe). FEED-189: on
/// Windows gpui's restart hands the relaunch to a PowerShell script, and a
/// script that fails to run or start the app leaves the machine with no app
/// and nothing logged. There the new exe is started directly and waits for
/// this process itself ([`coding::process::await_relaunch`]). gpui's restart
/// stays the fallback, and the path everywhere else.
pub(crate) fn relaunch(restart_path: Option<PathBuf>, cx: &mut App) {
    let exe = cfg!(target_os = "windows")
        .then(|| restart_path.clone().or_else(|| std::env::current_exe().ok()))
        .flatten();
    let Some(exe) = exe else {
        if let Some(path) = restart_path {
            cx.set_restart_path(path);
        }
        cx.restart();
        return;
    };
    let pid = std::process::id();
    cx.spawn(async move |cx| {
        // Off the foreground: `CreateProcessW` can pump the Win32 message loop
        // and re-enter the app while it is borrowed (gpui's own note).
        let spawned = cx
            .background_executor()
            .spawn(async move { coding::process::spawn_after_exit(&exe, pid) })
            .await;
        cx.update(|cx| match spawned {
            Ok(()) => {
                log::info!("[ui] update: relaunch started, quitting");
                cx.quit();
            }
            Err(err) => {
                log::warn!("[ui] update: relaunch failed to start ({err}), using gpui's restart");
                cx.restart();
            }
        });
    })
    .detach();
}

/// Whether the blocked-mode poll still has work to do: no release with an
/// install plan is known yet (EXP-316).
fn model_needs_plan(state: &UpdateState) -> bool {
    state
        .available
        .as_ref()
        .and_then(|info| info.plan.as_ref())
        .is_none()
}

/// Kick off the update check loop (call once from the app bootstrap, after
/// the globals are installed): check now, then re-check every
/// [`RECHECK_INTERVAL`] for as long as the app runs. Non-blocking: the
/// network hits run on the background executor and only touch the foreground
/// to flip the flag; every failure (offline, rate-limited, malformed JSON) is
/// silently retried next interval. Also sweeps the updater staging dir —
/// leftovers only exist after a crash mid-pipeline.
pub fn check_for_updates(cx: &mut App) {
    // Materialize the global up front so the shell's `global_ref` never has to
    // create it from an immutable render context.
    let model = UpdateState::global(cx);

    // Staging channel never nags (§11.2).
    if !CHANNEL_CHECKS_UPDATES {
        return;
    }
    // EXP-1555: a shots capture (`EXP_DEV_READY_FILE`, packages/shots) never
    // photographs the update banner (a race against the release feed).
    if std::env::var_os("EXP_DEV_READY_FILE").is_some() {
        return;
    }

    cx.background_executor()
        .spawn(async move { updater::cleanup_staging() })
        .detach();
    watch_restart(cx, model.clone());

    cx.spawn(async move |cx| {
        loop {
            let result = cx
                .background_executor()
                .spawn(async move { fetch_latest() })
                .await;

            if let Ok(Some(info)) = result {
                cx.update(|cx| {
                    model.update(cx, |state, cx| state.offer(info, cx));
                    auto_install(cx);
                });
            }

            // The task dies with the app executor — no shutdown bookkeeping.
            cx.background_executor().timer(RECHECK_INTERVAL).await;
        }
    })
    .detach();
}

/// One message per pipeline step, marshaled from the background download
/// thread to the foreground state entity.
enum UpdateEvent {
    Progress { received: u64, total: Option<u64> },
    Installing,
    Ready { restart_path: Option<PathBuf> },
    Failed { message: String },
}

/// Run the download → verify → swap pipeline for the release the check found.
/// No-op unless the state is `Available`/`Failed` (guards double-clicks) and
/// the info carries a plan.
pub fn start_update(cx: &mut App) {
    let model = UpdateState::global(cx);
    let plan = model.update(cx, |state, cx| {
        if !matches!(state.phase, UpdatePhase::Available | UpdatePhase::Failed { .. }) {
            return None;
        }
        let plan = state.available.as_ref()?.plan.clone()?;
        state.phase = UpdatePhase::Downloading { received: 0, total: None };
        cx.notify();
        Some(plan)
    });
    let Some(plan) = plan else {
        return;
    };

    let (tx, rx) = flume::unbounded::<UpdateEvent>();

    cx.background_executor()
        .spawn(async move {
            let event = match run_pipeline(&plan, &tx) {
                Ok(restart_path) => UpdateEvent::Ready { restart_path },
                Err(err) => {
                    log::warn!("[ui] update: pipeline failed: {err:#}");
                    // The whole chain, down to the HTTP status/body or the
                    // transport cause — a bare step name explains nothing.
                    UpdateEvent::Failed { message: format!("{err:#}") }
                }
            };
            let _ = tx.send(event);
        })
        .detach();

    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let done = matches!(event, UpdateEvent::Ready { .. } | UpdateEvent::Failed { .. });
            cx.update(|cx| {
                model.update(cx, |state, cx| {
                    state.phase = match event {
                        UpdateEvent::Progress { received, total } => {
                            UpdatePhase::Downloading { received, total }
                        }
                        UpdateEvent::Installing => UpdatePhase::Installing,
                        UpdateEvent::Ready { restart_path } => {
                            state.ready_since = Some(Instant::now());
                            UpdatePhase::ReadyToRestart { restart_path }
                        }
                        UpdateEvent::Failed { message } => UpdatePhase::Failed { message },
                    };
                    cx.notify();
                });
            });
            if done {
                break;
            }
        }
    })
    .detach();
}

/// Blocking pipeline body (background executor): fetch SHA256SUMS, stream the
/// asset into the staging dir, verify, swap. Returns the restart path.
fn run_pipeline(
    plan: &UpdatePlan,
    tx: &flume::Sender<UpdateEvent>,
) -> anyhow::Result<Option<PathBuf>> {
    use anyhow::Context as _;

    let staging = updater::staging_dir().context("no user data dir")?;
    run_pipeline_in(plan, tx, &staging)
}

/// [`run_pipeline`] staging into `staging`. #EXP-1763: the tests pass their
/// fixture's own dir, so parallel test processes never share (and clean up)
/// the user's real staging dir.
fn run_pipeline_in(
    plan: &UpdatePlan,
    tx: &flume::Sender<UpdateEvent>,
    staging: &std::path::Path,
) -> anyhow::Result<Option<PathBuf>> {
    use anyhow::Context as _;

    // Signature checked before any download (EXP-1430): a tampered feed fails fast.
    let sums = updater::fetch_verified_sums(&plan.sums_url, plan.sig_url.as_deref())
        .context("fetch the signed SHA256SUMS.txt")?;

    let dest = staging.join(&plan.asset_name);
    // Throttle progress to ~4/s — every 64KB chunk would flood the foreground.
    let mut last_sent: Option<Instant> = None;
    updater::download(&plan.asset_url, &dest, |received, total| {
        let due = last_sent.is_none_or(|at| at.elapsed() >= Duration::from_millis(250));
        if due || Some(received) == total {
            last_sent = Some(Instant::now());
            let _ = tx.send(UpdateEvent::Progress { received, total });
        }
    })
    .context("download update")?;

    let _ = tx.send(UpdateEvent::Installing);
    let result = updater::verify_sha256(&dest, &sums, &plan.asset_name)
        .and_then(|()| updater::install(&plan.strategy, &dest));
    if result.is_err() {
        let _ = std::fs::remove_file(&dest);
    }
    result
}

/// GitHub release payload — only the fields we read.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    /// `uploaded` once complete; anything else is a partial upload.
    state: Option<String>,
}

/// Blocking fetch + compare. `Ok(Some)` = a newer desktop release,
/// `Ok(None)` = up to date, `Err(())` = network/parse failure (don't record
/// the check so we retry).
///
/// EXP-1761: the advert decides; the Releases API is asked only when the
/// advert is unknown, or when the advertised release lacks our asset and an
/// older newer-than-us one might carry it (EXP-1693).
fn fetch_latest() -> Result<Option<UpdateInfo>, ()> {
    let advertised = updater::feed::advertised_latest("desktop")
        .map_err(|err| log::warn!("[ui] update: advert check failed: {err:#}"))
        .ok()
        .flatten();
    let info = match check_advert(advertised.as_deref(), current_version(), plan_from_downloads) {
        AdvertCheck::UpToDate => return Ok(None),
        AdvertCheck::Newer(info) if info.plan.is_some() || !can_self_update() => Some(info),
        AdvertCheck::Newer(info) => match fetch_from_api() {
            Ok(Some(listed)) if listed.plan.is_some() => Some(listed),
            _ => Some(info),
        },
        AdvertCheck::Unknown => fetch_from_api()?,
    };
    if info.as_ref().is_some_and(|info| info.plan.is_none()) {
        // Answer the "why is there only a browser button" question from the
        // log too (EXP-316) — the blocking view surfaces the same reason.
        match updater::self_update_unavailable_reason() {
            Some(reason) => log::info!("[ui] update: self-update unavailable: {reason}"),
            None => log::info!("[ui] update: no newer release carries this platform's asset — banner-only"),
        }
    }
    Ok(info)
}

fn can_self_update() -> bool {
    matches!(updater::capability(), updater::Capability::SelfUpdate(_))
}

/// What the advert says about this build.
enum AdvertCheck {
    /// No advert (unreachable, unset): ask the Releases API.
    Unknown,
    /// The advertised release is not newer than this build.
    UpToDate,
    /// A newer release; `plan` is `None` when it can't be installed here.
    Newer(UpdateInfo),
}

/// EXP-1761: decide on the advertised version alone; `plan_for` (given the
/// version) builds the plan from the release's download URLs.
fn check_advert(
    advertised: Option<&str>,
    current: &str,
    plan_for: impl FnOnce(&str) -> Option<UpdatePlan>,
) -> AdvertCheck {
    let Some(version) = advertised.map(|version| version.trim_start_matches('v')) else {
        return AdvertCheck::Unknown;
    };
    if !is_newer(version, current) {
        return AdvertCheck::UpToDate;
    }
    AdvertCheck::Newer(UpdateInfo {
        version: version.to_string(),
        url: updater::feed::release_page(&format!("{DESKTOP_TAG_PREFIX}{version}")),
        plan: plan_for(version),
    })
}

/// The plan for `desktop-v<version>` from plain download URLs, the asset
/// probed (HEAD) first: a release still missing it is banner-only (EXP-1693).
fn plan_from_downloads(version: &str) -> Option<UpdatePlan> {
    let updater::Capability::SelfUpdate(strategy) = updater::capability() else {
        return None;
    };
    let asset_name = updater::expected_asset_name(&strategy);
    let files = updater::feed::probe_release(&format!("{DESKTOP_TAG_PREFIX}{version}"), &asset_name)
        .map_err(|err| log::warn!("[ui] update: release probe failed: {err:#}"))
        .ok()??;
    Some(UpdatePlan {
        asset_name,
        asset_url: files.asset_url,
        sums_url: files.sums_url,
        sig_url: files.sig_url,
        strategy,
    })
}

/// The Releases-API path (the fallback): list, pick by [`select_release`].
fn fetch_from_api() -> Result<Option<UpdateInfo>, ()> {
    // The app's one shared HTTP client (EXP-304). GitHub is a different host
    // from the instance, so this just gets its own pooled connection — but it
    // is still one client, one TLS stack, one place to configure.
    let response = api::http::shared()
        .get(releases_api())
        .timeout(Duration::from_secs(10))
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|err| {
            log::warn!("[ui] update: release check failed: {err}");
        })?;
    if !response.status().is_success() {
        log::warn!("[ui] update: release check answered {}", response.status());
        return Err(());
    }
    // Decode with `serde_json` rather than reqwest's `json()` — the workspace
    // client is built without the `json` feature (image_paste.rs relies on the
    // same).
    let body = response.text().map_err(|_| ())?;
    let releases: Vec<Release> = serde_json::from_str(&body).map_err(|err| {
        log::warn!("[ui] update: release check returned malformed JSON: {err}");
    })?;
    Ok(select_release(&releases, current_version(), build_plan))
}

/// EXP-1693: what to offer from the published desktop releases newer than
/// `current`: the newest one this install can apply, so a release still
/// missing our asset (Windows' leg attaches late, a macOS notarization flake)
/// doesn't hide the one before it. No appliable one = the newest, banner-only
/// (the browser fallback, which is also the macOS signing gate). The API lists
/// by creation date, so versions are compared, not list positions.
fn select_release(
    releases: &[Release],
    current: &str,
    plan_for: impl Fn(&[ReleaseAsset]) -> Option<UpdatePlan>,
) -> Option<UpdateInfo> {
    let mut newer: Vec<(&str, &Release)> = releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| Some((release.tag_name.strip_prefix(DESKTOP_TAG_PREFIX)?, release)))
        .filter(|(version, _)| is_newer(version, current))
        .collect();
    newer.sort_by_key(|(version, _)| std::cmp::Reverse(parse_semver(version)));
    let (version, release, plan) = newer
        .iter()
        .find_map(|&(version, release)| plan_for(&release.assets).map(|plan| (version, release, Some(plan))))
        .or_else(|| newer.first().map(|&(version, release)| (version, release, None)))?;
    Some(UpdateInfo {
        version: version.to_string(),
        url: release.html_url.clone().unwrap_or_else(|| RELEASES_PAGE.to_string()),
        plan,
    })
}

/// Pair the install capability with the checked release's assets. `None`
/// (banner-only) whenever the platform can't swap itself or the release
/// doesn't carry both our asset and the checksums file.
fn build_plan(assets: &[ReleaseAsset]) -> Option<UpdatePlan> {
    let updater::Capability::SelfUpdate(strategy) = updater::capability() else {
        return None;
    };
    let asset_name = updater::expected_asset_name(&strategy);
    let find = |name: &str| {
        assets
            .iter()
            .find(|asset| asset.name == name && asset.state.as_deref().is_none_or(|state| state == "uploaded"))
            .map(|asset| asset.browser_download_url.clone())
    };
    Some(UpdatePlan {
        asset_url: find(&asset_name)?,
        sums_url: find(updater::SUMS_ASSET)?,
        sig_url: find(updater::SIG_ASSET),
        asset_name,
        strategy,
    })
}

/// Numeric `major.minor.patch` compare (pre-release/build metadata ignored —
/// good enough for a "download available" nudge; the human decides). Shared
/// with the "My devices" rows, which nudge on `CLIENT_LATEST_VERSION_*`.
pub(crate) fn is_newer(candidate: &str, current: &str) -> bool {
    parse_semver(candidate) > parse_semver(current)
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let core = v.trim().split(['-', '+']).next().unwrap_or(v);
    let mut parts = core.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_ordering() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn semver_ignores_prerelease_and_build() {
        assert_eq!(parse_semver("1.2.3-rc.1"), (1, 2, 3));
        assert_eq!(parse_semver("1.2.3+build.5"), (1, 2, 3));
        assert_eq!(parse_semver("desktop-v1.2.3"), (0, 0, 0)); // prefix must be stripped first
        assert_eq!(parse_semver("2.0"), (2, 0, 0));
    }

    #[test]
    fn tag_prefix_gates_desktop_releases() {
        assert_eq!("desktop-v1.4.0".strip_prefix(DESKTOP_TAG_PREFIX), Some("1.4.0"));
        assert_eq!("v1.4.0".strip_prefix(DESKTOP_TAG_PREFIX), None);
        assert_eq!("android-v1.4.0".strip_prefix(DESKTOP_TAG_PREFIX), None);
    }

    #[test]
    fn plan_requires_both_asset_and_sums() {
        // Whatever this host's capability, a release with NO assets can never
        // yield a plan.
        assert!(build_plan(&[]).is_none());
    }

    /// EXP-1693: `plan_for` stands in for the capability: a release with an
    /// asset named `ours` is appliable.
    fn pick(releases: &str, current: &str) -> Option<(String, bool)> {
        let releases: Vec<Release> = serde_json::from_str(releases).unwrap();
        select_release(&releases, current, |assets| {
            assets.iter().any(|asset| asset.name == "ours").then(|| UpdatePlan {
                asset_name: "ours".to_string(),
                asset_url: String::new(),
                sums_url: String::new(),
                sig_url: None,
                strategy: updater::Strategy::AppImage { appimage: PathBuf::new() },
            })
        })
        .map(|info| (info.version, info.plan.is_some()))
    }

    const OURS: &str = r#"[{"name":"ours","browser_download_url":"u"}]"#;

    #[test]
    fn select_falls_back_to_the_newest_release_with_the_asset() {
        let list = format!(
            r#"[{{"tag_name":"desktop-v1.3.0","assets":[]}},
                {{"tag_name":"android-v9.0.0","assets":{OURS}}},
                {{"tag_name":"desktop-v1.2.0","assets":{OURS}}},
                {{"tag_name":"desktop-v1.1.0","assets":{OURS}}}]"#
        );
        assert_eq!(pick(&list, "1.0.0"), Some(("1.2.0".to_string(), true)));
    }

    #[test]
    fn select_compares_versions_not_list_order() {
        let list = format!(
            r#"[{{"tag_name":"desktop-v1.1.0","assets":{OURS}}},
                {{"tag_name":"desktop-v1.10.0","assets":{OURS}}},
                {{"tag_name":"desktop-v1.9.0","assets":{OURS}}}]"#
        );
        assert_eq!(pick(&list, "1.0.0"), Some(("1.10.0".to_string(), true)));
    }

    #[test]
    fn select_goes_banner_only_when_no_newer_release_has_the_asset() {
        // 1.2.0 has the asset but is the running version: never a downgrade.
        let list = format!(
            r#"[{{"tag_name":"desktop-v1.3.0","assets":[]}},
                {{"tag_name":"desktop-v1.2.0","assets":{OURS}}}]"#
        );
        assert_eq!(pick(&list, "1.2.0"), Some(("1.3.0".to_string(), false)));
        assert_eq!(pick(&list, "1.3.0"), None);
    }

    #[test]
    fn select_skips_drafts_and_prereleases() {
        let list = format!(
            r#"[{{"tag_name":"desktop-v1.4.0","draft":true,"assets":{OURS}}},
                {{"tag_name":"desktop-v1.3.0","prerelease":true,"assets":{OURS}}},
                {{"tag_name":"desktop-v1.2.0","assets":{OURS}}}]"#
        );
        assert_eq!(pick(&list, "1.0.0"), Some(("1.2.0".to_string(), true)));
    }

    fn plan_stub(name: &str) -> UpdatePlan {
        UpdatePlan {
            asset_name: name.to_string(),
            asset_url: String::new(),
            sums_url: String::new(),
            sig_url: None,
            strategy: updater::Strategy::AppImage { appimage: PathBuf::new() },
        }
    }

    /// EXP-1761: the advert decides, without the Releases API.
    #[test]
    fn the_advert_decides_newer_or_up_to_date() {
        let newer = check_advert(Some("0.14.144"), "0.14.143", |version| {
            assert_eq!(version, "0.14.144");
            Some(plan_stub("ours"))
        });
        let AdvertCheck::Newer(info) = newer else { panic!("0.14.144 is newer") };
        assert_eq!(info.version, "0.14.144");
        assert_eq!(info.url, "https://github.com/Niach/exponential/releases/tag/desktop-v0.14.144");
        assert!(info.plan.is_some());

        // Never a downgrade, never a re-install: the plan isn't even probed.
        for current in ["0.14.144", "0.14.145"] {
            let check = check_advert(Some("0.14.144"), current, |_| panic!("no probe when up to date"));
            assert!(matches!(check, AdvertCheck::UpToDate));
        }
        // No advert = ask the API.
        assert!(matches!(check_advert(None, "0.14.143", |_| panic!("no probe")), AdvertCheck::Unknown));
    }

    /// EXP-1693 on the advert path: a release still missing our asset is
    /// banner-only, never a broken install.
    #[test]
    fn an_advertised_release_without_our_asset_is_banner_only() {
        let AdvertCheck::Newer(info) = check_advert(Some("2.0.0"), "1.0.0", |_| None) else {
            panic!("2.0.0 is newer")
        };
        assert_eq!(info.version, "2.0.0");
        assert!(info.plan.is_none());
    }

    /// The e2e tests share process-wide env vars (`EXP_UPDATE_*`, `APPIMAGE`).
    #[cfg(target_os = "linux")]
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A fixture "GitHub + instance": `route(method, path)` answers each
    /// request (`None` = 404), one request per connection, until the test ends.
    /// Returns the base URL and the log of `METHOD path` lines served.
    #[cfg(target_os = "linux")]
    fn fixture_server(
        route: impl Fn(&str, &str) -> Option<Vec<u8>> + Send + 'static,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let served = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let mut line = request.split_whitespace();
                let (method, path) = (line.next().unwrap_or("").to_string(), line.next().unwrap_or("").to_string());
                served.lock().unwrap().push(format!("{method} {path}"));
                let (status, body) = match route(&method, &path) {
                    Some(body) => ("200 OK", body),
                    None => ("404 Not Found", Vec::new()),
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                if method != "HEAD" {
                    let _ = stream.write_all(&body);
                }
            }
        });
        (base, log)
    }

    /// The payload, its sums line and a temp AppImage for the e2e runs.
    #[cfg(target_os = "linux")]
    struct Fixture {
        dir: PathBuf,
        appimage: PathBuf,
        asset_name: String,
        payload: Vec<u8>,
        sums: String,
    }

    #[cfg(target_os = "linux")]
    fn fixture(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("exp-update-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let appimage = dir.join("Exponential.AppImage");
        std::fs::write(&appimage, b"old-version").unwrap();
        let asset_name = updater::expected_asset_name(&updater::Strategy::AppImage { appimage: appimage.clone() });
        let payload = vec![9u8; 150_000];
        let digest = {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(&payload);
            hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        let sums = format!("{digest}  {asset_name}\n");
        Fixture { dir, appimage, asset_name, payload, sums }
    }

    /// Run the found plan's pipeline: the AppImage must be swapped and the
    /// events must show progress and the install step.
    #[cfg(target_os = "linux")]
    fn install_and_check(info: UpdateInfo, fx: &Fixture) {
        let plan = info.plan.expect("self-update plan built");
        assert_eq!(plan.asset_name, fx.asset_name);
        let (tx, rx) = flume::unbounded();
        // The fixture's own staging dir, never the user's (#EXP-1763).
        let restart_path = run_pipeline_in(&plan, &tx, &fx.dir.join("staging")).unwrap();
        assert_eq!(restart_path, Some(fx.appimage.clone()));
        assert_eq!(std::fs::read(&fx.appimage).unwrap(), fx.payload);
        let events: Vec<_> = rx.drain().collect();
        assert!(events
            .iter()
            .any(|e| matches!(e, UpdateEvent::Progress { received, .. } if *received > 0)));
        assert!(events.iter().any(|e| matches!(e, UpdateEvent::Installing)));
        let _ = std::fs::remove_dir_all(&fx.dir);
    }

    /// EXP-1761, headless end-to-end (Linux): the advert names 99.0.0, the
    /// plan comes from plain download URLs (HEAD-probed), the Releases API is
    /// never asked, and the pipeline swaps the AppImage.
    #[cfg(target_os = "linux")]
    #[test]
    fn end_to_end_from_the_advert_without_the_releases_api() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fx = fixture("advert");
        let (asset, sums, payload) = (fx.asset_name.clone(), fx.sums.clone(), fx.payload.clone());
        let (base, served) = fixture_server(move |_, path| {
            let release = "/download/desktop-v99.0.0/";
            match path.strip_prefix(release) {
                _ if path == "/api/version" => {
                    Some(br#"{"desktop":{"min":null,"latest":"99.0.0"}}"#.to_vec())
                }
                Some(name) if name == asset => Some(payload.clone()),
                Some(name) if name == updater::SUMS_ASSET => Some(sums.clone().into_bytes()),
                _ => None,
            }
        });

        std::env::set_var("EXP_UPDATE_ADVERT", format!("{base}/api/version"));
        std::env::set_var("EXP_UPDATE_DOWNLOAD_BASE", format!("{base}/download"));
        std::env::set_var("EXP_UPDATE_API", format!("{base}/releases-api"));
        std::env::set_var("APPIMAGE", &fx.appimage);
        let info = fetch_latest().unwrap().expect("newer release found");
        for key in ["EXP_UPDATE_ADVERT", "EXP_UPDATE_DOWNLOAD_BASE", "EXP_UPDATE_API", "APPIMAGE"] {
            std::env::remove_var(key);
        }

        assert_eq!(info.version, "99.0.0");
        let plan = info.plan.as_ref().expect("plan from the download URLs");
        assert_eq!(plan.asset_url, format!("{base}/download/desktop-v99.0.0/{}", fx.asset_name));
        assert_eq!(plan.sums_url, format!("{base}/download/desktop-v99.0.0/{}", updater::SUMS_ASSET));
        assert_eq!(plan.sig_url, None, "the fixture release is unsigned (404)");
        install_and_check(info, &fx);
        let served = served.lock().unwrap().clone();
        assert!(served.iter().all(|line| !line.contains("releases-api")), "{served:?}");
        assert!(served.iter().any(|line| line.starts_with("HEAD /download/desktop-v99.0.0/")), "{served:?}");
    }

    /// EXP-1761: an advertised release missing our asset asks the Releases
    /// API for an older one that has it (EXP-1693); the API failing too (rate
    /// limited) leaves the advertised release as banner-only.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_advertised_release_without_our_asset_falls_back_to_banner_only() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fx = fixture("banner");
        let (base, _served) = fixture_server(move |_, path| {
            (path == "/api/version").then(|| br#"{"desktop":{"latest":"99.0.0"}}"#.to_vec())
        });
        std::env::set_var("EXP_UPDATE_ADVERT", format!("{base}/api/version"));
        std::env::set_var("EXP_UPDATE_DOWNLOAD_BASE", format!("{base}/download"));
        std::env::set_var("EXP_UPDATE_API", format!("{base}/releases-api"));
        std::env::set_var("APPIMAGE", &fx.appimage);
        let info = fetch_latest();
        for key in ["EXP_UPDATE_ADVERT", "EXP_UPDATE_DOWNLOAD_BASE", "EXP_UPDATE_API", "APPIMAGE"] {
            std::env::remove_var(key);
        }
        let info = info.unwrap().expect("the advertised release is still offered");
        assert_eq!(info.version, "99.0.0");
        assert!(info.plan.is_none(), "banner-only, never a broken install");
        let _ = std::fs::remove_dir_all(&fx.dir);
    }

    /// The fallback (Linux): no advert (404), so the Releases API list drives
    /// check → plan → pipeline as before EXP-1761.
    #[cfg(target_os = "linux")]
    #[test]
    fn end_to_end_from_the_releases_api_when_the_advert_is_unknown() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fx = fixture("api");
        let (asset, sums, payload) = (fx.asset_name.clone(), fx.sums.clone(), fx.payload.clone());
        let listener_base = std::sync::Arc::new(std::sync::OnceLock::<String>::new());
        let base_for_route = listener_base.clone();
        let (base, _served) = fixture_server(move |_, path| {
            let base = base_for_route.get().unwrap();
            match path {
                "/releases-api" => Some(
                    format!(
                        r#"[{{"tag_name":"desktop-v99.0.0","html_url":"{base}/releases",
                            "assets":[
                              {{"name":"{}","browser_download_url":"{base}/sums"}},
                              {{"name":"{asset}","browser_download_url":"{base}/asset"}}
                            ]}}]"#,
                        updater::SUMS_ASSET
                    )
                    .into_bytes(),
                ),
                "/sums" => Some(sums.clone().into_bytes()),
                "/asset" => Some(payload.clone()),
                _ => None,
            }
        });
        listener_base.set(base.clone()).unwrap();

        std::env::set_var("EXP_UPDATE_ADVERT", format!("{base}/api/version"));
        std::env::set_var("EXP_UPDATE_API", format!("{base}/releases-api"));
        std::env::set_var("APPIMAGE", &fx.appimage);
        let info = fetch_latest().unwrap().expect("newer release found");
        for key in ["EXP_UPDATE_ADVERT", "EXP_UPDATE_API", "APPIMAGE"] {
            std::env::remove_var(key);
        }
        assert_eq!(info.version, "99.0.0");
        assert_eq!(info.url, format!("{base}/releases"));
        install_and_check(info, &fx);
    }

    #[test]
    fn release_assets_deserialize() {
        let release: Release = serde_json::from_str(
            r#"{
                "tag_name": "desktop-v9.9.9",
                "html_url": "https://example.com/r",
                "assets": [
                    {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.com/sums"},
                    {"name": "Exponential-production-x86_64.AppImage", "browser_download_url": "https://example.com/ai"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].name, "SHA256SUMS.txt");
    }
}
