//! EXP-1533 — a claude profile's LONG-LIVED login: the token `claude
//! setup-token` mints (one year, `user:inference` scope), kept beside the
//! profile's own config and handed to every run as `CLAUDE_CODE_OAUTH_TOKEN`.
//!
//! Why it exists (#FEED-175): a browser login is an access token plus a
//! single-use refresh grant, and the CLI, our keep-alive and any copy of it
//! all rotate that one grant — whoever loses the race is signed out. A
//! setup token has NO refresh grant: nothing can rotate it away, so a
//! profile that holds one stays signed in until the token's year is up.
//!
//! The trade-off, by Anthropic's design: the token only carries
//! `user:inference`, so it cannot read the usage endpoint or name its
//! account. A profile keeps its browser login (when it has one) for exactly
//! that — the email it shows and the usage windows — while runs spend the
//! token. A token-only profile shows the email it last signed in as
//! (`agent_profiles::known_email`) and the live windows its runs report.
//!
//! The token is a secret: it is written 0600 ([`eagent::atomic_file`]),
//! never logged, never `Debug`ged, and never leaves the machine — the
//! device command that minted it reports only the expiry.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file inside the profile's config dir. Dot-named like the CLI's own
/// `.credentials.json`, and ours alone: the CLI never reads it.
pub const TOKEN_FILE: &str = ".exp-setup-token.json";

/// The env var the CLI takes a setup token from (it wins over any stored
/// credential, so a profile's browser login cannot shadow it).
pub const TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// How long `claude setup-token` says its token lives (the CLI's own
/// constant, `31536000` seconds), used when the screen names no duration.
pub const TOKEN_LIFETIME_SECS: i64 = 365 * 24 * 60 * 60;

/// The token prefix the CLI mints (`sk-ant-oat01-…`): the scraper's anchor.
pub const TOKEN_PREFIX: &str = "sk-ant-oat";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupToken {
    pub token: String,
    /// ISO instant the token was minted.
    pub created_at: String,
    /// ISO instant it stops working.
    pub expires_at: String,
}

impl std::fmt::Debug for SetupToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupToken")
            .field("token", &"<redacted>")
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl SetupToken {
    /// A token minted at `now` that lives `lifetime_secs`.
    pub fn minted(token: String, now: chrono::DateTime<chrono::Utc>, lifetime_secs: i64) -> Self {
        let expires = now + chrono::Duration::seconds(lifetime_secs.max(0));
        Self {
            token,
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            expires_at: expires.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    /// Whether the token still works at `now` (an unreadable expiry counts as
    /// expired: a token we cannot date is not one we promise).
    pub fn valid_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|expires| expires.with_timezone(&chrono::Utc) > now)
            .unwrap_or(false)
    }
}

/// The token stored in `profile_dir`, if any (expired or not).
pub fn read(profile_dir: &Path) -> Option<SetupToken> {
    let raw = std::fs::read_to_string(profile_dir.join(TOKEN_FILE)).ok()?;
    let token: SetupToken = serde_json::from_str(&raw).ok()?;
    token.token.starts_with(TOKEN_PREFIX).then_some(token)
}

/// The token stored in `profile_dir` when it still works now.
pub fn valid(profile_dir: &Path) -> Option<SetupToken> {
    read(profile_dir).filter(|token| token.valid_at(chrono::Utc::now()))
}

/// Store `token` in `profile_dir` (0600, one rename).
pub fn write(profile_dir: &Path, token: &SetupToken) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(token).map_err(std::io::Error::other)?;
    eagent::atomic_file::write_atomic(&profile_dir.join(TOKEN_FILE), &json)
}

/// Forget the token (a sign-out). A missing file is already forgotten.
pub fn remove(profile_dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(profile_dir.join(TOKEN_FILE)) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

/// The spawn env pair a run in `profile_dir` gets: the token when it still
/// works, nothing otherwise (the run then uses the profile's browser login).
pub fn env(profile_dir: &Path) -> Option<(String, String)> {
    valid(profile_dir).map(|token| (TOKEN_ENV.to_string(), token.token))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-claude-token-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_token_round_trips_private_and_expires() {
        let dir = temp_dir("round-trip");
        let now = chrono::Utc::now();
        let token = SetupToken::minted("sk-ant-oat01-abc".into(), now, TOKEN_LIFETIME_SECS);
        write(&dir, &token).unwrap();
        assert_eq!(read(&dir), Some(token.clone()));
        assert_eq!(env(&dir), Some((TOKEN_ENV.to_string(), "sk-ant-oat01-abc".to_string())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(TOKEN_FILE)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert!(!format!("{token:?}").contains("abc"), "never Debug a token");
        assert!(token.valid_at(now));
        assert!(!token.valid_at(now + chrono::Duration::seconds(TOKEN_LIFETIME_SECS + 1)));

        let expired = SetupToken::minted("sk-ant-oat01-old".into(), now - chrono::Duration::days(400), TOKEN_LIFETIME_SECS);
        write(&dir, &expired).unwrap();
        assert!(read(&dir).is_some());
        assert_eq!(valid(&dir), None);
        assert_eq!(env(&dir), None);

        remove(&dir).unwrap();
        remove(&dir).unwrap();
        assert_eq!(read(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_without_a_setup_token_is_ignored() {
        let dir = temp_dir("junk");
        std::fs::write(dir.join(TOKEN_FILE), r#"{"token":"hello","createdAt":"x","expiresAt":"y"}"#).unwrap();
        assert_eq!(read(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
