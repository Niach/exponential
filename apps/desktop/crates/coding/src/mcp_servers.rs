//! EXP-792: resolve a launch's team MCP server picks into the wire the agent
//! adapters render ([`crate::argv::McpServerWire`]) plus the spawn-env pairs
//! carrying the device-held secrets — and everything else the device side
//! does with those secrets: per-server readiness for the heartbeat, the
//! refresh sweep, the `mcp_oauth_start` / `mcp_oauth_code` command bodies
//! both hosts run, and the device-initiated sign-in the CLI and the desktop
//! settings pane share.
//!
//! The server holds NON-SECRET config only ([`api::mcp_servers`]); every
//! credential is a 0600 file in [`TokenStore`] under the signed-in account.
//! Nothing here logs a value, and every value that reaches a child process
//! does so through its environment, never argv or a config file
//! ([`ResolvedMcp::env`] → the spawn env + the steer redactor).
//!
//! **Mid-run expiry is ACCEPTED and SURFACED, never brokered (EXP-808 —
//! decided, do not reopen).** An OAuth token is resolved ONCE, at spawn, and
//! handed to the agent as an env value; there is no broker sitting between
//! the agent and its MCP server, so nothing can rotate that value while the
//! run is alive. Spawn-time refresh ([`fresh_token`]) and the heartbeat sweep
//! ([`refresh_expiring`], both on [`REFRESH_MARGIN_SECS`]) make a mid-run
//! death rare; when it happens the run loses that ONE server's tools and
//! nothing else. The answer is copy, not machinery: a launch whose token
//! cannot outlive a plausible run warns ([`RUN_HORIZON_SECS`] →
//! [`ResolvedMcp::warnings`]), and an expired sign-in reads as
//! [`sign_in_expired`], which names the server and the fix. No broker
//! process, no per-agent refresh hook.

use std::path::Path;
use std::time::{Duration, Instant};

use api::mcp_servers::{list_for_device, McpReadinessReport, McpServerConfig};
use api::token_store::{SecretKind, TokenStore};
use api::trpc::TrpcClient;
use serde_json::Value;

use crate::argv::{McpServerWire, McpWireTransport};
use crate::mcp_oauth::{self, Loopback, McpOauthError, PendingFlow, StartContext, TokenSet};

/// The per-server env var prefix: `EXP_MCP_TOKEN_<n>` carries an OAuth
/// bearer token, `EXP_MCP_ENV_<n>_<NAME>` a typed header/env value, where
/// `<n>` is the server's 1-based position in the launch's pick.
pub const MCP_SERVER_ENV_PREFIX: &str = "EXP_MCP";

/// An OAuth access token this close to expiry is refreshed before use (the
/// resolver) and by the heartbeat sweep.
pub const REFRESH_MARGIN_SECS: u64 = 10 * 60;

/// EXP-808 — a plausible coding run. A token that expires inside this window
/// will very likely die MID-RUN, where nothing can rotate it (see the module
/// note), so the resolver says so at launch instead of letting the agent
/// discover it as a 401 an hour in.
pub const RUN_HORIZON_SECS: u64 = 4 * 60 * 60;

/// `devices.heartbeat.mcpReadiness` / `mcpServers.reportReadiness` cap.
pub const MAX_READINESS_ENTRIES: usize = 64;

/// How long a fetched `listForDevice` copy serves the readiness sweep before
/// it is re-read (a command or a sign-in invalidates it at once).
pub const CONFIG_REFRESH: Duration = Duration::from_secs(5 * 60);

/// The two readiness sentences a missing credential produces — the same
/// text the launch blocker shows, so the web's "not ready" caption and the
/// desktop's blocker read alike.
pub const NOT_SIGNED_IN: &str = "not signed in on this machine";
pub const SIGN_IN_EXPIRED: &str = "sign-in expired on this machine";

/// The full expired-sign-in sentence (EXP-808). The bare
/// [`SIGN_IN_EXPIRED`] stem left the reader with a state and no move — and
/// this is text a run hits MID-flight, where "which of my servers went
/// quiet, and what do I do about it" is the whole question. So it NAMES the
/// server and the fix, in the readiness rows and in the launch blocker
/// alike. `detail` is the refresh failure, when there is one.
pub fn sign_in_expired(server: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!(
            "{SIGN_IN_EXPIRED} ({detail}). Sign in to {server} again, then resume the run."
        ),
        None => format!("{SIGN_IN_EXPIRED}. Sign in to {server} again, then resume the run."),
    }
}

/// The config key the launcher's own MCP entry owns (`mcpServers.exponential`
/// / `mcp_servers.exponential`); a team server may not fold to it.
pub const RESERVED_CONFIG_KEY: &str = "exponential";

/// A launch's resolved MCP servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedMcp {
    /// In pick order; `exponential` is NOT among them.
    pub servers: Vec<McpServerWire>,
    /// Spawn-env pairs (`EXP_MCP_TOKEN_1` → the token …). Every value here
    /// is a SECRET: it goes into the child env and the steer redactor, and
    /// nowhere else (never argv, never a config file, never a log).
    pub env: Vec<(String, String)>,
    /// EXP-808: launch-time notes, in pick order — today, the servers whose
    /// sign-in cannot outlive a plausible run ([`RUN_HORIZON_SECS`]). Never
    /// a secret and never a blocker: the run starts, and the user is told
    /// which server will go quiet and roughly when.
    pub warnings: Vec<String>,
}

impl ResolvedMcp {
    /// The secret values, for the steer redactor.
    pub fn secret_values(&self) -> Vec<String> {
        self.env.iter().map(|(_, v)| v.clone()).collect()
    }
}

/// Why a pick cannot launch here — rendered as a launch blocker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpBlocker {
    /// The server's display name (or id when the config is unknown).
    pub server: String,
    /// Human sentence: "not signed in on this machine", "no value for
    /// header X-Api-Key on this machine", "unknown server".
    pub reason: String,
}

impl std::fmt::Display for McpBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP server {}: {}", self.server, self.reason)
    }
}

// ---------------------------------------------------------------------------
// Env var names
// ---------------------------------------------------------------------------

/// `EXP_MCP_TOKEN_<n>` — the n-th picked server's OAuth bearer.
pub fn token_env_name(position: usize) -> String {
    format!("{MCP_SERVER_ENV_PREFIX}_TOKEN_{position}")
}

/// `EXP_MCP_ENV_<n>_<NAME>` — a typed http header value; `NAME` is the
/// header name upper-cased with everything non-alphanumeric folded to `_`.
pub fn value_env_name(position: usize, name: &str) -> String {
    format!("{MCP_SERVER_ENV_PREFIX}_ENV_{position}_{}", env_suffix(name))
}

/// `X-Api-Key` → `X_API_KEY`.
pub fn env_suffix(name: &str) -> String {
    let folded: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if folded.is_empty() {
        "VALUE".to_string()
    } else {
        folded
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The ONE declared secret position of an `auth: secret` server (the
/// server validates exactly one; a thinner row falls back to the first).
fn secret_name(config: &McpServerConfig) -> Option<&str> {
    config.secret_names().first().map(String::as_str)
}

fn missing_value(config: &McpServerConfig) -> String {
    match secret_name(config) {
        Some(name) if config.is_http() => format!("no value for header {name} on this machine"),
        Some(name) => format!("no value for {name} on this machine"),
        None => "no secret position declared for this server".to_string(),
    }
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

/// Resolve `ids` (the launch's `mcp_server_ids`) for this machine.
///
/// `data_dir` + `account_id` locate the secret store; `trpc` fetches the
/// non-secret config (`mcpServers.listForDevice`). Returns the wire +
/// env, or the FIRST blocker.
pub fn resolve(
    data_dir: &Path,
    account_id: &str,
    trpc: &TrpcClient,
    ids: &[String],
) -> Result<ResolvedMcp, McpBlocker> {
    if ids.is_empty() {
        return Ok(ResolvedMcp::default());
    }
    let configs = list_for_device(trpc).map_err(|err| McpBlocker {
        server: ids[0].clone(),
        reason: format!("could not load the team's MCP servers ({})", err.user_message()),
    })?;
    resolve_with(data_dir, account_id, &configs, ids, now_secs())
}

/// [`resolve`] against an already-fetched config list (`now` = unix secs).
pub fn resolve_with(
    data_dir: &Path,
    account_id: &str,
    configs: &[McpServerConfig],
    ids: &[String],
    now: u64,
) -> Result<ResolvedMcp, McpBlocker> {
    let store = TokenStore::new(data_dir.to_path_buf());
    let mut resolved = ResolvedMcp::default();
    for (index, id) in ids.iter().enumerate() {
        let position = index + 1;
        let config = configs
            .iter()
            .find(|config| &config.id == id)
            .ok_or_else(|| McpBlocker {
                server: id.clone(),
                reason: "unknown server (removed?)".to_string(),
            })?;
        let blocker = |reason: String| McpBlocker {
            server: config.name.clone(),
            reason,
        };
        // `exponential` is the launcher's own entry in every rendered
        // config; a team server folding to that key would be dropped
        // silently by the renderers, so refuse it up front.
        if McpServerWire::config_key(&config.name) == RESERVED_CONFIG_KEY {
            return Err(blocker("name is reserved".to_string()));
        }
        let transport = if config.is_http() {
            McpWireTransport::Http {
                url: config.url.clone().unwrap_or_default(),
            }
        } else {
            McpWireTransport::Stdio {
                command: config.command.clone().unwrap_or_default(),
                args: config.args.clone(),
            }
        };
        let mut wire = McpServerWire {
            id: config.id.clone(),
            name: McpServerWire::config_key(&config.name),
            transport,
            headers: Vec::new(),
            token_env: None,
            env: Vec::new(),
        };
        match config.auth.as_str() {
            "oauth" => {
                let set = TokenSet::load(&store, account_id, &config.id)
                    .ok_or_else(|| blocker(NOT_SIGNED_IN.to_string()))?;
                let set = fresh_token(&store, account_id, config, set, now).map_err(blocker)?;
                // EXP-808: the expiry is KNOWN here and the value is about to
                // be frozen into a child's env, so this is the last moment
                // anyone can be told it will not last the run.
                if let Some(warning) = expiry_warning(&config.name, &set, now) {
                    log::warn!("{warning}");
                    resolved.warnings.push(warning);
                }
                let var = token_env_name(position);
                wire.headers
                    .push(("Authorization".to_string(), format!("Bearer ${{{var}}}")));
                wire.token_env = Some(var.clone());
                resolved.env.push((var, set.access_token));
            }
            "secret" => {
                let name = secret_name(config)
                    .ok_or_else(|| blocker(missing_value(config)))?
                    .to_string();
                let value = store
                    .get(
                        account_id,
                        SecretKind::McpValue {
                            server_id: config.id.clone(),
                            name: name.clone(),
                        },
                    )
                    .ok_or_else(|| blocker(missing_value(config)))?;
                if config.is_http() {
                    let var = value_env_name(position, &name);
                    wire.headers.push((name, format!("${{{var}}}")));
                    resolved.env.push((var, value));
                } else {
                    // A stdio server's env NAME is the var itself: the
                    // launcher sets `<NAME>=value` and the config says
                    // `${NAME}` (codex forwards named vars verbatim).
                    wire.env.push((name.clone(), format!("${{{name}}}")));
                    resolved.env.push((name, value));
                }
            }
            _ => {}
        }
        resolved.servers.push(wire);
    }
    Ok(resolved)
}

/// The token set to use NOW: refreshed (and persisted) when inside the
/// margin and refreshable; the old one when still valid; an error once
/// expired and unrefreshable.
fn fresh_token(
    store: &TokenStore,
    account_id: &str,
    config: &McpServerConfig,
    set: TokenSet,
    now: u64,
) -> Result<TokenSet, String> {
    if !set.expires_within(now, REFRESH_MARGIN_SECS) {
        return Ok(set);
    }
    let resource = config.url.clone().unwrap_or_default();
    match mcp_oauth::refresh(&set, &resource, now) {
        Ok(rotated) => {
            rotated
                .save(store, account_id, &config.id)
                .map_err(|e| e.0)?;
            Ok(rotated)
        }
        Err(error) if !set.is_expired(now) => {
            log::debug!("MCP token refresh for {} failed (still valid): {error}", config.name);
            Ok(set)
        }
        Err(error) => Err(sign_in_expired(&config.name, Some(&error.to_string()))),
    }
}

/// EXP-808 — the launch-time note for a token that will not outlive a
/// plausible run ([`RUN_HORIZON_SECS`]). `None` when it lasts longer than
/// that, and for a set whose provider named no expiry at all.
///
/// It says "resume the run" rather than "we will refresh it": the value is
/// already on its way into a child's environment, and refreshing the stored
/// copy afterwards does not reach that child (the module note).
fn expiry_warning(server: &str, set: &TokenSet, now: u64) -> Option<String> {
    let left = set.seconds_left(now)?;
    if left >= RUN_HORIZON_SECS {
        return None;
    }
    Some(format!(
        "MCP server {server}: this run's sign-in expires in {}. {server}'s tools stop working then; sign in again and resume the run to pick up a fresh token.",
        humanize_remaining(left)
    ))
}

/// `"6 min"` / `"2 h 05 min"` — short enough for a one-line launch note.
fn humanize_remaining(secs: u64) -> String {
    let minutes = secs / 60;
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {:02} min", minutes / 60, minutes % 60)
    }
}

// ---------------------------------------------------------------------------
// Readiness + the refresh sweep
// ---------------------------------------------------------------------------

/// This machine's readiness for every config, in config order, capped at
/// [`MAX_READINESS_ENTRIES`]. Pure local reads — no network.
pub fn readiness(
    data_dir: &Path,
    account_id: &str,
    configs: &[McpServerConfig],
    now: u64,
) -> Vec<McpReadinessReport> {
    let store = TokenStore::new(data_dir.to_path_buf());
    configs
        .iter()
        .take(MAX_READINESS_ENTRIES)
        .map(|config| {
            let mut entry = McpReadinessReport {
                server_id: config.id.clone(),
                ready: true,
                expires_at: None,
                error: None,
            };
            match config.auth.as_str() {
                "oauth" => match TokenSet::load(&store, account_id, &config.id) {
                    Some(set) if set.is_expired(now) => {
                        entry.ready = false;
                        entry.error = Some(sign_in_expired(&config.name, None));
                        entry.expires_at = set.expires_at_iso();
                    }
                    Some(set) => entry.expires_at = set.expires_at_iso(),
                    None => {
                        entry.ready = false;
                        entry.error = Some(NOT_SIGNED_IN.to_string());
                    }
                },
                "secret" => {
                    let present = secret_name(config).is_some_and(|name| {
                        store
                            .get(
                                account_id,
                                SecretKind::McpValue {
                                    server_id: config.id.clone(),
                                    name: name.to_string(),
                                },
                            )
                            .is_some()
                    });
                    if !present {
                        entry.ready = false;
                        entry.error = Some(missing_value(config));
                    }
                }
                _ => {}
            }
            if let Some(error) = &mut entry.error {
                // The server caps `error` at 500 chars.
                if error.len() > 500 {
                    error.truncate(500);
                }
            }
            entry
        })
        .collect()
}

/// A stable digest of a readiness list — the heartbeat's change detector
/// (like `agent_accounts::accounts_key`: identity, not a JSON compare).
pub fn readiness_key(entries: &[McpReadinessReport]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&McpReadinessReport> = entries.iter().collect();
    sorted.sort_by(|a, b| a.server_id.cmp(&b.server_id));
    let mut hasher = Sha256::new();
    for entry in sorted {
        hasher.update(entry.server_id.as_bytes());
        hasher.update([0, u8::from(entry.ready)]);
        hasher.update(entry.expires_at.as_deref().unwrap_or("").as_bytes());
        hasher.update([0]);
        hasher.update(entry.error.as_deref().unwrap_or("").as_bytes());
        hasher.update([1]);
    }
    format!("{:x}", hasher.finalize())
}

/// Refresh every OAuth set expiring within [`REFRESH_MARGIN_SECS`] that has
/// a refresh token; returns how many rotated. Failures are logged (without
/// the token) and left to the readiness row once the set actually expires.
pub fn refresh_expiring(
    data_dir: &Path,
    account_id: &str,
    configs: &[McpServerConfig],
    now: u64,
) -> usize {
    let store = TokenStore::new(data_dir.to_path_buf());
    let mut rotated = 0;
    for config in configs.iter().filter(|config| config.is_oauth()) {
        let Some(set) = TokenSet::load(&store, account_id, &config.id) else {
            continue;
        };
        if !set.expires_within(now, REFRESH_MARGIN_SECS) || set.refresh_token.is_none() {
            continue;
        }
        match mcp_oauth::refresh(&set, config.url.as_deref().unwrap_or_default(), now) {
            Ok(next) => match next.save(&store, account_id, &config.id) {
                Ok(()) => rotated += 1,
                Err(error) => log::warn!("MCP token for {} refreshed but not stored: {error}", config.name),
            },
            Err(error) => log::info!("MCP token refresh for {} failed: {error}", config.name),
        }
    }
    rotated
}

/// One readiness sweep's result: the entries + their key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessSnapshot {
    pub key: String,
    pub entries: Vec<McpReadinessReport>,
}

/// The host-side sweep state: a cached `listForDevice` copy (re-read every
/// [`CONFIG_REFRESH`], or at once after [`McpReadinessState::invalidate`]),
/// so a 30 s beat costs local file reads, not a query.
#[derive(Default)]
pub struct McpReadinessState {
    configs: Vec<McpServerConfig>,
    fetched_at: Option<Instant>,
    force: bool,
}

impl McpReadinessState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The configs the last sweep worked from.
    pub fn configs(&self) -> &[McpServerConfig] {
        &self.configs
    }

    /// A command / sign-in changed what this machine holds (or what the
    /// server lists): re-read on the next sweep.
    pub fn invalidate(&mut self) {
        self.force = true;
    }

    fn fetch_due(&self) -> bool {
        self.force
            || self
                .fetched_at
                .is_none_or(|at| at.elapsed() >= CONFIG_REFRESH)
    }

    /// Refresh the config copy when due, run the token refresh sweep, and
    /// answer this machine's readiness. `None` when no config was ever
    /// fetched (offline at boot) — nothing to report yet.
    pub fn sweep(
        &mut self,
        data_dir: &Path,
        account_id: &str,
        trpc: &TrpcClient,
    ) -> Option<ReadinessSnapshot> {
        if self.fetch_due() {
            match list_for_device(trpc) {
                Ok(configs) => {
                    self.configs = configs;
                    self.fetched_at = Some(Instant::now());
                    self.force = false;
                }
                Err(error) => {
                    log::debug!("mcpServers.listForDevice failed: {error}");
                    // Keep serving the stale copy; retry on the next beat.
                    self.fetched_at.get_or_insert(Instant::now());
                    if self.configs.is_empty() {
                        return None;
                    }
                }
            }
        }
        let now = now_secs();
        refresh_expiring(data_dir, account_id, &self.configs, now);
        let entries = readiness(data_dir, account_id, &self.configs, now);
        Some(ReadinessSnapshot {
            key: readiness_key(&entries),
            entries,
        })
    }
}

/// Report this machine's readiness NOW (after a sign-in / typed value):
/// re-reads the config list and pushes every entry.
pub fn report_now(
    data_dir: &Path,
    account_id: &str,
    trpc: &TrpcClient,
    device_id: &str,
) -> Result<Vec<McpReadinessReport>, api::ApiError> {
    let configs = list_for_device(trpc)?;
    let entries = readiness(data_dir, account_id, &configs, now_secs());
    if !entries.is_empty() {
        api::mcp_servers::report_readiness(trpc, device_id, &entries)?;
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Device commands (both hosts)
// ---------------------------------------------------------------------------

/// What a host hands the command bodies: where the secrets live, who owns
/// them, the tRPC client (its base URL is the app base the CIMD document
/// lives at) and the steer device id readiness is reported under.
#[derive(Clone)]
pub struct HostContext<'a> {
    pub data_dir: &'a Path,
    pub account_id: &'a str,
    pub trpc: &'a TrpcClient,
    pub device_id: &'a str,
}

/// The literal `redirectUri` payload value that means "bind a loopback
/// listener on the device".
pub const LOOPBACK_REDIRECT: &str = "loopback";

/// An `mcp_oauth_start` that got as far as an authorize URL.
#[derive(Debug)]
pub enum OauthStart {
    /// The hosted callback relays the code as an `mcp_oauth_code` command;
    /// nothing else to do here.
    Hosted { authorize_url: String },
    /// The code lands on this listener: the host waits on a background
    /// thread ([`oauth_finish_loopback`]).
    Loopback {
        authorize_url: String,
        loopback: Loopback,
        pending: PendingFlow,
    },
}

impl OauthStart {
    pub fn authorize_url(&self) -> &str {
        match self {
            OauthStart::Hosted { authorize_url } | OauthStart::Loopback { authorize_url, .. } => {
                authorize_url
            }
        }
    }

    /// The command's EARLY completion message: `{"phase":"authorize","url"}`.
    pub fn message(&self) -> String {
        serde_json::json!({ "phase": "authorize", "url": self.authorize_url() }).to_string()
    }
}

/// The `mcp_oauth_code` / loopback completion message:
/// `{"phase":"done","expiresAt"?}`.
pub fn done_message(expires_at: Option<&str>) -> String {
    let mut doc = serde_json::json!({ "phase": "done" });
    if let Some(expires_at) = expires_at {
        doc["expiresAt"] = Value::String(expires_at.to_string());
    }
    doc.to_string()
}

fn payload_str<'a>(payload: &'a Value, key: &str) -> Result<&'a str, String> {
    payload[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Malformed command payload.".to_string())
}

fn config_for(trpc: &TrpcClient, server_id: &str) -> Result<McpServerConfig, String> {
    list_for_device(trpc)
        .map_err(|error| format!("could not load the team's MCP servers ({})", error.user_message()))?
        .into_iter()
        .find(|config| config.id == server_id)
        .ok_or_else(|| "unknown server (removed?)".to_string())
}

/// Run an `mcp_oauth_start` payload (`{serverId, state, redirectUri}`) up
/// to the authorize URL. The host completes the command with
/// [`OauthStart::message`] at once, then (loopback only) waits.
pub fn oauth_start(host: &HostContext<'_>, payload: &Value) -> Result<OauthStart, String> {
    let server_id = payload_str(payload, "serverId")?;
    let state = payload_str(payload, "state")?;
    let redirect = payload_str(payload, "redirectUri")?;
    let config = config_for(host.trpc, server_id)?;
    if !config.is_oauth() {
        return Err("this server does not use OAuth".to_string());
    }
    let url = config
        .url
        .clone()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "this server has no URL".to_string())?;
    let ctx = StartContext {
        data_dir: host.data_dir,
        account_id: host.account_id,
        app_base: host.trpc.base_url(),
    };
    let now = now_secs();
    if redirect == LOOPBACK_REDIRECT {
        let loopback = Loopback::bind().map_err(|e| e.0)?;
        let redirect_uri = loopback.redirect_uri();
        let (authorize_url, pending) =
            mcp_oauth::begin(&ctx, &config.id, &url, &config.scopes, &redirect_uri, state, now)
                .map_err(|e| e.0)?;
        Ok(OauthStart::Loopback {
            authorize_url,
            loopback,
            pending,
        })
    } else {
        let (authorize_url, _pending) =
            mcp_oauth::begin(&ctx, &config.id, &url, &config.scopes, redirect, state, now)
                .map_err(|e| e.0)?;
        Ok(OauthStart::Hosted { authorize_url })
    }
}

/// The loopback tail of [`oauth_start`]: wait for the browser, exchange,
/// store, tell the server (`finishOAuth`) and re-report readiness. Blocks
/// up to [`mcp_oauth::LOOPBACK_TIMEOUT`] — run it on its own thread.
/// Returns the token's ISO expiry.
pub fn oauth_finish_loopback(
    host: &HostContext<'_>,
    pending: PendingFlow,
    loopback: Loopback,
) -> Result<Option<String>, String> {
    let outcome = loopback
        .wait(mcp_oauth::LOOPBACK_TIMEOUT, &pending.state)
        .map_err(|e| e.0)
        .and_then(|params| {
            mcp_oauth::complete(host.data_dir, host.account_id, &pending, &params.code, now_secs())
                .map_err(|e| e.0)
        });
    match &outcome {
        Ok(set) => {
            let expires_at = set.expires_at_iso();
            if let Err(error) = api::mcp_servers::finish_oauth(
                host.trpc,
                &pending.state,
                true,
                expires_at.as_deref(),
                None,
            ) {
                log::warn!("mcpServers.finishOAuth failed (readiness still reports): {error}");
            }
            if let Err(error) = report_now(host.data_dir, host.account_id, host.trpc, host.device_id) {
                log::debug!("readiness report after sign-in failed: {error}");
            }
        }
        Err(reason) => {
            let store = TokenStore::new(host.data_dir.to_path_buf());
            pending.forget(&store, host.account_id);
            let _ = api::mcp_servers::finish_oauth(host.trpc, &pending.state, false, None, Some(reason));
        }
    }
    outcome.map(|set| set.expires_at_iso())
}

/// Run an `mcp_oauth_code` payload (`{serverId, state, code}`): exchange
/// with the pending flow's verifier, store, re-report readiness. Returns
/// the token's ISO expiry for [`done_message`].
pub fn oauth_code(host: &HostContext<'_>, payload: &Value) -> Result<Option<String>, String> {
    let server_id = payload_str(payload, "serverId")?;
    let state = payload_str(payload, "state")?;
    let code = payload_str(payload, "code")?;
    let store = TokenStore::new(host.data_dir.to_path_buf());
    let pending = PendingFlow::load(&store, host.account_id, server_id, state)
        .ok_or_else(|| "no sign-in is waiting for that code on this machine".to_string())?;
    let result = mcp_oauth::complete(host.data_dir, host.account_id, &pending, code, now_secs());
    match result {
        Ok(set) => {
            if let Err(error) = report_now(host.data_dir, host.account_id, host.trpc, host.device_id) {
                log::debug!("readiness report after sign-in failed: {error}");
            }
            Ok(set.expires_at_iso())
        }
        Err(error) => {
            pending.forget(&store, host.account_id);
            Err(error.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Device-initiated sign-in + typed values (CLI, desktop settings pane)
// ---------------------------------------------------------------------------

/// The paste fallback's redirect: an unroutable loopback port, so the
/// browser shows the redirect URL (with the code) in its address bar for
/// the person to copy. Loopback IPs are accepted on any port (RFC 8252).
pub const PASTE_REDIRECT_URI: &str = "http://127.0.0.1:1/callback";

/// A device-initiated sign-in in progress (no server flow: the device owns
/// state, verifier and listener).
#[derive(Debug)]
pub struct LocalLogin {
    pub authorize_url: String,
    pub pending: PendingFlow,
    /// `Some` for the loopback variant.
    pub loopback: Option<Loopback>,
}

/// Start a LOCAL sign-in to `config`: loopback listener (`paste == false`)
/// or the paste fallback. The caller shows/opens `authorize_url`, then
/// calls [`finish_local_login`] (loopback) or [`finish_pasted_login`].
pub fn begin_local_login(
    data_dir: &Path,
    account_id: &str,
    app_base: &str,
    config: &McpServerConfig,
    paste: bool,
) -> Result<LocalLogin, McpOauthError> {
    if !config.is_oauth() {
        return Err(McpOauthError("this server does not use OAuth".to_string()));
    }
    let url = config
        .url
        .clone()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| McpOauthError("this server has no URL".to_string()))?;
    let ctx = StartContext {
        data_dir,
        account_id,
        app_base,
    };
    let state = mcp_oauth::random_state();
    let loopback = if paste { None } else { Some(Loopback::bind()?) };
    let redirect_uri = loopback
        .as_ref()
        .map(Loopback::redirect_uri)
        .unwrap_or_else(|| PASTE_REDIRECT_URI.to_string());
    let (authorize_url, pending) = mcp_oauth::begin(
        &ctx,
        &config.id,
        &url,
        &config.scopes,
        &redirect_uri,
        &state,
        now_secs(),
    )?;
    Ok(LocalLogin {
        authorize_url,
        pending,
        loopback,
    })
}

/// Wait on the loopback listener, exchange and store. Blocks up to
/// [`mcp_oauth::LOOPBACK_TIMEOUT`].
pub fn finish_local_login(
    data_dir: &Path,
    account_id: &str,
    login: LocalLogin,
) -> Result<TokenSet, McpOauthError> {
    let loopback = login
        .loopback
        .ok_or_else(|| McpOauthError("this sign-in expects a pasted URL".to_string()))?;
    let store = TokenStore::new(data_dir.to_path_buf());
    let params = loopback
        .wait(mcp_oauth::LOOPBACK_TIMEOUT, &login.pending.state)
        .inspect_err(|_| login.pending.forget(&store, account_id))?;
    mcp_oauth::complete(data_dir, account_id, &login.pending, &params.code, now_secs())
        .inspect_err(|_| login.pending.forget(&store, account_id))
}

/// Finish a paste-fallback sign-in with the redirect URL the person copied.
pub fn finish_pasted_login(
    data_dir: &Path,
    account_id: &str,
    login: &LocalLogin,
    pasted_url: &str,
) -> Result<TokenSet, McpOauthError> {
    let store = TokenStore::new(data_dir.to_path_buf());
    let params = mcp_oauth::parse_redirect(pasted_url.trim())?;
    if params.state != login.pending.state {
        return Err(McpOauthError(
            "that URL belongs to a different sign-in; start over".to_string(),
        ));
    }
    mcp_oauth::complete(data_dir, account_id, &login.pending, &params.code, now_secs())
        .inspect_err(|_| login.pending.forget(&store, account_id))
}

/// Store the typed value for one declared name of an `auth: secret` server.
/// `name` must be one the server declares; the value never leaves the store.
pub fn set_secret(
    data_dir: &Path,
    account_id: &str,
    config: &McpServerConfig,
    name: &str,
    value: &str,
) -> Result<(), String> {
    if config.auth != "secret" {
        return Err("this server does not take a typed secret".to_string());
    }
    if !config.secret_names().iter().any(|declared| declared == name) {
        return Err(format!(
            "{name} is not a declared {} of this server (declared: {})",
            if config.is_http() { "header" } else { "env var" },
            config.secret_names().join(", ")
        ));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("the value is empty".to_string());
    }
    TokenStore::new(data_dir.to_path_buf())
        .set(
            account_id,
            SecretKind::McpValue {
                server_id: config.id.clone(),
                name: name.to_string(),
            },
            value,
        )
        .map_err(|error| error.to_string())
}

/// Drop every credential this machine holds for `server_id` (sign out /
/// forget). Idempotent.
pub fn forget_server(data_dir: &Path, account_id: &str, server_id: &str) {
    TokenStore::new(data_dir.to_path_buf()).delete_mcp_server(account_id, server_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_oauth::tests::{as_metadata, StubServer};
    use crate::test_support::{canned_server, temp_dir};
    use api::StaticToken;
    use std::sync::Arc;

    fn store_token(dir: &Path, server_id: &str, set: &TokenSet) {
        set.save(&TokenStore::new(dir.to_path_buf()), "acct", server_id)
            .unwrap();
    }

    fn token(access: &str, expires_at: Option<u64>, refresh: Option<&str>, endpoint: &str) -> TokenSet {
        TokenSet {
            access_token: access.into(),
            refresh_token: refresh.map(str::to_string),
            expires_at,
            token_type: "Bearer".into(),
            scope: None,
            token_endpoint: endpoint.into(),
            client_id: "cid".into(),
            issuer: "https://auth.example.com".into(),
        }
    }

    fn oauth_config(id: &str, name: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: name.into(),
            url: Some("https://mcp.example.com/mcp".into()),
            auth: "oauth".into(),
            scopes: vec!["read".into()],
            ..Default::default()
        }
    }

    fn secret_http(id: &str, name: &str, header: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: name.into(),
            url: Some("https://mcp.example.com/mcp".into()),
            auth: "secret".into(),
            header_names: vec![header.into()],
            ..Default::default()
        }
    }

    fn secret_stdio(id: &str, name: &str, env: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: name.into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "server".into()],
            auth: "secret".into(),
            env_names: vec![env.into()],
            ..Default::default()
        }
    }

    #[test]
    fn env_var_names_follow_the_contract() {
        assert_eq!(token_env_name(1), "EXP_MCP_TOKEN_1");
        assert_eq!(value_env_name(2, "X-Api-Key"), "EXP_MCP_ENV_2_X_API_KEY");
        assert_eq!(env_suffix("  "), "VALUE");
    }

    #[test]
    fn empty_pick_resolves_to_nothing_without_a_fetch() {
        let dir = temp_dir("resolve-empty");
        // An unreachable base proves nothing is fetched.
        let trpc = TrpcClient::new("http://127.0.0.1:1", Arc::new(StaticToken("t".into())));
        assert_eq!(resolve(&dir.0, "acct", &trpc, &[]).unwrap(), ResolvedMcp::default());
    }

    #[test]
    fn resolve_builds_wires_and_env_in_pick_order() {
        let dir = temp_dir("resolve-order");
        store_token(&dir.0, "s-oauth", &token("at-1", Some(10_000), None, "https://a/token"));
        let store = TokenStore::new(dir.0.clone());
        store
            .set(
                "acct",
                SecretKind::McpValue {
                    server_id: "s-http".into(),
                    name: "X-Api-Key".into(),
                },
                "key-1",
            )
            .unwrap();
        store
            .set(
                "acct",
                SecretKind::McpValue {
                    server_id: "s-stdio".into(),
                    name: "GITHUB_TOKEN".into(),
                },
                "ghp_x",
            )
            .unwrap();
        let none = McpServerConfig {
            id: "s-none".into(),
            name: "Docs".into(),
            url: Some("https://docs.example.com/mcp".into()),
            ..Default::default()
        };
        let configs = vec![
            oauth_config("s-oauth", "Linear"),
            secret_http("s-http", "Sentry Bridge", "X-Api-Key"),
            secret_stdio("s-stdio", "GitHub", "GITHUB_TOKEN"),
            none,
        ];
        let ids: Vec<String> = ["s-http", "s-oauth", "s-none", "s-stdio"]
            .iter()
            .map(|id| id.to_string())
            .collect();
        let resolved = resolve_with(&dir.0, "acct", &configs, &ids, 1_000).unwrap();
        assert_eq!(resolved.servers.len(), 4);
        let http = &resolved.servers[0];
        assert_eq!(http.name, "sentry_bridge");
        assert_eq!(
            http.headers,
            vec![("X-Api-Key".to_string(), "${EXP_MCP_ENV_1_X_API_KEY}".to_string())]
        );
        assert_eq!(http.token_env, None);
        let oauth = &resolved.servers[1];
        assert_eq!(oauth.name, "linear");
        assert_eq!(
            oauth.headers,
            vec![("Authorization".to_string(), "Bearer ${EXP_MCP_TOKEN_2}".to_string())]
        );
        assert_eq!(oauth.token_env.as_deref(), Some("EXP_MCP_TOKEN_2"));
        let docs = &resolved.servers[2];
        assert!(docs.headers.is_empty() && docs.env.is_empty() && docs.token_env.is_none());
        let stdio = &resolved.servers[3];
        assert_eq!(
            stdio.transport,
            McpWireTransport::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "server".into()]
            }
        );
        assert_eq!(
            stdio.env,
            vec![("GITHUB_TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())]
        );
        assert_eq!(
            resolved.env,
            vec![
                ("EXP_MCP_ENV_1_X_API_KEY".to_string(), "key-1".to_string()),
                ("EXP_MCP_TOKEN_2".to_string(), "at-1".to_string()),
                ("GITHUB_TOKEN".to_string(), "ghp_x".to_string()),
            ]
        );
        assert_eq!(resolved.secret_values(), vec!["key-1", "at-1", "ghp_x"]);
        // No secret ever lands in the wire.
        let wire = serde_json::to_string(&resolved.servers).unwrap();
        for secret in ["key-1", "at-1", "ghp_x"] {
            assert!(!wire.contains(secret), "{wire}");
        }
    }

    #[test]
    fn resolve_blocks_on_the_first_missing_secret_with_the_contract_text() {
        let dir = temp_dir("resolve-block");
        let configs = vec![
            oauth_config("s-oauth", "Linear"),
            secret_http("s-http", "Sentry", "X-Api-Key"),
            secret_stdio("s-stdio", "GitHub", "GITHUB_TOKEN"),
        ];
        let block = |ids: &[&str]| {
            let ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
            resolve_with(&dir.0, "acct", &configs, &ids, 0).unwrap_err()
        };
        assert_eq!(
            block(&["s-oauth"]),
            McpBlocker {
                server: "Linear".into(),
                reason: "not signed in on this machine".into()
            }
        );
        assert_eq!(
            block(&["s-http"]).reason,
            "no value for header X-Api-Key on this machine"
        );
        assert_eq!(block(&["s-stdio"]).reason, "no value for GITHUB_TOKEN on this machine");
        let unknown = block(&["s-gone"]);
        assert_eq!(unknown.server, "s-gone");
        assert_eq!(unknown.reason, "unknown server (removed?)");
        assert_eq!(unknown.to_string(), "MCP server s-gone: unknown server (removed?)");
    }

    /// A server whose name folds to the launcher's own `exponential` key
    /// is refused, whatever its auth — the renderers would drop it.
    #[test]
    fn resolve_refuses_the_reserved_exponential_key() {
        let dir = temp_dir("resolve-reserved");
        let configs = vec![McpServerConfig {
            id: "s".into(),
            name: "Exponential".into(),
            url: Some("https://mcp.example.com/mcp".into()),
            ..Default::default()
        }];
        let blocked = resolve_with(&dir.0, "acct", &configs, &["s".to_string()], 0).unwrap_err();
        assert_eq!(
            blocked,
            McpBlocker {
                server: "Exponential".into(),
                reason: "name is reserved".into()
            }
        );
    }

    #[test]
    fn resolve_refreshes_a_token_inside_the_margin_and_persists_it() {
        let dir = temp_dir("resolve-refresh");
        let stub = StubServer::new(vec![(
            "POST /token",
            200,
            r#"{"access_token":"at-2","expires_in":3600,"refresh_token":"rt-2"}"#.to_string(),
        )]);
        let endpoint = format!("{}/token", stub.base);
        store_token(&dir.0, "s", &token("at-1", Some(1_500), Some("rt-1"), &endpoint));
        let configs = vec![oauth_config("s", "Linear")];
        let resolved = resolve_with(&dir.0, "acct", &configs, &["s".to_string()], 1_000).unwrap();
        assert_eq!(resolved.env[0].1, "at-2");
        assert!(stub.last_body().starts_with("grant_type=refresh_token&refresh_token=rt-1"));
        let stored = TokenSet::load(&TokenStore::new(dir.0.clone()), "acct", "s").unwrap();
        assert_eq!(stored.access_token, "at-2");
        assert_eq!(stored.refresh_token.as_deref(), Some("rt-2"));
        assert_eq!(stored.expires_at, Some(4_600));
    }

    #[test]
    fn resolve_keeps_a_valid_token_when_refresh_fails_and_blocks_once_expired() {
        let dir = temp_dir("resolve-refresh-fail");
        let stub = StubServer::new(vec![(
            "POST /token",
            400,
            r#"{"error":"invalid_grant"}"#.to_string(),
        )]);
        let endpoint = format!("{}/token", stub.base);
        store_token(&dir.0, "s", &token("at-1", Some(1_500), Some("rt-1"), &endpoint));
        let configs = vec![oauth_config("s", "Linear")];
        let ids = ["s".to_string()];
        let resolved = resolve_with(&dir.0, "acct", &configs, &ids, 1_000).unwrap();
        assert_eq!(resolved.env[0].1, "at-1", "still valid: the old token serves");
        let blocked = resolve_with(&dir.0, "acct", &configs, &ids, 2_000).unwrap_err();
        assert!(blocked.reason.starts_with("sign-in expired on this machine"), "{blocked}");
        // EXP-808: and it says which server, and what to do about it.
        assert!(
            blocked.reason.ends_with("Sign in to Linear again, then resume the run."),
            "{blocked}"
        );
        assert!(!blocked.reason.contains("rt-1") && !blocked.reason.contains("at-1"));
    }

    /// EXP-808 — mid-run expiry is ACCEPTED, so a launch that cannot outlive
    /// its token says so instead of failing anonymously an hour in: the note
    /// names the server and the time left, it never blocks the run, and it
    /// never carries the token.
    #[test]
    fn a_token_that_cannot_outlive_the_run_warns_without_blocking_it() {
        let dir = temp_dir("resolve-expiry-warning");
        let configs = vec![oauth_config("s", "Sentry")];
        let ids = ["s".to_string()];
        let endpoint = "https://auth.example.com/token";

        // Comfortably past the horizon: nothing worth saying.
        store_token(
            &dir.0,
            "s",
            &token("at-long", Some(1_000 + RUN_HORIZON_SECS + 60), None, endpoint),
        );
        let resolved = resolve_with(&dir.0, "acct", &configs, &ids, 1_000).unwrap();
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        // Inside it, and unrefreshable — exactly the gap the decision
        // accepts. The run still starts.
        store_token(
            &dir.0,
            "s",
            &token("at-short", Some(1_000 + 90 * 60), None, endpoint),
        );
        let resolved = resolve_with(&dir.0, "acct", &configs, &ids, 1_000).unwrap();
        assert_eq!(resolved.env[0].1, "at-short", "a warning is not a blocker");
        let warning = &resolved.warnings[0];
        assert_eq!(resolved.warnings.len(), 1);
        assert!(warning.starts_with("MCP server Sentry:"), "{warning}");
        assert!(warning.contains("1 h 30 min"), "{warning}");
        assert!(warning.contains("resume the run"), "{warning}");
        assert!(!warning.contains("at-short"), "never a secret: {warning}");

        // A provider that named no expiry is not guessed at.
        store_token(&dir.0, "s", &token("at-forever", None, None, endpoint));
        let resolved = resolve_with(&dir.0, "acct", &configs, &ids, 1_000).unwrap();
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);

        assert_eq!(humanize_remaining(59), "0 min");
        assert_eq!(humanize_remaining(6 * 60), "6 min");
        assert_eq!(humanize_remaining(2 * 3_600 + 5 * 60), "2 h 05 min");
        assert_eq!(
            sign_in_expired("Notion", None),
            "sign-in expired on this machine. Sign in to Notion again, then resume the run."
        );
    }

    #[test]
    fn resolve_fetches_the_config_list_over_trpc() {
        let dir = temp_dir("resolve-trpc");
        let body = serde_json::json!({"result":{"data":[
            {"id":"s","teamId":"t","name":"Docs","transport":"http","url":"https://docs.example.com/mcp","auth":"none"}
        ]}})
        .to_string();
        let base = canned_server(vec![(200, body)]);
        let trpc = TrpcClient::new(&base, Arc::new(StaticToken("t".into())));
        let resolved = resolve(&dir.0, "acct", &trpc, &["s".to_string()]).unwrap();
        assert_eq!(resolved.servers[0].name, "docs");
        assert!(resolved.env.is_empty());
    }

    #[test]
    fn readiness_reports_every_config_and_keys_stably() {
        let dir = temp_dir("readiness");
        store_token(&dir.0, "s-ok", &token("at", Some(5_000), None, "https://a/token"));
        store_token(&dir.0, "s-old", &token("at", Some(10), Some("rt"), "https://a/token"));
        let configs = vec![
            oauth_config("s-ok", "Linear"),
            oauth_config("s-old", "Notion"),
            oauth_config("s-none", "Sentry"),
            secret_http("s-key", "Bridge", "X-Api-Key"),
            McpServerConfig {
                id: "s-open".into(),
                name: "Docs".into(),
                ..Default::default()
            },
        ];
        let entries = readiness(&dir.0, "acct", &configs, 1_000);
        assert_eq!(entries.len(), 5);
        assert!(entries[0].ready);
        assert_eq!(entries[0].expires_at.as_deref(), Some("1970-01-01T01:23:20.000Z"));
        assert!(!entries[1].ready);
        // EXP-808: the expired row NAMES its server and the move, so the
        // reader is not left holding a state with no next step.
        assert_eq!(
            entries[1].error.as_deref(),
            Some("sign-in expired on this machine. Sign in to Notion again, then resume the run.")
        );
        assert_eq!(entries[2].error.as_deref(), Some(NOT_SIGNED_IN));
        assert_eq!(
            entries[3].error.as_deref(),
            Some("no value for header X-Api-Key on this machine")
        );
        assert!(entries[4].ready && entries[4].error.is_none());

        let key = readiness_key(&entries);
        let mut reversed = entries.clone();
        reversed.reverse();
        assert_eq!(readiness_key(&reversed), key, "order-independent");
        set_secret(&dir.0, "acct", &configs[3], "X-Api-Key", "k").unwrap();
        let after = readiness(&dir.0, "acct", &configs, 1_000);
        assert!(after[3].ready);
        assert_ne!(readiness_key(&after), key);
    }

    #[test]
    fn refresh_sweep_rotates_only_expiring_refreshable_sets() {
        let dir = temp_dir("sweep");
        let stub = StubServer::new(vec![(
            "POST /token",
            200,
            r#"{"access_token":"at-new","expires_in":3600}"#.to_string(),
        )]);
        let endpoint = format!("{}/token", stub.base);
        store_token(&dir.0, "s-soon", &token("at", Some(1_300), Some("rt"), &endpoint));
        store_token(&dir.0, "s-fresh", &token("at", Some(9_000), Some("rt"), &endpoint));
        store_token(&dir.0, "s-norefresh", &token("at", Some(1_300), None, &endpoint));
        let configs = vec![
            oauth_config("s-soon", "A"),
            oauth_config("s-fresh", "B"),
            oauth_config("s-norefresh", "C"),
        ];
        assert_eq!(refresh_expiring(&dir.0, "acct", &configs, 1_000), 1);
        assert_eq!(stub.paths(), vec!["/token"]);
        let store = TokenStore::new(dir.0.clone());
        assert_eq!(TokenSet::load(&store, "acct", "s-soon").unwrap().access_token, "at-new");
        assert_eq!(TokenSet::load(&store, "acct", "s-soon").unwrap().refresh_token.as_deref(), Some("rt"));
        assert_eq!(TokenSet::load(&store, "acct", "s-fresh").unwrap().access_token, "at");
    }

    #[test]
    fn readiness_state_fetches_once_then_serves_local_reads() {
        let dir = temp_dir("state");
        let body = serde_json::json!({"result":{"data":[
            {"id":"s","name":"Docs","auth":"none"}
        ]}})
        .to_string();
        let base = canned_server(vec![(200, body)]);
        let trpc = TrpcClient::new(&base, Arc::new(StaticToken("t".into())));
        let mut state = McpReadinessState::new();
        let first = state.sweep(&dir.0, "acct", &trpc).expect("snapshot");
        assert_eq!(first.entries.len(), 1);
        assert!(first.entries[0].ready);
        // The canned server answered once; a second sweep must NOT need it.
        let second = state.sweep(&dir.0, "acct", &trpc).expect("snapshot");
        assert_eq!(second, first);
        assert_eq!(state.configs().len(), 1);
        // Offline at boot: nothing to report.
        let dead = TrpcClient::new("http://127.0.0.1:1", Arc::new(StaticToken("t".into())));
        assert!(McpReadinessState::new().sweep(&dir.0, "acct", &dead).is_none());
    }

    /// Compared as parsed values: `serde_json`'s `preserve_order` feature
    /// flips object key order across crate combinations.
    #[test]
    fn command_messages_are_the_contract_json() {
        let parse = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let start = OauthStart::Hosted {
            authorize_url: "https://auth/x?a=1".into(),
        };
        assert_eq!(
            parse(start.message()),
            serde_json::json!({"phase": "authorize", "url": "https://auth/x?a=1"})
        );
        assert_eq!(parse(done_message(None)), serde_json::json!({"phase": "done"}));
        assert_eq!(
            parse(done_message(Some("2026-09-09T10:00:00.000Z"))),
            serde_json::json!({"phase": "done", "expiresAt": "2026-09-09T10:00:00.000Z"})
        );
    }

    #[test]
    fn oauth_start_hosted_persists_the_pending_flow_and_code_completes_it() {
        let dir = temp_dir("cmd-hosted");
        // ONE stub plays the AS (metadata + DCR + token) and the tRPC server
        // (listForDevice / reportReadiness) — routed by path.
        let stub = StubServer::with(|base| {
            let list = serde_json::json!({"result":{"data":[{
                "id":"s","name":"Linear","transport":"http",
                "url":format!("{base}/mcp"),"auth":"oauth","scopes":["read"]
            }]}})
            .to_string();
            vec![
                ("GET /.well-known/oauth-authorization-server", 200, as_metadata(base)),
                ("POST /register", 201, r#"{"client_id":"dyn"}"#.to_string()),
                ("POST /token", 200, r#"{"access_token":"at","expires_in":60}"#.to_string()),
                ("GET /api/trpc/mcpServers.listForDevice", 200, list),
                (
                    "POST /api/trpc/mcpServers.reportReadiness",
                    200,
                    r#"{"result":{"data":{"ok":true}}}"#.to_string(),
                ),
            ]
        });
        let trpc = TrpcClient::new(&stub.base, Arc::new(StaticToken("t".into())));
        let host = HostContext {
            data_dir: &dir.0,
            account_id: "acct",
            trpc: &trpc,
            device_id: "dev",
        };
        let start = oauth_start(
            &host,
            &serde_json::json!({
                "serverId": "s", "state": "st-1",
                "redirectUri": "https://app.example.com/api/mcp-oauth/callback"
            }),
        )
        .unwrap();
        assert!(matches!(start, OauthStart::Hosted { .. }));
        assert!(start.authorize_url().contains("redirect_uri=https%3A%2F%2Fapp.example.com%2Fapi%2Fmcp-oauth%2Fcallback"));
        // http app base (the stub) is not public https → DCR, not CIMD.
        assert!(start.authorize_url().contains("client_id=dyn&"));

        let expires = oauth_code(
            &host,
            &serde_json::json!({"serverId": "s", "state": "st-1", "code": "the-code"}),
        )
        .unwrap();
        assert!(expires.is_some());
        let store = TokenStore::new(dir.0.clone());
        assert_eq!(TokenSet::load(&store, "acct", "s").unwrap().access_token, "at");
        assert!(PendingFlow::load(&store, "acct", "s", "st-1").is_none());
        let paths = stub.paths();
        assert!(paths.contains(&"/api/trpc/mcpServers.reportReadiness".to_string()));
        let readiness_body = stub
            .requests
            .lock()
            .unwrap()
            .iter()
            .find(|raw| raw.contains("reportReadiness"))
            .cloned()
            .unwrap();
        assert!(readiness_body.contains(r#""serverId":"s","ready":true"#), "{readiness_body}");
        // A repeat (redelivered command) has no pending flow to answer.
        let repeat = oauth_code(
            &host,
            &serde_json::json!({"serverId": "s", "state": "st-1", "code": "the-code"}),
        )
        .unwrap_err();
        assert!(repeat.contains("no sign-in is waiting"));
        assert_eq!(
            oauth_start(&host, &serde_json::json!({"serverId": "s"})).unwrap_err(),
            "Malformed command payload."
        );
    }

    #[test]
    fn oauth_start_loopback_binds_a_listener_and_finishes_through_finish_oauth() {
        let dir = temp_dir("cmd-loopback");
        let stub = StubServer::with(|base| {
            let list = serde_json::json!({"result":{"data":[{
                "id":"s","name":"Linear","url":format!("{base}/mcp"),"auth":"oauth"
            }]}})
            .to_string();
            vec![
                ("GET /.well-known/oauth-authorization-server", 200, as_metadata(base)),
                ("POST /register", 201, r#"{"client_id":"dyn"}"#.to_string()),
                ("POST /token", 200, r#"{"access_token":"at"}"#.to_string()),
                ("GET /api/trpc/mcpServers.listForDevice", 200, list),
                (
                    "POST /api/trpc/mcpServers.finishOAuth",
                    200,
                    r#"{"result":{"data":{"ok":true}}}"#.to_string(),
                ),
                (
                    "POST /api/trpc/mcpServers.reportReadiness",
                    200,
                    r#"{"result":{"data":{"ok":true}}}"#.to_string(),
                ),
            ]
        });
        let trpc = TrpcClient::new(&stub.base, Arc::new(StaticToken("t".into())));
        let host = HostContext {
            data_dir: &dir.0,
            account_id: "acct",
            trpc: &trpc,
            device_id: "dev",
        };
        let start = oauth_start(
            &host,
            &serde_json::json!({"serverId": "s", "state": "st-2", "redirectUri": "loopback"}),
        )
        .unwrap();
        let OauthStart::Loopback {
            authorize_url,
            loopback,
            pending,
        } = start
        else {
            panic!("expected the loopback variant");
        };
        let port = loopback.port();
        assert!(authorize_url.contains(&format!("redirect_uri=http%3A%2F%2F127.0.0.1%3A{port}%2Fcallback")));
        // The browser comes back on the listener.
        let redirect = std::thread::spawn(move || {
            let mut stream = loop {
                if let Ok(stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                    break stream;
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            use std::io::{Read, Write};
            stream
                .write_all(b"GET /callback?code=c&state=st-2 HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut out = String::new();
            let _ = stream.read_to_string(&mut out);
            out
        });
        let expires = oauth_finish_loopback(&host, pending, loopback).unwrap();
        assert_eq!(expires, None, "no expires_in → no expiry");
        assert!(redirect.join().unwrap().starts_with("HTTP/1.1 200"));
        let requests = stub.requests.lock().unwrap().clone();
        let finish = requests
            .iter()
            .find(|raw| raw.contains("finishOAuth"))
            .expect("finishOAuth called");
        assert!(finish.contains(r#"{"state":"st-2","ok":true}"#), "{finish}");
        assert!(requests.iter().any(|raw| raw.contains("reportReadiness")));
    }

    #[test]
    fn local_login_paste_round_trips_and_rejects_a_foreign_state() {
        let dir = temp_dir("local-paste");
        let stub = StubServer::with(|base| {
            vec![
                ("GET /.well-known/oauth-authorization-server", 200, as_metadata(base)),
                ("POST /register", 201, r#"{"client_id":"dyn"}"#.to_string()),
                ("POST /token", 200, r#"{"access_token":"at","expires_in":100}"#.to_string()),
            ]
        });
        let config = McpServerConfig {
            id: "s".into(),
            name: "Linear".into(),
            url: Some(format!("{}/mcp", stub.base)),
            auth: "oauth".into(),
            ..Default::default()
        };
        let login = begin_local_login(&dir.0, "acct", &stub.base, &config, true).unwrap();
        assert!(login.loopback.is_none());
        assert!(login
            .authorize_url
            .contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcallback"));
        let foreign = finish_pasted_login(
            &dir.0,
            "acct",
            &login,
            "http://127.0.0.1:1/callback?code=c&state=other",
        )
        .unwrap_err();
        assert!(foreign.0.contains("different sign-in"));
        let pasted = format!(
            "http://127.0.0.1:1/callback?code=c&state={}",
            login.pending.state
        );
        let set = finish_pasted_login(&dir.0, "acct", &login, &pasted).unwrap();
        assert_eq!(set.access_token, "at");
        assert_eq!(readiness(&dir.0, "acct", &[config.clone()], 0)[0].ready, true);
        forget_server(&dir.0, "acct", "s");
        assert!(!readiness(&dir.0, "acct", &[config], 0)[0].ready);
    }

    #[test]
    fn set_secret_requires_a_declared_name_and_a_value() {
        let dir = temp_dir("set-secret");
        let config = secret_http("s", "Bridge", "X-Api-Key");
        assert!(set_secret(&dir.0, "acct", &config, "Other", "v")
            .unwrap_err()
            .contains("not a declared header"));
        assert!(set_secret(&dir.0, "acct", &config, "X-Api-Key", "  ")
            .unwrap_err()
            .contains("empty"));
        set_secret(&dir.0, "acct", &config, "X-Api-Key", " v1 ").unwrap();
        assert!(readiness(&dir.0, "acct", &[config.clone()], 0)[0].ready);
        let open = McpServerConfig {
            id: "o".into(),
            ..Default::default()
        };
        assert!(set_secret(&dir.0, "acct", &open, "X", "v").is_err());
    }
}
