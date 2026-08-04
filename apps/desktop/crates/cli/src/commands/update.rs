//! `exponential update` + the auto-update engine (EXP-403). The desktop
//! owns `releases/latest`, so this lists releases and filters by the
//! `cli-v` tag prefix; the asset is a bare binary (`exponential-<target>`)
//! verified against SHA256SUMS.txt and swapped in with the updater's
//! atomic-rename install.
//!
//! Auto-update: opted into on the first interactive run (`cliAutoUpdate` in
//! settings.json). One-shot commands check at most daily and RE-EXEC
//! themselves after an install (same pid, same argv — the new binary picks
//! the work up from the start); the daemon checks on its own cadence and
//! on the web "Update" button, restarting only while no session is live.
//! The deferral is visible remotely (EXP-411): heartbeats carry the
//! live-session count, so a parked request reads "Update queued" in the
//! machine rows instead of spinning until the last session closes.

use std::process::ExitCode;

use anyhow::{bail, Context as _};
use serde::Deserialize;

use super::{reject_unknown_flags, CommandResult};
use crate::prefs;

const RELEASES_URL: &str = "https://api.github.com/repos/Niach/exponential/releases?per_page=100";
const TAG_PREFIX: &str = "cli-v";

/// One-shot commands re-check at most daily; the daemon a bit more often.
pub const ONE_SHOT_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
pub const DAEMON_CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug)]
pub enum UpdateOutcome {
    /// The binary on disk was replaced — the RUNNING process is still the
    /// old code; re-exec (or tell the user) to pick it up.
    Updated { version: String },
    UpToDate { latest: String },
    NoRelease,
}

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    println!("Current version: {}", crate::cli_version());
    match check_and_install()? {
        UpdateOutcome::Updated { version } => {
            println!("Updated to {version}.");
            Ok(ExitCode::SUCCESS)
        }
        UpdateOutcome::UpToDate { latest } => {
            println!("Already up to date ({latest} is the latest).");
            Ok(ExitCode::SUCCESS)
        }
        UpdateOutcome::NoRelease => {
            println!("No CLI release found.");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// The engine: resolve the newest cli-v* release, and when it is newer than
/// the running version, download + verify + atomically install it over the
/// current binary. Stamps the throttle timestamp on every attempt.
pub fn check_and_install() -> anyhow::Result<UpdateOutcome> {
    prefs::set_last_update_check(&crate::context::data_dir(), prefs::now_epoch());
    let current = crate::cli_version();

    let url = std::env::var("EXP_UPDATE_API")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| RELEASES_URL.to_string());
    let body = updater::fetch_text(&url).context("list releases")?;
    let releases: Vec<Release> = serde_json::from_str(&body).context("decode the release list")?;

    let Some((version, release)) = releases.iter().find_map(|release| {
        let version = release.tag_name.strip_prefix(TAG_PREFIX)?;
        Some((version.to_string(), release))
    }) else {
        return Ok(UpdateOutcome::NoRelease);
    };

    if !is_newer(&version, current) {
        return Ok(UpdateOutcome::UpToDate { latest: version });
    }
    log::info!("updating exponential {current} -> {version}");

    let asset_name = updater::cli_asset_name();
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| {
            format!(
                "release {} has no asset {asset_name} for this machine",
                release.tag_name
            )
        })?;
    let sums = release
        .assets
        .iter()
        .find(|asset| asset.name == updater::SUMS_ASSET)
        .context("release has no SHA256SUMS.txt")?;

    let exe = std::env::current_exe().context("resolve the running binary path")?;
    let parent = exe.parent().context("binary has no parent directory")?;
    let probe = parent.join(".exp-cli-write-probe");
    if std::fs::write(&probe, b"x").is_err() {
        bail!(
            "cannot write to {} — re-run the install script (or update with elevated permissions)",
            parent.display()
        );
    }
    let _ = std::fs::remove_file(&probe);

    let staging = updater::staging_dir().context("resolve the staging dir")?;
    let staged = staging.join(&asset_name);
    updater::download(&asset.browser_download_url, &staged, |_received, _total| {})
        .context("download the release asset")?;
    let sums_text =
        updater::fetch_text(&sums.browser_download_url).context("fetch SHA256SUMS.txt")?;
    updater::verify_sha256(&staged, &sums_text, &asset_name).context("checksum verification")?;

    // The AppImage strategy is a plain chmod-755 + atomic rename on unix —
    // exactly a bare-binary self-replace.
    updater::install(&updater::Strategy::AppImage { appimage: exe.clone() }, &staged)
        .context("install the update")?;
    Ok(UpdateOutcome::Updated { version })
}

/// Whether an auto-check is due (enabled + throttle elapsed).
pub fn auto_check_due(data_dir: &std::path::Path, interval_secs: u64) -> bool {
    if prefs::auto_update(data_dir) != Some(true) {
        return false;
    }
    match prefs::last_update_check(data_dir) {
        Some(last) => prefs::now_epoch().saturating_sub(last) >= interval_secs,
        None => true,
    }
}

/// One-shot-command hook (called from main before dispatch): when auto-update
/// is on and a daily check is due, update quietly and RE-EXEC the new binary
/// with the original argv — same pid, work starts over on current code.
/// Every failure path just proceeds on the current binary.
pub fn maybe_auto_update_and_reexec() {
    let data_dir = crate::context::data_dir();
    if !auto_check_due(&data_dir, ONE_SHOT_CHECK_INTERVAL_SECS) {
        return;
    }
    match check_and_install() {
        Ok(UpdateOutcome::Updated { version }) => {
            eprintln!("(exponential updated to {version} — restarting)");
            exec_self();
            // exec only returns on failure; the old code keeps working.
        }
        Ok(_) => {}
        Err(err) => log::debug!("auto-update check failed: {err:#}"),
    }
}

/// Replace this process with the (freshly installed) binary at the same
/// path, same argv. Same pid — a daemon pidfile stays valid across it.
pub fn exec_self() {
    use std::os::unix::process::CommandExt as _;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(exe).args(args).exec();
    log::warn!("re-exec after update failed: {err}");
}

/// Numeric `major.minor.patch` compare; pre-release/build suffixes ignored.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn tuple(version: &str) -> (u64, u64, u64) {
        let core = version.split(['-', '+']).next().unwrap_or("");
        let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    }
    tuple(candidate) > tuple(current)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn numeric_not_lexicographic() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.9.0", "0.9.0"));
        assert!(!is_newer("0.8.52", "0.9.0"));
    }
}
