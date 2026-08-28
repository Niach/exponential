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
//! * **pi** — pi has no usage surface, but when its default provider is an
//!   Anthropic OAuth credential the same endpoint answers for it.
//!
//! Poll policy lives in [`crate::usage_cache`]; this module is the parsing
//! and the orchestration. Everything is BLOCKING — callers run
//! [`collect_if_due`] off the UI/main thread (the desktop's device-sync beat,
//! the daemon's device worker).

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::CodingAgent;
use crate::agent_accounts::{iso_from_unix_secs, now_iso, AgentAccounts};
use crate::doctor::{DoctorReport, MIN_CLAUDE_VERSION};
use crate::settings::Settings;
use crate::usage_cache::{self, AgentCacheEntry, PollOutcome};

/// Hard cap on the windows one agent may report — the clients render a list,
/// the server clamps to the same number, and a runaway answer must never
/// become an unbounded jsonb column.
pub const MAX_WINDOWS: usize = 10;

/// The Anthropic OAuth usage endpoint (claude + pi's Anthropic provider).
pub const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Whole-request budget for the usage GET.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

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
/// `five_hour`/`seven_day` objects. Entries flagged `is_active: false` are
/// dropped — an inactive window has nothing to render — and an enabled
/// `extra_usage` becomes the `credits` window.
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
    if entry.get("is_active").and_then(Value::as_bool) == Some(false) {
        return None;
    }
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
        percent: read_percent(entry)?,
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

/// pi's Anthropic OAuth credential, when that is what it would run on — the
/// same endpoint answers for it. Returns `None` for an API-key provider, a
/// non-Anthropic default, and for an EXPIRED token (pi refreshes its own;
/// we never do).
pub fn pi_anthropic_oauth(auth_json: Option<&str>, now_ms: i64) -> Option<ClaudeOauthCredential> {
    let value: Value = serde_json::from_str(auth_json?).ok()?;
    let entry = value.get("anthropic")?;
    let kind = entry.get("type").and_then(Value::as_str).unwrap_or_default();
    if !kind.eq_ignore_ascii_case("oauth") {
        return None;
    }
    let credential = parse_claude_credentials(&entry.to_string())?;
    (!credential.expired(now_ms)).then_some(credential)
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
    #[cfg(target_os = "macos")]
    {
        let mut cmd = terminal::process::background_command("/usr/bin/security");
        cmd.args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ]);
        match crate::doctor::output_with_timeout(cmd, crate::doctor::PROBE_TIMEOUT) {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout);
                if let Some(credential) = parse_claude_credentials(raw.trim()) {
                    return CredentialRead::Found(credential);
                }
            }
            Ok(output) => {
                // 44 = "the item cannot be found" — a file-based install.
                // Anything else is a refusal (ACL denial, locked keychain).
                if output.status.code() != Some(44) {
                    return CredentialRead::Denied;
                }
            }
            // A timeout is the ACL prompt nobody is there to answer.
            Err(_) => return CredentialRead::Denied,
        }
    }
    let Some(path) = claude_credentials_path() else {
        return CredentialRead::Missing;
    };
    if !path.exists() {
        return CredentialRead::Missing;
    }
    match std::fs::read_to_string(&path) {
        Ok(raw) => match parse_claude_credentials(&raw) {
            Some(credential) => CredentialRead::Found(credential),
            None => CredentialRead::Missing,
        },
        Err(_) => CredentialRead::Denied,
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
pub fn collect_if_due(
    data_dir: &Path,
    settings: &Settings,
    report: &DoctorReport,
    now: u64,
) -> AgentStatusPayload {
    let stamp = iso_from_unix_secs(now as i64).unwrap_or_else(now_iso);
    let mut accounts = report.agent_accounts(&stamp);
    let mut usage = AgentUsageMap::new();
    let mut cache = usage_cache::load(data_dir);
    let mut changed = false;

    for agent in CodingAgent::ALL {
        let id = agent.id().to_string();
        let check = report.check_for(agent);
        // Installed = a version resolved. A signed-OUT agent has an account
        // row (`signedIn: false`) but nothing to fetch.
        if check.version.is_none() || check.signed_out() {
            continue;
        }
        // An API-key / Bedrock / Vertex claude has no subscription windows:
        // the account row still ships, but a stale OAuth item left in the
        // keychain must never be polled on its behalf.
        if agent == CodingAgent::Claude && !check.usage_eligible {
            continue;
        }
        let mut entry = cache.get(&id).cloned().unwrap_or_default();
        if usage_cache::poll_due(&entry, now) {
            changed = true;
            // Claim the slot BEFORE the (slow) fetch and persist it, so the
            // sibling process sharing this token (IDE vs daemon) sees the
            // poll as taken instead of spending a second request.
            entry.next_poll_at_secs = now + usage_cache::MIN_POLL_SECS;
            cache.insert(id.clone(), entry.clone());
            usage_cache::save(data_dir, &cache);
            let probe = probe_agent(agent, settings, check.version.as_deref(), &mut entry, now);
            if let Some(account) = probe.account {
                // Persist the identity: the not-due beats in between re-use it
                // instead of dropping back to the doctor's presence-only row.
                entry.account = Some(account.clone());
                accounts.insert(id.clone(), account);
            }
            usage_cache::apply_outcome(&mut entry, probe.outcome, probe.windows, now, &stamp);
            cache.insert(id.clone(), entry.clone());
        } else if let Some(account) = &entry.account {
            // Not due: the cached identity still enriches what the doctor's
            // presence-only probe could not name (codex's email/plan).
            let mut account = account.clone();
            account.checked_at = stamp.clone();
            accounts
                .entry(id.clone())
                .and_modify(|existing| {
                    if existing.email.is_none() && existing.plan.is_none() && account.signed_in {
                        *existing = account.clone();
                    }
                })
                .or_insert(account);
        }
        if let Some(snapshot) = &entry.usage {
            usage.insert(id, snapshot.clone());
        }
    }

    if changed {
        usage_cache::save(data_dir, &cache);
    }
    AgentStatusPayload { accounts, usage }
}

/// One agent's fetch result, before the cache folds it in.
struct AgentProbe {
    outcome: PollOutcome,
    windows: Option<Vec<UsageWindow>>,
    account: Option<crate::agent_accounts::AgentAccount>,
}

fn probe_agent(
    agent: CodingAgent,
    settings: &Settings,
    version: Option<&str>,
    entry: &mut AgentCacheEntry,
    now: u64,
) -> AgentProbe {
    match agent {
        CodingAgent::Claude => {
            let user_agent = claude_user_agent(version);
            match read_claude_credential() {
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
        CodingAgent::Pi => {
            let state = crate::doctor::read_pi_credentials();
            match pi_anthropic_oauth(state.auth_json.as_deref(), now as i64 * 1000) {
                Some(credential) => {
                    fetch_and_parse(&credential.access_token, &claude_user_agent(None))
                }
                None => AgentProbe {
                    outcome: PollOutcome::Failed,
                    windows: None,
                    account: None,
                },
            }
        }
        CodingAgent::Codex => {
            let program = settings.resolved_path_for(agent);
            match crate::codex_app_server::probe(
                &program,
                &terminal::pty::login_path(),
                crate::codex_app_server::PROBE_TIMEOUT,
            ) {
                Ok(probe) => {
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

    /// The modern `limits[]` body: four rendered windows (session, week, a
    /// per-model one, and the enabled credits pool), with the inactive
    /// entry dropped.
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
                ("credits", "Credits", 7),
            ]
        );
        assert_eq!(windows[0].resets_at.as_deref(), Some("2026-08-28T14:00:00.000Z"));
        assert_eq!(windows[2].resets_at, None);
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
    fn pi_anthropic_oauth_only_answers_for_a_live_oauth_provider() {
        let auth = r#"{"anthropic":{"type":"oauth","access":"tok","expires":1756000000000}}"#;
        let credential = pi_anthropic_oauth(Some(auth), 1_755_000_000_000).unwrap();
        assert_eq!(credential.access_token, "tok");
        // Expired → no fetch (we never refresh someone else's credential).
        assert_eq!(pi_anthropic_oauth(Some(auth), 1_757_000_000_000), None);
        // An API-key provider has no usage endpoint.
        assert_eq!(
            pi_anthropic_oauth(Some(r#"{"anthropic":{"type":"api","key":"sk"}}"#), 0),
            None
        );
        // A different default provider entirely.
        assert_eq!(
            pi_anthropic_oauth(Some(r#"{"openai":{"type":"oauth","access":"tok"}}"#), 0),
            None
        );
        assert_eq!(pi_anthropic_oauth(None, 0), None);
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
        };
        let missing = |tool| ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some("not found".to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
        };
        let report = DoctorReport {
            claude: signed_out,
            codex: missing(Tool::Codex),
            pi: missing(Tool::Pi),
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
}
