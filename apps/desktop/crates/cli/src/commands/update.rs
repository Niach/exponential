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
            nudge_running_daemon(&crate::context::data_dir(), &mut std::io::stdout());
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

    let exe = running_exe().context("resolve the running binary path")?;
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

/// EXP-641: the binary on disk just changed under a running daemon, which
/// keeps executing the OLD inode until it re-execs. Restart it through its
/// service when it is idle; with live sessions (never kill an agent under a
/// user) or without a service, say what to do instead. Its own gated/
/// scheduled check would eventually get there too — but "eventually" is up
/// to 6h, and a 426-gated daemon is useless meanwhile.
fn nudge_running_daemon(data_dir: &std::path::Path, out: &mut impl std::io::Write) {
    use super::daemon;
    let Some(pid) = daemon::daemon_pid(data_dir) else {
        return;
    };
    let live = crate::registry::sessions_owned_by(data_dir, pid);
    if live > 0 {
        let _ = writeln!(
            out,
            "The daemon (pid {pid}) still runs the previous version and has {live} live session(s); restart it once they finish: {}",
            daemon::restart_hint()
        );
        return;
    }
    match daemon::restart_service() {
        Ok(true) => {
            let _ = writeln!(out, "Daemon restarted on the new version.");
        }
        Ok(false) => {
            let _ = writeln!(
                out,
                "The daemon (pid {pid}) still runs the previous version — restart it to pick up the update."
            );
        }
        Err(err) => {
            let _ = writeln!(
                out,
                "Could not restart the daemon (pid {pid}): {err:#} — restart it by hand: {}",
                daemon::restart_hint()
            );
        }
    }
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
            // EXP-641: a daemon on this machine now runs a stale inode too.
            nudge_running_daemon(&data_dir, &mut std::io::stderr());
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
    let Ok(exe) = running_exe() else {
        return;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(exe).args(args).exec();
    log::warn!("re-exec after update failed: {err}");
}

/// The path this binary lives at, healed of Linux's post-replace artifact
/// (EXP-414): once the installer renames the new binary over the running
/// one, `/proc/self/exe` — and so `std::env::current_exe()` — reads
/// `<path> (deleted)`. Left as-is, the re-exec after an install fails with
/// ENOENT (the daemon keeps running the OLD version, so the update loops),
/// and a NEXT install writes a literal `exponential (deleted)` file next to
/// the real binary. Strip the suffix whenever the raw path is gone and the
/// stripped sibling exists.
pub fn running_exe() -> std::io::Result<std::path::PathBuf> {
    Ok(heal_deleted_suffix(std::env::current_exe()?))
}

fn heal_deleted_suffix(path: std::path::PathBuf) -> std::path::PathBuf {
    const SUFFIX: &str = " (deleted)";
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path;
    };
    let Some(stripped) = name.strip_suffix(SUFFIX) else {
        return path;
    };
    // Prefer the stripped sibling even when a literal `… (deleted)` file
    // exists — a previous unfixed build may have installed one.
    let healed = path.with_file_name(stripped);
    if healed.exists() {
        healed
    } else {
        path
    }
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
    use super::{heal_deleted_suffix, is_newer};

    #[test]
    fn numeric_not_lexicographic() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.9.0", "0.9.0"));
        assert!(!is_newer("0.8.52", "0.9.0"));
    }

    #[test]
    fn deleted_suffix_heals_to_the_real_binary() {
        let dir = std::env::temp_dir().join(format!(
            "exp-cli-heal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("exponential");
        std::fs::write(&real, b"bin").unwrap();

        // The /proc/self/exe post-replace reading resolves to the real path.
        let suffixed = dir.join("exponential (deleted)");
        assert_eq!(heal_deleted_suffix(suffixed.clone()), real);

        // Even a literal stray `… (deleted)` file (from a pre-fix build)
        // must not win over the real sibling.
        std::fs::write(&suffixed, b"stray").unwrap();
        assert_eq!(heal_deleted_suffix(suffixed.clone()), real);

        // No real sibling → leave the path alone.
        std::fs::remove_file(&real).unwrap();
        assert_eq!(heal_deleted_suffix(suffixed.clone()), suffixed);

        // An ordinary path passes through untouched.
        assert_eq!(heal_deleted_suffix(real.clone()), real);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
