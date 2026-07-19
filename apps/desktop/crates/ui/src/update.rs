//! In-app update check + self-update pipeline (masterplan v5 §11.2, EXP-22).
//!
//! The app hits the GitHub Releases API at launch and then every
//! [`RECHECK_INTERVAL`] while running (non-blocking, offline-safe) — when the
//! latest published `desktop-v*` release is newer than the compiled version it
//! flips a global that the [`crate::shell::Shell`] shell renders as a
//! dismissible banner. The old launch-only, daily-debounced check meant a
//! long-running IDE (or one sharing its stamp file with dev runs) effectively
//! NEVER reported an update (EXP-68) — with near-daily releases the banner
//! could stay silent forever.
//!
//! When the running install can self-update (see [`updater::capability`]) AND
//! the release carries the asset this platform needs, the banner's button runs
//! the full pipeline in-app: download (with progress) → SHA256SUMS verify →
//! swap → "Restart to update" (gpui `set_restart_path` + `restart`). In every
//! other case — dev builds, unwritable installs, releases without our asset
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

/// GitHub Releases "latest published, non-draft, non-prerelease" endpoint —
/// the exact URL the packet spec names. `EXP_UPDATE_API` overrides it at
/// runtime so the whole pipeline can be driven against a fixture release
/// (dev/testing only; release builds never set it).
const RELEASES_API: &str = "https://api.github.com/repos/Niach/exponential/releases/latest";
/// Human release page — the banner's fallback link when a release omits its
/// own `html_url`.
const RELEASES_PAGE: &str = "https://github.com/Niach/exponential/releases/latest";
/// Only desktop releases (`desktop-v1.2.3`) drive the desktop banner — the repo
/// also ships web `v*` and `android-v*` tags whose versions are unrelated, and
/// `releases/latest` can point at any of them.
const DESKTOP_TAG_PREFIX: &str = "desktop-v";
/// Re-check cadence while the app runs. One unauthenticated Releases-API call
/// per interval (limit: 60/h/IP) — an IDE left open for days still learns
/// about new releases, which the old launch-only check never did (EXP-68).
const RECHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);
/// GitHub rejects API calls without a User-Agent.
const USER_AGENT: &str = "exp-desktop-update-check";

/// Whether this build channel checks for updates at all. Production does;
/// staging is skipped (see the module doc). A runtime `const` rather than a
/// `#[cfg]` on the whole path so every helper stays referenced (no dead-code
/// warnings on staging builds).
#[cfg(not(feature = "staging"))]
const CHANNEL_CHECKS_UPDATES: bool = true;
#[cfg(feature = "staging")]
const CHANNEL_CHECKS_UPDATES: bool = false;

/// The version this binary was compiled at. Release CI injects the real tag
/// version via `EXP_DESKTOP_VERSION` (the team `Cargo.toml` version is a
/// static `0.1.0` placeholder that would never advance); local/dev builds fall
/// back to it, which simply means the comparison always reports an update
/// available off a real release — harmless in dev.
fn current_version() -> &'static str {
    option_env!("EXP_DESKTOP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn releases_api() -> String {
    std::env::var("EXP_UPDATE_API").unwrap_or_else(|_| RELEASES_API.to_string())
}

/// The human release page — the blocking view's browser fallback when no
/// release/plan is known (staging, dev, or an assetless release).
pub fn releases_page_url() -> &'static str {
    RELEASES_PAGE
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
                // keeps its retry affordance) and the dismissal.
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
    /// threads) collapse to one transition. When no release is known yet it
    /// kicks off ONE immediate `fetch_latest()` — bypassing the 4h loop — so
    /// the blocking view can offer the in-app "Update now" pipeline rather than
    /// only a browser link. On the staging channel (which publishes no assets
    /// and never checks) the fetch is skipped and the view degrades to the
    /// browser-link fallback.
    pub fn set_blocked(cx: &mut App) {
        let model = Self::global(cx);
        let need_fetch = model.update(cx, |state, cx| {
            if state.blocked {
                return false;
            }
            state.blocked = true;
            cx.notify();
            // Fetch only if we don't already know a release and the channel
            // checks (staging has no published assets to install).
            state.available.is_none() && CHANNEL_CHECKS_UPDATES
        });
        if !need_fetch {
            return;
        }
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { fetch_latest() })
                .await;
            if let Ok(Some(info)) = result {
                let _ = cx.update(|cx| {
                    model.update(cx, |state, cx| state.offer(info, cx));
                });
            }
        })
        .detach();
    }
}

struct UpdateGlobal(Entity<UpdateState>);
impl Global for UpdateGlobal {}

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

    cx.background_executor()
        .spawn(async move { updater::cleanup_staging() })
        .detach();

    cx.spawn(async move |cx| {
        loop {
            let result = cx
                .background_executor()
                .spawn(async move { fetch_latest() })
                .await;

            if let Ok(Some(info)) = result {
                cx.update(|cx| {
                    model.update(cx, |state, cx| state.offer(info, cx));
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
                    UpdateEvent::Failed { message: format!("{err}") }
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
    let sums = updater::fetch_text(&plan.sums_url).context("fetch SHA256SUMS.txt")?;

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
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// Blocking fetch + compare. `Ok(Some)` = a newer desktop release,
/// `Ok(None)` = up to date / non-desktop latest, `Err(())` = network/parse
/// failure (don't record the check so we retry).
fn fetch_latest() -> Result<Option<UpdateInfo>, ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    // `into_string` + `serde_json` rather than `into_json` — the team
    // `ureq` is built without the `json` feature (image_paste.rs relies on the
    // same), so decode the body ourselves.
    let body = agent
        .get(&releases_api())
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| ())?
        .into_string()
        .map_err(|_| ())?;
    let release: Release = serde_json::from_str(&body).map_err(|_| ())?;

    // Only desktop releases carry a version comparable to ours.
    let Some(version) = release.tag_name.strip_prefix(DESKTOP_TAG_PREFIX) else {
        return Ok(None);
    };
    if is_newer(version, current_version()) {
        Ok(Some(UpdateInfo {
            version: version.to_string(),
            url: release.html_url.unwrap_or_else(|| RELEASES_PAGE.to_string()),
            plan: build_plan(&release.assets),
        }))
    } else {
        Ok(None)
    }
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
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    Some(UpdatePlan {
        asset_url: find(&asset_name)?,
        sums_url: find(updater::SUMS_ASSET)?,
        asset_name,
        strategy,
    })
}

/// Numeric `major.minor.patch` compare (pre-release/build metadata ignored —
/// good enough for a "download available" nudge; the human decides).
fn is_newer(candidate: &str, current: &str) -> bool {
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

    /// Headless end-to-end (Linux): a fixture "GitHub" serves the release
    /// JSON, SHA256SUMS.txt, and the AppImage bytes; `$APPIMAGE` points at a
    /// temp file. check → plan → pipeline must swap the file and finish with
    /// a `Ready` event carrying the AppImage as the restart path.
    #[cfg(target_os = "linux")]
    #[test]
    fn end_to_end_pipeline_against_fixture_release() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let dir = std::env::temp_dir().join(format!("exp-update-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let appimage = dir.join("Exponential.AppImage");
        std::fs::write(&appimage, b"old-version").unwrap();

        let strategy = updater::Strategy::AppImage { appimage: appimage.clone() };
        let asset_name = updater::expected_asset_name(&strategy);
        let payload = vec![9u8; 150_000];
        let digest = {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(&payload);
            hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let release_json = format!(
            r#"{{"tag_name":"desktop-v99.0.0","html_url":"http://{addr}/releases",
                "assets":[
                  {{"name":"{}","browser_download_url":"http://{addr}/sums"}},
                  {{"name":"{asset_name}","browser_download_url":"http://{addr}/asset"}}
                ]}}"#,
            updater::SUMS_ASSET
        );
        let sums = format!("{digest}  {asset_name}\n");
        let payload_for_server = payload.clone();
        let server = std::thread::spawn(move || {
            // One request per connection: release JSON, sums, asset (order-free).
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let body: Vec<u8> = if request.starts_with("GET /sums") {
                    sums.clone().into_bytes()
                } else if request.starts_with("GET /asset") {
                    payload_for_server.clone()
                } else {
                    release_json.clone().into_bytes()
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });

        std::env::set_var("EXP_UPDATE_API", format!("http://{addr}/release"));
        std::env::set_var("APPIMAGE", &appimage);
        let info = fetch_latest().unwrap().expect("newer release found");
        std::env::remove_var("EXP_UPDATE_API");
        std::env::remove_var("APPIMAGE");

        assert_eq!(info.version, "99.0.0");
        let plan = info.plan.expect("self-update plan built");
        assert_eq!(plan.asset_name, asset_name);

        let (tx, rx) = flume::unbounded();
        let restart_path = run_pipeline(&plan, &tx).unwrap();
        server.join().unwrap();

        assert_eq!(restart_path, Some(appimage.clone()));
        assert_eq!(std::fs::read(&appimage).unwrap(), payload);
        let events: Vec<_> = rx.drain().collect();
        assert!(events
            .iter()
            .any(|e| matches!(e, UpdateEvent::Progress { received, .. } if *received > 0)));
        assert!(events.iter().any(|e| matches!(e, UpdateEvent::Installing)));

        let _ = std::fs::remove_dir_all(&dir);
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
