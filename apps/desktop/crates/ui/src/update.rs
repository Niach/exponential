//! In-app update check (masterplan v5 §11.2).
//!
//! On launch the app hits the GitHub Releases API once (daily-debounced,
//! non-blocking, offline-safe) and — when the latest published `desktop-v*`
//! release is newer than the compiled version — flips a global flag that the
//! [`crate::workspace::Workspace`] shell renders as a dismissible
//! "Update available — download" banner linking the release page.
//!
//! This is deliberately NOT auto-update: no download, no self-replace, no
//! zsync. Full Sparkle-style auto-update is post-release (§11.2). The check is
//! best-effort — every failure path (offline, rate-limited, malformed JSON) is
//! swallowed so it can never take the shell down or block startup.
//!
//! The **staging** channel skips the check entirely: staging builds track
//! `next.exponential.at` and are hand-deployed, so a production release tag
//! must never nag a staging user (mirrors the login `CLOUD_INSTANCE` split).

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use gpui::{App, AppContext as _, Entity, Global};
use serde::Deserialize;

/// GitHub Releases "latest published, non-draft, non-prerelease" endpoint —
/// the exact URL the packet spec names.
const RELEASES_API: &str = "https://api.github.com/repos/Niach/exponential/releases/latest";
/// Human release page — the banner's fallback link when a release omits its
/// own `html_url`.
const RELEASES_PAGE: &str = "https://github.com/Niach/exponential/releases/latest";
/// Only desktop releases (`desktop-v1.2.3`) drive the desktop banner — the repo
/// also ships web `v*` and `android-v*` tags whose versions are unrelated, and
/// `releases/latest` can point at any of them.
const DESKTOP_TAG_PREFIX: &str = "desktop-v";
/// Daily debounce (§11.2): one network check per 24h across launches.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
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
/// version via `EXP_DESKTOP_VERSION` (the workspace `Cargo.toml` version is a
/// static `0.1.0` placeholder that would never advance); local/dev builds fall
/// back to it, which simply means the comparison always reports an update
/// available off a real release — harmless in dev.
fn current_version() -> &'static str {
    option_env!("EXP_DESKTOP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// A newer release the user can download.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// The dotted version (tag with the `desktop-v` prefix stripped).
    pub version: String,
    /// The release page to open in the browser.
    pub url: String,
}

/// Shell-observed update state. Lives behind a gpui global so the check task
/// (which only has an `App`) and the per-window `Workspace` render both reach
/// the same entity.
#[derive(Default)]
pub struct UpdateState {
    available: Option<UpdateInfo>,
    dismissed: bool,
}

impl UpdateState {
    /// The banner to show, or `None` when there is no update or the user
    /// dismissed it this session.
    pub fn banner(&self) -> Option<&UpdateInfo> {
        if self.dismissed {
            None
        } else {
            self.available.as_ref()
        }
    }

    /// Dismiss the banner for the rest of this session (not persisted — a fresh
    /// launch that still sees a newer release shows it again).
    pub fn dismiss(&mut self) {
        self.dismissed = true;
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
}

struct UpdateGlobal(Entity<UpdateState>);
impl Global for UpdateGlobal {}

/// Kick off the launch-time update check (call once from the app bootstrap,
/// after the globals are installed). Non-blocking: the network hit runs on the
/// background executor and only touches the foreground to flip the flag.
pub fn check_for_updates(cx: &mut App) {
    // Materialize the global up front so the shell's `global_ref` never has to
    // create it from an immutable render context.
    let model = UpdateState::global(cx);

    // Staging channel never nags (§11.2).
    if !CHANNEL_CHECKS_UPDATES {
        return;
    }
    // Daily debounce: skip the network entirely if we checked recently.
    if !due_for_check() {
        return;
    }

    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { fetch_latest() })
            .await;

        // Only record the check on a real network response (Ok) so an offline
        // launch retries next time instead of going quiet for a day.
        if let Ok(maybe) = result {
            record_check();
            if let Some(info) = maybe {
                let _ = cx.update(|cx| {
                    model.update(cx, |state, cx| {
                        state.available = Some(info);
                        cx.notify();
                    });
                });
            }
        }
    })
    .detach();
}

/// GitHub release payload — only the two fields we read.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
}

/// Blocking fetch + compare. `Ok(Some)` = a newer desktop release,
/// `Ok(None)` = up to date / non-desktop latest, `Err(())` = network/parse
/// failure (don't record the check so we retry).
fn fetch_latest() -> Result<Option<UpdateInfo>, ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    // `into_string` + `serde_json` rather than `into_json` — the workspace
    // `ureq` is built without the `json` feature (image_paste.rs relies on the
    // same), so decode the body ourselves.
    let body = agent
        .get(RELEASES_API)
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
        }))
    } else {
        Ok(None)
    }
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

// ---- Daily-debounce stamp -------------------------------------------------

/// Per-user file holding the epoch-seconds of the last successful check.
fn stamp_path() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join(if cfg!(target_os = "macos") {
        "Exponential"
    } else {
        "exponential"
    });
    Some(dir.join("update-check.txt"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// True when we've never checked or the last check is older than the interval.
fn due_for_check() -> bool {
    let Some(path) = stamp_path() else {
        return true;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return true;
    };
    let Ok(last) = text.trim().parse::<u64>() else {
        return true;
    };
    now_secs().saturating_sub(last) >= CHECK_INTERVAL.as_secs()
}

/// Best-effort persist of "we checked just now".
fn record_check() {
    let Some(path) = stamp_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, now_secs().to_string());
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
}
