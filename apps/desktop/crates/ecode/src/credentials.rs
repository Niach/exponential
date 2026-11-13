//! A remote's credentials: the shape of the server's
//! `repositories.credentials` op (`{remoteUrl, username, token, expiresAt}`,
//! #EXP-1614), whatever produced them (the server, a GitHub token, the device
//! flow, a host's keyring). The token never reaches a `Debug`, a `Display`,
//! an argv or a log: every printable form is redacted (`remote.json`).

use std::fmt;

use ecode_core::{credential_helper_key, needs_refresh, parse_remote, redact, remote_url, RemoteScheme};
use serde::Deserialize;

use crate::{Error, Result};

/// Where to push and as whom. `remote_url` is canonical
/// (`https://host/path.git` or `exp://team/repo`, see [`Credentials::new`]).
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    remote_url: String,
    username: String,
    token: String,
    /// ISO-8601 UTC; `None` = never expires.
    #[serde(default)]
    expires_at: Option<String>,
}

impl Credentials {
    /// Credentials for a remote; `Invalid` unless the URL parses
    /// (`remote.json`: https or exp, no userinfo, no query). The URL is
    /// canonicalised (`.git` added, host lowercased).
    pub fn new(
        remote_url: &str,
        username: impl Into<String>,
        token: impl Into<String>,
        expires_at: Option<String>,
    ) -> Result<Self> {
        let remote = parse_remote(remote_url).ok_or_else(|| Error::Invalid("unsupported remote URL".into()))?;
        Ok(Self { remote_url: ecode_core::remote_url(&remote), username: username.into(), token: token.into(), expires_at })
    }

    /// A GitHub repository (`owner/name`) with an installation, OAuth or
    /// personal token: `x-access-token` is accepted for all three.
    pub fn github(full_name: &str, token: impl Into<String>) -> Self {
        Self {
            remote_url: format!("https://github.com/{full_name}.git"),
            username: "x-access-token".into(),
            token: token.into(),
            expires_at: None,
        }
    }

    /// The same credentials with an expiry.
    pub fn with_expiry(mut self, expires_at: Option<String>) -> Self {
        self.expires_at = expires_at;
        self
    }

    /// Re-validates a deserialised value (the server's answer).
    pub fn validated(self) -> Result<Self> {
        Self::new(&self.remote_url, self.username, self.token, self.expires_at)
    }

    /// The tokenless remote URL: what `origin` is set to.
    pub fn remote_url(&self) -> &str {
        &self.remote_url
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// The raw token. Hand it to a process only through
    /// [`Credentials::credential_answer`] or an env var, never an argv.
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }

    /// The expiry in Unix milliseconds (`None` = never, or unparseable).
    pub fn expires_at_ms(&self) -> Option<i64> {
        self.expires_at.as_deref().and_then(parse_iso8601_ms)
    }

    /// Refresh within the contract margin (`remote.json`).
    pub fn needs_refresh(&self, now_ms: i64) -> bool {
        needs_refresh(self.expires_at_ms(), now_ms)
    }

    /// The host (lowercased, with its port).
    pub fn host(&self) -> String {
        parse_remote(&self.remote_url).map(|r| r.host).unwrap_or_default()
    }

    /// The repository path without `.git` (`owner/name` on GitHub).
    pub fn path(&self) -> String {
        parse_remote(&self.remote_url).map(|r| r.path).unwrap_or_default()
    }

    /// Whether this is an `https://` remote (git's credential helpers apply).
    pub fn is_https(&self) -> bool {
        parse_remote(&self.remote_url).is_some_and(|r| r.scheme == RemoteScheme::Https)
    }

    /// `credential.<remote url>.helper`, scoped to exactly this remote
    /// (`remote.json`); `None` for `exp://` remotes.
    pub fn helper_key(&self) -> Option<String> {
        credential_helper_key(&self.remote_url)
    }

    /// `credential.https://<host>.useHttpPath`: makes git match the
    /// path-scoped helper above.
    pub fn use_http_path_key(&self) -> Option<String> {
        self.is_https().then(|| format!("credential.https://{}.useHttpPath", self.host()))
    }

    /// The URL with the credentials embedded: only ever for one git argv
    /// (a first clone), never stored.
    pub fn authed_url(&self) -> String {
        self.with_userinfo(&self.token)
    }

    /// The loggable form of [`Credentials::authed_url`].
    pub fn redacted_url(&self) -> String {
        self.with_userinfo(ecode_core::REDACTION)
    }

    fn with_userinfo(&self, secret: &str) -> String {
        match parse_remote(&self.remote_url) {
            Some(remote) if remote.scheme == RemoteScheme::Https => {
                format!("https://{}:{secret}@{}/{}.git", self.username, remote.host, remote.path)
            }
            Some(remote) => remote_url(&remote),
            None => self.remote_url.clone(),
        }
    }

    /// The git-credential protocol answer (`username=…\npassword=…\n`).
    pub fn credential_answer(&self) -> String {
        format!("username={}\npassword={}\n", self.username, self.token)
    }

    /// `text` with the token and the authed URL redacted.
    pub fn scrub(&self, text: &str) -> String {
        let without_url = text.replace(&self.authed_url(), &self.redacted_url());
        redact(&without_url, &[self.token.as_str()])
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("remote_url", &self.remote_url)
            .field("username", &self.username)
            .field("token", &ecode_core::REDACTION)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl fmt::Display for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted_url())
    }
}

/// `YYYY-MM-DDTHH:MM[:SS][.fff]Z` → Unix milliseconds (fractions dropped).
pub fn parse_iso8601_ms(raw: &str) -> Option<i64> {
    let (date, time) = raw.trim().split_once('T')?;
    let time = time.strip_suffix('Z').unwrap_or(time).split('.').next()?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    if d.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (t.next()??, t.next()??, t.next().unwrap_or(Some(0))?);
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }
    // days_from_civil (Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some((days * 86400 + hour * 3600 + minute * 60 + second) * 1000)
}

/// Where credentials come from: the server (`exponential`), a GitHub token
/// or the device flow (`github`), a host's keyring, a fixed value.
pub trait CredentialSource: Send + Sync {
    /// Credentials for the repository, fresh enough for one network op.
    fn credentials(&self) -> Result<Credentials>;
}

/// A fixed value (a personal token, a test).
impl CredentialSource for Credentials {
    fn credentials(&self) -> Result<Credentials> {
        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_forms_match_the_coding_crate() {
        let c = Credentials::github("acme/web", "ghs_secret");
        assert_eq!(c.remote_url(), "https://github.com/acme/web.git");
        assert_eq!(c.authed_url(), "https://x-access-token:ghs_secret@github.com/acme/web.git");
        assert_eq!(c.redacted_url(), "https://x-access-token:***@github.com/acme/web.git");
        assert_eq!(c.helper_key().unwrap(), "credential.https://github.com/acme/web.git.helper");
        assert_eq!(c.use_http_path_key().unwrap(), "credential.https://github.com.useHttpPath");
        assert_eq!(c.path(), "acme/web");
        assert!(!format!("{c:?}{c}").contains("ghs_secret"));
        assert_eq!(c.scrub("x ghs_secret y"), "x *** y");
    }

    #[test]
    fn any_host_and_the_exp_scheme() {
        let c = Credentials::new("https://Git.Example.com:8443/team/app", "bot", "t0k", None).unwrap();
        assert_eq!(c.remote_url(), "https://git.example.com:8443/team/app.git");
        assert_eq!(c.helper_key().unwrap(), "credential.https://git.example.com:8443/team/app.git.helper");
        assert_eq!(c.use_http_path_key().unwrap(), "credential.https://git.example.com:8443.useHttpPath");
        let exp = Credentials::new("exp://acme/app", "bot", "t0k", None).unwrap();
        assert_eq!(exp.helper_key(), None);
        assert_eq!(exp.authed_url(), "exp://acme/app");
        assert!(Credentials::new("http://insecure/x", "u", "t", None).is_err());
    }

    #[test]
    fn expiry_and_refresh() {
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.500Z"), Some(1000));
        assert_eq!(parse_iso8601_ms("2026-07-03T12:55:00Z"), Some(1_783_083_300_000));
        let c = Credentials::github("a/b", "t").with_expiry(Some("1970-01-01T00:10:00Z".into()));
        assert!(!c.needs_refresh(0));
        assert!(c.needs_refresh(5 * 60 * 1000));
    }
}
