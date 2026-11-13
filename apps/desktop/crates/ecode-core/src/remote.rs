//! eCode's remotes (`fixtures/remote.json` is the spec): parse a remote URL,
//! the git config key that scopes a credential helper to exactly that remote,
//! when to refresh a token, and the log redaction.

use serde::{Deserialize, Serialize};

use crate::{REDACTION, REFRESH_MARGIN_MS};

/// A remote's scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteScheme {
    /// A git host.
    Https,
    /// eCode's own, served by `git-remote-exp`.
    Exp,
}

/// A parsed remote: `host` lowercased (with its port), `path` without `.git`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remote {
    pub scheme: RemoteScheme,
    pub host: String,
    pub path: String,
}

/// `[A-Za-z0-9.-]+(:[0-9]{1,5})?`
fn valid_host(host: &str) -> bool {
    let (name, port) = match host.split_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host, None),
    };
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && port.is_none_or(|p| (1..=5).contains(&p.len()) && p.chars().all(|c| c.is_ascii_digit()))
}

/// `[A-Za-z0-9._-]+`, never `.` or `..`.
fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && segment != "."
        && segment != ".."
}

/// `https://host[:port]/a/b[.git]` or `exp://<team>/<repo>`; `None` otherwise
/// (http, ssh, credentials in the URL, a query or fragment, `..`).
pub fn parse_remote(url: &str) -> Option<Remote> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (RemoteScheme::Https, rest)
    } else {
        (RemoteScheme::Exp, url.strip_prefix("exp://")?)
    };
    // `([^/]+)\/(.+)$`: `.` stops at JS line terminators.
    let (host, path) = rest.split_once('/')?;
    if host.is_empty() || path.is_empty() || path.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    let host = host.to_lowercase();
    if !valid_host(&host) || host.starts_with('.') || host.starts_with('-') {
        return None;
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<&str> = path.split('/').collect();
    if !segments.iter().all(|s| valid_segment(s)) || (scheme == RemoteScheme::Exp && segments.len() != 1) {
        return None;
    }
    Some(Remote { scheme, host, path: path.to_string() })
}

/// The canonical URL git is given (`https://host/path.git`, `exp://team/repo`).
pub fn remote_url(remote: &Remote) -> String {
    match remote.scheme {
        RemoteScheme::Https => format!("https://{}/{}.git", remote.host, remote.path),
        RemoteScheme::Exp => format!("exp://{}/{}", remote.host, remote.path),
    }
}

/// `credential.<url>.helper`, scoped to exactly this https remote; exp
/// remotes authenticate inside `git-remote-exp`, so they get none.
pub fn credential_helper_key(url: &str) -> Option<String> {
    let remote = parse_remote(url).filter(|r| r.scheme == RemoteScheme::Https)?;
    Some(format!("credential.{}.helper", remote_url(&remote)))
}

/// Refresh when the token expires within the contract margin; `None` = never expires.
pub fn needs_refresh(expires_at: Option<i64>, now: i64) -> bool {
    needs_refresh_within(expires_at, now, REFRESH_MARGIN_MS)
}

/// [`needs_refresh`] with an explicit margin.
pub fn needs_refresh_within(expires_at: Option<i64>, now: i64, margin_ms: i64) -> bool {
    expires_at.is_some_and(|at| now + margin_ms >= at)
}

/// Every secret (longest first, empty ones ignored) replaced by the redaction mark.
pub fn redact<S: AsRef<str>>(text: &str, secrets: &[S]) -> String {
    let mut sorted: Vec<&str> = secrets.iter().map(AsRef::as_ref).filter(|s| !s.is_empty()).collect();
    sorted.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then_with(|| a.cmp(b)));
    sorted.dedup();
    let mut out = text.to_string();
    for secret in sorted {
        out = out.replace(secret, REDACTION);
    }
    out
}
