//! EXP-484 — how much of each agent's rate-limit windows this machine has
//! used, as the `devices.agentUsage` wire map, plus the collector that keeps
//! it fresh without ever spending more requests than the policy allows.
//!
//! ```json
//! { "claude": { "fetchedAt": "2026-08-28T10:00:00.000Z", "stale": false,
//!               "windows": [ { "key": "session", "label": "5h",
//!                              "percent": 42, "resetsAt": "…" } ] } }
//! ```
//!
//! Three sources, one shape:
//!
//! * **claude** — the OAuth usage endpoint, read with the CLI's OWN
//!   credential. The credential is read (never written, never refreshed,
//!   never logged) straight from the store `claude` keeps it in, used for
//!   exactly one GET, and dropped. An API-key/Bedrock login is not eligible
//!   at all ([`crate::doctor::ClaudeAuthStatus::usage_eligible`]).
//! * **codex** — its own `codex app-server` JSON-RPC surface
//!   ([`crate::codex_app_server`]); `~/.codex/auth.json` is never touched.
//!
//! Two of them also have a LIVE publisher ([`live`]): a running codex
//! session is pushed `account/rateLimits/updated`, a running claude session
//! prints a `rate_limit_event` per turn (EXP-819), and both land here
//! without a request of their own.
//!
//! Poll policy lives in [`crate::usage_cache`]; this module is the parsing
//! and the orchestration. Everything is BLOCKING — callers run
//! [`collect_if_due`] off the UI/main thread (the desktop's device-sync beat,
//! the daemon's device worker).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::CodingAgent;
use crate::agent_accounts::{iso_from_unix_secs, now_iso, AgentAccount, AgentAccounts};
use crate::doctor::{DoctorReport, MIN_CLAUDE_VERSION};
use crate::settings::Settings;
use crate::usage_cache::{self, AgentCacheEntry, PollOutcome};

/// Hard cap on the windows one agent may report — the clients render a list,
/// the server clamps to the same number, and a runaway answer must never
/// become an unbounded jsonb column.
pub const MAX_WINDOWS: usize = 10;

/// The Anthropic OAuth usage endpoint.
pub const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Whole-request budget for the usage GET.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// EXP-808 — how many account PROFILES of ONE agent a collection pass fans
/// out to.
///
/// The cap is a REQUEST-RATE bound, not a wire bound: the floors
/// ([`usage_cache::MIN_POLL_SECS`], [`usage_cache::SHARED_TTL_SECS`]) are per
/// LOGIN, so N monitored logins are N times the request rate against an
/// endpoint that tolerates ~20/hour. The device's ACTIVE login is always
/// inside it — it is ordered first, never truncated away.
///
/// EXP-849 raised it from 5: a person juggling a handful of client accounts
/// had their sixth login silently drop out of the numbers. Anything still
/// past the cap is reported with `unmonitored: true`
/// ([`crate::agent_accounts::AgentProfileEntry::unmonitored`]) rather than
/// looking like a login with no usage — the identity always ships, only the
/// probe is rationed. It must stay ≤ the server's own clamp on
/// `agentAccounts[].profiles` (`MAX_AGENT_PROFILES` in
/// `packages/db-schema/src/schema.ts`), or a row nobody can read would cost
/// a request.
pub const MAX_USAGE_PROFILES: usize = 12;

/// EXP-808 — the stagger window for the NON-ACTIVE profiles.
///
/// The poll floors ([`usage_cache::MIN_POLL_SECS`],
/// [`usage_cache::SHARED_TTL_SECS`]) are per cache ENTRY, i.e. per LOGIN, so
/// N profiles are N times the request rate against an endpoint that tolerates
/// ~20/hour — and, worse, one beat would fan out into N keychain reads, N
/// `codex app-server` spawns and N GETs at once. So exactly ONE non-active
/// profile ACROSS THE MACHINE is eligible per window, in rotation: a beat
/// costs at most one probe more than the pre-profile build did.
pub const PROFILE_STAGGER_SECS: u64 = 60;

/// One rate-limit window. `key` identifies it across probes (the per-client
/// "which window do I show" preference is stored against it), `label` is the
/// rendered caption, `percent` is 0-100 and `resets_at` is an ISO instant or
/// null.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageWindow {
    pub key: String,
    pub label: String,
    pub percent: u8,
    pub resets_at: Option<String>,
}

/// One agent's usage snapshot. `stale` = these numbers are older than the
/// last attempt (an expired credential, a 401/429, a failed fetch) — the
/// clients dim them and caption "as of …" rather than lying.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentUsage {
    pub fetched_at: String,
    pub stale: bool,
    pub windows: Vec<UsageWindow>,
}

/// `{ agent: usage }` — `BTreeMap` for a deterministic wire.
pub type AgentUsageMap = BTreeMap<String, AgentUsage>;

// ---------------------------------------------------------------------------
// Claude — the OAuth usage body
// ---------------------------------------------------------------------------

/// Parse the usage endpoint's body into windows.
///
/// The modern answer carries a `limits[]` array (each entry a window with a
/// type, a utilization and a reset); an older one carries the two named
/// `five_hour`/`seven_day` objects. An enabled `extra_usage` becomes the
/// `credits` window.
///
/// EXP-688: entries flagged `is_active: false` are KEPT at 0%. Claude's own
/// app shows the idle session window ("0% used · starts when a message is
/// sent"); dropping it made the whole "Current session" group disappear on a
/// machine that had not talked to the agent yet.
///
/// `None` = not a usage body at all (an error page, a changed schema): the
/// caller keeps its previous numbers and marks them stale rather than
/// showing zeroes.
pub fn parse_claude_usage(body: &str) -> Option<Vec<UsageWindow>> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut windows = Vec::new();
    match value.get("limits").and_then(Value::as_array) {
        Some(limits) => {
            for entry in limits {
                if let Some(window) = claude_window(entry) {
                    windows.push(window);
                }
            }
        }
        None => {
            for (field, kind) in [("five_hour", "session"), ("seven_day", "weekly")] {
                let Some(entry) = value.get(field) else {
                    continue;
                };
                let Some((key, label)) = claude_window_identity(kind, None) else {
                    continue;
                };
                let Some(percent) = read_percent(entry) else {
                    continue;
                };
                windows.push(UsageWindow {
                    key,
                    label,
                    percent,
                    resets_at: read_reset(entry),
                });
            }
            if windows.is_empty() && value.get("extra_usage").is_none() {
                return None;
            }
        }
    }
    if let Some(extra) = value.get("extra_usage") {
        if extra.get("is_enabled").and_then(Value::as_bool) == Some(true) {
            windows.push(UsageWindow {
                key: "credits".to_string(),
                label: "Credits".to_string(),
                percent: read_percent(extra).unwrap_or(0),
                resets_at: read_reset(extra),
            });
        }
    }
    windows.truncate(MAX_WINDOWS);
    Some(windows)
}

fn claude_window(entry: &Value) -> Option<UsageWindow> {
    let inactive = entry.get("is_active").and_then(Value::as_bool) == Some(false);
    // Live shape (verified 2026-08-27): `kind` ∈ session|weekly_all|
    // weekly_scoped, the scoped window naming its model under
    // `scope.model.display_name`, `percent` 0-100. The older field names
    // stay accepted so a schema wobble degrades to "still parses".
    let kind = entry
        .get("kind")
        .or_else(|| entry.get("type"))
        .or_else(|| entry.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let display = entry
        .get("scope")
        .and_then(|scope| scope.get("model"))
        .and_then(|model| model.get("display_name"))
        .or_else(|| entry.get("display_name"))
        .or_else(|| entry.get("displayName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|display| !display.is_empty());
    let (key, label) = claude_window_identity(kind, display)?;
    Some(UsageWindow {
        key,
        label,
        // An INACTIVE window reports no utilization at all — that is 0%,
        // not a reason to hide it (EXP-688). An active one without a readable
        // percent is a schema wobble and is dropped rather than shown as 0%.
        percent: match read_percent(entry) {
            Some(percent) => percent,
            None if inactive => 0,
            None => return None,
        },
        resets_at: read_reset(entry),
    })
}

/// The locked key/label vocabulary: `session`/`5h`, `weekly`/`Week`, and
/// everything else a per-MODEL window keyed `model:<display lowercased>`.
fn claude_window_identity(kind: &str, display: Option<&str>) -> Option<(String, String)> {
    let kind = kind.trim().to_ascii_lowercase();
    match kind.as_str() {
        "session" | "five_hour" | "5h" => {
            Some(("session".to_string(), "5h".to_string()))
        }
        "weekly" | "weekly_all" | "seven_day" | "week" => {
            Some(("weekly".to_string(), "Week".to_string()))
        }
        _ => {
            let display = display.map(str::to_string).or_else(|| {
                (!kind.is_empty()).then(|| kind.clone())
            })?;
            let key = format!("model:{}", display.to_ascii_lowercase());
            Some((clamp_key(key), clamp_label(display)))
        }
    }
}

fn clamp_key(key: String) -> String {
    key.chars().take(64).collect()
}

fn clamp_label(label: String) -> String {
    label.chars().take(32).collect()
}

/// A 0-100 percentage off any of the field names the answers have used.
fn read_percent(entry: &Value) -> Option<u8> {
    for field in ["utilization", "used_percent", "usedPercent", "percent"] {
        if let Some(raw) = entry.get(field).and_then(Value::as_f64) {
            return Some(raw.round().clamp(0.0, 100.0) as u8);
        }
    }
    None
}

/// An ISO reset stamp off either an ISO string or a unix-seconds number.
fn read_reset(entry: &Value) -> Option<String> {
    for field in ["resets_at", "resetsAt", "reset_at", "resetAt"] {
        match entry.get(field) {
            Some(Value::String(stamp)) if !stamp.trim().is_empty() => {
                return Some(stamp.trim().to_string())
            }
            Some(Value::Number(number)) => {
                if let Some(secs) = number.as_i64() {
                    return iso_from_unix_secs(secs);
                }
            }
            _ => {}
        }
    }
    None
}

/// EXP-819 — the top-level `rateLimitType` + `utilization` (+ `resetsAt`)
/// triple a `rate_limit_event` names for its LIMITING window: the fallback
/// [`parse_claude_rate_limit_windows`] reads when the frame carries no
/// `unifiedWindows` map at all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClaudeLimitingWindow<'a> {
    /// `five_hour` | `seven_day` | `seven_day_opus` | …
    pub kind: &'a str,
    /// A FRACTION of the window (0-1), the same scale as `unifiedWindows`.
    pub utilization: f64,
    /// Unix seconds.
    pub resets_at_secs: Option<i64>,
}

/// EXP-819 — the windows a LIVE claude session's `rate_limit_event`
/// (`rate_limit_info.unifiedWindows`, EXP-784) reports, in the vocabulary
/// [`parse_claude_usage`] locks: `five_hour` → `session`, `seven_day` →
/// `weekly`.
///
/// Measured off the CLI's own schema (2.1.267): three OPTIONAL arms —
/// `five_hour`, `seven_day`, `seven_day_overage_included` — each
/// `{ utilization, resetsAt }`, `utilization` a FRACTION of the window (0-1;
/// above 1 when usage legitimately runs past the cap) and `resetsAt` unix
/// SECONDS, read off the `anthropic-ratelimit-unified-*` response headers on
/// every turn (an older build, 2.1.263, carried the fraction alone). The
/// overage-included arm is a per-MODEL bucket whose endpoint counterpart is
/// unmeasured, so it is deliberately not published: a guessed key would sit
/// beside the endpoint's model-scoped weekly as a duplicate row. Everything
/// the frame does not carry stays what the endpoint last said — see
/// [`merge_live_windows`].
///
/// `limiting` is the frame's top-level triple, read only for a window the
/// map does not already carry (and never for a model-scoped kind, for the
/// same duplicate-row reason).
pub fn parse_claude_rate_limit_windows(
    unified_windows: Option<&Value>,
    limiting: Option<ClaudeLimitingWindow<'_>>,
) -> Vec<UsageWindow> {
    let mut windows: Vec<UsageWindow> = Vec::new();
    if let Some(unified) = unified_windows.and_then(Value::as_object) {
        for field in ["five_hour", "seven_day"] {
            let Some(entry) = unified.get(field) else {
                continue;
            };
            let Some(utilization) = entry.get("utilization").and_then(Value::as_f64) else {
                continue;
            };
            let Some((key, label)) = claude_window_identity(field, None) else {
                continue;
            };
            windows.push(UsageWindow {
                key,
                label,
                percent: fraction_percent(utilization),
                resets_at: read_reset(entry),
            });
        }
    }
    if let Some(limiting) = limiting {
        if let Some((key, label)) = claude_window_identity(limiting.kind, None)
            .filter(|(key, _)| !key.starts_with("model:"))
        {
            if !windows.iter().any(|window| window.key == key) {
                windows.push(UsageWindow {
                    key,
                    label,
                    percent: fraction_percent(limiting.utilization),
                    resets_at: limiting.resets_at_secs.and_then(iso_from_unix_secs),
                });
            }
        }
    }
    windows.truncate(MAX_WINDOWS);
    windows
}

/// A 0-1 fraction (the rate-limit headers' scale) as the wire's 0-100.
fn fraction_percent(fraction: f64) -> u8 {
    if !fraction.is_finite() {
        return 0;
    }
    (fraction * 100.0).round().clamp(0.0, 100.0) as u8
}

/// EXP-819 — a live frame's windows laid OVER the last full report, by key.
///
/// A key the frame carries takes the frame's percent, and its reset when the
/// frame names one (the report's stays otherwise); every other window — the
/// endpoint's model-scoped weekly, its credits — keeps the report's numbers
/// rather than vanishing; a key the report never had is appended. The order
/// is the report's, so the bar never reshuffles between a poll and a turn.
pub fn merge_live_windows(reported: &[UsageWindow], live: &[UsageWindow]) -> Vec<UsageWindow> {
    let mut merged: Vec<UsageWindow> = reported.to_vec();
    for window in live {
        match merged.iter_mut().find(|slot| slot.key == window.key) {
            Some(slot) => {
                slot.percent = window.percent;
                if window.resets_at.is_some() {
                    slot.resets_at = window.resets_at.clone();
                }
            }
            None => merged.push(window.clone()),
        }
    }
    merged.truncate(MAX_WINDOWS);
    merged
}

/// The User-Agent the CLI itself sends — the usage endpoint answers a
/// browser-shaped agent differently. `version` is the doctor's version line
/// (`"2.1.215 (Claude Code)"`); an unknown one falls back to the minimum
/// supported release.
pub fn claude_user_agent(version: Option<&str>) -> String {
    let fallback = format!(
        "{}.{}.{}",
        MIN_CLAUDE_VERSION.0, MIN_CLAUDE_VERSION.1, MIN_CLAUDE_VERSION.2
    );
    let version = version
        .and_then(|line| line.split_whitespace().next())
        .filter(|token| !token.is_empty())
        .unwrap_or(&fallback);
    format!("claude-cli/{version} (external, cli)")
}

// ---------------------------------------------------------------------------
// Codex — the app-server answers
// ---------------------------------------------------------------------------

/// `account/rateLimits/read` → windows. Codex reports up to two windows by
/// DURATION rather than by name, so the labels derive from the duration:
/// 300 min = the 5h session, 10080 = the week, 43200 = the month, anything
/// else keeps its raw minutes. A credits BALANCE is not a window (it has no
/// percentage and no reset) and is deliberately dropped.
pub fn parse_codex_rate_limits(value: &Value) -> Vec<UsageWindow> {
    let limits = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .unwrap_or(value);
    let mut windows = Vec::new();
    for field in ["primary", "secondary"] {
        let Some(entry) = limits.get(field) else {
            continue;
        };
        let Some(percent) = read_percent(entry) else {
            continue;
        };
        let minutes = ["windowDurationMins", "window_duration_mins", "windowMinutes", "window_minutes", "durationMins"]
            .into_iter()
            .find_map(|name| entry.get(name).and_then(Value::as_i64));
        let (key, label) = codex_window_identity(minutes, field);
        windows.push(UsageWindow {
            key,
            label,
            percent,
            resets_at: read_reset(entry),
        });
    }
    windows.truncate(MAX_WINDOWS);
    windows
}

fn codex_window_identity(minutes: Option<i64>, field: &str) -> (String, String) {
    match minutes {
        Some(300) => ("session".to_string(), "5h".to_string()),
        Some(10080) => ("weekly".to_string(), "Week".to_string()),
        Some(43200) => ("43200".to_string(), "Month".to_string()),
        Some(mins) => (mins.to_string(), format!("{mins}m")),
        // No duration reported: key by the slot so primary and secondary
        // never collide (a shared key would break per-window selection).
        None => (
            field.to_string(),
            match field {
                "primary" => "Primary".to_string(),
                _ => "Secondary".to_string(),
            },
        ),
    }
}

/// `account/read` → the account row. `{"account": null}` is an explicit
/// signed-out answer; an `apiKey` account names no address and captions
/// `api key`. `None` = an answer this build cannot read (fail open — the
/// doctor's presence-only row stands).
pub fn parse_codex_account(value: &Value, now: &str) -> Option<crate::agent_accounts::AgentAccount> {
    use crate::agent_accounts::AgentAccount;
    let account = value.get("account").unwrap_or(value);
    if account.is_null() {
        return Some(AgentAccount {
            signed_in: false,
            checked_at: now.to_string(),
            ..AgentAccount::default()
        });
    }
    let object = account.as_object()?;
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|found| !found.is_empty())
            .map(str::to_string)
    };
    let kind = text("type").unwrap_or_default();
    let api_key = kind.eq_ignore_ascii_case("apikey") || kind.eq_ignore_ascii_case("api_key");
    let plan = if api_key {
        Some("api key".to_string())
    } else {
        text("planType").or_else(|| text("plan_type")).or_else(|| text("plan"))
    };
    Some(AgentAccount {
        signed_in: true,
        email: text("email"),
        plan,
        checked_at: now.to_string(),
        ..AgentAccount::default()
    })
}

// ---------------------------------------------------------------------------
// The credential (READ-ONLY, never persisted, never logged)
// ---------------------------------------------------------------------------

/// An agent's own OAuth credential, borrowed for exactly one usage GET.
/// `Debug` is redacted — this struct must never be able to print a token.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeOauthCredential {
    pub access_token: String,
    /// Expiry in unix MILLIseconds (the field's own unit); `None` = unknown,
    /// which is treated as live (the endpoint's 401 is the real gate).
    pub expires_at_ms: Option<i64>,
    pub subscription_type: Option<String>,
}

impl fmt::Debug for ClaudeOauthCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeOauthCredential")
            .field("access_token", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("subscription_type", &self.subscription_type)
            .finish()
    }
}

impl ClaudeOauthCredential {
    /// Whether the token is past its own expiry — we never refresh one, so
    /// an expired credential means "no numbers until the user's CLI renews
    /// it" (the previous numbers stay, marked stale).
    pub fn expired(&self, now_ms: i64) -> bool {
        self.expires_at_ms.is_some_and(|at| now_ms >= at)
    }
}

/// Parse `claude`'s credential JSON (the keychain item's payload and the
/// `.credentials.json` file share it). Only the OAuth branch is read —
/// nothing else in the object is touched.
pub fn parse_claude_credentials(raw: &str) -> Option<ClaudeOauthCredential> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let oauth = value.get("claudeAiOauth").unwrap_or(&value);
    let access_token = ["accessToken", "access_token", "access"]
        .into_iter()
        .find_map(|field| oauth.get(field).and_then(Value::as_str))
        .map(str::trim)
        .filter(|token| !token.is_empty())?
        .to_string();
    let expires_at_ms = ["expiresAt", "expires_at", "expires"]
        .into_iter()
        .find_map(|field| oauth.get(field).and_then(Value::as_i64));
    let subscription_type = ["subscriptionType", "subscription_type"]
        .into_iter()
        .find_map(|field| oauth.get(field).and_then(Value::as_str))
        .map(str::to_string);
    Some(ClaudeOauthCredential {
        access_token,
        expires_at_ms,
        subscription_type,
    })
}


/// What a credential read produced. `Denied` is its own answer on purpose:
/// a macOS Keychain ACL prompt on a headless daemon must back OFF for an
/// hour, not retry every three minutes.
#[derive(Debug, PartialEq, Eq)]
pub enum CredentialRead {
    Found(ClaudeOauthCredential),
    /// No credential anywhere — the CLI is signed out (or signed in some
    /// other way).
    Missing,
    /// The store refused or timed out.
    Denied,
}

/// Read claude's OAuth credential WITHOUT touching it: the macOS keychain
/// item first (`security find-generic-password -w`, read-only), else the
/// `.credentials.json` file under `CLAUDE_CONFIG_DIR` (or `~/.claude`).
/// Never written, never refreshed, never logged.
pub fn read_claude_credential() -> CredentialRead {
    read_claude_credential_in(None)
}

/// [`read_claude_credential`] for ONE config dir — EXP-792: an account
/// profile's `CLAUDE_CONFIG_DIR`. The file under that dir is read first
/// (the credential a relocated config keeps beside itself); on macOS the
/// keychain item claude names after a non-default dir (the service suffixed
/// with the dir's hash) is tried when the file is absent. `None` = the
/// ambient login, keychain first as before.
pub fn read_claude_credential_in(config_dir: Option<&Path>) -> CredentialRead {
    if let Some(dir) = config_dir {
        match read_credential_file(&dir.join(".credentials.json")) {
            CredentialRead::Missing => {}
            found_or_denied => return found_or_denied,
        }
        #[cfg(target_os = "macos")]
        {
            return match read_keychain_credential(&profile_keychain_service(dir)) {
                Some(read) => read,
                None => CredentialRead::Missing,
            };
        }
        #[cfg(not(target_os = "macos"))]
        {
            return CredentialRead::Missing;
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(read) = read_keychain_credential("Claude Code-credentials") {
        return read;
    }
    let Some(path) = claude_credentials_path() else {
        return CredentialRead::Missing;
    };
    read_credential_file(&path)
}

/// The `.credentials.json` read: absent → `Missing`, unreadable → `Denied`.
fn read_credential_file(path: &Path) -> CredentialRead {
    if !path.exists() {
        return CredentialRead::Missing;
    }
    match std::fs::read_to_string(path) {
        Ok(raw) => match parse_claude_credentials(&raw) {
            Some(credential) => CredentialRead::Found(credential),
            None => CredentialRead::Missing,
        },
        Err(_) => CredentialRead::Denied,
    }
}

/// The keychain service name claude uses for a NON-default config dir:
/// its default service plus `-<first 8 hex of sha256(dir)>`.
#[cfg(target_os = "macos")]
fn profile_keychain_service(dir: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(dir.to_string_lossy().as_bytes());
    format!("Claude Code-credentials-{}", &format!("{digest:x}")[..8])
}

/// One read-only `security find-generic-password -w` for `service`.
/// `None` = "no such item" (fall through to the file); `Some(Denied)` = a
/// refusal or a timeout (the ACL prompt nobody answers).
#[cfg(target_os = "macos")]
fn read_keychain_credential(service: &str) -> Option<CredentialRead> {
    let mut cmd = terminal::process::background_command("/usr/bin/security");
    cmd.args(["find-generic-password", "-s", service, "-w"]);
    match crate::doctor::output_with_timeout(cmd, crate::doctor::PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let raw = String::from_utf8_lossy(&output.stdout);
            parse_claude_credentials(raw.trim()).map(CredentialRead::Found)
        }
        // 44 = "the item cannot be found" — a file-based install.
        // Anything else is a refusal (ACL denial, locked keychain).
        Ok(output) if output.status.code() == Some(44) => None,
        Ok(_) | Err(_) => Some(CredentialRead::Denied),
    }
}

fn claude_credentials_path() -> Option<std::path::PathBuf> {
    let root = match std::env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) if !dir.trim().is_empty() => std::path::PathBuf::from(dir.trim()),
        _ => dirs::home_dir()?.join(".claude"),
    };
    Some(root.join(".credentials.json"))
}

/// The outcome of one usage GET.
#[derive(Debug, PartialEq, Eq)]
pub enum UsageFetch {
    Ok(String),
    /// 401/403 — the credential no longer answers for usage.
    Unauthorized,
    /// 429 — back off past the policy's floor.
    RateLimited,
    Failed,
}

/// GET the OAuth usage endpoint with `access_token`. Blocking, 10 s, over
/// the app's ONE shared HTTP client (EXP-304).
pub fn fetch_oauth_usage(access_token: &str, user_agent: &str) -> UsageFetch {
    let response = api::http::shared()
        .get(CLAUDE_USAGE_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", user_agent)
        .header("Accept", "application/json")
        .timeout(FETCH_TIMEOUT)
        .send();
    match response {
        Ok(response) => match response.status().as_u16() {
            200 => match response.text() {
                Ok(body) => UsageFetch::Ok(body),
                Err(_) => UsageFetch::Failed,
            },
            401 | 403 => UsageFetch::Unauthorized,
            429 => UsageFetch::RateLimited,
            _ => UsageFetch::Failed,
        },
        Err(_) => UsageFetch::Failed,
    }
}

// ---------------------------------------------------------------------------
// Live windows, published by a running session
// ---------------------------------------------------------------------------

/// EXP-754 — the rate-limit windows a LIVE agent session already knows.
///
/// One machine runs ONE `codex app-server` per session, and that connection
/// is pushed `account/rateLimits/updated` on every turn. Those numbers are
/// fresher than anything [`collect_if_due`] could fetch, and fetching them
/// itself costs a SECOND app-server spawn against the same account (the very
/// contention [`crate::usage_cache::SHARED_TTL_SECS`] exists to bound). So
/// the adapter publishes here and the collector reads here.
///
/// EXP-819: a claude session publishes too — its stream-json prints a
/// `rate_limit_event` per turn whose `unifiedWindows` carry the session and
/// weekly windows ([`super::parse_claude_rate_limit_windows`]). That frame
/// is a SUBSET of the OAuth usage endpoint's report (no model-scoped weekly,
/// no credits), so the collector lays it over the endpoint's last report by
/// key ([`super::merge_live_windows`]) instead of replacing it.
///
/// Process-global on purpose: publisher (the engine's codex and claude
/// adapters) and reader (the desktop's device-sync beat, the daemon's
/// device worker) live in the same process. A SIBLING process still shares the on-disk
/// [`crate::usage_cache`], which this path writes exactly like a fetch would.
///
/// `coding` never depends on `engine`, so the publish side is a plain
/// function the adapter calls, not a hook the engine installs.
pub mod live {
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::UsageWindow;
    use crate::agent::CodingAgent;

    /// What one agent's live sessions have published on this machine.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct LiveUsage {
        /// The last windows a session reported (empty = a session is
        /// attached but has not heard a rate-limit frame yet).
        pub windows: Vec<UsageWindow>,
        /// Unix seconds of the last [`publish`].
        pub updated_at_secs: Option<u64>,
        /// How many sessions are attached RIGHT NOW. `> 0` is what makes the
        /// numbers current no matter how long ago the last frame arrived
        /// (an idle turn-less session reports nothing new).
        pub sessions: usize,
    }

    static LIVE: OnceLock<Mutex<HashMap<CodingAgent, LiveUsage>>> = OnceLock::new();

    fn live() -> MutexGuard<'static, HashMap<CodingAgent, LiveUsage>> {
        let lock = LIVE.get_or_init(|| Mutex::new(HashMap::new()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// A live session for `agent` has started. Hold the guard for the run.
    #[must_use = "dropping the guard immediately detaches the session"]
    pub fn attach(agent: CodingAgent) -> Attached {
        live().entry(agent).or_default().sessions += 1;
        Attached(agent)
    }

    /// One attached session. `Drop` is the release path, so a panicked or
    /// abandoned run detaches itself; an owner that knows the session ended
    /// simply drops it earlier.
    pub struct Attached(CodingAgent);

    impl Drop for Attached {
        fn drop(&mut self) {
            let mut live = live();
            let entry = live.entry(self.0).or_default();
            // Saturating: a double release must never wrap to usize::MAX and
            // pin the numbers "live" forever.
            entry.sessions = entry.sessions.saturating_sub(1);
        }
    }

    /// A session reported new windows. Latest-wins, like every other usage
    /// slot; the stamp is what makes them expire after the session ends.
    pub fn publish(agent: CodingAgent, windows: Vec<UsageWindow>) {
        let mut live = live();
        let entry = live.entry(agent).or_default();
        entry.windows = windows;
        entry.updated_at_secs = Some(crate::run_registry::now_secs());
    }

    /// What this machine's sessions last said about `agent`, if anything.
    pub fn snapshot(agent: CodingAgent) -> Option<LiveUsage> {
        live().get(&agent).cloned()
    }

    /// Tests only: the registry is process-global, so a test that asserts on
    /// it starts from empty (and takes the suite's own lock to stay so).
    #[cfg(test)]
    pub fn reset() {
        live().clear();
    }
}

// ---------------------------------------------------------------------------
// The collector
// ---------------------------------------------------------------------------

/// What one collection pass produced — the two jsonb columns the register
/// and the heartbeat carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentStatusPayload {
    pub accounts: AgentAccounts,
    pub usage: AgentUsageMap,
}

impl AgentStatusPayload {
    /// `agentAccounts` as a wire value; `None` when there is nothing to say
    /// (no agent installed) so the column is left untouched.
    pub fn accounts_json(&self) -> Option<Value> {
        (!self.accounts.is_empty())
            .then(|| serde_json::to_value(&self.accounts).ok())
            .flatten()
    }

    /// `agentUsage` as a wire value; `None` when no agent reported windows.
    pub fn usage_json(&self) -> Option<Value> {
        (!self.usage.is_empty())
            .then(|| serde_json::to_value(&self.usage).ok())
            .flatten()
    }
}

/// Refresh whatever the poll policy says is due and answer the CURRENT
/// status for every installed agent (cached numbers included — a pass that
/// fetches nothing still reports).
///
/// Blocking: keychain reads, one HTTPS GET per due agent, one `codex
/// app-server` spawn. Callers run it off the UI/main thread. `now` is unix
/// seconds — passed in so one pass stamps one instant (and tests are
/// deterministic).
///
/// EXP-754: an agent a LIVE session already reports for ([`live`]) skips all
/// of that — no spawn, no request, and no poll floor either. The one
/// exception is identity: a rate-limit frame names nobody, so a due beat
/// with no cached account still spends one probe to name it — for an agent
/// whose probe CAN name one ([`probe_names_account`]). EXP-819: claude's
/// live frame covers only the session and weekly windows; the endpoint's
/// other windows keep their last polled numbers for as long as the session
/// keeps turning (an idle gap past the shared TTL, or the session's end,
/// hands the beat back to the poll).
///
/// EXP-808: the pass covers every ACCOUNT PROFILE of every agent, not just
/// the device's active login — each profile's numbers land in its own
/// `profiles[].usage` row (the top-level `agentUsage` map stays the active
/// login's). The fan-out is capped at [`MAX_USAGE_PROFILES`] per agent and
/// staggered by [`PROFILE_STAGGER_SECS`], so one beat never spends a probe
/// on every login of every agent.
pub fn collect_if_due(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    now: u64,
) -> AgentStatusPayload {
    collect_inner(data_dir, settings, report, now, None)
}

/// [`collect_if_due`], plus the ONE login a forced refresh
/// ([`force_collect`]) puts past the stagger.
fn collect_inner(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    now: u64,
    forced: Option<(CodingAgent, &str)>,
) -> AgentStatusPayload {
    let stamp = iso_from_unix_secs(now as i64).unwrap_or_else(now_iso);
    // EXP-792 (EXP-747 B3): the heartbeat's account map carries this
    // machine's profiles; a machine with only the ambient login sends the
    // pre-profile payload unchanged. EXP-808: the same pass answers which of
    // those logins may be asked for usage at all, so no profile is probed
    // twice.
    let detail = report.agent_accounts_detailed(settings, data_dir, &stamp);
    let mut accounts = detail.accounts;
    let mut usage = AgentUsageMap::new();
    let mut cache = usage_cache::load(data_dir);
    let mut changed = false;
    // EXP-849: read off the run registry at most ONCE per pass, and only when
    // a codex keep-alive is actually in question.
    let mut used_logins: Option<std::collections::BTreeSet<String>> = None;

    for target in usage_targets(data_dir, report, &detail.usage_eligible, now, forced) {
        let agent = target.agent;
        let id = agent.id().to_string();
        // EXP-792 (EXP-747 B6): the cache is keyed per LOGIN
        // (`agent:profileId`), so one profile's 429 never backs off its
        // siblings.
        let cache_id = usage_cache::entry_key(&id, &target.profile);
        let check = report.check_for(agent);
        let mut entry = cache.get(&cache_id).cloned().unwrap_or_default();
        let mut polled = false;
        // EXP-849 — codex keep-alive: a probe this pass was going to make
        // anyway asks codex to refresh the login's own token while it answers,
        // once per [`usage_cache::CODEX_REFRESH_INTERVAL_SECS`] per login.
        //
        // Only for a login this machine RUNS: the device default, or one some
        // recorded run used. A parked account (added, signed in, never run
        // here) is deliberately left to expire — keeping a credential warm is
        // the machine asserting it needs it, and this machine does not.
        //
        // Claude has NO keep-alive (EXP-852): its refresh would have to be
        // ours, under the CLI's own lock.
        let keep_alive = agent == CodingAgent::Codex
            && usage_cache::refresh_due(&entry, now)
            && (target.active || {
                let used = used_logins
                    .get_or_insert_with(|| logins_used_on_this_machine(data_dir));
                used.contains(&cache_id)
            });
        // EXP-754: a live session on this machine has already been told the
        // numbers. Reading them spawns nothing, sends nothing and contends
        // with no sibling process, so this runs BEFORE (and instead of) the
        // poll policy. EXP-808: a rate-limit frame names no LOGIN either, so
        // the live numbers only ever answer for the ACTIVE profile — the one
        // a run without an explicit account lands on.
        match target.active.then(|| live_probe(agent, &entry, now)).flatten() {
            Some(probe) => {
                // Read BEFORE the apply: `apply_outcome` stamps `fetched_at`,
                // which is what `poll_due` keys on.
                let due = usage_cache::poll_due(&entry, now);
                // A failed probe left the numbers dimmed. A live session
                // confirming those exact numbers is the freshest attempt
                // there is, so it clears the flag (and refreshes the
                // sibling's `fetched_at`) even mid-backoff.
                let dimmed = entry.usage.as_ref().is_some_and(|usage| usage.stale);
                if due || dimmed || probe.outcome == PollOutcome::Changed {
                    // Deliberately past `MIN_POLL_SECS`: the floor exists to
                    // ration requests, and a live read is not one. Moving
                    // numbers reach the bar as fast as codex reports them.
                    changed = true;
                    usage_cache::apply_outcome(&mut entry, probe.outcome, probe.windows, now, &stamp);
                    cache.insert(cache_id.clone(), entry.clone());
                }
                // A rate-limit frame names nobody, so a machine that has
                // never probed (fresh cache, or a login just called
                // `usage_cache::forget`) would report codex's presence-only
                // row for the whole session. Spend ONE app-server probe on
                // the identity when a beat is due; every later beat rides
                // the live shortcut above.
                if due && entry.account.is_none() && probe_names_account(agent) {
                    polled = true;
                    changed = true;
                    // Claim the slot before the (slow) spawn, as the poll arm
                    // does — the live apply already moved it, never backwards.
                    entry.next_poll_at_secs = entry
                        .next_poll_at_secs
                        .max(now + usage_cache::MIN_POLL_SECS);
                    cache.insert(cache_id.clone(), entry.clone());
                    usage_cache::save(data_dir, &cache);
                    let probe = probe_agent(
                        agent,
                        settings,
                        check.version.as_deref(),
                        target.dir.as_deref(),
                        &mut entry,
                        now,
                        keep_alive,
                    );
                    // Only the identity: the live windows are at least as
                    // fresh as this probe's, so its outcome never dims them.
                    if let Some(account) = probe.account {
                        entry.account = Some(account.clone());
                        apply_account(&mut accounts, &id, &target, account);
                    }
                    cache.insert(cache_id.clone(), entry.clone());
                }
            }
            // No live session (or its numbers went stale): today's path.
            None if target.may_poll && usage_cache::poll_due(&entry, now) => {
                polled = true;
                changed = true;
                // Claim the slot BEFORE the (slow) fetch and persist it, so the
                // sibling process sharing this token (IDE vs daemon) sees the
                // poll as taken instead of spending a second request.
                entry.next_poll_at_secs = now + usage_cache::MIN_POLL_SECS;
                cache.insert(cache_id.clone(), entry.clone());
                usage_cache::save(data_dir, &cache);
                let probe = probe_agent(
                    agent,
                    settings,
                    check.version.as_deref(),
                    target.dir.as_deref(),
                    &mut entry,
                    now,
                    keep_alive,
                );
                if let Some(account) = probe.account {
                    // Persist the identity: the not-due beats in between re-use it
                    // instead of dropping back to the doctor's presence-only row.
                    entry.account = Some(account.clone());
                    apply_account(&mut accounts, &id, &target, account);
                }
                usage_cache::apply_outcome(&mut entry, probe.outcome, probe.windows, now, &stamp);
                cache.insert(cache_id.clone(), entry.clone());
            }
            None => {}
        }
        // A pass that did not probe — nothing due, the live numbers answered,
        // or this login's stagger slot has not come round — keeps the
        // identity the last probe named: it still enriches what the doctor's
        // presence-only check could not name (codex's email/plan), and a
        // rate-limit frame names nobody.
        if let (false, Some(account)) = (polled, &entry.account) {
            let mut account = account.clone();
            account.checked_at = stamp.clone();
            enrich_account(&mut accounts, &id, &target, account);
        }
        // EXP-808: the top-level `agentUsage` map is the ACTIVE login's;
        // every login also carries its own numbers in its profile row, so a
        // beat that polled nobody still reports what the cache holds.
        if let Some(snapshot) = &entry.usage {
            if target.active {
                usage.insert(id.clone(), snapshot.clone());
            }
            if let Some(row) = profile_row(&mut accounts, &id, &target.profile) {
                row.usage = Some(snapshot.clone());
            }
        }
    }

    if changed {
        usage_cache::save(data_dir, &cache);
    }
    // EXP-849: last, so it sees every outcome this pass folded in.
    apply_health(&mut accounts, &cache, data_dir);
    AgentStatusPayload { accounts, usage }
}

/// EXP-792: a FORCED refresh for ONE login (`agent_usage_refresh`): the
/// poll policy's schedule and the shared TTL are set aside for this one
/// pass, the 429 floor is not. `Err(until)` names the unix second the
/// floor lifts (the host replies with it instead of polling); `Ok` is the
/// same payload [`collect_if_due`] would answer, with that login re-read.
///
/// EXP-808: `profile` names the account profile to refresh (`system`, blank
/// or `None`-ish = the ambient login). The forced login is also put past the
/// stagger for this pass — a person pressing Refresh is not a beat.
pub fn force_collect(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    agent: CodingAgent,
    profile: &str,
    now: u64,
) -> Result<AgentStatusPayload, u64> {
    // An id this machine does not have — a picker that raced a profile
    // deletion — refreshes
    // the ambient login, the one a run without an account lands on.
    let profile = crate::agent_profiles::get(data_dir, agent, profile.trim())
        .map(|row| row.id)
        .unwrap_or_else(|| crate::agent_profiles::SYSTEM_PROFILE.to_string());
    let cache_id = usage_cache::entry_key(agent.id(), &profile);
    {
        let mut cache = usage_cache::load(data_dir);
        let mut entry = cache.get(&cache_id).cloned().unwrap_or_default();
        usage_cache::force_due(&mut entry, now)?;
        cache.insert(cache_id, entry);
        usage_cache::save(data_dir, &cache);
    }
    Ok(collect_inner(
        data_dir,
        settings,
        report,
        now,
        Some((agent, &profile)),
    ))
}

/// EXP-849 — "use this account here": make `profile` this machine's DEFAULT
/// login for `agent`, then re-read that login's numbers so the next heartbeat
/// ships the moved `active` flag (and its usage) right away.
///
/// The ONE body behind all three entry points — the desktop's own control, the
/// desktop's `agent_profile_use` command handler and the CLI daemon's — so the
/// refusals are the same sentence wherever the switch was asked for. It is
/// non-destructive by construction: a device-local pointer moves, and no
/// credential is read, written, copied or revoked (`agent_login` stays the
/// sign-in, and `codex logout` is never in this path).
///
/// `Err` is the sentence to show: an id this machine does not have, a login
/// that is not signed in here (making it the default would break every later
/// start, and the fix is a sign-in), or an unwritable index.
pub fn use_profile(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    agent: CodingAgent,
    profile: &str,
    now: u64,
) -> Result<AgentStatusPayload, String> {
    let profile = profile.trim();
    if crate::agent_profiles::get(data_dir, agent, profile).is_none() {
        return Err(format!("No such {} account on this machine.", agent.id()));
    }
    // Identity as this machine sees it right now — the profile's own `auth
    // status`, never a synced row that may be minutes old.
    let stamp = now_iso();
    let accounts = report.agent_accounts_with_profiles(settings, data_dir, &stamp);
    let signed_in = accounts
        .get(agent.id())
        .map(|account| match account.profiles.iter().find(|row| row.id == profile) {
            Some(row) => row.signed_in,
            // A single-login machine has no profile rows: the ambient login IS
            // the account row.
            None => crate::agent_profiles::is_system(Some(profile)) && account.signed_in,
        })
        .unwrap_or(false);
    if !signed_in {
        return Err(format!(
            "That {} account is not signed in on this machine — sign in there first.",
            agent.id()
        ));
    }
    crate::agent_profiles::set_active_profile(data_dir, agent, profile)
        .map_err(|err| format!("Could not switch the {} account here: {err}", agent.id()))?;
    // Past the shared TTL on purpose: the numbers the clients show for this
    // machine are the ACTIVE login's, and it just changed. A rate-limited
    // refusal reports what the cache holds instead — the pointer moved either
    // way, so a switch that already happened must not read as a failure.
    Ok(
        force_collect(data_dir, settings, report, agent, profile, now)
            .unwrap_or_else(|_| collect_if_due(data_dir, settings, report, now)),
    )
}

// ---------------------------------------------------------------------------
// EXP-808 — which logins a pass touches
// ---------------------------------------------------------------------------

/// One LOGIN a collection pass looks at.
#[derive(Clone, Debug, PartialEq, Eq)]
struct UsageTarget {
    agent: CodingAgent,
    /// The account profile's id; `system` is the ambient login.
    profile: String,
    /// The profile's config dir (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`) —
    /// `None` for the ambient login.
    dir: Option<PathBuf>,
    /// The device's default login for this agent: it owns the top-level
    /// account fields and the top-level `agentUsage` entry.
    active: bool,
    /// Whether this pass may spend a probe on it. Always true for the
    /// active login and for a forced refresh; for the others it is the
    /// [`PROFILE_STAGGER_SECS`] rotation's answer. A target that may not
    /// poll still REPORTS what the cache holds.
    may_poll: bool,
}

/// The logins this pass may look at, in agent order with each agent's ACTIVE
/// login first (so [`MAX_USAGE_PROFILES`] can never truncate it away).
///
/// A login that cannot answer for usage at all is not a target: an
/// uninstalled agent, a signed-out one, an API-key/Bedrock/Vertex claude
/// (whose account row still ships — a stale OAuth item in the keychain must
/// never be polled on its behalf), and any profile whose own `auth status`
/// did not come back signed in.
///
/// Exactly ONE non-active login across the machine is put past the stagger
/// per [`PROFILE_STAGGER_SECS`] window, in rotation.
fn usage_targets(
    data_dir: &Path,
    report: &DoctorReport,
    eligible: &BTreeMap<String, bool>,
    now: u64,
    forced: Option<(CodingAgent, &str)>,
) -> Vec<UsageTarget> {
    let mut targets: Vec<UsageTarget> = Vec::new();
    // Indices into `targets`, in a stable order — the stagger's rotation.
    let mut secondary: Vec<usize> = Vec::new();
    for agent in CodingAgent::ALL {
        // Installed = a version resolved. Nothing else about the ambient
        // login gates a PROFILE: a machine may well have signed the default
        // out and kept working in a named account.
        if report.check_for(agent).version.is_none() {
            continue;
        }
        let active = crate::agent_profiles::active_profile(data_dir, agent);
        let profiles = monitored_profiles(data_dir, agent);
        for profile in profiles {
            let is_active = profile.id == active;
            let key = usage_cache::entry_key(agent.id(), &profile.id);
            let ok = match eligible.get(&key) {
                Some(ok) => *ok,
                // No profile row was probed: this is the ambient login on a
                // machine with nothing but the ambient login.
                None => {
                    profile.is_system() && report.ambient_usage_eligible(agent)
                }
            };
            if !ok {
                continue;
            }
            let dir = crate::agent_profiles::profile_dir(data_dir, agent, &profile.id);
            targets.push(UsageTarget {
                agent,
                profile: profile.id,
                dir,
                active: is_active,
                may_poll: is_active,
            });
            if !is_active {
                secondary.push(targets.len() - 1);
            }
        }
    }
    if !secondary.is_empty() {
        let slot = (now / PROFILE_STAGGER_SECS) as usize % secondary.len();
        targets[secondary[slot]].may_poll = true;
    }
    if let Some((agent, profile)) = forced {
        for target in targets.iter_mut() {
            if target.agent == agent && target.profile == profile {
                target.may_poll = true;
            }
        }
    }
    targets
}

/// EXP-849 — the logins of `agent` a pass may spend a probe on at all: every
/// profile on the machine, the ACTIVE one first, truncated at
/// [`MAX_USAGE_PROFILES`]. The ONE place the cap is applied, so the
/// `unmonitored` flag the wire carries and the probe fan-out can never
/// disagree about which logins are inside it.
fn monitored_profiles(
    data_dir: &Path,
    agent: CodingAgent,
) -> Vec<crate::agent_profiles::AgentProfile> {
    let active = crate::agent_profiles::active_profile(data_dir, agent);
    let mut profiles = crate::agent_profiles::list(data_dir, agent);
    // Stable: the active login first, the rest in list order.
    profiles.sort_by_key(|profile| profile.id != active);
    profiles.truncate(MAX_USAGE_PROFILES);
    profiles
}

/// EXP-849 — the logins (`agent:profile` cache keys) some recorded run on this
/// machine actually used. The keep-alive's "is this account in use here?"
/// test: a login nothing ever ran on is PARKED, and parking an account means
/// letting it go cold.
fn logins_used_on_this_machine(data_dir: &Path) -> std::collections::BTreeSet<String> {
    crate::run_registry::all(data_dir)
        .into_iter()
        .map(|record| {
            usage_cache::entry_key(
                record.agent.id(),
                record
                    .account()
                    .as_deref()
                    .unwrap_or(crate::agent_profiles::SYSTEM_PROFILE),
            )
        })
        .collect()
}

/// EXP-849 — stamp every wire row's `health` (and the `unmonitored` flag)
/// from what the probe CACHE knows, after the pass folded its own results in.
///
/// Health is deliberately not a by-product of the probe loop: a login the
/// pass never looked at (not due, past the stagger, past the cap, signed out)
/// still has a health to report, and the one source that can answer for all
/// of them is the cache. `signed_in == false` short-circuits to `signed_out`
/// — an old `needs_relogin` from before the sign-out must not outlive it.
fn apply_health(
    accounts: &mut AgentAccounts,
    cache: &usage_cache::UsageCache,
    data_dir: &Path,
) {
    let health_of = |agent: &str, profile: &str, signed_in: bool| {
        if !signed_in {
            return crate::agent_accounts::Health::SignedOut;
        }
        cache
            .get(&usage_cache::entry_key(agent, profile))
            .and_then(|entry| entry.health.as_deref())
            .map(crate::agent_accounts::Health::parse)
            // Signed in, never probed: say so rather than claiming health.
            .unwrap_or(crate::agent_accounts::Health::Unknown)
    };
    for (agent, account) in accounts.iter_mut() {
        if account.profiles.is_empty() {
            // A single-login machine: the ambient login IS the account row.
            account.health = Some(
                health_of(
                    agent,
                    crate::agent_profiles::SYSTEM_PROFILE,
                    account.signed_in,
                )
                .as_str()
                .to_string(),
            );
            continue;
        }
        let monitored: Vec<String> = CodingAgent::parse(agent)
            .map(|agent| monitored_profiles(data_dir, agent))
            .unwrap_or_default()
            .into_iter()
            .map(|profile| profile.id)
            .collect();
        for row in &mut account.profiles {
            row.health = Some(health_of(agent, &row.id, row.signed_in).as_str().to_string());
            row.unmonitored = !monitored.contains(&row.id);
        }
        // The top-level fields mirror the ACTIVE profile, health included.
        if let Some(active) = account
            .profiles
            .iter()
            .find(|row| row.active)
            .and_then(|row| row.health.clone())
        {
            account.health = Some(active);
        }
    }
}

/// One login's row inside the wire map, when the payload carries profiles at
/// all (a single-login machine has none, by design).
fn profile_row<'a>(
    accounts: &'a mut AgentAccounts,
    agent: &str,
    profile: &str,
) -> Option<&'a mut crate::agent_accounts::AgentProfileEntry> {
    accounts
        .get_mut(agent)?
        .profiles
        .iter_mut()
        .find(|row| row.id == profile)
}

/// Fold a probe's identity into the wire map: the ACTIVE login owns the
/// top-level fields (a client that reads only them names the login a default
/// run lands on), and every login owns its own `profiles` row.
///
/// EXP-808: the fields are copied one by one on purpose. A wholesale insert
/// replaced the whole account — and with it the `profiles` array the pass had
/// just built.
fn apply_account(
    accounts: &mut AgentAccounts,
    agent: &str,
    target: &UsageTarget,
    account: AgentAccount,
) {
    if target.active {
        match accounts.get_mut(agent) {
            Some(existing) => {
                existing.signed_in = account.signed_in;
                existing.email = account.email.clone();
                existing.plan = account.plan.clone();
                existing.checked_at = account.checked_at.clone();
            }
            None => {
                accounts.insert(agent.to_string(), account.clone());
            }
        }
    }
    if let Some(row) = profile_row(accounts, agent, &target.profile) {
        row.signed_in = account.signed_in;
        row.email = account.email;
        row.plan = account.plan;
        row.checked_at = account.checked_at;
    }
}

/// [`apply_account`] for a CACHED identity: it fills what the doctor's
/// presence-only check could not name (codex's email/plan) and never
/// overwrites an answer that already names somebody.
fn enrich_account(
    accounts: &mut AgentAccounts,
    agent: &str,
    target: &UsageTarget,
    account: AgentAccount,
) {
    if target.active {
        match accounts.get_mut(agent) {
            Some(existing) => {
                if existing.email.is_none() && existing.plan.is_none() && account.signed_in {
                    existing.signed_in = true;
                    existing.email = account.email.clone();
                    existing.plan = account.plan.clone();
                    existing.checked_at = account.checked_at.clone();
                }
            }
            None => {
                accounts.insert(agent.to_string(), account.clone());
            }
        }
    }
    if let Some(row) = profile_row(accounts, agent, &target.profile) {
        if row.email.is_none() && row.plan.is_none() && account.signed_in {
            row.signed_in = true;
            row.email = account.email;
            row.plan = account.plan;
            row.checked_at = account.checked_at;
        }
    }
}

/// One agent's fetch result, before the cache folds it in.
struct AgentProbe {
    outcome: PollOutcome,
    windows: Option<Vec<UsageWindow>>,
    account: Option<crate::agent_accounts::AgentAccount>,
}

/// EXP-754 — the windows a LIVE session already published, when they are
/// worth reporting: a session is attached right now, or one just ended and
/// its last numbers are still inside the shared TTL.
///
/// `None` means "nobody is telling us" — the caller falls back to the poll
/// policy and its spawn. codex pushes `account/rateLimits/updated` down the
/// app-server connection and claude prints a `rate_limit_event` per turn
/// (EXP-819).
fn live_probe(agent: CodingAgent, entry: &AgentCacheEntry, now: u64) -> Option<AgentProbe> {
    let source = live_source(agent)?;
    let live = live::snapshot(agent)?;
    if live.windows.is_empty() {
        return None;
    }
    let current = live.sessions > 0
        || live
            .updated_at_secs
            .is_some_and(|at| now.saturating_sub(at) < usage_cache::SHARED_TTL_SECS);
    if !current {
        return None;
    }
    let windows = match source {
        LiveSource::Whole => live.windows,
        LiveSource::Partial => {
            let reported = entry
                .usage
                .as_ref()
                .map(|usage| usage.windows.as_slice())
                .unwrap_or_default();
            merge_live_windows(reported, &live.windows)
        }
    };
    let outcome = if usage_cache::windows_hash(&windows) == entry.last_windows_hash {
        PollOutcome::Unchanged
    } else {
        PollOutcome::Changed
    };
    Some(AgentProbe {
        outcome,
        windows: Some(windows),
        // A rate-limit frame names no identity; the cached one stays.
        account: None,
    })
}

/// What a live publisher's frame covers, relative to the agent's poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveSource {
    /// The same windows the poll would fetch (codex: both come off the
    /// app-server), so the frame REPLACES the report.
    Whole,
    /// A subset of the poll's report (claude: the session and weekly windows,
    /// never the endpoint's model-scoped weekly or credits), laid over it by
    /// key.
    Partial,
}

/// The agents with a live publisher.
fn live_source(agent: CodingAgent) -> Option<LiveSource> {
    match agent {
        CodingAgent::Codex => Some(LiveSource::Whole),
        CodingAgent::Claude => Some(LiveSource::Partial),
    }
}

/// EXP-819 — whether [`probe_agent`] can NAME the login. Only codex's can
/// (`account/read` beside the windows); claude's usage GET names nobody —
/// its identity is the doctor's credential read — so a live session of its
/// own owes the identity no probe: it would spend a request to
/// learn the same `None`.
fn probe_names_account(agent: CodingAgent) -> bool {
    matches!(agent, CodingAgent::Codex)
}

/// Probe ONE login's usage. EXP-808: `config_dir` is the account profile's
/// config dir — claude reads the credential kept beside it, codex answers
/// with that `CODEX_HOME` in its app-server's env. `None` = the ambient
/// login.
fn probe_agent(
    agent: CodingAgent,
    settings: &Settings,
    version: Option<&str>,
    config_dir: Option<&Path>,
    entry: &mut AgentCacheEntry,
    now: u64,
    keep_alive: bool,
) -> AgentProbe {
    match agent {
        CodingAgent::Claude => {
            let user_agent = claude_user_agent(version);
            match read_claude_credential_in(config_dir) {
                CredentialRead::Denied => {
                    // The keychain refused (or nobody answered its prompt):
                    // stop asking for an hour.
                    entry.credential_denied_until_secs =
                        Some(now + usage_cache::CREDENTIAL_DENIED_BACKOFF_SECS);
                    AgentProbe {
                        outcome: PollOutcome::Failed,
                        windows: None,
                        account: None,
                    }
                }
                CredentialRead::Missing => AgentProbe {
                    outcome: PollOutcome::Failed,
                    windows: None,
                    account: None,
                },
                CredentialRead::Found(credential) => {
                    if credential.expired(now as i64 * 1000) {
                        return AgentProbe {
                            outcome: PollOutcome::Failed,
                            windows: None,
                            account: None,
                        };
                    }
                    fetch_and_parse(&credential.access_token, &user_agent)
                }
            }
        }
        CodingAgent::Codex => {
            let program = settings.resolved_path_for(agent);
            // EXP-808: the profile's `CODEX_HOME`, so the app-server answers
            // for THAT login's account and its windows.
            let env: Vec<(String, String)> = config_dir
                .zip(crate::agent_profiles::config_env_var(agent))
                .map(|(dir, var)| (var.to_string(), dir.to_string_lossy().into_owned()))
                .into_iter()
                .collect();
            match crate::codex_app_server::probe_in(
                &program,
                &terminal::pty::login_path(),
                &env,
                crate::codex_app_server::PROBE_TIMEOUT,
                keep_alive,
            ) {
                Ok(probe) => {
                    // EXP-849: the app-server answered, so the keep-alive (if
                    // this was one) happened — start its next cadence. A
                    // refusal leaves the stamp alone, so the next due poll
                    // tries again instead of waiting out six hours.
                    if keep_alive {
                        entry.refreshed_at_secs = Some(now);
                    }
                    let stamp = iso_from_unix_secs(now as i64).unwrap_or_else(now_iso);
                    let account = probe
                        .account
                        .as_ref()
                        .and_then(|value| parse_codex_account(value, &stamp));
                    let windows = probe.rate_limits.as_ref().map(parse_codex_rate_limits);
                    AgentProbe {
                        outcome: match &windows {
                            Some(_) => PollOutcome::Changed,
                            None => PollOutcome::Failed,
                        },
                        windows,
                        account,
                    }
                }
                Err(_) => AgentProbe {
                    outcome: PollOutcome::Failed,
                    windows: None,
                    account: None,
                },
            }
        }
    }
}

fn fetch_and_parse(access_token: &str, user_agent: &str) -> AgentProbe {
    match fetch_oauth_usage(access_token, user_agent) {
        UsageFetch::Ok(body) => match parse_claude_usage(&body) {
            Some(windows) => AgentProbe {
                outcome: PollOutcome::Changed,
                windows: Some(windows),
                account: None,
            },
            None => AgentProbe {
                outcome: PollOutcome::Failed,
                windows: None,
                account: None,
            },
        },
        UsageFetch::Unauthorized => AgentProbe {
            outcome: PollOutcome::Unauthorized,
            windows: None,
            account: None,
        },
        UsageFetch::RateLimited => AgentProbe {
            outcome: PollOutcome::RateLimited,
            windows: None,
            account: None,
        },
        UsageFetch::Failed => AgentProbe {
            outcome: PollOutcome::Failed,
            windows: None,
            account: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The modern `limits[]` body: every window it lists, in report order,
    /// plus the enabled credits pool. EXP-688: an inactive window is kept —
    /// at 0% when it reports no utilization at all.
    #[test]
    fn claude_limits_body_parses_into_the_locked_windows() {
        let body = r#"{
            "limits": [
                {"kind": "session", "group": "session", "percent": 42, "severity": "ok", "resets_at": "2026-08-28T14:00:00.000Z", "scope": null, "is_active": true},
                {"kind": "weekly_all", "group": "weekly", "percent": 61.4, "severity": "warning", "resets_at": "2026-09-01T00:00:00.000Z", "scope": null, "is_active": true},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "severity": "ok", "resets_at": null, "scope": {"model": {"display_name": "Fable"}}, "is_active": true},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 3, "resets_at": null, "scope": {"model": {"display_name": "Sonnet"}}, "is_active": false}
            ],
            "extra_usage": {"is_enabled": true, "utilization": 7}
        }"#;
        let windows = parse_claude_usage(body).unwrap();
        assert_eq!(
            windows
                .iter()
                .map(|window| (window.key.as_str(), window.label.as_str(), window.percent))
                .collect::<Vec<_>>(),
            vec![
                ("session", "5h", 42),
                ("weekly", "Week", 61),
                ("model:fable", "Fable", 12),
                ("model:sonnet", "Sonnet", 3),
                ("credits", "Credits", 7),
            ]
        );
        assert_eq!(windows[0].resets_at.as_deref(), Some("2026-08-28T14:00:00.000Z"));
        assert_eq!(windows[2].resets_at, None);

        // The idle machine: the session window is inactive and reports no
        // utilization — it still renders, at 0% (EXP-688).
        let idle = r#"{"limits": [
            {"kind": "session", "group": "session", "resets_at": null, "scope": null, "is_active": false}
        ]}"#;
        let windows = parse_claude_usage(idle).unwrap();
        assert_eq!(
            windows
                .iter()
                .map(|window| (window.key.as_str(), window.label.as_str(), window.percent))
                .collect::<Vec<_>>(),
            vec![("session", "5h", 0)]
        );
    }

    /// The legacy body (no `limits[]`) still yields the two named windows,
    /// and unix reset stamps normalize to ISO.
    #[test]
    fn claude_legacy_body_falls_back_to_the_named_windows() {
        let body = r#"{
            "five_hour": {"utilization": 10, "resets_at": 1756000000},
            "seven_day": {"utilization": 90, "resets_at": "2026-09-01T00:00:00.000Z"}
        }"#;
        let windows = parse_claude_usage(body).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "session");
        assert_eq!(windows[0].resets_at.as_deref(), Some("2025-08-24T01:46:40.000Z"));
        assert_eq!(windows[1].key, "weekly");
        assert_eq!(windows[1].percent, 90);

        // Not a usage body at all → None (keep the old numbers, stale).
        assert_eq!(parse_claude_usage("{\"error\":\"nope\"}"), None);
        assert_eq!(parse_claude_usage("<html>"), None);
        // An empty limits array is a VALID answer: no windows.
        assert_eq!(parse_claude_usage("{\"limits\":[]}"), Some(Vec::new()));
    }

    #[test]
    fn claude_user_agent_mirrors_the_cli() {
        assert_eq!(
            claude_user_agent(Some("2.1.251 (Claude Code)")),
            "claude-cli/2.1.251 (external, cli)"
        );
        assert_eq!(
            claude_user_agent(None),
            format!(
                "claude-cli/{}.{}.{} (external, cli)",
                MIN_CLAUDE_VERSION.0, MIN_CLAUDE_VERSION.1, MIN_CLAUDE_VERSION.2
            )
        );
    }

    /// Codex labels its two windows by DURATION; a single month window
    /// renders as `Month`, and the credits balance is never a window.
    #[test]
    fn codex_rate_limits_label_by_window_duration() {
        let value: Value = serde_json::from_str(
            r#"{"rateLimits": {
                "primary": {"usedPercent": 23, "windowDurationMins": 300, "resetsAt": 1756000000},
                "secondary": {"usedPercent": 71.6, "windowDurationMins": 10080}
            }, "creditsBalance": 12.5}"#,
        )
        .unwrap();
        let windows = parse_codex_rate_limits(&value);
        assert_eq!(
            windows
                .iter()
                .map(|window| (window.key.as_str(), window.label.as_str(), window.percent))
                .collect::<Vec<_>>(),
            vec![("session", "5h", 23), ("weekly", "Week", 72)]
        );
        assert_eq!(windows[0].resets_at.as_deref(), Some("2025-08-24T01:46:40.000Z"));
        assert!(
            !windows.iter().any(|window| window.key == "credits"),
            "a credits BALANCE is not a window"
        );

        // A single 43200-minute window is the monthly plan shape.
        let value: Value = serde_json::from_str(
            r#"{"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":43200}}}"#,
        )
        .unwrap();
        let windows = parse_codex_rate_limits(&value);
        assert_eq!(windows.len(), 1);
        assert_eq!((windows[0].key.as_str(), windows[0].label.as_str()), ("43200", "Month"));

        // An unknown duration keeps its raw minutes; no answer at all = no
        // windows (never a fabricated zero).
        let value: Value =
            serde_json::from_str(r#"{"rateLimits":{"primary":{"usedPercent":9,"windowDurationMins":60}}}"#)
                .unwrap();
        assert_eq!(parse_codex_rate_limits(&value)[0].label, "60m");
        assert!(parse_codex_rate_limits(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn codex_account_reads_chatgpt_apikey_and_signed_out() {
        let chatgpt: Value = serde_json::from_str(
            r#"{"account":{"type":"chatgpt","email":"dev@acme.test","planType":"pro"}}"#,
        )
        .unwrap();
        let account = parse_codex_account(&chatgpt, "NOW").unwrap();
        assert!(account.signed_in);
        assert_eq!(account.email.as_deref(), Some("dev@acme.test"));
        assert_eq!(account.plan.as_deref(), Some("pro"));
        assert_eq!(account.checked_at, "NOW");

        let api_key: Value = serde_json::from_str(r#"{"account":{"type":"apiKey"}}"#).unwrap();
        let account = parse_codex_account(&api_key, "NOW").unwrap();
        assert!(account.signed_in);
        assert_eq!(account.email, None);
        assert_eq!(account.plan.as_deref(), Some("api key"));

        let signed_out: Value = serde_json::from_str(r#"{"account":null}"#).unwrap();
        let account = parse_codex_account(&signed_out, "NOW").unwrap();
        assert!(!account.signed_in);
        assert_eq!(account.plan, None);

        // An answer this build cannot read fails OPEN.
        assert_eq!(parse_codex_account(&serde_json::json!("nope"), "NOW"), None);
    }

    #[test]
    fn claude_credentials_parse_and_expire() {
        let raw = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat-secret","expiresAt":1756000000000,"subscriptionType":"max","scopes":["user:inference"]}}"#;
        let credential = parse_claude_credentials(raw).unwrap();
        assert_eq!(credential.access_token, "sk-ant-oat-secret");
        assert_eq!(credential.subscription_type.as_deref(), Some("max"));
        assert!(!credential.expired(1_755_999_999_000));
        assert!(credential.expired(1_756_000_000_000));

        // The token NEVER reaches a log line.
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("sk-ant-oat-secret"), "{rendered}");
        assert!(rendered.contains("<redacted>"));

        // An unknown expiry is treated as live (the 401 is the real gate).
        let credential =
            parse_claude_credentials(r#"{"claudeAiOauth":{"accessToken":"tok"}}"#).unwrap();
        assert!(!credential.expired(i64::MAX));
        // No OAuth branch at all (an API-key install) → nothing to borrow.
        assert_eq!(parse_claude_credentials(r#"{"apiKey":"sk-x"}"#), None);
        assert_eq!(parse_claude_credentials("not json"), None);
    }


    #[test]
    fn usage_serializes_the_locked_wire_shape() {
        let usage = AgentUsage {
            fetched_at: "2026-08-28T10:00:00.000Z".into(),
            stale: false,
            windows: vec![UsageWindow {
                key: "session".into(),
                label: "5h".into(),
                percent: 42,
                resets_at: Some("2026-08-28T14:00:00.000Z".into()),
            }],
        };
        assert_eq!(
            serde_json::to_string(&usage).unwrap(),
            r#"{"fetchedAt":"2026-08-28T10:00:00.000Z","stale":false,"windows":[{"key":"session","label":"5h","percent":42,"resetsAt":"2026-08-28T14:00:00.000Z"}]}"#
        );
        // `resetsAt` is present-with-null, never absent (locked vocabulary).
        let unbounded = AgentUsage {
            windows: vec![UsageWindow {
                key: "credits".into(),
                label: "Credits".into(),
                percent: 0,
                resets_at: None,
            }],
            ..usage
        };
        assert!(serde_json::to_string(&unbounded)
            .unwrap()
            .contains(r#""resetsAt":null"#));
    }

    /// EXP-484: a pass over a report with nothing to poll reports the
    /// doctor's accounts, fetches nothing, and writes no cache — the
    /// no-network half of the collector, locked so a refactor cannot start
    /// spending requests for a signed-out or uninstalled agent.
    #[test]
    fn collect_reports_accounts_without_polling_a_signed_out_agent() {
        use crate::agent_accounts::AgentAccount;
        use crate::doctor::{Tool, ToolCheck};

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-collect-if-due-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let signed_out = ToolCheck {
            tool: Tool::Claude,
            ok: false,
            version: Some("2.1.251 (Claude Code)".to_string()),
            error: Some("signed out".to_string()),
            authed: Some(false),
            account: Some(AgentAccount::default()),
            usage_eligible: false,
            acp: None,
            acp_note: None,
        };
        let missing = |tool| ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some("not found".to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
        };
        let report = DoctorReport {
            claude: signed_out,
            codex: missing(Tool::Codex),
            git: missing(Tool::Git),
        };
        let payload = collect_if_due(&dir, &Settings::default(), &report, 1_756_000_000);
        assert_eq!(payload.accounts.keys().collect::<Vec<_>>(), vec!["claude"]);
        assert!(!payload.accounts["claude"].signed_in);
        assert_eq!(
            payload.accounts["claude"].checked_at,
            "2025-08-24T01:46:40.000Z"
        );
        assert!(payload.usage.is_empty(), "a signed-out agent is never polled");
        assert_eq!(payload.usage_json(), None);
        assert!(
            !dir.join("agent-usage.json").exists(),
            "nothing was fetched, so nothing was cached"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // EXP-754 — the live path
    // -----------------------------------------------------------------

    /// [`live`] is process-global, so the tests that assert on it run one at
    /// a time and each starts from empty.
    static LIVE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn live_lock() -> std::sync::MutexGuard<'static, ()> {
        match LIVE_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn usage_dir(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-live-usage-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn session_window(percent: u8) -> UsageWindow {
        UsageWindow {
            key: "session".to_string(),
            label: "5h".to_string(),
            percent,
            resets_at: Some("2026-09-06T14:00:00.000Z".to_string()),
        }
    }

    /// codex installed and signed in, nothing else on the machine — so a pass
    /// touches codex and only codex.
    fn codex_ready_report() -> DoctorReport {
        use crate::doctor::{Tool, ToolCheck};

        let missing = |tool| ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some("not found".to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
        };
        DoctorReport {
            claude: missing(Tool::Claude),
            codex: ToolCheck {
                tool: Tool::Codex,
                ok: true,
                version: Some("codex-cli 0.144.5".to_string()),
                error: None,
                authed: Some(true),
                account: None,
                usage_eligible: false,
                acp: None,
                acp_note: None,
            },
            git: missing(Tool::Git),
        }
    }

    /// The codex path points at nothing: if the collector fell back to its
    /// `codex app-server` probe the numbers could only come back missing or
    /// stale. They come back live — and the SECOND pass proves the live read
    /// is not held back by `MIN_POLL_SECS`.
    #[test]
    fn live_codex_windows_land_without_spawning_the_app_server() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-codex");
        let settings = Settings {
            codex_path: "/nonexistent/codex".to_string(),
            ..Settings::default()
        };
        let report = codex_ready_report();

        let session = live::attach(CodingAgent::Codex);
        live::publish(CodingAgent::Codex, vec![session_window(4)]);

        let now = crate::run_registry::now_secs();
        let payload = collect_if_due(&dir, &settings, &report, now);
        let usage = payload.usage.get("codex").expect("the live windows");
        assert_eq!(usage.windows, vec![session_window(4)]);
        assert!(!usage.stale, "a live read is a read, not a fallback");

        // Ten seconds later — deep inside the poll floor — moving numbers
        // still land, because reading them costs nothing.
        live::publish(CodingAgent::Codex, vec![session_window(9)]);
        let payload = collect_if_due(&dir, &settings, &report, now + 10);
        let usage = payload.usage.get("codex").expect("the live windows");
        assert_eq!(usage.windows, vec![session_window(9)]);
        assert!(!usage.stale);

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session that ended long enough ago is not a source any more: the
    /// pass goes back to the probe, and the probe's failure dims the numbers
    /// the last real fetch left behind instead of reporting the dead
    /// session's.
    #[test]
    fn a_stale_live_snapshot_with_no_session_falls_back_to_the_probe() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("stale-live");
        let settings = Settings {
            codex_path: "/nonexistent/codex".to_string(),
            ..Settings::default()
        };

        // A session ran, published, and ended.
        drop(live::attach(CodingAgent::Codex));
        live::publish(CodingAgent::Codex, vec![session_window(4)]);

        // What an earlier real poll cached.
        let earlier = vec![session_window(71)];
        let mut entry = AgentCacheEntry::default();
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(earlier.clone()),
            1_000,
            "EARLIER",
        );
        let mut cache = usage_cache::UsageCache::default();
        cache.insert(usage_cache::entry_key("codex", "system"), entry);
        usage_cache::save(&dir, &cache);

        let now = crate::run_registry::now_secs() + usage_cache::SHARED_TTL_SECS + 60;
        let payload = collect_if_due(&dir, &settings, &codex_ready_report(), now);
        let usage = payload.usage.get("codex").expect("the cached windows");
        assert_eq!(usage.windows, earlier, "a dead session is not a source");
        assert_eq!(usage.fetched_at, "EARLIER", "the numbers keep their own age");
        assert!(usage.stale, "the probe could not run, so the bar is dimmed");
        let reloaded = usage_cache::load(&dir);
        assert_eq!(
            reloaded
                .get(&usage_cache::entry_key("codex", "system"))
                .unwrap()
                .next_poll_at_secs,
            now + usage_cache::FAILED_BACKOFF_SECS
        );

        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A failed probe dims the numbers and pins a 5-minute backoff. A live
    /// session confirming those EXACT numbers is a read, so it undims them
    /// (and refreshes what the sibling process sees) without waiting the
    /// backoff out.
    #[test]
    fn a_live_confirmation_undims_a_stale_entry_inside_its_backoff() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-undim");
        let settings = Settings {
            codex_path: "/nonexistent/codex".to_string(),
            ..Settings::default()
        };
        let now = crate::run_registry::now_secs();
        let windows = vec![session_window(37)];

        // What the last good fetch cached, then a probe that failed.
        let mut entry = AgentCacheEntry::default();
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now - 400,
            "EARLIER",
        );
        // The identity is already known, so this pass owes nobody a probe.
        entry.account = Some(crate::agent_accounts::AgentAccount {
            signed_in: true,
            email: Some("dev@example.com".to_string()),
            ..Default::default()
        });
        usage_cache::apply_outcome(&mut entry, PollOutcome::Failed, None, now, "EARLIER");
        assert!(entry.usage.as_ref().unwrap().stale);
        assert!(!usage_cache::poll_due(&entry, now), "deep in the backoff");
        let mut cache = usage_cache::UsageCache::default();
        cache.insert(usage_cache::entry_key("codex", "system"), entry);
        usage_cache::save(&dir, &cache);

        // A session starts and reports the very same percentages.
        let session = live::attach(CodingAgent::Codex);
        live::publish(CodingAgent::Codex, windows.clone());

        let payload = collect_if_due(&dir, &settings, &codex_ready_report(), now);
        let usage = payload.usage.get("codex").expect("the live windows");
        assert_eq!(usage.windows, windows);
        assert!(!usage.stale, "a live confirmation is the freshest attempt");
        assert_ne!(usage.fetched_at, "EARLIER", "and it re-ages the numbers");

        // The sibling process reading the file sees the same.
        let reloaded = usage_cache::load(&dir);
        let stored = reloaded
            .get(&usage_cache::entry_key("codex", "system"))
            .expect("the shared entry");
        assert!(!stored.usage.as_ref().unwrap().stale);
        assert_eq!(stored.fetched_at_secs, now);

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A rate-limit frame names nobody. A machine that has never probed (a
    /// fresh cache, or a login that just called `usage_cache::forget`) still
    /// spends ONE app-server probe on the identity while a session runs —
    /// and only one: the beats after it ride the live shortcut.
    #[cfg(unix)]
    #[test]
    fn a_live_session_still_probes_once_to_name_the_codex_account() {
        use std::os::unix::fs::PermissionsExt as _;

        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-identity");
        // A stand-in codex that records the spawn and exits: the probe fails
        // (no app-server answers), but the marker proves it ran.
        let marker = dir.join("probed");
        let program = dir.join("codex-stub.sh");
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho ran >> {}\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let settings = Settings {
            codex_path: program.to_string_lossy().to_string(),
            ..Settings::default()
        };

        let session = live::attach(CodingAgent::Codex);
        let windows = vec![session_window(12)];
        live::publish(CodingAgent::Codex, windows.clone());

        let now = crate::run_registry::now_secs();
        let payload = collect_if_due(&dir, &settings, &codex_ready_report(), now);
        assert!(
            marker.exists(),
            "a nameless account owes the identity one probe"
        );
        let usage = payload.usage.get("codex").expect("the live windows");
        assert_eq!(usage.windows, windows, "the live windows still land");
        assert!(!usage.stale, "and the failed identity probe never dims them");

        // The next beat is inside the floor: no second spawn.
        std::fs::remove_file(&marker).unwrap();
        live::publish(CodingAgent::Codex, vec![session_window(13)]);
        let payload = collect_if_due(&dir, &settings, &codex_ready_report(), now + 10);
        assert!(!marker.exists(), "every later beat rides the live shortcut");
        assert_eq!(
            payload.usage.get("codex").expect("the live windows").windows,
            vec![session_window(13)],
            "moving numbers still reach the bar"
        );

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // EXP-819 — claude's live frame
    // -----------------------------------------------------------------

    /// The measured `unifiedWindows` object (2.1.267): fractions and unix
    /// seconds, both arms. A rejected window reports `1.0`.
    #[test]
    fn claude_rate_limit_windows_parse_the_measured_unified_map() {
        let unified = serde_json::json!({
            "five_hour": { "utilization": 1.0, "resetsAt": 1788703200 },
            "seven_day": { "utilization": 0.62, "resetsAt": 1789066800 },
        });
        assert_eq!(
            parse_claude_rate_limit_windows(Some(&unified), None),
            vec![
                UsageWindow {
                    key: "session".to_string(),
                    label: "5h".to_string(),
                    percent: 100,
                    resets_at: Some("2026-09-06T14:00:00.000Z".to_string()),
                },
                UsageWindow {
                    key: "weekly".to_string(),
                    label: "Week".to_string(),
                    percent: 62,
                    resets_at: Some("2026-09-10T19:00:00.000Z".to_string()),
                },
            ]
        );
    }

    /// The scale is a fraction: `0.91` is 91 %, not 1 %; past the cap clamps
    /// to 100; a reset-less arm (2.1.263) still lands, without a reset. The
    /// overage-included arm is dropped, and an unparseable map is no windows.
    #[test]
    fn claude_rate_limit_windows_scale_clamp_and_skip_the_overage_bucket() {
        let unified = serde_json::json!({
            "five_hour": { "utilization": 0.91 },
            "seven_day": { "utilization": 1.37, "resetsAt": 1789066800 },
            "seven_day_overage_included": { "utilization": 0.1, "resetsAt": 1789066800 },
        });
        let windows = parse_claude_rate_limit_windows(Some(&unified), None);
        assert_eq!(
            windows.iter().map(|window| (window.key.as_str(), window.percent)).collect::<Vec<_>>(),
            vec![("session", 91), ("weekly", 100)]
        );
        assert_eq!(windows[0].resets_at, None);
        assert!(parse_claude_rate_limit_windows(Some(&serde_json::json!("nope")), None).is_empty());
        assert!(parse_claude_rate_limit_windows(None, None).is_empty());
    }

    /// No map: the top-level limiting triple answers for its one window —
    /// unless it names a model-scoped kind (the endpoint's row would get a
    /// duplicate) — and never overrides an arm the map carries.
    #[test]
    fn claude_rate_limit_windows_fall_back_to_the_limiting_window() {
        let limiting = ClaudeLimitingWindow {
            kind: "seven_day",
            utilization: 0.335,
            resets_at_secs: Some(1789066800),
        };
        assert_eq!(
            parse_claude_rate_limit_windows(None, Some(limiting)),
            vec![UsageWindow {
                key: "weekly".to_string(),
                label: "Week".to_string(),
                percent: 34,
                resets_at: Some("2026-09-10T19:00:00.000Z".to_string()),
            }]
        );
        let opus = ClaudeLimitingWindow { kind: "seven_day_opus", ..limiting };
        assert!(parse_claude_rate_limit_windows(None, Some(opus)).is_empty());
        let unified = serde_json::json!({ "seven_day": { "utilization": 0.5 } });
        let windows = parse_claude_rate_limit_windows(Some(&unified), Some(limiting));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].percent, 50, "the map wins over the triple");
    }

    /// The merge keeps the endpoint's order and its extra rows, moves the
    /// percent of every key the frame carries, takes the frame's reset only
    /// when it names one, and appends a key the endpoint never had.
    #[test]
    fn live_windows_merge_over_the_endpoint_report_by_key() {
        let window = |key: &str, label: &str, percent: u8, resets: Option<&str>| UsageWindow {
            key: key.to_string(),
            label: label.to_string(),
            percent,
            resets_at: resets.map(str::to_string),
        };
        let reported = vec![
            window("session", "5h", 40, Some("2026-09-05T19:00:00.000Z")),
            window("model:opus", "Opus", 33, Some("2026-09-10T00:00:00.000Z")),
            window("weekly", "Week", 10, Some("2026-09-10T00:00:00.000Z")),
        ];
        let live = vec![
            window("weekly", "Week", 12, None),
            window("session", "5h", 55, Some("2026-09-06T14:00:00.000Z")),
            window("credits", "Credits", 3, None),
        ];
        assert_eq!(
            merge_live_windows(&reported, &live),
            vec![
                window("session", "5h", 55, Some("2026-09-06T14:00:00.000Z")),
                window("model:opus", "Opus", 33, Some("2026-09-10T00:00:00.000Z")),
                window("weekly", "Week", 12, Some("2026-09-10T00:00:00.000Z")),
                window("credits", "Credits", 3, None),
            ]
        );
        assert_eq!(merge_live_windows(&[], &live), live, "nothing reported yet: the frame as is");
    }

    /// claude installed, signed in with an OAuth login the endpoint answers
    /// for, nothing else on the machine.
    fn claude_ready_report() -> DoctorReport {
        use crate::doctor::{Tool, ToolCheck};

        let missing = |tool| ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some("not found".to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
        };
        DoctorReport {
            claude: ToolCheck {
                tool: Tool::Claude,
                ok: true,
                version: Some("2.1.267 (Claude Code)".to_string()),
                error: None,
                authed: Some(true),
                account: None,
                usage_eligible: true,
                acp: None,
                acp_note: None,
            },
            codex: missing(Tool::Codex),
            git: missing(Tool::Git),
        }
    }

    /// EXP-819 — a live claude session moves the session and weekly windows
    /// per turn, inside the poll floor, WITHOUT touching the endpoint's
    /// model-scoped row: it keeps the last polled numbers instead of
    /// vanishing. The cache entry is fresh and not due, so the pass owes the
    /// endpoint nothing (and a claude probe would name no account anyway).
    #[test]
    fn live_claude_windows_lay_over_the_endpoint_report() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-claude");
        let settings = Settings::default();
        let report = claude_ready_report();
        let window = |key: &str, label: &str, percent: u8, resets: &str| UsageWindow {
            key: key.to_string(),
            label: label.to_string(),
            percent,
            resets_at: Some(resets.to_string()),
        };
        let opus = window("model:opus", "Opus", 33, "2026-09-10T00:00:00.000Z");

        let now = crate::run_registry::now_secs();
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("claude", crate::agent_profiles::SYSTEM_PROFILE),
            cached(
                vec![
                    window("session", "5h", 40, "2026-09-05T19:00:00.000Z"),
                    opus.clone(),
                    window("weekly", "Week", 10, "2026-09-10T00:00:00.000Z"),
                ],
                now - 10,
            ),
        );
        usage_cache::save(&dir, &cache);

        let session = live::attach(CodingAgent::Claude);
        live::publish(
            CodingAgent::Claude,
            vec![
                window("session", "5h", 55, "2026-09-06T14:00:00.000Z"),
                UsageWindow { key: "weekly".to_string(), label: "Week".to_string(), percent: 12, resets_at: None },
            ],
        );
        let payload = collect_if_due(&dir, &settings, &report, now);
        let usage = payload.usage.get("claude").expect("the merged windows");
        assert_eq!(
            usage.windows,
            vec![
                window("session", "5h", 55, "2026-09-06T14:00:00.000Z"),
                opus.clone(),
                window("weekly", "Week", 12, "2026-09-10T00:00:00.000Z"),
            ],
            "the frame's keys move, the endpoint's row and its reset stay"
        );
        assert!(!usage.stale);

        // The next turn, seconds later: still lands, still over the report.
        live::publish(CodingAgent::Claude, vec![window("session", "5h", 58, "2026-09-06T14:00:00.000Z")]);
        let payload = collect_if_due(&dir, &settings, &report, now + 10);
        let usage = payload.usage.get("claude").expect("the merged windows");
        assert_eq!(
            usage.windows.iter().map(|window| (window.key.as_str(), window.percent)).collect::<Vec<_>>(),
            vec![("session", 58), ("model:opus", 33), ("weekly", 12)]
        );

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only codex's probe names the login; a live claude session never owes
    /// the identity a request.
    #[test]
    fn only_a_codex_probe_is_owed_the_identity() {
        assert!(probe_names_account(CodingAgent::Codex));
        assert!(!probe_names_account(CodingAgent::Claude));
        assert_eq!(live_source(CodingAgent::Codex), Some(LiveSource::Whole));
        assert_eq!(live_source(CodingAgent::Claude), Some(LiveSource::Partial));
    }

    #[test]
    fn payload_json_is_absent_when_there_is_nothing_to_say() {
        let empty = AgentStatusPayload::default();
        assert_eq!(empty.accounts_json(), None);
        assert_eq!(empty.usage_json(), None);

        let mut payload = AgentStatusPayload::default();
        payload.usage.insert("claude".into(), AgentUsage::default());
        assert!(payload.usage_json().is_some());
        assert_eq!(payload.accounts_json(), None);
    }

    /// EXP-792: a forced refresh clears the schedule + shared TTL but stops
    /// at the 429 floor.
    #[test]
    fn force_due_honors_only_the_rate_limit_floor() {
        let mut entry = usage_cache::AgentCacheEntry {
            fetched_at_secs: 1_000,
            next_poll_at_secs: 5_000,
            credential_denied_until_secs: Some(9_000),
            ..Default::default()
        };
        assert!(!usage_cache::poll_due(&entry, 1_010));
        assert_eq!(usage_cache::force_due(&mut entry, 1_010), Ok(()));
        assert!(usage_cache::poll_due(&entry, 1_010));
        usage_cache::apply_outcome(&mut entry, PollOutcome::RateLimited, None, 1_010, "s");
        assert_eq!(
            entry.rate_limited_until_secs,
            Some(1_010 + usage_cache::RATE_LIMITED_FLOOR_SECS)
        );
        assert_eq!(
            usage_cache::force_due(&mut entry, 1_020),
            Err(1_010 + usage_cache::RATE_LIMITED_FLOOR_SECS)
        );
        assert_eq!(
            usage_cache::force_due(&mut entry, 1_010 + usage_cache::RATE_LIMITED_FLOOR_SECS),
            Ok(())
        );
        // A successful read lifts the floor.
        usage_cache::apply_outcome(&mut entry, PollOutcome::Changed, Some(Vec::new()), 2_000, "s");
        assert_eq!(entry.rate_limited_until_secs, None);
    }

    // -----------------------------------------------------------------
    // EXP-808 — the per-profile fan-out
    // -----------------------------------------------------------------

    /// codex installed, signed in and NAMED — the doctor row a machine with
    /// account profiles has (a row is what makes the pass look at profiles
    /// at all).
    fn codex_named_report() -> DoctorReport {
        let mut report = codex_ready_report();
        report.codex.account = Some(AgentAccount {
            signed_in: true,
            checked_at: String::new(),
            ..AgentAccount::default()
        });
        report
    }

    /// A cache entry that already holds numbers and is nowhere near due.
    fn cached(windows: Vec<UsageWindow>, now: u64) -> AgentCacheEntry {
        AgentCacheEntry {
            usage: Some(AgentUsage {
                fetched_at: "2026-09-09T10:00:00.000Z".to_string(),
                stale: false,
                windows,
            }),
            fetched_at_secs: now,
            next_poll_at_secs: now + 10_000,
            ..AgentCacheEntry::default()
        }
    }

    /// The login keys `agent-usage.json` holds right now.
    fn cache_keys(dir: &std::path::Path) -> Vec<String> {
        let raw = std::fs::read_to_string(dir.join("agent-usage.json")).unwrap_or_default();
        match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(object)) => object.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// EXP-849 — the health a pass stamps on the wire map: signed out wins
    /// outright, a probed login carries the cache's verdict, a never-probed
    /// one is `unknown`, the top-level fields mirror the ACTIVE profile, and
    /// a login past [`MAX_USAGE_PROFILES`] is flagged `unmonitored` rather
    /// than looking like a login with no usage.
    #[test]
    fn apply_health_stamps_every_row_from_the_probe_cache() {
        use crate::agent_accounts::{AgentProfileEntry, Health};
        let dir = usage_dir("health");
        let mut ids = Vec::new();
        for n in 0..MAX_USAGE_PROFILES + 1 {
            ids.push(
                crate::agent_profiles::create(&dir, CodingAgent::Claude, &format!("acct {n}"))
                    .unwrap()
                    .id,
            );
        }
        let mut cache = usage_cache::UsageCache::default();
        cache.insert(
            usage_cache::entry_key("claude", crate::agent_profiles::SYSTEM_PROFILE),
            AgentCacheEntry {
                health: Some(Health::Ok.as_str().into()),
                ..AgentCacheEntry::default()
            },
        );
        cache.insert(
            usage_cache::entry_key("claude", &ids[0]),
            AgentCacheEntry {
                health: Some(Health::NeedsRelogin.as_str().into()),
                ..AgentCacheEntry::default()
            },
        );
        // A stale `needs_relogin` for a login that has since signed OUT must
        // not outlive the sign-out.
        cache.insert(
            usage_cache::entry_key("claude", &ids[1]),
            AgentCacheEntry {
                health: Some(Health::NeedsRelogin.as_str().into()),
                ..AgentCacheEntry::default()
            },
        );

        let mut accounts = AgentAccounts::new();
        let mut rows = vec![AgentProfileEntry {
            id: crate::agent_profiles::SYSTEM_PROFILE.into(),
            signed_in: true,
            active: true,
            ..AgentProfileEntry::default()
        }];
        for (n, id) in ids.iter().enumerate() {
            rows.push(AgentProfileEntry {
                id: id.clone(),
                signed_in: n != 1,
                ..AgentProfileEntry::default()
            });
        }
        accounts.insert(
            "claude".into(),
            AgentAccount {
                signed_in: true,
                profiles: rows,
                ..AgentAccount::default()
            },
        );
        // A single-login agent: the account row IS the ambient login.
        accounts.insert(
            "codex".into(),
            AgentAccount {
                signed_in: false,
                ..AgentAccount::default()
            },
        );

        apply_health(&mut accounts, &cache, &dir);

        let claude = &accounts["claude"];
        let row = |id: &str| {
            claude
                .profiles
                .iter()
                .find(|row| row.id == id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            row(crate::agent_profiles::SYSTEM_PROFILE).health(),
            Health::Ok
        );
        // The top-level fields mirror the active profile.
        assert_eq!(claude.health(), Health::Ok);
        assert_eq!(row(&ids[0]).health(), Health::NeedsRelogin);
        assert_eq!(row(&ids[1]).health(), Health::SignedOut, "signed out wins");
        // Signed in, never probed.
        assert_eq!(row(&ids[2]).health(), Health::Unknown);
        // The device badge is the worst of the lot.
        assert_eq!(
            crate::agent_accounts::worst_health(&accounts),
            Health::NeedsRelogin
        );
        assert_eq!(accounts["codex"].health(), Health::SignedOut);

        // Exactly the logins past the cap are flagged, and the active one is
        // never among them.
        let unmonitored: Vec<String> = claude
            .profiles
            .iter()
            .filter(|row| row.unmonitored)
            .map(|row| row.id.clone())
            .collect();
        assert_eq!(
            unmonitored.len(),
            claude.profiles.len() - MAX_USAGE_PROFILES,
            "{unmonitored:?}"
        );
        assert!(!unmonitored.contains(&crate::agent_profiles::SYSTEM_PROFILE.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-808 — the plan a pass runs: every login of the agent, the ACTIVE
    /// one first (so the cap can never truncate it away), at most
    /// [`MAX_USAGE_PROFILES`] of them, and exactly ONE non-active login past
    /// the stagger per window — rotating, so every login gets its turn
    /// without any beat fanning out to all of them.
    #[test]
    fn the_profile_fan_out_is_capped_ordered_and_staggered() {
        let dir = usage_dir("targets");
        let report = codex_named_report();
        let mut eligible = BTreeMap::new();
        eligible.insert(
            usage_cache::entry_key("codex", crate::agent_profiles::SYSTEM_PROFILE),
            true,
        );
        let mut ids = Vec::new();
        // Two logins past the cap, so the truncation is exercised whatever
        // [`MAX_USAGE_PROFILES`] is set to.
        for n in 0..MAX_USAGE_PROFILES + 1 {
            let profile =
                crate::agent_profiles::create(&dir, CodingAgent::Codex, &format!("acct {n}"))
                    .unwrap();
            eligible.insert(usage_cache::entry_key("codex", &profile.id), true);
            ids.push(profile.id);
        }
        // The device's default is the LAST profile created: past the cap in
        // list order, so only the ordering keeps it in the plan.
        let active = ids.last().unwrap().clone();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &active).unwrap();

        let targets = usage_targets(&dir, &report, &eligible, 0, None);
        assert_eq!(
            targets.len(),
            MAX_USAGE_PROFILES,
            "{} logins, {MAX_USAGE_PROFILES} probed",
            ids.len() + 1
        );
        assert_eq!(targets[0].profile, active);
        assert!(targets[0].active && targets[0].may_poll);
        assert_eq!(
            targets.iter().filter(|target| target.may_poll).count(),
            2,
            "the active login plus ONE rotation slot, never the whole fan-out"
        );
        // A custom profile carries its config dir; the ambient login does not.
        assert!(targets[0].dir.is_some());
        assert!(targets
            .iter()
            .find(|target| target.profile == crate::agent_profiles::SYSTEM_PROFILE)
            .unwrap()
            .dir
            .is_none());

        // One window each: over a full rotation every non-active login is
        // polled exactly once.
        let secondaries = MAX_USAGE_PROFILES - 1;
        let mut polled: Vec<String> = Vec::new();
        for window in 0..secondaries as u64 {
            let targets =
                usage_targets(&dir, &report, &eligible, window * PROFILE_STAGGER_SECS, None);
            let mut turn: Vec<String> = targets
                .iter()
                .filter(|target| target.may_poll && !target.active)
                .map(|target| target.profile.clone())
                .collect();
            assert_eq!(turn.len(), 1, "window {window}");
            polled.push(turn.remove(0));
        }
        polled.sort();
        polled.dedup();
        assert_eq!(polled.len(), secondaries, "every login gets a window");

        // A person pressing Refresh is not a beat: the forced login polls
        // whatever the rotation says.
        let forced = ids[0].clone();
        let targets = usage_targets(&dir, &report, &eligible, 0, Some((CodingAgent::Codex, &forced)));
        assert!(
            targets
                .iter()
                .find(|target| target.profile == forced)
                .unwrap()
                .may_poll
        );

        // A login that cannot answer for usage (signed out, or an API-key
        // claude) is not a target at all — no probe is ever spent on it.
        eligible.insert(usage_cache::entry_key("codex", &ids[0]), false);
        let targets = usage_targets(&dir, &report, &eligible, 0, None);
        assert!(!targets.iter().any(|target| target.profile == ids[0]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-808 — the payload: every login's numbers ride its OWN profile row
    /// (which is what the web reads first), the top-level `agentUsage` map
    /// stays the ACTIVE login's, and a pass never drops the `profiles` array
    /// it just built.
    #[test]
    fn every_profile_reports_its_own_windows_and_the_map_stays_the_active_login() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("profile-windows");
        // `login status` answers 0 with no output = signed in, so both
        // profiles read as real logins without a codex on this machine.
        let settings = Settings {
            codex_path: "/usr/bin/true".to_string(),
            ..Settings::default()
        };
        let work = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Work").unwrap();
        let home = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Home").unwrap();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();

        // Every login already has numbers and none is due, so this pass
        // spends no probe at all — it only has to REPORT.
        let now = 1_800_000_000;
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("codex", crate::agent_profiles::SYSTEM_PROFILE),
            cached(vec![session_window(11)], now),
        );
        cache.insert(
            usage_cache::entry_key("codex", &work.id),
            cached(vec![session_window(22)], now),
        );
        cache.insert(
            usage_cache::entry_key("codex", &home.id),
            cached(vec![session_window(33)], now),
        );
        usage_cache::save(&dir, &cache);

        let payload = collect_if_due(&dir, &settings, &codex_named_report(), now);
        let codex = &payload.accounts["codex"];
        assert_eq!(codex.profiles.len(), 3, "the profile rows survive the pass");
        let row = |id: &str| {
            codex
                .profiles
                .iter()
                .find(|row| row.id == id)
                .unwrap_or_else(|| panic!("no row for {id}"))
        };
        let windows = |id: &str| row(id).usage.as_ref().expect("this login's own numbers").windows.clone();
        assert_eq!(windows(crate::agent_profiles::SYSTEM_PROFILE), vec![session_window(11)]);
        assert_eq!(windows(&work.id), vec![session_window(22)]);
        assert_eq!(windows(&home.id), vec![session_window(33)]);
        assert!(row(&work.id).active && !row(&home.id).active);
        assert_eq!(
            payload.usage["codex"].windows,
            vec![session_window(22)],
            "the top-level map is the ACTIVE login's, not the ambient one's"
        );
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-808 — and when everything IS due, one beat still only claims the
    /// active login's slot plus one rotation slot: the poll floors are per
    /// LOGIN, so an unstaggered fan-out would multiply this machine's
    /// request rate by the number of accounts on it.
    #[test]
    fn one_beat_claims_the_active_login_and_a_single_rotation_slot() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("stagger-claims");
        let settings = Settings {
            codex_path: "/usr/bin/true".to_string(),
            ..Settings::default()
        };
        let report = codex_named_report();
        let work = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Work").unwrap();
        let home = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Home").unwrap();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();

        // Nothing cached: all three logins are due at once.
        let now = 1_800_000_000;
        collect_if_due(&dir, &settings, &report, now);
        let claimed = cache_keys(&dir);
        assert_eq!(claimed.len(), 2, "3 logins due, 2 probed: {claimed:?}");
        assert!(claimed.contains(&usage_cache::entry_key("codex", &work.id)));

        // One window on, the OTHER non-active login gets its turn.
        collect_if_due(&dir, &settings, &report, now + PROFILE_STAGGER_SECS);
        let claimed = cache_keys(&dir);
        assert_eq!(claimed.len(), 3, "one per window, never all at once: {claimed:?}");
        for id in [crate::agent_profiles::SYSTEM_PROFILE, &work.id, &home.id] {
            assert!(claimed.contains(&usage_cache::entry_key("codex", id)), "{id}");
        }
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-808 — a forced refresh names a LOGIN, not just an agent: the
    /// profile's own slot is the one that is cleared and re-read, and an id
    /// this machine does not have falls back to the ambient login rather
    /// than refreshing nothing at all.
    #[test]
    fn a_forced_refresh_targets_one_profile() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("force-profile");
        let settings = Settings {
            codex_path: "/usr/bin/true".to_string(),
            ..Settings::default()
        };
        let report = codex_named_report();
        let work = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Work").unwrap();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();

        // Everything is fresh and far from due, so only a FORCED login moves.
        let now = 1_800_000_000;
        let mut cache = usage_cache::load(&dir);
        for id in [crate::agent_profiles::SYSTEM_PROFILE, work.id.as_str()] {
            cache.insert(
                usage_cache::entry_key("codex", id),
                cached(vec![session_window(7)], now),
            );
        }
        usage_cache::save(&dir, &cache);

        force_collect(
            &dir,
            &settings,
            &report,
            CodingAgent::Codex,
            crate::agent_profiles::SYSTEM_PROFILE,
            now,
        )
        .unwrap();
        let cache = usage_cache::load(&dir);
        let system = cache
            .get(&usage_cache::entry_key("codex", crate::agent_profiles::SYSTEM_PROFILE))
            .unwrap()
            .clone();
        let active = cache.get(&usage_cache::entry_key("codex", &work.id)).unwrap();
        assert_eq!(system.fetched_at_secs, now, "the named login was re-read");
        assert!(
            system.usage.as_ref().is_some_and(|usage| usage.stale),
            "the probe failed, so its numbers dimmed — it ran"
        );
        assert_eq!(
            active.fetched_at_secs, now,
            "the active login's slot is untouched by another login's refresh"
        );
        assert!(active.usage.as_ref().is_some_and(|usage| !usage.stale));

        // An unknown id (a picker that raced a deletion) refreshes the
        // ambient login instead of refusing.
        assert!(force_collect(&dir, &settings, &report, CodingAgent::Codex, "deadbeef", now + 1).is_ok());
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
