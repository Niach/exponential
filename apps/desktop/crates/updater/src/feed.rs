//! The release feed without `api.github.com` (EXP-1761).
//!
//! The Releases API allows 60 unauthenticated calls per hour per IP, and every
//! app, CLI and script behind one address shares that budget: a busy machine
//! exhausted it and its IDE never saw a published release. So the newest
//! version comes from the instance's advert (`/api/version`, set per release
//! by the release shepherd), and the files come from plain
//! `github.com/<repo>/releases/download/<tag>/<name>` URLs, which the download
//! already uses and which no API limit covers. A HEAD probe tells whether a
//! release carries this platform's asset yet (EXP-1693: a missing asset means
//! banner-only, never a broken install). The API list stays a fallback only,
//! for when the advert is unreachable or unset.

use std::time::Duration;

use anyhow::{Context as _, Result};

use crate::{client, transient_status, transport_cause, with_retries, Attempt, SIG_ASSET, SUMS_ASSET, USER_AGENT};

/// The cloud instance's client-version advert. Every published desktop and CLI
/// release is advertised there, so it answers for self-hosted installs too.
pub const ADVERT_URL: &str = "https://app.exponential.at/api/version";
/// Release files, per tag: `<base>/<tag>/<asset>`.
pub const DOWNLOAD_BASE: &str = "https://github.com/Niach/exponential/releases/download";
/// A release's human page: `<base>/<tag>`.
pub const RELEASE_PAGE_BASE: &str = "https://github.com/Niach/exponential/releases/tag";

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// The advert URL (`EXP_UPDATE_ADVERT` overrides it: fixtures, dev).
pub fn advert_url() -> String {
    env_or("EXP_UPDATE_ADVERT", ADVERT_URL)
}

/// The download URL of `asset` in release `tag` (`EXP_UPDATE_DOWNLOAD_BASE`
/// overrides the base: fixtures, dev).
pub fn asset_url(tag: &str, asset: &str) -> String {
    format!("{}/{tag}/{asset}", env_or("EXP_UPDATE_DOWNLOAD_BASE", DOWNLOAD_BASE).trim_end_matches('/'))
}

/// The human page of release `tag`.
pub fn release_page(tag: &str) -> String {
    format!("{RELEASE_PAGE_BASE}/{tag}")
}

/// `platform`'s (`desktop`, `cli`) advertised latest version from an
/// `/api/version` body; `None` when unset or the body is not the advert.
pub fn parse_advert(body: &str, platform: &str) -> Option<String> {
    latest_in(&serde_json::from_str(body).ok()?, platform)
}

fn latest_in(advert: &serde_json::Value, platform: &str) -> Option<String> {
    let latest = advert.get(platform)?.get("latest")?.as_str()?.trim();
    (!latest.is_empty()).then(|| latest.to_string())
}

/// `platform`'s advertised latest version. `Ok(None)` = the instance
/// advertises none (unset); `Err` = unreachable or not an advert.
pub fn advertised_latest(platform: &str) -> Result<Option<String>> {
    let url = advert_url();
    let advert: serde_json::Value =
        serde_json::from_str(&crate::fetch_text(&url)?).with_context(|| format!("GET {url}: not JSON"))?;
    Ok(latest_in(&advert, platform))
}

/// Whether `url` serves a file: `Ok(false)` on a 404 (not uploaded yet),
/// `Err` when the answer says nothing (offline, 5xx after retries, 403).
pub fn exists(url: &str) -> Result<bool> {
    with_retries(|| {
        let client = client().map_err(|error| Attempt { error, transient: false })?;
        let response = client
            .head(url)
            .timeout(Duration::from_secs(15))
            .header("User-Agent", USER_AGENT)
            .send()
            .map_err(|err| Attempt {
                error: anyhow::anyhow!("HEAD {url}: {}", transport_cause(&err)),
                transient: true,
            })?;
        let status = response.status();
        if status.is_success() {
            Ok(true)
        } else if status == reqwest::StatusCode::NOT_FOUND {
            Ok(false)
        } else {
            Err(Attempt {
                error: anyhow::anyhow!("HEAD {url}: HTTP {status}"),
                transient: transient_status(status),
            })
        }
    })
}

/// The files of one release an install needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseFiles {
    pub asset_url: String,
    pub sums_url: String,
    /// `None` when the release carries no [`SIG_ASSET`] (a build with a pinned
    /// key then refuses it in the pipeline, EXP-1430).
    pub sig_url: Option<String>,
}

/// Probe release `tag` for `asset` and its checksums. `Ok(None)` = the
/// release does not carry both (yet): banner-only.
pub fn probe_release(tag: &str, asset: &str) -> Result<Option<ReleaseFiles>> {
    let binary_url = asset_url(tag, asset);
    if !exists(&binary_url)? {
        return Ok(None);
    }
    let sums_url = asset_url(tag, SUMS_ASSET);
    if !exists(&sums_url)? {
        return Ok(None);
    }
    let sig_url = asset_url(tag, SIG_ASSET);
    let sig_url = exists(&sig_url)?.then_some(sig_url);
    Ok(Some(ReleaseFiles { asset_url: binary_url, sums_url, sig_url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_advert_names_each_platforms_latest() {
        let body = r#"{"desktop":{"min":"0.14.89","latest":"0.14.144"},"cli":{"min":null,"latest":null}}"#;
        assert_eq!(parse_advert(body, "desktop").as_deref(), Some("0.14.144"));
        assert_eq!(parse_advert(body, "cli"), None);
        assert_eq!(parse_advert(body, "ios"), None);
        assert_eq!(parse_advert("<html>", "desktop"), None);
        assert_eq!(parse_advert(r#"{"desktop":{"latest":" "}}"#, "desktop"), None);
    }

    #[test]
    fn release_files_are_plain_download_urls() {
        assert_eq!(
            format!("{DOWNLOAD_BASE}/desktop-v0.14.144/SHA256SUMS.txt"),
            "https://github.com/Niach/exponential/releases/download/desktop-v0.14.144/SHA256SUMS.txt"
        );
        assert!(!DOWNLOAD_BASE.contains("api.github.com"));
        assert_eq!(
            release_page("desktop-v0.14.144"),
            "https://github.com/Niach/exponential/releases/tag/desktop-v0.14.144"
        );
    }
}
