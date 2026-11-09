//! `exponential update` + the auto-update engine (EXP-403). The newest
//! version comes from the instance advert (`/api/version` → `cli.latest`,
//! EXP-1761) and the files from plain `releases/download/cli-v<ver>/` URLs,
//! neither under GitHub's 60/h/IP API limit; the release list (filtered by
//! the `cli-v` tag prefix) is the fallback only. The asset is a bare binary (`exponential-<target>`)
//! verified against the (signed) SHA256SUMS.txt and swapped in with the updater's
//! atomic-rename install.
//!
//! Auto-update: ON unless opted out (EXP-1489: `exponential update --auto
//! off`, stored as `cliAutoUpdate: false` in settings.json; an install that
//! never answered the prompt, a headless server, updates too). One-shot
//! commands check at most daily and RE-EXEC themselves after an install
//! (same pid, same argv — the new binary picks the work up from the start);
//! the daemon checks every [`DAEMON_CHECK_INTERVAL_SECS`] and on the web
//! "Update" button, and restarts by its turn rule. The wait is visible
//! remotely (EXP-411): heartbeats carry the live-session count, so it
//! reads "Update queued" in the machine rows.

use std::process::ExitCode;

use anyhow::{bail, Context as _};
use serde::Deserialize;

use super::{reject_unknown_flags, CommandResult};
use crate::prefs;

const RELEASES_URL: &str = "https://api.github.com/repos/Niach/exponential/releases?per_page=100";
const TAG_PREFIX: &str = "cli-v";

/// One-shot commands re-check at most daily; the daemon every 15 minutes
/// (EXP-1489: a release reaches every server within minutes; EXP-1761: the
/// advert and the download URLs carry no API rate limit).
pub const ONE_SHOT_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
pub const DAEMON_CHECK_INTERVAL_SECS: u64 = 15 * 60;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
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
    let mut args = args.to_vec();
    // EXP-1111: `--auto on|off` sets the persisted auto-update choice (the
    // installer's default-on, and the opt-out) instead of updating now.
    if let Some(raw) = super::take_value(&mut args, "--auto") {
        reject_unknown_flags(&args)?;
        return set_auto(&crate::context::data_dir(), &raw);
    }
    reject_unknown_flags(&args)?;
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

/// `on`/`off` (plus the usual spellings) → the stored choice (pure).
fn parse_auto(raw: &str) -> Option<bool> {
    match raw.trim().to_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Some(true),
        "off" | "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// `exponential update --auto on|off` (bare `--auto` prints the choice).
/// Enabling on an install that never checked stamps the throttle: the binary
/// was just fetched, so the first check waits a day instead of re-asking
/// GitHub on the very next command.
fn set_auto(data_dir: &std::path::Path, raw: &str) -> CommandResult {
    if raw.trim().is_empty() {
        let state = match prefs::auto_update(data_dir) {
            Some(false) => "off",
            _ => "on",
        };
        println!("Auto-update is {state}.");
        return Ok(ExitCode::SUCCESS);
    }
    let Some(enabled) = parse_auto(raw) else {
        bail!("--auto takes on or off (got `{raw}`)");
    };
    prefs::set_auto_update(data_dir, enabled);
    if enabled && prefs::last_update_check(data_dir).is_none() {
        prefs::set_last_update_check(data_dir, prefs::now_epoch());
    }
    if enabled {
        println!(
            "Auto-update on: commands check daily, the daemon every {} minutes.",
            DAEMON_CHECK_INTERVAL_SECS / 60
        );
    } else {
        println!("Auto-update off: `exponential update` updates by hand.");
    }
    Ok(ExitCode::SUCCESS)
}

/// The engine: resolve the newest cli-v* release, and when it is newer than
/// the running version, download + verify + atomically install it over the
/// current binary. Stamps the throttle timestamp on every attempt.
pub fn check_and_install() -> anyhow::Result<UpdateOutcome> {
    prefs::set_last_update_check(&crate::context::data_dir(), prefs::now_epoch());
    let current = crate::cli_version();
    let asset_name = updater::cli_asset_name();
    let target = match resolve(&asset_name, current)? {
        Resolved::NoRelease => return Ok(UpdateOutcome::NoRelease),
        Resolved::UpToDate { latest } => return Ok(UpdateOutcome::UpToDate { latest }),
        Resolved::Install(target) => target,
    };
    let version = target.version.clone();
    log::info!("updating exponential {current} -> {version}");

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
    updater::download(&target.files.asset_url, &staged, |_received, _total| {})
        .context("download the release asset")?;
    // EXP-1430: a build with a pinned feed key refuses sums it cannot verify.
    let sums_text = updater::fetch_verified_sums(&target.files.sums_url, target.files.sig_url.as_deref())
        .context("fetch the signed SHA256SUMS.txt")?;
    updater::verify_sha256(&staged, &sums_text, &asset_name).context("checksum verification")?;

    // The AppImage strategy is a plain chmod-755 + atomic rename on unix —
    // exactly a bare-binary self-replace.
    updater::install(&updater::Strategy::AppImage { appimage: exe.clone() }, &staged)
        .context("install the update")?;
    Ok(UpdateOutcome::Updated { version })
}

/// A newer release this machine can install, and its files.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    version: String,
    files: updater::feed::ReleaseFiles,
}

#[derive(Debug, PartialEq, Eq)]
enum Resolved {
    Install(Target),
    UpToDate { latest: String },
    NoRelease,
}

/// EXP-1761: the advert first, the release list only when it can't answer.
fn resolve(asset_name: &str, current: &str) -> anyhow::Result<Resolved> {
    let advertised = updater::feed::advertised_latest("cli")
        .map_err(|err| log::warn!("update: advert check failed: {err:#}"))
        .ok()
        .flatten();
    let probe = |version: &str| {
        updater::feed::probe_release(&format!("{TAG_PREFIX}{version}"), asset_name)
            .map_err(|err| log::warn!("update: release probe failed: {err:#}"))
            .ok()
            .flatten()
    };
    match resolve_advert(advertised.as_deref(), current, probe) {
        Some(resolved) => Ok(resolved),
        None => Ok(resolve_listed(&list_releases()?, asset_name, current)),
    }
}

/// The advert's answer, `None` = ask the release list: no advert, or the
/// advertised release lacks this machine's binary (an older one may have it).
fn resolve_advert(
    advertised: Option<&str>,
    current: &str,
    probe: impl FnOnce(&str) -> Option<updater::feed::ReleaseFiles>,
) -> Option<Resolved> {
    let version = advertised?.trim_start_matches('v');
    if !is_newer(version, current) {
        return Some(Resolved::UpToDate { latest: version.to_string() });
    }
    let files = probe(version)?;
    Some(Resolved::Install(Target { version: version.to_string(), files }))
}

/// The release-list path: the newest cli-v* release carrying `asset_name`.
fn resolve_listed(releases: &[Release], asset_name: &str, current: &str) -> Resolved {
    let Some((version, release)) = newest_release(releases, asset_name) else {
        return Resolved::NoRelease;
    };
    if !is_newer(version, current) {
        return Resolved::UpToDate { latest: version.to_string() };
    }
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    let tag = &release.tag_name;
    Resolved::Install(Target {
        version: version.to_string(),
        files: updater::feed::ReleaseFiles {
            asset_url: url(asset_name).unwrap_or_else(|| updater::feed::asset_url(tag, asset_name)),
            sums_url: url(updater::SUMS_ASSET).unwrap_or_else(|| updater::feed::asset_url(tag, updater::SUMS_ASSET)),
            sig_url: url(updater::SIG_ASSET),
        },
    })
}

/// The published releases, newest-created first (`EXP_UPDATE_API` overrides
/// the feed).
fn list_releases() -> anyhow::Result<Vec<Release>> {
    let url = std::env::var("EXP_UPDATE_API")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| RELEASES_URL.to_string());
    let body = updater::fetch_text(&url).context("list releases")?;
    serde_json::from_str(&body).context("decode the release list")
}

/// #EXP-1628: what an update would do here, without doing it.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateCheck {
    /// A newer build for this machine is published.
    Available { version: String },
    /// The newest published build for this machine is the running one.
    UpToDate { latest: String },
    /// No published release carries this machine's binary.
    NoRelease { asset: String },
}

/// #EXP-1628: the web Update click's answer: [`check_and_install`]'s
/// resolution alone (nothing downloaded, the throttle untouched), so
/// "nothing installable" reaches the person instead of the click's flag
/// silently clearing.
pub fn check_only() -> anyhow::Result<UpdateCheck> {
    let asset_name = updater::cli_asset_name();
    Ok(check_of(resolve(&asset_name, crate::cli_version())?, &asset_name))
}

fn check_of(resolved: Resolved, asset_name: &str) -> UpdateCheck {
    match resolved {
        Resolved::Install(target) => UpdateCheck::Available { version: target.version },
        Resolved::UpToDate { latest } => UpdateCheck::UpToDate { latest },
        Resolved::NoRelease => UpdateCheck::NoRelease { asset: asset_name.to_string() },
    }
}

#[cfg(test)]
fn resolve_check(releases: &[Release], asset_name: &str, current: &str) -> UpdateCheck {
    check_of(resolve_listed(releases, asset_name, current), asset_name)
}

/// #EXP-1628: the `update_check` command's completion: `(ok, message)`;
/// only an installable build is `ok` (the click's flag then applies it).
pub fn update_check_answer(check: anyhow::Result<UpdateCheck>) -> (bool, String) {
    match check {
        Ok(UpdateCheck::Available { version }) => (true, format!("Updating to {version}.")),
        Ok(UpdateCheck::UpToDate { latest }) => {
            (false, format!("Already up to date: {latest} is the newest published build for this machine."))
        }
        Ok(UpdateCheck::NoRelease { asset }) => (false, format!("No published build for this machine ({asset}).")),
        Err(err) => (false, format!("The update check failed: {err:#}")),
    }
}

/// EXP-641: the binary on disk just changed under a running daemon, which
/// keeps executing the OLD inode until it re-execs. Restart it through its
/// service when it is idle; with live sessions (never kill an agent under a
/// user) or without a service, say what to do instead. Its own gated/
/// scheduled check would eventually get there too — but "eventually" is up
/// to 6h, and a 426-gated daemon is useless meanwhile.
///
/// FEED-36: with live sessions the daemon is asked for the update through
/// the server (`devices.requestUpdate`, best-effort — the same flag the web
/// "Update" button sets), so it parks the request itself, the machine row
/// reads "Update queued" with what holds it, and the daemon's restart rule
/// (`updater::restart`, EXP-1489) applies it instead of its 6h check.
fn nudge_running_daemon(data_dir: &std::path::Path, out: &mut impl std::io::Write) {
    use super::daemon;
    let Some(pid) = daemon::daemon_pid(data_dir) else {
        return;
    };
    let live = crate::registry::sessions_owned_by(data_dir, pid);
    if live > 0 {
        let queued = request_daemon_update();
        let _ = writeln!(out, "{}", live_sessions_notice(pid, live, queued, daemon::restart_hint()));
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

/// FEED-36: what `exponential update` says when the daemon still hosts
/// sessions. `queued` = the server holds the request (the daemon parks it
/// and applies it by its restart rule); otherwise only the hand restart is
/// left to name.
fn live_sessions_notice(pid: u32, live: usize, queued: bool, restart_hint: &str) -> String {
    let cap_minutes = updater::restart::MAX_WAIT.as_secs() / 60;
    let mut text = format!(
        "The daemon (pid {pid}) still runs the previous version and has {live} live session(s); it restarts at their next pause between turns, within {cap_minutes} minutes, and they continue on the new version."
    );
    if queued {
        text.push_str(" The update is queued: the Devices page offers \"Update now\" to restart at once.");
    }
    text.push_str(&format!(" To restart it by hand now: {restart_hint}"));
    text
}

/// FEED-36: ask the server to flag this device for an update (the web
/// button's own path). Best-effort — a signed-out CLI or an unreachable
/// server just means the notice names the hand restart alone.
fn request_daemon_update() -> bool {
    let Ok(ctx) = crate::context::load() else {
        return false;
    };
    match api::devices::request_update(&ctx.trpc, &ctx.device_id(), false) {
        Ok(()) => true,
        Err(err) => {
            log::debug!("devices.requestUpdate failed: {err}");
            false
        }
    }
}

/// Whether an auto-check is due (not opted out + throttle elapsed).
/// EXP-1489: an install that never chose (`None`) updates too.
pub fn auto_check_due(data_dir: &std::path::Path, interval_secs: u64) -> bool {
    if prefs::auto_update(data_dir) == Some(false) {
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

/// #FEED-186: the HIGHEST published `cli-v*` release that carries this
/// machine's binary. The API lists by creation date, not version, and a
/// tag whose build never published this target's asset must not hide an
/// older release that has it (nor fail the whole check).
fn newest_release<'a>(releases: &'a [Release], asset_name: &str) -> Option<(&'a str, &'a Release)> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter(|release| release.assets.iter().any(|asset| asset.name == asset_name))
        .filter_map(|release| Some((release.tag_name.strip_prefix(TAG_PREFIX)?, release)))
        .fold(None, |best: Option<(&str, &Release)>, (version, release)| match best {
            Some((top, _)) if !is_newer(version, top) => best,
            _ => Some((version, release)),
        })
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
    use super::{
        heal_deleted_suffix, is_newer, live_sessions_notice, newest_release, parse_auto, resolve_advert,
        resolve_check, update_check_answer, Asset, Release, Resolved, Target, UpdateCheck,
    };
    use updater::feed::ReleaseFiles;

    fn files() -> ReleaseFiles {
        ReleaseFiles {
            asset_url: "https://github.com/Niach/exponential/releases/download/cli-v0.14.144/bin".into(),
            sums_url: "https://github.com/Niach/exponential/releases/download/cli-v0.14.144/SHA256SUMS.txt".into(),
            sig_url: None,
        }
    }

    /// EXP-1761: the advert decides without the release list (the API's
    /// 60/h/IP budget); an advertised release missing this machine's binary,
    /// or no advert at all, falls back to the list.
    #[test]
    fn the_advert_resolves_without_the_release_list() {
        assert_eq!(
            resolve_advert(Some("0.14.144"), "0.14.143", |version| {
                assert_eq!(version, "0.14.144");
                Some(files())
            }),
            Some(Resolved::Install(Target { version: "0.14.144".into(), files: files() }))
        );
        for current in ["0.14.144", "0.14.145"] {
            assert_eq!(
                resolve_advert(Some("0.14.144"), current, |_| panic!("no probe when up to date")),
                Some(Resolved::UpToDate { latest: "0.14.144".into() })
            );
        }
        assert_eq!(resolve_advert(Some("0.14.144"), "0.14.143", |_| None), None);
        assert_eq!(resolve_advert(None, "0.14.143", |_| panic!("no probe")), None);
    }

    /// #EXP-1628: the Update click's answer. Only an installable build is
    /// ok; "nothing for this machine" and a failed check say why.
    #[test]
    fn the_update_check_answers_what_an_update_would_do() {
        let bin = "exponential-x86_64-unknown-linux-gnu";
        let releases = vec![
            release("cli-v0.14.101", false, &[bin]),
            release("cli-v0.14.102", false, &["exponential-aarch64-apple-darwin"]),
            release("cli-v0.14.103", true, &[bin]),
        ];
        assert_eq!(
            resolve_check(&releases, bin, "0.14.100"),
            UpdateCheck::Available { version: "0.14.101".to_string() }
        );
        assert_eq!(
            resolve_check(&releases, bin, "0.14.101"),
            UpdateCheck::UpToDate { latest: "0.14.101".to_string() }
        );
        assert_eq!(
            resolve_check(&releases, "exponential-riscv", "0.14.101"),
            UpdateCheck::NoRelease { asset: "exponential-riscv".to_string() }
        );
        assert_eq!(
            update_check_answer(Ok(UpdateCheck::Available { version: "0.14.101".into() })),
            (true, "Updating to 0.14.101.".to_string())
        );
        assert_eq!(
            update_check_answer(Ok(UpdateCheck::UpToDate { latest: "0.14.101".into() })),
            (false, "Already up to date: 0.14.101 is the newest published build for this machine.".to_string())
        );
        let (ok, message) = update_check_answer(Err(anyhow::anyhow!("list releases: 503")));
        assert!(!ok);
        assert_eq!(message, "The update check failed: list releases: 503");
    }

    fn release(tag: &str, draft: bool, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.to_string(),
            draft,
            assets: assets
                .iter()
                .map(|name| Asset { name: name.to_string(), browser_download_url: String::new() })
                .collect(),
        }
    }

    #[test]
    fn the_newest_release_is_the_highest_version_carrying_this_machines_binary() {
        let bin = "exponential-x86_64-unknown-linux-gnu";
        let releases = vec![
            release("desktop-v0.14.104", false, &["Exponential.AppImage"]),
            release("cli-v0.14.99", false, &[bin]),
            release("cli-v0.14.103", true, &[bin]),
            release("cli-v0.14.102", false, &["exponential-aarch64-apple-darwin"]),
            release("cli-v0.14.101", false, &[bin]),
            release("cli-v0.14.100", false, &[bin]),
        ];
        assert_eq!(newest_release(&releases, bin).map(|(version, _)| version), Some("0.14.101"));
        assert!(newest_release(&releases, "exponential-riscv").is_none());
    }

    #[test]
    fn the_auto_switch_parses() {
        assert_eq!(parse_auto("on"), Some(true));
        assert_eq!(parse_auto(" OFF "), Some(false));
        assert_eq!(parse_auto("1"), Some(true));
        assert_eq!(parse_auto("maybe"), None);
    }

    #[test]
    fn numeric_not_lexicographic() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.9.0", "0.9.0"));
        assert!(!is_newer("0.8.52", "0.9.0"));
    }

    /// FEED-36: the notice names the idle rule, the queued state and the
    /// hand restart — never a bare "restart it once they finish".
    #[test]
    fn the_live_sessions_notice_names_the_idle_rule_and_the_queue() {
        let queued = live_sessions_notice(42, 3, true, "systemctl --user restart exponential-daemon");
        assert!(queued.starts_with("The daemon (pid 42) still runs the previous version and has 3 live session(s); it restarts at their next pause between turns, within 60 minutes, and they continue on the new version."), "{queued}");
        assert!(queued.contains("The update is queued"), "{queued}");
        assert!(queued.contains("\"Update now\""), "{queued}");
        assert!(queued.ends_with("To restart it by hand now: systemctl --user restart exponential-daemon"), "{queued}");
        let unqueued = live_sessions_notice(42, 1, false, "hint");
        assert!(!unqueued.contains("queued"), "{unqueued}");
        assert!(unqueued.ends_with("To restart it by hand now: hint"), "{unqueued}");
    }

    /// EXP-1489: auto-update is on unless opted out — an install that never
    /// answered the prompt (a headless server) updates too.
    #[test]
    fn auto_update_is_on_unless_opted_out() {
        let dir = std::env::temp_dir().join(format!("exp-cli-auto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(super::auto_check_due(&dir, 60), "never chose = on");
        crate::prefs::set_auto_update(&dir, false);
        assert!(!super::auto_check_due(&dir, 60), "opted out");
        crate::prefs::set_auto_update(&dir, true);
        crate::prefs::set_last_update_check(&dir, crate::prefs::now_epoch());
        assert!(!super::auto_check_due(&dir, 60), "throttled");
        let _ = std::fs::remove_dir_all(&dir);
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
