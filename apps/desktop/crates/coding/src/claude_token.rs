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
//! `user:inference` (the CLI asks for exactly that scope and nothing can
//! widen it), so it cannot read the usage endpoint (`user:profile`) or name
//! its account. A profile keeps its browser login (when it has one) for
//! exactly that — the email it shows and every usage window — while runs
//! spend the token. A token-only profile shows the email it last signed in
//! as (`agent_profiles::known_email`), and its 5h + weekly windows come from
//! the `anthropic-ratelimit-unified-*` headers every model response carries,
//! as the CLI reports them in its `rate_limit_event`: live from its runs, and
//! between runs from [`probe_usage`] (EXP-1607). The per-model weekly is the
//! endpoint's alone, so a token-only profile never shows it.
//!
//! The token is a secret: it is written 0600 ([`eagent::atomic_file`]),
//! never logged, never `Debug`ged, and never leaves the machine — the
//! device command that minted it reports only the expiry.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_usage::{parse_claude_rate_limit_windows, ClaudeLimitingWindow, UsageWindow};

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

/// EXP-1607 — the arguments of the usage probe: ONE model request through
/// the unmodified CLI (Anthropic permits subscription credentials in Claude
/// Code only, so we never send a request of our own with the token), as
/// small as the CLI allows: the cheapest model, a one-character prompt and
/// system prompt, no tools, no MCP servers, no settings, no transcript.
/// Measured on 2.1.316: ~360 input + ~120 output tokens, ~2 s.
pub const PROBE_ARGS: &[&str] = &[
    "-p",
    ".",
    "--model",
    "haiku",
    "--max-turns",
    "1",
    "--tools",
    "",
    "--system-prompt",
    ".",
    "--strict-mcp-config",
    "--setting-sources",
    "",
    "--no-session-persistence",
    "--output-format",
    "stream-json",
    "--verbose",
];

/// How long the probe may take before it is killed (a cold CLI start plus
/// one short request; the doctor's probes are far quicker).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The env a probe must not inherit: each OUTRANKS `CLAUDE_CODE_OAUTH_TOKEN`
/// in the CLI's credential order, so a stray one would answer for an API
/// key or a cloud account instead of this login.
const OUTRANKING_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// EXP-1607 — the windows a probe's stream-json reported: the LAST
/// `rate_limit_event` that carried any (the CLI prints one per response).
/// `None` when no frame carried windows (an API-key answer, an auth error,
/// a CLI too old to print the frame).
pub fn parse_probe_output(stdout: &str) -> Option<Vec<UsageWindow>> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter(|frame| frame.get("type").and_then(Value::as_str) == Some("rate_limit_event"))
        .filter_map(|frame| {
            let info = frame.get("rate_limit_info")?;
            let kind = info.get("rateLimitType").and_then(Value::as_str);
            let utilization = info.get("utilization").and_then(Value::as_f64);
            let limiting = kind.zip(utilization).map(|(kind, utilization)| ClaudeLimitingWindow {
                kind,
                utilization,
                resets_at_secs: info.get("resetsAt").and_then(Value::as_i64),
            });
            let windows = parse_claude_rate_limit_windows(info.get("unifiedWindows"), limiting);
            (!windows.is_empty()).then_some(windows)
        })
        .last()
}

/// EXP-1607 — read a token-only login's 5h + weekly windows: run
/// [`PROBE_ARGS`] with the token in `scratch_dir` (its own config home, so
/// the probe never touches the profile's stored login or transcripts).
/// Blocking. The caller rations it: it is a model request, and it counts
/// (a few hundred tokens) against the very windows it reads.
pub fn probe_usage(
    program: &str,
    path_env: &str,
    token: &SetupToken,
    scratch_dir: &Path,
) -> Option<Vec<UsageWindow>> {
    std::fs::create_dir_all(scratch_dir).ok()?;
    let mut cmd = eagent::process::background_command(program);
    for key in OUTRANKING_ENV {
        cmd.env_remove(key);
    }
    cmd.env("PATH", path_env)
        .env("CLAUDE_CONFIG_DIR", scratch_dir)
        .env(TOKEN_ENV, &token.token)
        .current_dir(scratch_dir)
        .args(PROBE_ARGS);
    let output = crate::doctor::output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    parse_probe_output(&String::from_utf8_lossy(&output.stdout))
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

    /// The measured 2.1.316 probe output (an inference-only login): the
    /// windows come off the LAST frame that carries any, the other lines are
    /// noise, and output with no frame is no answer.
    #[test]
    fn a_probe_reads_the_windows_off_the_rate_limit_event() {
        let stdout = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1793905200,"rateLimitType":"seven_day","unifiedWindows":{"seven_day":{"utilization":0.2,"resetsAt":1793905200}}}}"#,
            "not json",
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1793905200,"rateLimitType":"seven_day","overageStatus":"rejected","unifiedWindows":{"five_hour":{"utilization":0.07,"resetsAt":1793577600},"seven_day":{"utilization":0.31,"resetsAt":1793905200,"status":"allowed"}}}}"#,
            r#"{"type":"result","subtype":"success"}"#,
        ]
        .join("\n");
        let windows = parse_probe_output(&stdout).expect("windows");
        assert_eq!(
            windows.iter().map(|window| (window.key.as_str(), window.percent)).collect::<Vec<_>>(),
            vec![("session", 7), ("weekly", 31)]
        );
        assert_eq!(windows[1].resets_at.as_deref(), Some("2026-11-05T19:00:00.000Z"));
        // A frame without the map still answers for its limiting window.
        let triple = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1793905200,"rateLimitType":"five_hour","utilization":1.0}}"#;
        assert_eq!(parse_probe_output(triple).unwrap()[0].percent, 100);
        assert_eq!(parse_probe_output(r#"{"type":"result"}"#), None);
        assert_eq!(parse_probe_output(""), None);
    }

    /// The probe is one tiny model request through the CLI itself and never
    /// keeps a transcript.
    #[test]
    fn the_probe_is_one_minimal_print_mode_turn() {
        let args = PROBE_ARGS.join(" ");
        for flag in ["-p", "--model haiku", "--max-turns 1", "--no-session-persistence", "--output-format stream-json", "--strict-mcp-config"] {
            assert!(args.contains(flag), "{flag}");
        }
    }

    #[test]
    fn a_file_without_a_setup_token_is_ignored() {
        let dir = temp_dir("junk");
        std::fs::write(dir.join(TOKEN_FILE), r#"{"token":"hello","createdAt":"x","expiresAt":"y"}"#).unwrap();
        assert_eq!(read(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
