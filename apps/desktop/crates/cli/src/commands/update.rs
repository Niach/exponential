//! `exponential update` — self-update from the `cli-v*` GitHub release
//! train. The desktop owns `releases/latest`, so this lists releases and
//! filters by tag prefix; the asset is a bare binary
//! (`exponential-<target>`) verified against SHA256SUMS.txt and swapped in
//! with the updater's atomic-rename install.

use std::process::ExitCode;

use anyhow::{bail, Context as _};
use serde::Deserialize;

use super::{reject_unknown_flags, CommandResult};

// per_page=100 (the GitHub max): the releases list is shared across FOUR tag
// trains (v*/desktop-v*/android-v*/ios-v*), so a small window could bury the
// newest cli-v* release behind other trains' churn.
const RELEASES_URL: &str = "https://api.github.com/repos/Niach/exponential/releases?per_page=100";
const TAG_PREFIX: &str = "cli-v";

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

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let current = crate::cli_version();
    println!("Current version: {current}");

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
        println!("No CLI release found.");
        return Ok(ExitCode::SUCCESS);
    };

    if !is_newer(&version, current) {
        println!("Already up to date ({version} is the latest).");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Updating to {version}...");

    let asset_name = updater::cli_asset_name();
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| format!("release {} has no asset {asset_name} for this machine", release.tag_name))?;
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
    updater::download(&asset.browser_download_url, &staged, |received, total| {
        if let Some(total) = total {
            eprint!("\r  {} / {} KiB", received / 1024, total / 1024);
        }
    })
    .context("download the release asset")?;
    eprintln!();
    let sums_text = updater::fetch_text(&sums.browser_download_url).context("fetch SHA256SUMS.txt")?;
    updater::verify_sha256(&staged, &sums_text, &asset_name).context("checksum verification")?;

    // The AppImage strategy is a plain chmod-755 + atomic rename on unix —
    // exactly a bare-binary self-replace.
    updater::install(
        &updater::Strategy::AppImage { appimage: exe.clone() },
        &staged,
    )
    .context("install the update")?;
    println!("Updated to {version}.");
    Ok(ExitCode::SUCCESS)
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
