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
//!   credential, straight from the store `claude` keeps it in. Ordinarily it
//!   is borrowed for exactly one GET and dropped; ONLY with `claudeKeepAlive`
//!   on (EXP-852, off by default) is it also REFRESHED in place, under the
//!   CLI's own `.oauth_refresh.lock` ([`crate::claude_oauth`]), so the daemon
//!   and the user's own CLI go on sharing one login. Either way it is never
//!   logged, never copied off this machine, and never written to any store
//!   but the one it came from. An API-key/Bedrock login is not eligible at
//!   all ([`crate::doctor::ClaudeAuthStatus::usage_eligible`]).
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
use crate::claude_oauth::{self, RefreshOutcome, RefreshRequest, StoreRead, WriteOutcome};
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
/// `codex app-server` spawns and N GETs at once. So at most ONE non-active
/// profile ACROSS THE MACHINE is probed per window: a beat costs at most one
/// probe more than the pre-profile build did.
///
/// EXP-862: the window is a SPACING, not a slot lottery — the due login with
/// the oldest numbers takes it, and a login this machine has never read at all
/// skips the queue entirely (see [`usage_targets`]).
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
///
/// EXP-881: laying over is only ever sound when the frame is NEWER than the
/// report — the caller ([`live_probe`]) checks that against
/// `endpoint_fetched_at_secs` first. This function merges what it is given
/// and asks no questions about time.
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
// The credential (borrowed for one GET, never logged)
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
    /// Whether the token is past its own expiry.
    ///
    /// EXP-881: an expired token is now ALWAYS given a rotation attempt
    /// first, whatever `claudeKeepAlive` says — an expired credential makes
    /// every usage read 401, and a 401 is read as `Needs re-login`. So
    /// reaching here means the rotation was tried on this beat and could NOT
    /// happen (a sibling process holds the machine-wide claim, the store
    /// carries no refresh token, the grant came back `invalid_grant`). The
    /// answer is then the same as it always was: no numbers this pass, the
    /// previous ones stay, dimmed.
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

/// Read claude's OAuth credential for the AMBIENT login, for one usage GET:
/// the macOS keychain item first (`security find-generic-password -w`,
/// read-only), else the `.credentials.json` file under `CLAUDE_CONFIG_DIR`
/// (or `~/.claude`). Never logged.
pub fn read_claude_credential() -> CredentialRead {
    read_claude_credential_in(None)
}

/// [`read_claude_credential`] for ONE config dir — EXP-792: an account
/// profile's `CLAUDE_CONFIG_DIR`. The file under that dir is read first
/// (the credential a relocated config keeps beside itself); on macOS the
/// keychain item claude names after a non-default dir (the service suffixed
/// with the dir's hash) is tried when the file is absent. `None` = the
/// ambient login, keychain first as before.
///
/// EXP-852: the store itself is [`crate::claude_oauth`]'s — the module that
/// also WRITES it — so the search order, the keychain service naming and the
/// `Denied` rule have exactly one implementation. This is the read-only view
/// of it: the whole document reduced to the three fields a usage GET needs.
pub fn read_claude_credential_in(config_dir: Option<&Path>) -> CredentialRead {
    match claude_oauth::read_store(config_dir) {
        // A document with no readable OAuth branch (an API-key install) is
        // nothing to borrow, not a refusal.
        StoreRead::Found(store) => store
            .credential()
            .map(CredentialRead::Found)
            .unwrap_or(CredentialRead::Missing),
        StoreRead::Missing => CredentialRead::Missing,
        StoreRead::Denied => CredentialRead::Denied,
    }
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
/// EXP-909: the registry is keyed by `(agent, profile)` — the LOGIN the run
/// spends, not just the agent — because a run on a secondary account must
/// never move the ambient login's numbers (and vice versa). A transcript
/// REPLAY attaches nothing at all: it spends no tokens.
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

    /// What one LOGIN's live sessions have published on this machine.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct LiveUsage {
        /// The last windows a session reported (empty = a session is
        /// attached but has not heard a rate-limit frame yet).
        pub windows: Vec<UsageWindow>,
        /// Unix seconds of the last [`publish`].
        pub updated_at_secs: Option<u64>,
        /// How many sessions are attached RIGHT NOW to this login. EXP-881:
        /// `> 0` only WIDENS how long a frame is trusted (a live run refreshes
        /// its own numbers every turn, so a frame within the endpoint's own
        /// cadence is as good as a fetch); it never makes an OLD frame
        /// current. An idle turn-less session publishes nothing, and its last
        /// frame ages out exactly like a detached one's.
        pub sessions: usize,
    }

    /// `(agent, profile)` — the profile is [`crate::agent_profiles::profile_id`]
    /// of the run's launch account, so `system` = the ambient login.
    type LiveKey = (CodingAgent, String);

    static LIVE: OnceLock<Mutex<HashMap<LiveKey, LiveUsage>>> = OnceLock::new();

    fn live() -> MutexGuard<'static, HashMap<LiveKey, LiveUsage>> {
        let lock = LIVE.get_or_init(|| Mutex::new(HashMap::new()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// A live session for `agent` on `profile` has started. Hold the guard
    /// for the run.
    #[must_use = "dropping the guard immediately detaches the session"]
    pub fn attach(agent: CodingAgent, profile: &str) -> Attached {
        live()
            .entry((agent, profile.to_string()))
            .or_default()
            .sessions += 1;
        Attached(agent, profile.to_string())
    }

    /// One attached session. `Drop` is the release path, so a panicked or
    /// abandoned run detaches itself; an owner that knows the session ended
    /// simply drops it earlier.
    pub struct Attached(CodingAgent, String);

    impl Drop for Attached {
        fn drop(&mut self) {
            let mut live = live();
            let entry = live.entry((self.0, self.1.clone())).or_default();
            // Saturating: a double release must never wrap to usize::MAX and
            // pin the numbers "live" forever.
            entry.sessions = entry.sessions.saturating_sub(1);
        }
    }

    /// A session reported new windows for the login it runs on. Latest-wins,
    /// like every other usage slot; the stamp is what decides whether the
    /// frame still answers ([`super::live_probe`]).
    pub fn publish(agent: CodingAgent, profile: &str, windows: Vec<UsageWindow>) {
        let mut live = live();
        let entry = live.entry((agent, profile.to_string())).or_default();
        entry.windows = windows;
        entry.updated_at_secs = Some(crate::run_registry::now_secs());
    }

    /// What this machine's sessions on `profile` last said about `agent`, if
    /// anything. A run on another login is INVISIBLE here — that is the point
    /// of the key.
    pub fn snapshot(agent: CodingAgent, profile: &str) -> Option<LiveUsage> {
        live().get(&(agent, profile.to_string())).cloned()
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
/// EXP-754: a login a LIVE session already reports for ([`live`]) skips all
/// of that — no spawn, no request, and no poll floor either — for as long as
/// its last frame is CURRENT ([`live_probe`], EXP-881: a frame that has aged
/// past the cadence, or whose window has reset, stops answering and the poll
/// takes over). The one exception is identity: a rate-limit frame names
/// nobody, so a due beat with no cached account still spends one probe to
/// name it — for an agent whose probe CAN name one ([`probe_names_account`]).
/// EXP-819: claude's live frame covers only the session and weekly windows,
/// so the endpoint is still read every
/// [`usage_cache::LIVE_ENDPOINT_POLL_SECS`] for the others (the model-scoped
/// weekly, credits) — they used to freeze for the whole run.
///
/// EXP-909: the live registry is keyed by `(agent, LOGIN)`, so every profile
/// in the plan consults its own sessions. A run on a secondary account moves
/// that account's `profiles[].usage` row and nothing else.
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
    // EXP-849/EXP-852: read off the run registry at most ONCE per pass, and
    // only when a keep-alive (either agent's) is actually in question.
    let mut used_logins: Option<std::collections::BTreeSet<String>> = None;

    // EXP-862: the plan reads the cache this pass then updates — one load,
    // taken before the loop so "has this login ever been read?" and the
    // probe accounting can never disagree.
    let plan = usage_targets(data_dir, report, &detail.usage_eligible, &cache, now, forced);

    for target in plan {
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
        let keep_alive = agent == CodingAgent::Codex
            && usage_cache::refresh_due(&entry, now)
            && (target.active || {
                let used = used_logins
                    .get_or_insert_with(|| logins_used_on_this_machine(data_dir));
                used.contains(&cache_id)
            });
        // EXP-852 — claude keep-alive. SAME eligibility rule as codex's (a
        // login this machine RUNS: the device default, or one some recorded
        // run used — never a parked account on a machine that is not its
        // home) and the SAME machine-wide claim, but the cadence is the
        // TOKEN'S OWN EXPIRY, not an interval: `claude_refresh_due` reads a
        // cached `expiresAt`, so a healthy login costs nothing on a beat.
        // Opt-in (`claudeKeepAlive`, OFF by default) until the strace gate
        // has passed.
        //
        // Deliberately NOT riding the usage probe the way codex's does:
        // codex's is a flag on a request the poll already makes, claude's is a
        // separate POST to another host that the usage budget does not ration
        // — and a secondary profile's probe slot can be 20+ minutes apart
        // (60s stagger × up to 12 profiles × 2 agents, ANDed with the poll
        // floors), longer than any sane margin. So it runs here, every beat,
        // before the live/poll match; on success the probe below rides the
        // NEW token and skips a second store read.
        let scheduled_keep_alive = agent == CodingAgent::Claude
            && settings.claude_keep_alive
            && usage_cache::claude_refresh_due(&entry, now)
            && (target.active || {
                let used = used_logins.get_or_insert_with(|| logins_used_on_this_machine(data_dir));
                used.contains(&cache_id)
            });
        // EXP-881 — the SECOND gate: a token that has ALREADY expired is
        // refreshed whatever the setting says and whoever runs here. Not a
        // keep-alive in the EXP-852 sense (nothing is being kept warm): every
        // usage read on an expired token 401s, and a 401 paints `Needs
        // re-login` on an account that is perfectly fine. Rotating it is the
        // only way to learn which it is, so neither `claudeKeepAlive` nor
        // "this machine runs this login" gates it — a PARKED account's
        // numbers are shown too, and they are just as wrong.
        let expired_refresh = agent == CodingAgent::Claude
            && usage_cache::claude_token_expired(&entry, now)
            && usage_cache::claude_refresh_due(&entry, now);
        let claude_keep_alive = scheduled_keep_alive || expired_refresh;
        // ONE refresh actor per login, MACHINE-WIDE: the shared poll floors
        // keep the two processes from spending two requests, but a refresh
        // also ROTATES the credential — the IDE and the daemon both
        // refreshing one profile would rotate it twice, the loser writing a
        // token the winner already replaced. The claim is held across the
        // work below and released when this iteration ends; a process that
        // cannot take it probes WITHOUT the keep-alive.
        let refresh_claim = (keep_alive || claude_keep_alive)
            .then(|| usage_cache::claim_refresh(data_dir, &id, &target.profile, now))
            .flatten();
        let keep_alive = keep_alive && refresh_claim.is_some();
        let claude_keep_alive = claude_keep_alive && refresh_claim.is_some();
        let fresh_token = if claude_keep_alive {
            let token = claude_keep_alive_step(target.dir.as_deref(), &mut entry, now);
            // EXP-881: a rotation on a login whose numbers are DIMMED takes
            // down the wall the failure put up — the next read is the one
            // that can undim them, so it happens on THIS beat rather than
            // after the failure backoff. The 429 floor is untouched.
            if token.is_some() && entry.usage.as_ref().is_some_and(|usage| usage.stale) {
                usage_cache::note_token_rotated(&mut entry, now);
            }
            // Persist the backoff / dead marker / new expiry NOW, before the
            // slow probe, so the sibling process sees the attempt as taken.
            changed = true;
            cache.insert(cache_id.clone(), entry.clone());
            usage_cache::save(data_dir, &cache);
            token
        } else {
            None
        };
        // EXP-754: a live session on this login has already been told the
        // numbers. Reading them spawns nothing, sends nothing and contends
        // with no sibling process, so this runs BEFORE (and instead of) the
        // poll policy. EXP-909: EVERY login is consulted, not just the active
        // one — the registry is keyed by `(agent, profile)`, so a run on a
        // secondary account answers for THAT account and for nothing else.
        let live = live_probe(agent, &target.profile, &entry, now);
        // A PARTIAL frame (claude) never carries the endpoint's other windows,
        // so the endpoint keeps its own slower cadence under a live session —
        // otherwise the model-scoped weekly froze at its pre-run number for
        // as long as the run kept turning.
        let endpoint_owed = live.is_some()
            && live_source(agent) == Some(LiveSource::Partial)
            && target.may_poll
            && usage_cache::live_endpoint_due(&entry, now);
        match live.filter(|_| !endpoint_owed) {
            Some(probe) => {
                // Read BEFORE the apply: `apply_outcome` stamps `fetched_at`,
                // which is what `poll_due` keys on.
                let due = usage_cache::poll_due(&entry, now);
                // A failed probe left the numbers dimmed. A live session
                // confirming those exact numbers is the freshest attempt
                // there is, so it clears the flag (and refreshes the
                // sibling's `fetched_at`) even mid-backoff.
                let dimmed = entry.usage.as_ref().is_some_and(|usage| usage.stale);
                // EXP-881: `due` is NOT a reason to apply. A poll being owed
                // says nothing about the frame, and re-applying an UNCHANGED
                // frame restamped `fetched_at` — which made numbers that had
                // not moved in hours read as freshly fetched, the whole bug.
                // Only a frame that says something new, or one that clears a
                // dim, touches the cache.
                if dimmed || probe.outcome == PollOutcome::Changed {
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
                // EXP-909: `may_poll` too — a secondary login's identity
                // probe is still a request, and it waits its stagger slot
                // like every other one.
                if due && target.may_poll && entry.account.is_none() && probe_names_account(agent) {
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
                        fresh_token.as_deref(),
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
            None if target.may_poll && (endpoint_owed || usage_cache::poll_due(&entry, now)) => {
                polled = true;
                changed = true;
                // Claim the slot BEFORE the (slow) fetch and persist it, so the
                // sibling process sharing this token (IDE vs daemon) sees the
                // poll as taken instead of spending a second request.
                entry.next_poll_at_secs = now + usage_cache::MIN_POLL_SECS;
                entry.endpoint_due_at_secs = Some(now + usage_cache::LIVE_ENDPOINT_POLL_SECS);
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
                    fresh_token.as_deref(),
                );
                if let Some(account) = probe.account {
                    // Persist the identity: the not-due beats in between re-use it
                    // instead of dropping back to the doctor's presence-only row.
                    entry.account = Some(account.clone());
                    apply_account(&mut accounts, &id, &target, account);
                }
                // EXP-881: the ENDPOINT answered, so stamp the read that a
                // live frame is compared against. Only here — a live apply
                // moves `fetched_at_secs`, never this.
                if probe.windows.is_some() {
                    entry.endpoint_fetched_at_secs = Some(now);
                }
                usage_cache::apply_outcome(&mut entry, probe.outcome, probe.windows, now, &stamp);
                usage_cache::schedule_live_endpoint(&mut entry, now);
                cache.insert(cache_id.clone(), entry.clone());
            }
            None => {}
        }
        // Every probe this login could make is behind us: release the
        // keep-alive claim now rather than at the end of the iteration.
        drop(refresh_claim);
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

/// EXP-909 — a usage overlay just OPENED on a run hosted here: read THAT
/// login's numbers now, if the policy allows one.
///
/// Deliberately NOT [`force_collect`]: that one is a person pressing Refresh
/// and sets the shared TTL aside, while opening a popover is a glance. This
/// keeps every floor ([`usage_cache::poll_due`] — the schedule, the
/// machine-wide TTL, the 429, a refused keychain) and only puts the login past
/// the secondary STAGGER, so a run on a parked account still gets one read
/// instead of waiting out its rotation slot. `None` = nothing was due and the
/// caller keeps what it has.
pub fn refresh_on_demand(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    agent: CodingAgent,
    profile: &str,
    now: u64,
) -> Option<AgentStatusPayload> {
    let profile = crate::agent_profiles::get(data_dir, agent, profile.trim())
        .map(|row| row.id)
        .unwrap_or_else(|| crate::agent_profiles::SYSTEM_PROFILE.to_string());
    let cache_id = usage_cache::entry_key(agent.id(), &profile);
    let due = usage_cache::load(data_dir)
        .get(&cache_id)
        .is_none_or(|entry| usage_cache::poll_due(entry, now));
    due.then(|| collect_inner(data_dir, settings, report, now, Some((agent, &profile))))
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

/// EXP-862 — "remove account": delete `profile`'s login from THIS machine.
///
/// The ONE body behind every entry point (the desktop's own chip, its
/// `agent_profile_remove` command handler and the CLI daemon's), so the
/// refusals are the same sentence wherever the removal was asked for.
///
/// What it removes is the machine's copy of a login: the profile's config dir
/// (its credentials, the agent CLI's own files) and its index row. The ACCOUNT
/// itself is untouched — no `codex logout`, which would revoke it server-wide,
/// and no request of any kind leaves this machine. The device default falls
/// back to the ambient login when the removed profile held it.
///
/// `Err` is the sentence to show: the ambient login (which is the agent CLI's
/// own, not ours to delete), an id this machine does not have, a login a LIVE
/// run here is using, or an unwritable index.
pub fn remove_profile(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    agent: CodingAgent,
    profile: &str,
    live_accounts: &[String],
    now: u64,
) -> Result<AgentStatusPayload, String> {
    let profile = profile.trim();
    if crate::agent_profiles::is_system(Some(profile)) {
        return Err(format!(
            "That is this machine's own {} login, not one Exponential can remove.",
            agent.id()
        ));
    }
    if crate::agent_profiles::get(data_dir, agent, profile).is_none() {
        return Err(format!("No such {} account on this machine.", agent.id()));
    }
    // A live run reads the profile's config dir for as long as it turns:
    // pulling the credentials out from under it would break the run mid-turn
    // with an error nobody could place.
    if live_accounts
        .iter()
        .any(|account| account.trim() == profile)
    {
        return Err(
            "A live run on this machine still uses that account. End it first.".to_string(),
        );
    }
    crate::agent_profiles::remove(data_dir, agent, profile)
        .map_err(|err| format!("Could not remove the {} account here: {err}", agent.id()))?;
    // Its numbers and its identity go with it: a later profile created under
    // a recycled id must never inherit them.
    usage_cache::forget_profile(data_dir, agent.id(), profile);
    Ok(collect_if_due(data_dir, settings, report, now))
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
    /// active login, for a forced refresh and for a login this machine has
    /// never read (EXP-862 — no cache entry); for the others it is the
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
/// EXP-862: a login this machine has NEVER read (no cache entry at all — just
/// added, just signed in, just [`usage_cache::forget_profile`]d) is read on the
/// very next pass. Waiting for its rotation slot is what left a fresh account
/// captionless for minutes; the first read is also the cheapest one, since a
/// machine only ever gains an account by a person adding it.
///
/// Past that first read, at most ONE non-active login is probed per pass, the
/// one whose numbers are OLDEST, and never within [`PROFILE_STAGGER_SECS`] of
/// the previous secondary probe: the poll floors are per LOGIN, so an unspaced
/// fan-out would multiply this machine's request rate by the number of
/// accounts on it.
fn usage_targets(
    data_dir: &Path,
    report: &DoctorReport,
    eligible: &BTreeMap<String, bool>,
    cache: &usage_cache::UsageCache,
    now: u64,
    forced: Option<(CodingAgent, &str)>,
) -> Vec<UsageTarget> {
    let mut targets: Vec<UsageTarget> = Vec::new();
    // (index into `targets`, its numbers' age) for every non-active login a
    // probe could go to right now — the oldest wins the pass's one slot.
    let mut secondary: Vec<(usize, u64)> = Vec::new();
    // When this machine last spent a probe on a non-active login, as the
    // cache records it: the spacing the rotation keys on.
    let mut last_secondary_probe_secs: u64 = 0;
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
            // EXP-862: never read here = read now. Everything else waits its
            // turn below.
            let entry = cache.get(&key);
            let first_read = entry.is_none();
            targets.push(UsageTarget {
                agent,
                profile: profile.id,
                dir,
                active: is_active,
                may_poll: is_active || first_read,
            });
            if let (false, Some(entry)) = (is_active, entry) {
                last_secondary_probe_secs = last_secondary_probe_secs.max(entry.fetched_at_secs);
                if usage_cache::poll_due(entry, now) {
                    secondary.push((targets.len() - 1, entry.fetched_at_secs));
                }
            }
        }
    }
    if now.saturating_sub(last_secondary_probe_secs) >= PROFILE_STAGGER_SECS {
        // Oldest numbers first; ties keep the listing order, so the choice is
        // deterministic for one machine's state.
        if let Some((index, _)) = secondary.iter().min_by_key(|(_, fetched)| *fetched) {
            targets[*index].may_poll = true;
        }
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

/// EXP-754/EXP-881 — the windows a live session on `profile` published, when
/// they are still CURRENT.
///
/// "Current" is a question about the FRAME, not about attachment. A frame
/// answers when
/// * it is younger than the cadence that applies — the endpoint's own
///   ([`usage_cache::LIVE_ENDPOINT_POLL_SECS`]) while a session is attached
///   (a live run republishes every turn, so a recent frame is worth a fetch),
///   the machine-wide TTL ([`usage_cache::SHARED_TTL_SECS`]) once nobody is
///   attached — and
/// * no window it carries has RESET since ([`any_reset_passed`]): past its
///   `resets_at` the percentage is a lie about the new window, and reporting
///   it would show a full bar on an account that just came back.
///
/// EXP-881: an ATTACHED session no longer pins its numbers current forever.
/// An idle run publishes nothing, so `sessions > 0` only WIDENS the window a
/// frame is trusted for; it never resurrects an old one.
///
/// `None` means "nobody is telling us" — the caller falls back to the poll
/// policy and its spawn. codex pushes `account/rateLimits/updated` down the
/// app-server connection and claude prints a `rate_limit_event` per turn
/// (EXP-819).
fn live_probe(
    agent: CodingAgent,
    profile: &str,
    entry: &AgentCacheEntry,
    now: u64,
) -> Option<AgentProbe> {
    let source = live_source(agent)?;
    // EXP-909: per LOGIN — a run on another account says nothing about this
    // one's numbers.
    let live = live::snapshot(agent, profile)?;
    if live.windows.is_empty() {
        return None;
    }
    let frame_at = live.updated_at_secs?;
    let currency = if live.sessions > 0 {
        usage_cache::LIVE_ENDPOINT_POLL_SECS
    } else {
        usage_cache::SHARED_TTL_SECS
    };
    if now.saturating_sub(frame_at) >= currency {
        return None;
    }
    if any_reset_passed(&live.windows, now) {
        return None;
    }
    let windows = match source {
        LiveSource::Whole => live.windows,
        LiveSource::Partial => {
            // The frame covers only some keys, so it is laid OVER the last
            // report. A report read AFTER the frame already contains the
            // frame's turn: overlaying it then would drag those keys
            // backwards, so the frame simply stops answering.
            if entry
                .endpoint_fetched_at_secs
                .is_some_and(|read_at| read_at >= frame_at)
            {
                return None;
            }
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

/// EXP-881 — has any of these windows RESET since it was measured?
///
/// A percentage belongs to the window it was read in; once that window's
/// `resets_at` is behind us the number says nothing about the fresh one (and
/// a stuck 100 % is exactly what the user is waiting to see fall). A window
/// with no `resets_at`, or one whose stamp does not parse, never kills the
/// frame — an unreadable stamp is not evidence of a reset.
fn any_reset_passed(windows: &[UsageWindow], now: u64) -> bool {
    let now_ms = (now as i64).saturating_mul(1000);
    windows.iter().any(|window| {
        window
            .resets_at
            .as_deref()
            .and_then(crate::agent_accounts::unix_millis_from_iso)
            .is_some_and(|at| now_ms >= at)
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

/// EXP-852 — one claude keep-alive attempt for ONE login, under the CLI's own
/// locks ([`claude_oauth::refresh_if_expiring`]). Returns the access token the
/// usage GET should use, when there is one.
fn claude_keep_alive_step(
    config_dir: Option<&Path>,
    entry: &mut AgentCacheEntry,
    now: u64,
) -> Option<String> {
    claude_keep_alive_step_at(claude_oauth::CLAUDE_TOKEN_URL, config_dir, entry, now)
}

/// [`claude_keep_alive_step`] against ONE token endpoint — the seam the tests
/// point at a local server, since [`claude_oauth::CLAUDE_TOKEN_URL`] is a
/// const.
fn claude_keep_alive_step_at(
    endpoint: &str,
    config_dir: Option<&Path>,
    entry: &mut AgentCacheEntry,
    now: u64,
) -> Option<String> {
    // The grants this login already knows are dead: cloned because the
    // request borrows them while `entry` is written below.
    let dead = entry.dead_refresh_tokens.clone();
    let outcome = claude_oauth::refresh_if_expiring(RefreshRequest {
        config_dir,
        token_endpoint: endpoint,
        now,
        margin_secs: usage_cache::CLAUDE_REFRESH_MARGIN_SECS,
        dead_refresh_tokens: &dead,
        lock_options: crate::lockfile::LockOptions::oauth_refresh(),
    });
    // A fixed word, never a token: every arm below logs through this.
    let label = outcome.label();
    match outcome {
        RefreshOutcome::Refreshed {
            access_token,
            expires_at_ms,
            wrote,
        } => {
            usage_cache::note_refresh_ok(entry, expires_at_ms, now);
            match wrote {
                // The keychain refused cleanly and the rotated pair went to
                // `<root>/.credentials.json` instead. Nothing is lost, but the
                // login's store MOVED — worth a line at the next incident.
                WriteOutcome::SavedToFallbackFile => log::warn!(
                    "claude keep-alive: {label} ({wrote:?}) — the credential store refused, so the rotated token went to the fallback file"
                ),
                // A `claude logout` landed during the POST. The grant is
                // spent and the pair is kept for this one probe, but the
                // store stays signed out: the next beat finds it missing and
                // backs off an hour.
                WriteOutcome::StoreGone => log::warn!(
                    "claude keep-alive: {label} ({wrote:?}): the credential store vanished mid-refresh; the logout stands"
                ),
                _ => log::info!("claude keep-alive: {label} ({wrote:?})"),
            }
            Some(access_token)
        }
        RefreshOutcome::NotNeeded {
            access_token,
            expires_at_ms,
        } => {
            usage_cache::note_credential_expiry(entry, expires_at_ms);
            // A document with no (or a non-numeric) `expiresAt` cannot be
            // scheduled off its expiry, and `refresh_if_expiring` never POSTs
            // on a guess: without the hour backoff the `None` gate would
            // re-read the keychain on every beat to learn the same nothing.
            if expires_at_ms.is_none() {
                usage_cache::note_refresh_failed(
                    entry,
                    now,
                    usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS,
                );
            }
            log::debug!("claude keep-alive: {label}");
            Some(access_token)
        }
        // A store with no refresh token will not grow one without a relogin:
        // an hour, not ten minutes, or every beat re-reads the keychain.
        RefreshOutcome::NoRefreshToken => {
            usage_cache::note_refresh_failed(
                entry,
                now,
                usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS,
            );
            log::debug!("claude keep-alive: {label}");
            None
        }
        RefreshOutcome::InvalidGrant { dead_marker } => {
            usage_cache::note_dead_refresh_token(entry, dead_marker);
            entry.health = Some(
                crate::agent_accounts::Health::NeedsRelogin
                    .as_str()
                    .to_string(),
            );
            // The dead marker alone would still cost a store read every beat
            // (the precheck has to see the token to recognise it), so the
            // same long backoff applies.
            //
            // ACCEPTED CONSEQUENCE: `apply_outcome` flips health back to `ok`
            // if a later usage GET on the still-live ACCESS token succeeds —
            // health is the probe's fact (EXP-849) and we do not fight it.
            // The persisted dead marker is what matters; `needs_relogin`
            // returns for good with the first 401 after the access token
            // expires, which is the truth arriving a few hours late.
            usage_cache::note_refresh_failed(
                entry,
                now,
                usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS,
            );
            log::warn!("claude keep-alive: {label} — this login needs a re-login");
            None
        }
        // A sibling process or the CLI itself is refreshing right now. Nothing
        // was spent and nothing is owed: the next beat re-reads the store and
        // finds their token. Silent on purpose — it is the normal outcome on a
        // machine running both the IDE and the daemon.
        RefreshOutcome::Contended => None,
        // EXP-849's rule: a flaky network is not a broken account, so health
        // is untouched and only the backoff moves.
        RefreshOutcome::Failed(reason) => {
            usage_cache::note_refresh_failed(entry, now, usage_cache::REFRESH_FAILED_BACKOFF_SECS);
            log::warn!("claude keep-alive: {label} ({reason})");
            None
        }
        // A keychain ACL modal nobody will answer: stop asking for an hour,
        // exactly as a refused usage read does.
        RefreshOutcome::Denied => {
            entry.credential_denied_until_secs =
                Some(now + usage_cache::CREDENTIAL_DENIED_BACKOFF_SECS);
            log::warn!("claude keep-alive: {label} — the credential store refused");
            None
        }
    }
}

/// Probe ONE login's usage. EXP-808: `config_dir` is the account profile's
/// config dir — claude reads the credential kept beside it, codex answers
/// with that `CODEX_HOME` in its app-server's env. `None` = the ambient
/// login.
///
/// EXP-852: `fresh_access_token` is the token claude's keep-alive just
/// rotated for THIS login, when it ran on this beat — either on its schedule
/// or, EXP-881, because the cached token had already expired and a usage read
/// on a dead token would have looked like a broken account.
// One call site per match arm, and every argument is a fact the CALLER
// already holds (the plan's target, the pass's cache entry, the keep-alive's
// answers): bundling them into a struct would move the same fields one line
// up and hide which of them each agent arm actually reads.
#[allow(clippy::too_many_arguments)]
fn probe_agent(
    agent: CodingAgent,
    settings: &Settings,
    version: Option<&str>,
    config_dir: Option<&Path>,
    entry: &mut AgentCacheEntry,
    now: u64,
    keep_alive: bool,
    fresh_access_token: Option<&str>,
) -> AgentProbe {
    match agent {
        CodingAgent::Claude => {
            let user_agent = claude_user_agent(version);
            // EXP-852: the keep-alive put this exact token in the store a
            // moment ago and handed us a copy. Reading it back out is a
            // wasted keychain shell-out (and, on macOS, a second chance for
            // an ACL prompt) for a value we are already holding.
            if let Some(token) = fresh_access_token {
                return fetch_and_parse(token, &user_agent);
            }
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
                    // EXP-852: stamp what this read saw on EVERY read, the
                    // keep-alive's setting notwithstanding — the field is the
                    // refresh cadence's input, so flipping the setting on
                    // must not owe a blind store read first.
                    usage_cache::note_credential_expiry(entry, credential.expires_at_ms);
                    if credential.expired(now as i64 * 1000) {
                        // EXP-881: the rotation ran on this beat whatever the
                        // keep-alive setting said, so this is the arm where it
                        // could not happen — a sibling process holds the
                        // machine-wide claim, the store carries no refresh
                        // token, or the grant is dead. There are no numbers
                        // this pass; the old ones stay, dimmed, until a later
                        // beat rotates it or the user's own CLI does.
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
    use crate::agent_profiles::SYSTEM_PROFILE;

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

    /// EXP-881: the reset is far in the FUTURE on purpose. A live frame stops
    /// answering once a window it carries has reset (`any_reset_passed`), so a
    /// fixture with a stamp in the past would silently kill every live test
    /// the moment the wall clock caught up with it.
    fn session_window(percent: u8) -> UsageWindow {
        UsageWindow {
            key: "session".to_string(),
            label: "5h".to_string(),
            percent,
            resets_at: Some("2099-09-06T14:00:00.000Z".to_string()),
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

        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(4)]);

        let now = crate::run_registry::now_secs();
        let payload = collect_if_due(&dir, &settings, &report, now);
        let usage = payload.usage.get("codex").expect("the live windows");
        assert_eq!(usage.windows, vec![session_window(4)]);
        assert!(!usage.stale, "a live read is a read, not a fallback");

        // Ten seconds later — deep inside the poll floor — moving numbers
        // still land, because reading them costs nothing.
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(9)]);
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
        drop(live::attach(CodingAgent::Codex, SYSTEM_PROFILE));
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(4)]);

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
        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, windows.clone());

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

        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        let windows = vec![session_window(12)];
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, windows.clone());

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
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(13)]);
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
    /// vanishing. The cache entry is fresh and not due (its endpoint stamp
    /// included), so the pass owes the endpoint nothing.
    ///
    /// EXP-881: every reset here is in 2099 — a window that has already reset
    /// stops the frame answering at all — and the last leg locks the other
    /// half of the overlay rule: a report read AFTER the frame already
    /// contains that turn, so the frame steps aside instead of dragging the
    /// numbers back.
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
        let opus = window("model:opus", "Opus", 33, "2099-09-10T00:00:00.000Z");

        let now = crate::run_registry::now_secs();
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("claude", SYSTEM_PROFILE),
            cached(
                vec![
                    window("session", "5h", 40, "2099-09-05T19:00:00.000Z"),
                    opus.clone(),
                    window("weekly", "Week", 10, "2099-09-10T00:00:00.000Z"),
                ],
                now - 10,
            ),
        );
        usage_cache::save(&dir, &cache);

        let session = live::attach(CodingAgent::Claude, SYSTEM_PROFILE);
        live::publish(
            CodingAgent::Claude,
            SYSTEM_PROFILE,
            vec![
                window("session", "5h", 55, "2099-09-06T14:00:00.000Z"),
                UsageWindow { key: "weekly".to_string(), label: "Week".to_string(), percent: 12, resets_at: None },
            ],
        );
        let payload = collect_if_due(&dir, &settings, &report, now);
        let usage = payload.usage.get("claude").expect("the merged windows");
        assert_eq!(
            usage.windows,
            vec![
                window("session", "5h", 55, "2099-09-06T14:00:00.000Z"),
                opus.clone(),
                window("weekly", "Week", 12, "2099-09-10T00:00:00.000Z"),
            ],
            "the frame's keys move, the endpoint's row and its reset stay"
        );
        assert!(!usage.stale);

        // The next turn, seconds later: still lands, still over the report.
        live::publish(
            CodingAgent::Claude,
            SYSTEM_PROFILE,
            vec![window("session", "5h", 58, "2099-09-06T14:00:00.000Z")],
        );
        let payload = collect_if_due(&dir, &settings, &report, now + 10);
        let usage = payload.usage.get("claude").expect("the merged windows");
        assert_eq!(
            usage.windows.iter().map(|window| (window.key.as_str(), window.percent)).collect::<Vec<_>>(),
            vec![("session", 58), ("model:opus", 33), ("weekly", 12)]
        );

        // EXP-881 — an endpoint report read AFTER that frame already covers
        // the frame's turn. Re-laying the frame would push `session` back to
        // 58; the frame stops answering instead and the report stands.
        let mut cache = usage_cache::load(&dir);
        let mut entry = cache
            .get(&usage_cache::entry_key("claude", SYSTEM_PROFILE))
            .cloned()
            .expect("the live-applied entry");
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![
                window("session", "5h", 61, "2099-09-06T14:00:00.000Z"),
                opus.clone(),
                window("weekly", "Week", 14, "2099-09-10T00:00:00.000Z"),
            ]),
            now + 20,
            "LATER",
        );
        entry.endpoint_fetched_at_secs = Some(now + 20);
        entry.next_poll_at_secs = now + 10_000;
        entry.endpoint_due_at_secs = Some(now + 10_000);
        cache.insert(usage_cache::entry_key("claude", SYSTEM_PROFILE), entry);
        usage_cache::save(&dir, &cache);

        let payload = collect_if_due(&dir, &settings, &report, now + 30);
        let usage = payload.usage.get("claude").expect("the endpoint's report");
        assert_eq!(
            usage.windows.iter().map(|window| (window.key.as_str(), window.percent)).collect::<Vec<_>>(),
            vec![("session", 61), ("model:opus", 33), ("weekly", 14)],
            "a frame older than the report never overlays it"
        );
        assert_eq!(usage.fetched_at, "LATER", "and it never restamps it either");

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live frames restamp `fetched_at`/`next_poll_at` on every turn, so the
    /// endpoint's own schedule under a live claude run is a separate stamp: a
    /// poll sets it no sooner than the live cadence or any backoff it earned,
    /// a live apply leaves it alone, and a forced refresh makes it owed now.
    #[test]
    fn a_live_run_still_owes_the_endpoint_its_other_windows() {
        let now = 50_000;
        let mut entry = AgentCacheEntry::default();
        assert!(usage_cache::live_endpoint_due(&entry, now), "never polled = owed");

        usage_cache::apply_outcome(&mut entry, PollOutcome::Changed, Some(Vec::new()), now, "s");
        usage_cache::schedule_live_endpoint(&mut entry, now);
        assert_eq!(entry.endpoint_due_at_secs, Some(now + usage_cache::LIVE_ENDPOINT_POLL_SECS));

        // A burst of live applies keeps pushing the poll clock forward…
        for tick in 1..=30 {
            let windows = vec![UsageWindow { key: "session".into(), percent: tick, ..Default::default() }];
            usage_cache::apply_outcome(&mut entry, PollOutcome::Changed, Some(windows), now + u64::from(tick) * 20, "s");
        }
        assert!(!usage_cache::poll_due(&entry, now + 600));
        // …but the endpoint still comes due on its own cadence.
        assert!(!usage_cache::live_endpoint_due(&entry, now + 599));
        assert!(usage_cache::live_endpoint_due(&entry, now + 600));

        // A refused keychain read backs the owed poll off for its full hour.
        usage_cache::apply_outcome(&mut entry, PollOutcome::Failed, None, now, "s");
        entry.credential_denied_until_secs = Some(now + usage_cache::CREDENTIAL_DENIED_BACKOFF_SECS);
        usage_cache::schedule_live_endpoint(&mut entry, now);
        assert!(!usage_cache::live_endpoint_due(&entry, now + 1_800));

        assert_eq!(usage_cache::force_due(&mut entry, now), Ok(()));
        assert!(usage_cache::live_endpoint_due(&entry, now));

        // EXP-881: through all of that, `endpoint_fetched_at_secs` never
        // moved — only the collector's POLL arm stamps it, and no poll ran
        // here. It is the frame's yardstick, so a live apply must never touch
        // it (that is what made a frame look newer than the report forever).
        assert_eq!(entry.endpoint_fetched_at_secs, None);
    }

    /// EXP-909 — the registry is keyed by LOGIN, so a run on the ambient
    /// account says nothing about the machine's ACTIVE one. Before this, the
    /// only key was the agent and whatever ran last wrote the active login's
    /// numbers — a run on a second account silently rewrote the bar of an
    /// account it never touched.
    #[test]
    fn a_live_session_on_one_account_never_moves_anothers_numbers() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-per-login");
        let settings = Settings {
            codex_path: "/usr/bin/true".to_string(),
            ..Settings::default()
        };
        let work = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Work").unwrap();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();

        let now = crate::run_registry::now_secs();
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("codex", SYSTEM_PROFILE),
            cached(vec![session_window(11)], now),
        );
        cache.insert(
            usage_cache::entry_key("codex", &work.id),
            cached(vec![session_window(22)], now),
        );
        usage_cache::save(&dir, &cache);

        // A run on the AMBIENT login while `work` is the machine default.
        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(88)]);

        let payload = collect_if_due(&dir, &settings, &codex_named_report(), now);
        let row = |id: &str| {
            payload.accounts["codex"]
                .profiles
                .iter()
                .find(|row| row.id == id)
                .and_then(|row| row.usage.as_ref())
                .map(|usage| usage.windows.clone())
                .unwrap_or_default()
        };
        assert_eq!(row(SYSTEM_PROFILE), vec![session_window(88)], "the run's own login moved");
        assert_eq!(row(&work.id), vec![session_window(22)], "the active login did NOT");
        assert_eq!(
            payload.usage["codex"].windows,
            vec![session_window(22)],
            "and the top-level map still reports the active login"
        );

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-881 — an ATTACHED session widens how long its frame is trusted (a
    /// live run republishes every turn), it does not make an old frame
    /// current. Past the endpoint's own cadence an idle run's last frame
    /// stops answering and the poll takes the question back.
    #[test]
    fn a_live_snapshot_older_than_the_endpoints_cadence_stops_answering() {
        let _lock = live_lock();
        live::reset();
        let entry = AgentCacheEntry::default();
        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![session_window(4)]);
        let published = crate::run_registry::now_secs();

        assert!(
            live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, published).is_some(),
            "a frame from this turn answers"
        );
        assert!(
            live_probe(
                CodingAgent::Codex,
                SYSTEM_PROFILE,
                &entry,
                published + usage_cache::LIVE_ENDPOINT_POLL_SECS - 1,
            )
            .is_some(),
            "and it keeps answering inside the cadence, session attached"
        );
        assert!(
            live_probe(
                CodingAgent::Codex,
                SYSTEM_PROFILE,
                &entry,
                published + usage_cache::LIVE_ENDPOINT_POLL_SECS,
            )
            .is_none(),
            "past it the session being open is not evidence of anything"
        );

        // Detached, the frame is trusted for the shared TTL and no longer.
        drop(session);
        assert!(
            live_probe(
                CodingAgent::Codex,
                SYSTEM_PROFILE,
                &entry,
                published + usage_cache::SHARED_TTL_SECS - 1,
            )
            .is_some()
        );
        assert!(
            live_probe(
                CodingAgent::Codex,
                SYSTEM_PROFILE,
                &entry,
                published + usage_cache::SHARED_TTL_SECS,
            )
            .is_none()
        );
        live::reset();
    }

    /// EXP-881 — a percentage belongs to the window it was measured in. Once
    /// that window's `resets_at` is behind us the frame says nothing about
    /// the fresh one, and a stuck 100 % is exactly the number the user is
    /// waiting to see fall. An unreadable stamp is not evidence of a reset.
    #[test]
    fn a_window_whose_reset_has_passed_kills_the_live_snapshot() {
        let _lock = live_lock();
        live::reset();
        let entry = AgentCacheEntry::default();
        let now = crate::run_registry::now_secs();
        let window = |resets: Option<&str>| UsageWindow {
            key: "session".to_string(),
            label: "5h".to_string(),
            percent: 100,
            resets_at: resets.map(str::to_string),
        };

        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(
            CodingAgent::Codex,
            SYSTEM_PROFILE,
            vec![window(Some("2026-09-06T14:00:00.000Z"))],
        );
        assert!(
            live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, now).is_none(),
            "that window reset long ago"
        );

        live::publish(
            CodingAgent::Codex,
            SYSTEM_PROFILE,
            vec![window(Some("2099-09-06T14:00:00.000Z"))],
        );
        assert!(live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, now).is_some());

        // Neither an absent nor an unparsable stamp is evidence of a reset.
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![window(None)]);
        assert!(live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, now).is_some());
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![window(Some("soon"))]);
        assert!(live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, now).is_some());

        drop(session);
        live::reset();
    }

    /// EXP-881, THE regression — a frame that says exactly what the cache
    /// already holds must not restamp it. `apply_outcome` moves
    /// `fetched_at`, which is the age every client renders, so re-applying an
    /// UNCHANGED frame on a due beat made numbers that had not moved in hours
    /// read as freshly fetched. A poll being due is not a reason to apply
    /// anything.
    #[test]
    fn an_unchanged_live_frame_never_restamps_the_numbers() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("live-unchanged");
        let settings = Settings {
            codex_path: "/nonexistent/codex".to_string(),
            ..Settings::default()
        };
        let windows = vec![session_window(46)];
        let now = crate::run_registry::now_secs();

        let mut entry = AgentCacheEntry::default();
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now - 10_000,
            "EARLIER",
        );
        // The identity is known, so this pass owes nobody a probe…
        entry.account = Some(crate::agent_accounts::AgentAccount {
            signed_in: true,
            email: Some("dev@example.com".to_string()),
            ..Default::default()
        });
        // …and a poll IS due, which used to be reason enough to re-apply.
        entry.next_poll_at_secs = now - 1;
        assert!(usage_cache::poll_due(&entry, now));
        let mut cache = usage_cache::UsageCache::default();
        cache.insert(usage_cache::entry_key("codex", SYSTEM_PROFILE), entry);
        usage_cache::save(&dir, &cache);

        let session = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, windows.clone());

        let payload = collect_if_due(&dir, &settings, &codex_ready_report(), now);
        let usage = payload.usage.get("codex").expect("the cached windows");
        assert_eq!(usage.windows, windows);
        assert_eq!(
            usage.fetched_at, "EARLIER",
            "the numbers keep the age they actually have"
        );
        let stored = usage_cache::load(&dir)
            .get(&usage_cache::entry_key("codex", SYSTEM_PROFILE))
            .cloned()
            .expect("the entry");
        assert_eq!(stored.fetched_at_secs, now - 10_000, "nothing was restamped");

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-881 — claude's frame is a SUBSET of the endpoint's report, laid
    /// over it by key. That is only sound while the frame is the newer of the
    /// two: a report read after the frame already covers the frame's turn, so
    /// overlaying it would drag those keys backwards.
    #[test]
    fn a_live_frame_older_than_the_endpoint_report_stays_under_it() {
        let _lock = live_lock();
        live::reset();
        let window = |percent: u8| UsageWindow {
            key: "session".to_string(),
            label: "5h".to_string(),
            percent,
            resets_at: Some("2099-09-06T14:00:00.000Z".to_string()),
        };
        let session = live::attach(CodingAgent::Claude, SYSTEM_PROFILE);
        live::publish(CodingAgent::Claude, SYSTEM_PROFILE, vec![window(55)]);
        let published = crate::run_registry::now_secs();

        let mut entry = AgentCacheEntry::default();
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window(61)]),
            published,
            "REPORT",
        );

        // A report read BEFORE the frame: the frame is the newer word.
        entry.endpoint_fetched_at_secs = Some(published - 1);
        let probe = live_probe(CodingAgent::Claude, SYSTEM_PROFILE, &entry, published)
            .expect("the frame answers");
        assert_eq!(probe.windows.as_deref(), Some(&[window(55)][..]));

        // Read at the same second or later: the report stands on its own.
        entry.endpoint_fetched_at_secs = Some(published);
        assert!(live_probe(CodingAgent::Claude, SYSTEM_PROFILE, &entry, published).is_none());
        entry.endpoint_fetched_at_secs = Some(published + 30);
        assert!(live_probe(CodingAgent::Claude, SYSTEM_PROFILE, &entry, published + 30).is_none());

        // codex's frame is the WHOLE report, so no such question arises.
        let codex = live::attach(CodingAgent::Codex, SYSTEM_PROFILE);
        live::publish(CodingAgent::Codex, SYSTEM_PROFILE, vec![window(55)]);
        assert!(live_probe(CodingAgent::Codex, SYSTEM_PROFILE, &entry, published).is_some());

        drop(codex);
        drop(session);
        live::reset();
    }

    /// EXP-909 — the identity probe under a live session is still a REQUEST,
    /// and a secondary login's requests wait their stagger slot like every
    /// other one. Before this, consulting the live registry for every profile
    /// would have let a nameless secondary spawn an app-server on any beat,
    /// multiplying this machine's request rate by the number of accounts on
    /// it — the very thing the stagger exists to stop.
    #[cfg(unix)]
    #[test]
    fn a_secondary_logins_identity_probe_still_waits_its_stagger_slot() {
        use std::os::unix::fs::PermissionsExt as _;

        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("secondary-identity");
        // A stand-in codex that records the ARGV of every spawn (`login
        // status` answers 0 with no output, so both logins read as signed in
        // for the doctor). The identity probe is the one that asks for
        // `app-server`, which is how this test tells it from the doctor's own
        // spawns.
        let marker = dir.join("probed");
        let program = dir.join("codex-stub.sh");
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho \"$@\" >> {}\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let settings = Settings {
            codex_path: program.to_string_lossy().to_string(),
            ..Settings::default()
        };
        let home = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Home").unwrap();
        let other = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Other").unwrap();

        // The ambient login is the machine default. `home` is a secondary
        // that IS due a poll and whose identity nobody has named yet — but a
        // sibling secondary was read a second ago, so this beat's secondary
        // slot is already spent and `home` may not poll.
        let now = crate::run_registry::now_secs();
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("codex", SYSTEM_PROFILE),
            cached(vec![session_window(11)], now),
        );
        cache.insert(
            usage_cache::entry_key("codex", &home.id),
            AgentCacheEntry {
                fetched_at_secs: now - 10_000,
                next_poll_at_secs: 0,
                ..AgentCacheEntry::default()
            },
        );
        cache.insert(
            usage_cache::entry_key("codex", &other.id),
            cached(vec![session_window(33)], now - 1),
        );
        usage_cache::save(&dir, &cache);

        // The state the guard is about: a poll IS due for `home`, and its
        // stagger slot is NOT free.
        let eligible = std::collections::BTreeMap::from([
            (usage_cache::entry_key("codex", SYSTEM_PROFILE), true),
            (usage_cache::entry_key("codex", &home.id), true),
            (usage_cache::entry_key("codex", &other.id), true),
        ]);
        let targets = usage_targets(
            &dir,
            &codex_named_report(),
            &eligible,
            &usage_cache::load(&dir),
            now,
            None,
        );
        let target = targets
            .iter()
            .find(|target| target.profile == home.id)
            .expect("the secondary is a target");
        assert!(!target.may_poll, "its slot is spent this beat");
        assert!(usage_cache::poll_due(
            usage_cache::load(&dir)
                .get(&usage_cache::entry_key("codex", &home.id))
                .unwrap(),
            now
        ));

        // A run on that secondary login publishes its windows.
        let session = live::attach(CodingAgent::Codex, &home.id);
        live::publish(CodingAgent::Codex, &home.id, vec![session_window(64)]);

        let payload = collect_if_due(&dir, &settings, &codex_named_report(), now);
        let spawns = std::fs::read_to_string(&marker).unwrap_or_default();
        assert!(
            !spawns.contains("app-server"),
            "a secondary login's identity probe waits its slot: {spawns}"
        );
        let windows = payload.accounts["codex"]
            .profiles
            .iter()
            .find(|row| row.id == home.id)
            .and_then(|row| row.usage.as_ref())
            .map(|usage| usage.windows.clone())
            .unwrap_or_default();
        assert_eq!(
            windows,
            vec![session_window(64)],
            "its windows still land — reading them costs nothing"
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
            endpoint_due_at_secs: Some(now + 10_000),
            // EXP-881: the ENDPOINT produced this report, so its own stamp is
            // set too — a live frame is only laid over a report it is NEWER
            // than.
            endpoint_fetched_at_secs: Some(now),
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

    /// EXP-808/EXP-862 — the plan a pass runs: every login of the agent, the
    /// ACTIVE one first (so the cap can never truncate it away), at most
    /// [`MAX_USAGE_PROFILES`] of them, a login this machine has NEVER read
    /// always, and past that at most one non-active login per pass — the one
    /// whose numbers are oldest, spaced by [`PROFILE_STAGGER_SECS`].
    #[test]
    fn the_profile_fan_out_is_capped_ordered_and_spaced() {
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

        // A machine that has read NOTHING yet: every login it plans to look
        // at is read on this very pass. A fresh account waiting minutes for
        // its rotation slot is what EXP-862 fixed, and the first read is the
        // cheapest one — a machine only gains an account when a person adds
        // one.
        let now = 1_800_000_000;
        let empty = usage_cache::UsageCache::default();
        let targets = usage_targets(&dir, &report, &eligible, &empty, now, None);
        assert_eq!(
            targets.len(),
            MAX_USAGE_PROFILES,
            "{} logins, {MAX_USAGE_PROFILES} probed",
            ids.len() + 1
        );
        assert_eq!(targets[0].profile, active);
        assert!(targets[0].active && targets[0].may_poll);
        assert!(
            targets.iter().all(|target| target.may_poll),
            "every never-read login is read at once"
        );
        // A custom profile carries its config dir; the ambient login does not.
        assert!(targets[0].dir.is_some());
        assert!(targets
            .iter()
            .find(|target| target.profile == crate::agent_profiles::SYSTEM_PROFILE)
            .unwrap()
            .dir
            .is_none());

        // Now every login has numbers, each staler than the last. One pass
        // takes ONE of them: the oldest.
        let mut cache = usage_cache::UsageCache::default();
        let planned: Vec<String> = targets
            .iter()
            .map(|target| target.profile.clone())
            .collect();
        for (age, profile) in planned.iter().enumerate() {
            cache.insert(
                usage_cache::entry_key("codex", profile),
                AgentCacheEntry {
                    fetched_at_secs: now - 10_000 - age as u64 * 100,
                    ..AgentCacheEntry::default()
                },
            );
        }
        let oldest = planned.last().unwrap().clone();
        let targets = usage_targets(&dir, &report, &eligible, &cache, now, None);
        let polled: Vec<&str> = targets
            .iter()
            .filter(|target| target.may_poll && !target.active)
            .map(|target| target.profile.as_str())
            .collect();
        assert_eq!(
            polled,
            vec![oldest.as_str()],
            "one secondary per pass, the stalest first"
        );

        // A secondary probed moments ago holds the whole rotation back: the
        // poll floors are per LOGIN, so an unspaced fan-out would multiply
        // this machine's request rate by the number of accounts on it.
        let mut hot = usage_cache::UsageCache::default();
        for profile in &planned {
            hot.insert(
                usage_cache::entry_key("codex", profile),
                AgentCacheEntry {
                    fetched_at_secs: now - 1,
                    ..AgentCacheEntry::default()
                },
            );
        }
        let targets = usage_targets(&dir, &report, &eligible, &hot, now, None);
        assert!(
            !targets.iter().any(|target| target.may_poll && !target.active),
            "a secondary probed a second ago spaces the next one out"
        );

        // A person pressing Refresh is not a beat: the forced login polls
        // whatever the spacing says.
        let forced = ids[0].clone();
        let targets = usage_targets(
            &dir,
            &report,
            &eligible,
            &hot,
            now,
            Some((CodingAgent::Codex, &forced)),
        );
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
        let targets = usage_targets(&dir, &report, &eligible, &empty, now, None);
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
        // spends no probe at all — it only has to REPORT. EXP-881: the wall
        // clock, because the live leg below publishes against it and a frame
        // is only current relative to the real instant it was stamped at.
        let now = crate::run_registry::now_secs();
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

        // EXP-909 — a live session on the NON-active login. Its frame moves
        // that login's row and nothing else: not its siblings' rows, and not
        // the top-level map, which stays the active login's.
        let session = live::attach(CodingAgent::Codex, &home.id);
        live::publish(CodingAgent::Codex, &home.id, vec![session_window(77)]);
        let payload = collect_if_due(&dir, &settings, &codex_named_report(), now);
        let codex = &payload.accounts["codex"];
        let windows = |id: &str| {
            codex
                .profiles
                .iter()
                .find(|row| row.id == id)
                .unwrap_or_else(|| panic!("no row for {id}"))
                .usage
                .as_ref()
                .expect("this login's own numbers")
                .windows
                .clone()
        };
        assert_eq!(windows(&home.id), vec![session_window(77)], "its own row moved");
        assert_eq!(windows(&work.id), vec![session_window(22)], "the active login did not");
        assert_eq!(
            windows(crate::agent_profiles::SYSTEM_PROFILE),
            vec![session_window(11)],
            "and neither did the ambient one"
        );
        assert_eq!(
            payload.usage["codex"].windows,
            vec![session_window(22)],
            "the map still reports the ACTIVE login"
        );

        drop(session);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-862 — a machine that has read nothing reads every login it plans
    /// to look at on its first beat (nobody waits minutes for an account they
    /// just added), and from then on one beat claims the active login plus at
    /// most one secondary slot: the poll floors are per LOGIN, so an unspaced
    /// fan-out would multiply this machine's request rate by the number of
    /// accounts on it.
    #[test]
    fn a_first_beat_reads_every_login_then_one_secondary_per_window() {
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

        // Nothing cached: all three logins are read on this pass.
        let now = 1_800_000_000;
        collect_if_due(&dir, &settings, &report, now);
        let claimed = cache_keys(&dir);
        assert_eq!(claimed.len(), 3, "3 never-read logins, 3 probed: {claimed:?}");
        for id in [crate::agent_profiles::SYSTEM_PROFILE, &work.id, &home.id] {
            assert!(claimed.contains(&usage_cache::entry_key("codex", id)), "{id}");
        }

        // Every login now has an entry, and the probes just spent space the
        // next secondary out: a beat inside the window moves only the active
        // login's slot.
        let before: Vec<u64> = [work.id.as_str(), home.id.as_str()]
            .iter()
            .map(|id| {
                usage_cache::load(&dir)
                    .get(&usage_cache::entry_key("codex", id))
                    .unwrap()
                    .next_poll_at_secs
            })
            .collect();
        collect_if_due(&dir, &settings, &report, now + 1);
        let cache = usage_cache::load(&dir);
        let after: Vec<u64> = [work.id.as_str(), home.id.as_str()]
            .iter()
            .map(|id| {
                cache
                    .get(&usage_cache::entry_key("codex", id))
                    .unwrap()
                    .next_poll_at_secs
            })
            .collect();
        assert_eq!(before, after, "no secondary is re-probed inside the window");
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

    /// EXP-862 — "Remove account": the machine's copy of ONE login goes (its
    /// profile dir and its index row), the account itself is untouched, and
    /// three things are refused: the ambient login, an id this machine does
    /// not have, and an account a live run here is still using.
    #[test]
    fn removing_an_account_deletes_only_this_machines_login() {
        let _lock = live_lock();
        live::reset();
        let dir = usage_dir("remove-profile");
        let settings = Settings {
            codex_path: "/usr/bin/true".to_string(),
            ..Settings::default()
        };
        let report = codex_named_report();
        let now = 1_800_000_000;
        let work = crate::agent_profiles::create(&dir, CodingAgent::Codex, "Work").unwrap();
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();
        let dir_on_disk = crate::agent_profiles::profile_dir(&dir, CodingAgent::Codex, &work.id)
            .expect("the profile has a config dir");
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("codex", &work.id),
            cached(vec![session_window(42)], now),
        );
        usage_cache::save(&dir, &cache);

        let remove = |profile: &str, live: &[String]| {
            remove_profile(&dir, &settings, &report, CodingAgent::Codex, profile, live, now)
        };

        // The ambient login is the agent CLI's own — never ours to delete,
        // and `codex logout` is never in this path.
        let refusal = remove(crate::agent_profiles::SYSTEM_PROFILE, &[]).unwrap_err();
        assert!(refusal.contains("machine's own"), "{refusal}");
        assert!(remove("", &[]).is_err(), "a blank account id is the ambient login");
        assert_eq!(
            remove("deadbeef", &[]).unwrap_err(),
            "No such codex account on this machine."
        );
        assert_eq!(
            remove(&work.id, &[work.id.clone()]).unwrap_err(),
            "A live run on this machine still uses that account. End it first."
        );
        // Nothing was touched by any of those refusals.
        assert!(dir_on_disk.is_dir());

        assert!(remove(&work.id, &["another".to_string()]).is_ok());
        assert!(
            crate::agent_profiles::get(&dir, CodingAgent::Codex, &work.id).is_none(),
            "the index row is gone"
        );
        assert!(!dir_on_disk.exists(), "the credentials went with it");
        assert_eq!(
            crate::agent_profiles::active_profile(&dir, CodingAgent::Codex),
            crate::agent_profiles::SYSTEM_PROFILE,
            "the default falls back to the ambient login"
        );
        assert!(
            usage_cache::load(&dir)
                .get(&usage_cache::entry_key("codex", &work.id))
                .is_none(),
            "its numbers and its identity go with it"
        );
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // EXP-852 — claude's keep-alive
    // -----------------------------------------------------------------

    /// claude installed, signed in and NAMED — the doctor row a machine with
    /// account profiles has (a row is what makes the pass look at profiles at
    /// all; [`DoctorReport::agent_accounts`] carries only the agents whose
    /// check named one).
    fn claude_named_report() -> DoctorReport {
        let mut report = claude_ready_report();
        report.claude.account = Some(AgentAccount {
            signed_in: true,
            checked_at: String::new(),
            ..AgentAccount::default()
        });
        report
    }

    /// A stand-in `claude` whose `auth status` answers as a signed-in
    /// claude.ai subscription on first-party Anthropic — what makes an account
    /// PROFILE usage-eligible, and therefore a login a pass looks at at all.
    #[cfg(unix)]
    fn claude_auth_stub(dir: &std::path::Path) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let program = dir.join("claude-stub.sh");
        std::fs::write(
            &program,
            "#!/bin/sh\necho '{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"apiProvider\":\"firstParty\",\"email\":\"dev@acme.test\",\"subscriptionType\":\"max\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program.to_string_lossy().to_string()
    }

    /// A machine with one NAMED claude account beside the ambient login, both
    /// holding numbers too fresh to be due — so the only thing a beat can
    /// still do is the keep-alive.
    ///
    /// The named login's token sits inside [`usage_cache::CLAUDE_REFRESH_MARGIN_SECS`];
    /// the ambient one's is a month out, deliberately: its config dir is this
    /// MACHINE's own `~/.claude`, and no test may go near it.
    ///
    /// EXP-881: the `+60 s` expiry is now load-bearing TWICE — it is inside
    /// the margin (so the scheduled keep-alive is due) and NOT yet expired (so
    /// the unconditional expired-token refresh stays out of these tests). Move
    /// it into the past and every "the keep-alive did not run" case below
    /// starts refusing.
    #[cfg(unix)]
    fn claude_keep_alive_fixture(
        tag: &str,
        now: u64,
    ) -> (
        std::path::PathBuf,
        Settings,
        crate::agent_profiles::AgentProfile,
    ) {
        let dir = usage_dir(tag);
        let settings = Settings {
            claude_path: claude_auth_stub(&dir),
            ..Settings::default()
        };
        let work = crate::agent_profiles::create(&dir, CodingAgent::Claude, "Work").unwrap();
        // A store the step could READ but never spend: an access token and no
        // refresh token at all. It is what keeps these tests honest — a gate
        // that wrongly fired would answer `NoRefreshToken` and stamp
        // `refresh_backoff_until_secs`, which `assert_never_kept_alive`
        // refuses — and it does it without a request ever leaving the machine.
        std::fs::write(
            crate::agent_profiles::profile_dir(&dir, CodingAgent::Claude, &work.id)
                .expect("the profile has a config dir")
                .join(".credentials.json"),
            format!(
                r#"{{"claudeAiOauth":{{"accessToken":"at-1","expiresAt":{}}}}}"#,
                (now as i64 + 60) * 1000
            ),
        )
        .unwrap();
        let mut cache = usage_cache::load(&dir);
        cache.insert(
            usage_cache::entry_key("claude", crate::agent_profiles::SYSTEM_PROFILE),
            AgentCacheEntry {
                claude_expires_at_ms: Some((now as i64 + 30 * 86_400) * 1000),
                ..cached(vec![session_window(5)], now)
            },
        );
        cache.insert(
            usage_cache::entry_key("claude", &work.id),
            AgentCacheEntry {
                claude_expires_at_ms: Some((now as i64 + 60) * 1000),
                ..cached(vec![session_window(9)], now)
            },
        );
        usage_cache::save(&dir, &cache);
        (dir, settings, work)
    }

    /// Nothing was taken and nothing was recorded for `profile` — the
    /// assertion every "the keep-alive did not run" test makes.
    #[cfg(unix)]
    fn assert_never_kept_alive(dir: &std::path::Path, profile: &str) {
        let config_dir = crate::agent_profiles::profile_dir(dir, CodingAgent::Claude, profile)
            .expect("the profile has a config dir");
        assert!(
            !config_dir
                .join(crate::claude_oauth::REFRESH_LOCK_NAME)
                .exists(),
            "no refresh lock was ever taken under {}",
            config_dir.display()
        );
        let entry = usage_cache::load(dir)
            .get(&usage_cache::entry_key("claude", profile))
            .cloned()
            .expect("the login's cache entry");
        assert_eq!(entry.refresh_backoff_until_secs, None, "no refresh state");
        assert_eq!(entry.refreshed_at_secs, None, "no rotation was stamped");
        assert!(entry.dead_refresh_tokens.is_empty());
        assert_eq!(entry.credential_denied_until_secs, None);
    }

    /// EXP-881 — the MIRROR of [`assert_never_kept_alive`]: a refresh was
    /// attempted for `profile`. The fixture's store holds an access token and
    /// no refresh token, so the attempt cannot spend a grant and cannot leave
    /// the machine; what it does leave is `NoRefreshToken`'s backoff, which is
    /// the proof the gate fired.
    #[cfg(unix)]
    fn assert_refresh_attempted(dir: &std::path::Path, profile: &str) {
        let entry = usage_cache::load(dir)
            .get(&usage_cache::entry_key("claude", profile))
            .cloned()
            .expect("the login's cache entry");
        assert!(
            entry.refresh_backoff_until_secs.is_some(),
            "the keep-alive step ran and recorded its outcome"
        );
    }

    /// EXP-881 — put `profile`'s token in the PAST, in both places the
    /// collector reads it: the store the step opens and the cached expiry the
    /// gate keys on.
    #[cfg(unix)]
    fn expire_claude_login(dir: &std::path::Path, profile: &str, now: u64) {
        let expired = (now as i64 - 60) * 1000;
        std::fs::write(
            crate::agent_profiles::profile_dir(dir, CodingAgent::Claude, profile)
                .expect("the profile has a config dir")
                .join(".credentials.json"),
            format!(r#"{{"claudeAiOauth":{{"accessToken":"at-1","expiresAt":{expired}}}}}"#),
        )
        .unwrap();
        let mut cache = usage_cache::load(dir);
        let key = usage_cache::entry_key("claude", profile);
        let mut entry = cache.get(&key).cloned().expect("the login's cache entry");
        entry.claude_expires_at_ms = Some(expired);
        cache.insert(key, entry);
        usage_cache::save(dir, &cache);
    }

    /// EXP-852 — the keep-alive is OPT-IN: with `claudeKeepAlive` off (the
    /// default) a login whose token sits well inside the refresh margin is not
    /// touched at all, so a build that ships the setting off ships today's
    /// behaviour byte for byte.
    #[cfg(unix)]
    #[test]
    fn the_claude_keep_alive_is_off_by_default() {
        let _lock = live_lock();
        live::reset();
        let now = 1_800_000_000;
        let (dir, settings, work) = claude_keep_alive_fixture("keep-alive-off", now);
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(!settings.claude_keep_alive, "the default is OFF");
        assert!(
            usage_cache::claude_refresh_due(
                usage_cache::load(&dir)
                    .get(&usage_cache::entry_key("claude", &work.id))
                    .unwrap(),
                now
            ),
            "…and this login would otherwise be due"
        );

        collect_if_due(&dir, &settings, &claude_named_report(), now);

        assert_never_kept_alive(&dir, &work.id);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ONE refresh actor per login, machine-wide: the daemon beside the IDE
    /// holds this login's claim, so this pass leaves the credential alone
    /// rather than rotating a token the holder is already replacing.
    #[cfg(unix)]
    #[test]
    fn a_held_refresh_claim_parks_the_claude_keep_alive() {
        let _lock = live_lock();
        live::reset();
        let now = 1_800_000_000;
        let (dir, mut settings, work) = claude_keep_alive_fixture("keep-alive-claim", now);
        settings.claude_keep_alive = true;
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Claude, &work.id).unwrap();
        // The sibling process's claim, taken a moment ago (pid + when).
        std::fs::write(
            dir.join(format!("claude-{}.refresh.claim", work.id)),
            format!("4242 {now}"),
        )
        .unwrap();

        collect_if_due(&dir, &settings, &claude_named_report(), now);

        assert_never_kept_alive(&dir, &work.id);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A PARKED account — added and signed in here, but not the device default
    /// and never used by a recorded run — is deliberately left to expire.
    /// Keeping a credential warm is this machine asserting it needs the login,
    /// and this machine does not.
    #[cfg(unix)]
    #[test]
    fn a_parked_account_is_never_kept_alive() {
        let _lock = live_lock();
        live::reset();
        let now = 1_800_000_000;
        let (dir, mut settings, work) = claude_keep_alive_fixture("keep-alive-parked", now);
        settings.claude_keep_alive = true;
        // The ambient login stays the device default, so `work` is a login
        // this machine merely HOLDS…
        assert_eq!(
            crate::agent_profiles::active_profile(&dir, CodingAgent::Claude),
            crate::agent_profiles::SYSTEM_PROFILE
        );
        // …and no recorded run ever named it.
        assert!(crate::run_registry::all(&dir).is_empty());

        collect_if_due(&dir, &settings, &claude_named_report(), now);

        assert_never_kept_alive(&dir, &work.id);
        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-881 — an EXPIRED token is refreshed whatever `claudeKeepAlive`
    /// says. Keeping a live token warm is an opt-in convenience; rotating a
    /// DEAD one is the only way to tell a working account from a broken one,
    /// because every usage read on it 401s and a 401 is painted as
    /// `Needs re-login`.
    #[cfg(unix)]
    #[test]
    fn an_expired_login_refreshes_even_with_the_keep_alive_off() {
        let _lock = live_lock();
        live::reset();
        let now = 1_800_000_000;
        let (dir, settings, work) = claude_keep_alive_fixture("expired-keep-alive-off", now);
        crate::agent_profiles::set_active_profile(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(!settings.claude_keep_alive, "the setting is OFF");
        // Inside the margin but alive, the setting still decides…
        collect_if_due(&dir, &settings, &claude_named_report(), now);
        assert_never_kept_alive(&dir, &work.id);

        // …and past the expiry it does not.
        expire_claude_login(&dir, &work.id, now);
        collect_if_due(&dir, &settings, &claude_named_report(), now);
        assert_refresh_attempted(&dir, &work.id);

        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-881 — and the same for a PARKED login (added here, signed in here,
    /// never the default and never run here). EXP-852 leaves a parked
    /// account's live token to expire on purpose — but its numbers are shown
    /// on the Accounts surface like everybody's, and an expired token makes
    /// them read `Needs re-login`, so a dead one is still rotated.
    #[cfg(unix)]
    #[test]
    fn an_expired_parked_login_still_refreshes_for_its_numbers() {
        let _lock = live_lock();
        live::reset();
        let now = 1_800_000_000;
        let (dir, mut settings, work) = claude_keep_alive_fixture("expired-parked", now);
        settings.claude_keep_alive = true;
        // The ambient login is the device default and no run ever named
        // `work`: the EXP-852 gate refuses it …
        assert_eq!(
            crate::agent_profiles::active_profile(&dir, CodingAgent::Claude),
            SYSTEM_PROFILE
        );
        assert!(crate::run_registry::all(&dir).is_empty());
        collect_if_due(&dir, &settings, &claude_named_report(), now);
        assert_never_kept_alive(&dir, &work.id);

        // … until the token is actually dead.
        expire_claude_login(&dir, &work.id, now);
        collect_if_due(&dir, &settings, &claude_named_report(), now);
        assert_refresh_attempted(&dir, &work.id);

        live::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-881 — the point of rotating an expired token: the login stops
    /// reading `Needs re-login`. A rotation lands, the FAILED backoff it was
    /// sitting behind is released so the read happens on this beat, and the
    /// read's answer (a 200) is what sets the health — there is no new health
    /// code anywhere, just an account that can answer again.
    ///
    /// The usage GET has no URL seam ([`CLAUDE_USAGE_URL`] is a const), so the
    /// 200 is folded in through [`usage_cache::apply_outcome`] exactly as
    /// `probe_agent` would; the rotation itself is the real machinery against
    /// a canned token endpoint.
    #[test]
    fn an_expired_login_that_rotates_stops_reading_needs_relogin() {
        let dir = usage_dir("expired-rotates");
        let now = 1_800_000_000;
        let expired = (now as i64 - 60) * 1000;
        std::fs::write(
            dir.join(".credentials.json"),
            format!(
                r#"{{"claudeAiOauth":{{"accessToken":"at-old","refreshToken":"rt-1","expiresAt":{expired}}}}}"#
            ),
        )
        .unwrap();
        let (base, _requests) = crate::test_support::canned_server_recording(vec![(
            200,
            r#"{"access_token":"at-new","refresh_token":"rt-2","expires_in":28800}"#.to_string(),
        )]);

        // Where the login stood: a 401 named it broken and dimmed its numbers.
        let mut entry = AgentCacheEntry::default();
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![session_window(12)]),
            now - 10_000,
            "EARLIER",
        );
        usage_cache::apply_outcome(&mut entry, PollOutcome::Unauthorized, None, now - 1, "EARLIER");
        assert_eq!(
            entry.health.as_deref(),
            Some(crate::agent_accounts::Health::NeedsRelogin.as_str())
        );
        assert!(entry.usage.as_ref().unwrap().stale, "and the bar is dimmed");
        assert!(!usage_cache::poll_due(&entry, now), "behind the failure's wall");
        assert!(usage_cache::claude_token_expired(&entry, now) || entry.claude_expires_at_ms.is_none());

        // The rotation, through the real step against the canned endpoint.
        let token = claude_keep_alive_step_at(&base, Some(&dir), &mut entry, now);
        assert_eq!(token.as_deref(), Some("at-new"));
        assert!(!usage_cache::claude_token_expired(&entry, now), "a live token now");
        usage_cache::note_token_rotated(&mut entry, now);
        assert!(
            entry.next_poll_at_secs <= now,
            "the wall the FAILURE put up is down"
        );
        // What is left is only the machine-wide shared TTL, which every read
        // waits out — the login is no longer serving out a five-minute
        // failure backoff for a token that has since been replaced.
        assert!(usage_cache::poll_due(&entry, now + usage_cache::SHARED_TTL_SECS));

        // And the read the new token buys answers — which is the health.
        usage_cache::apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![session_window(12)]),
            now,
            "NOW",
        );
        assert_eq!(
            entry.health.as_deref(),
            Some(crate::agent_accounts::Health::Ok.as_str()),
            "the account was never broken, its token was"
        );
        assert!(!entry.usage.as_ref().unwrap().stale);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-852 — the step itself, through the real `claude_oauth` machinery
    /// against a local token endpoint: a token inside the margin is rotated
    /// under the CLI's own locks, exactly ONE grant is spent, the rotated pair
    /// lands in the store it came from, and the access token comes back for
    /// the usage GET to ride (so the probe skips a second store read).
    #[test]
    fn a_used_profile_refreshes_through_the_step() {
        let dir = usage_dir("keep-alive-step");
        let now = 1_700_000_000u64;
        seed_claude_credentials(&dir, (now as i64 + 60) * 1000);
        let (base, requests) = crate::test_support::canned_server_recording(vec![(
            200,
            r#"{"access_token":"at-2","refresh_token":"rt-2","expires_in":3600}"#.to_string(),
        )]);

        let mut entry = AgentCacheEntry::default();
        let token = claude_keep_alive_step_at(&base, Some(&dir), &mut entry, now);

        assert_eq!(
            token.as_deref(),
            Some("at-2"),
            "the probe rides the new token"
        );
        assert_eq!(requests.lock().unwrap().len(), 1, "exactly one POST");
        assert_eq!(
            entry.claude_expires_at_ms,
            Some(now as i64 * 1000 + 3_600_000),
            "the gate now schedules off the NEW expiry"
        );
        assert_eq!(entry.refreshed_at_secs, Some(now));
        assert_eq!(entry.refresh_backoff_until_secs, None);
        assert_eq!(
            entry.health, None,
            "a rotation is not the probe's verdict (EXP-849)"
        );
        assert!(
            !usage_cache::claude_refresh_due(&entry, now),
            "and it is done"
        );

        let stored = stored_claude_credentials(&dir);
        assert_eq!(stored["claudeAiOauth"]["accessToken"], "at-2");
        assert_eq!(stored["claudeAiOauth"]["refreshToken"], "rt-2");
        assert_eq!(
            stored["claudeAiOauth"]["subscriptionType"], "max",
            "the keys we do not understand survive"
        );
        assert!(
            !dir.join(crate::claude_oauth::REFRESH_LOCK_NAME).exists(),
            "the lock is released"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dead grant is the ONE failure that is the account's own answer: the
    /// login is flagged `needs_relogin`, the marker is remembered so a restart
    /// never re-spends it, and the backoff keeps the next beats off the store.
    #[test]
    fn an_invalid_grant_marks_the_login_needs_relogin() {
        let dir = usage_dir("keep-alive-dead");
        let now = 1_700_000_000u64;
        seed_claude_credentials(&dir, (now as i64 + 60) * 1000);
        let (base, requests) = crate::test_support::canned_server_recording(vec![(
            400,
            r#"{"error":"invalid_grant"}"#.to_string(),
        )]);

        let mut entry = AgentCacheEntry::default();
        assert_eq!(
            claude_keep_alive_step_at(&base, Some(&dir), &mut entry, now),
            None
        );

        assert_eq!(
            entry.health.as_deref(),
            Some(crate::agent_accounts::Health::NeedsRelogin.as_str())
        );
        assert_eq!(
            entry.dead_refresh_tokens,
            vec![crate::claude_oauth::dead_marker("rt-1")]
        );
        assert_eq!(
            entry.refresh_backoff_until_secs,
            Some(now + usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS),
            "the dead marker alone would still cost a store read every beat"
        );
        assert_eq!(requests.lock().unwrap().len(), 1);
        // The store is untouched: a dead grant never rewrites a credential.
        assert_eq!(
            stored_claude_credentials(&dir)["claudeAiOauth"]["accessToken"],
            "at-1"
        );

        // The next beat recognises the grant before it reaches the endpoint.
        assert_eq!(
            claude_keep_alive_step_at(&base, Some(&dir), &mut entry, now + 1),
            None
        );
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "a dead grant is never POSTed twice"
        );
        assert_eq!(entry.dead_refresh_tokens.len(), 1, "moved, not duplicated");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file-backed `.credentials.json` under `config_dir` — the store shape
    /// an account profile keeps beside its config.
    /// A document with no `expiresAt` cannot be scheduled off its expiry and
    /// is never refreshed on a guess, so the keep-alive backs it off for an
    /// hour instead of re-reading the store (on macOS: the keychain) on every
    /// 30 s beat.
    #[test]
    fn a_login_with_no_expiry_backs_off_an_hour_instead_of_polling_the_store() {
        let dir = usage_dir("keep-alive-no-expiry");
        let now = 1_700_000_000u64;
        std::fs::write(
            dir.join(".credentials.json"),
            serde_json::json!({
                "claudeAiOauth": { "accessToken": "at-1", "refreshToken": "rt-1" }
            })
            .to_string(),
        )
        .unwrap();
        let (base, requests) = crate::test_support::canned_server_recording(vec![]);

        let mut entry = AgentCacheEntry::default();
        assert!(usage_cache::claude_refresh_due(&entry, now), "never read: look once");
        let token = claude_keep_alive_step_at(&base, Some(&dir), &mut entry, now);

        assert_eq!(token.as_deref(), Some("at-1"), "the stored token serves");
        assert!(requests.lock().unwrap().is_empty(), "no POST on a guess");
        assert_eq!(entry.claude_expires_at_ms, None, "the read saw no expiry");
        assert_eq!(
            entry.refresh_backoff_until_secs,
            Some(now + usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS)
        );
        assert!(!usage_cache::claude_refresh_due(&entry, now + 30), "the next beat skips the store");
        assert!(usage_cache::claude_refresh_due(
            &entry,
            now + usage_cache::REFRESH_UNSUPPORTED_BACKOFF_SECS
        ));
        assert_eq!(entry.health, None, "not the account's answer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seed_claude_credentials(dir: &std::path::Path, expires_at_ms: i64) {
        std::fs::write(
            dir.join(".credentials.json"),
            serde_json::json!({
                "claudeAiOauth": {
                    "accessToken": "at-1",
                    "refreshToken": "rt-1",
                    "expiresAt": expires_at_ms,
                    "subscriptionType": "max",
                }
            })
            .to_string(),
        )
        .unwrap();
    }

    fn stored_claude_credentials(dir: &std::path::Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(".credentials.json")).unwrap())
            .unwrap()
    }
}
