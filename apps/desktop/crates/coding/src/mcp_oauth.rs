//! EXP-792: the OAuth client behind team MCP servers — discovery (RFC 9728
//! protected-resource metadata → RFC 8414 / OIDC authorization-server
//! metadata), the client-id decision (CIMD when the instance is a public
//! https app base and the provider advertises it, else RFC 7591 dynamic
//! registration cached per issuer), PKCE S256 (always — Linear accepts
//! nothing else), the authorize URL, the code exchange, the refresh, a
//! loopback listener for device-initiated sign-ins, and the paste fallback.
//!
//! Every credential this module produces goes to the 0600 secret store
//! ([`api::token_store`]) and nowhere else: never a log line, never a config
//! file, never the activity channel. The web instance holds NO token — the
//! hosted callback only relays the authorization CODE to the device that
//! started the flow (`mcp_oauth_code`), and the device exchanges it here
//! with the verifier it kept.
//!
//! Provider quirks the spike found, baked in rather than documented away:
//! Sentry serves its PRM ONLY at the path-suffixed well-known and 500s when a
//! CIMD document is unreachable (a non-public app base MUST take DCR, which
//! [`choose_client_id`] enforces); Notion's single scope is `default` and its
//! allowlist runs at the callback (nothing to do here but pass the scope
//! through); Linear is S256-only.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use api::token_store::{SecretKind, TokenStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Every discovery / token request's whole budget.
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a loopback listener waits for the browser to come back.
pub const LOOPBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The CIMD document the web instance serves (Lane S,
/// `routes/api/mcp-oauth/client.json`). Its URL IS the client id.
pub const CIMD_PATH: &str = "/api/mcp-oauth/client.json";

/// The client name a dynamic registration carries.
const CLIENT_NAME: &str = "Exponential";

/// Loopback redirect URIs registered alongside the hosted one so a later
/// device-initiated sign-in (any port, RFC 8252 §7.3) reuses the same DCR
/// client.
const LOOPBACK_REDIRECTS: [&str; 2] = ["http://127.0.0.1/callback", "http://localhost/callback"];

/// One failure sentence — shown to the person on the failed command row /
/// CLI line. Never carries a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpOauthError(pub String);

impl std::fmt::Display for McpOauthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for McpOauthError {}

fn err(message: impl Into<String>) -> McpOauthError {
    McpOauthError(message.into())
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// What discovery learned about a server's authorization server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// The AS issuer (from the PRM's `authorization_servers`, else the
    /// server's own origin).
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    /// `client_id_metadata_document_supported` (the CIMD signal).
    pub cimd_supported: bool,
    /// The PRM's `scopes_supported` (the AS's when the PRM has none).
    pub scopes_supported: Vec<String>,
    /// Which well-known answered the PRM (for the CLI's diagnostics).
    pub prm_url: Option<String>,
}

/// `scheme://host[:port]` and the path (without query/fragment) of a URL.
fn split_url(url: &str) -> Result<(String, String), McpOauthError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| err(format!("not an absolute URL: {url}")))?;
    if !matches!(scheme, "http" | "https") {
        return Err(err(format!("unsupported URL scheme: {scheme}")));
    }
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    if host.is_empty() {
        return Err(err(format!("not an absolute URL: {url}")));
    }
    let path = path.trim_end_matches('/').to_string();
    Ok((format!("{scheme}://{host}"), path))
}

/// The well-known candidates for a metadata `suffix` at `url`: the
/// path-suffixed form FIRST (RFC 9728 §3.1 / RFC 8414 §3.1 — Sentry answers
/// only there), then the root.
fn well_known_candidates(url: &str, suffix: &str) -> Result<Vec<String>, McpOauthError> {
    let (origin, path) = split_url(url)?;
    let mut candidates = Vec::new();
    if !path.is_empty() {
        candidates.push(format!("{origin}/.well-known/{suffix}{path}"));
    }
    candidates.push(format!("{origin}/.well-known/{suffix}"));
    Ok(candidates)
}

/// GET a JSON document; `Ok(None)` for any non-2xx or non-JSON answer
/// (discovery probes several URLs and only cares which one speaks),
/// `Err` only for a transport failure (no point probing the next one).
fn fetch_json(url: &str) -> Result<Option<Value>, McpOauthError> {
    let response = api::http::shared()
        .get(url)
        .header("accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .send()
        .map_err(|e| err(format!("could not reach {url}: {}", scrub_reqwest(&e))))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let text = response.text().map_err(|e| err(format!("{url}: {e}")))?;
    Ok(serde_json::from_str::<Value>(&text).ok())
}

/// reqwest's Display can carry the full URL and library internals — keep
/// only the kind of failure (the crate is reached through `api::http`, so
/// this classifies by text rather than by `reqwest::Error`'s predicates).
fn scrub_reqwest(e: &impl std::fmt::Display) -> String {
    let text = e.to_string().to_ascii_lowercase();
    if text.contains("timed out") || text.contains("timeout") {
        "timed out".to_string()
    } else if text.contains("connect") || text.contains("dns") {
        "connection failed".to_string()
    } else {
        "request failed".to_string()
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// RFC 9728 + RFC 8414 discovery for `server_url`.
pub fn discover(server_url: &str) -> Result<Discovery, McpOauthError> {
    let (origin, _) = split_url(server_url)?;
    let mut issuer = origin.clone();
    let mut scopes = Vec::new();
    let mut prm_url = None;
    for candidate in well_known_candidates(server_url, "oauth-protected-resource")? {
        if let Some(doc) = fetch_json(&candidate)? {
            let servers = string_list(doc.get("authorization_servers"));
            if let Some(first) = servers.first() {
                issuer = first.trim_end_matches('/').to_string();
            }
            scopes = string_list(doc.get("scopes_supported"));
            prm_url = Some(candidate);
            break;
        }
    }
    let mut candidates = well_known_candidates(&issuer, "oauth-authorization-server")?;
    candidates.extend(well_known_candidates(&issuer, "openid-configuration")?);
    for candidate in candidates {
        let Some(doc) = fetch_json(&candidate)? else {
            continue;
        };
        let (Some(authorization_endpoint), Some(token_endpoint)) = (
            doc.get("authorization_endpoint").and_then(Value::as_str),
            doc.get("token_endpoint").and_then(Value::as_str),
        ) else {
            continue;
        };
        if scopes.is_empty() {
            scopes = string_list(doc.get("scopes_supported"));
        }
        return Ok(Discovery {
            issuer: doc
                .get("issuer")
                .and_then(Value::as_str)
                .map(|value| value.trim_end_matches('/').to_string())
                .unwrap_or(issuer),
            authorization_endpoint: authorization_endpoint.to_string(),
            token_endpoint: token_endpoint.to_string(),
            registration_endpoint: doc
                .get("registration_endpoint")
                .and_then(Value::as_str)
                .map(str::to_string),
            cimd_supported: doc
                .get("client_id_metadata_document_supported")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            scopes_supported: scopes,
            prm_url,
        });
    }
    Err(err(format!(
        "no OAuth authorization server found for {server_url} (no metadata at {issuer})"
    )))
}

// ---------------------------------------------------------------------------
// Client id: CIMD or DCR
// ---------------------------------------------------------------------------

/// Whether `app_base` is a PUBLIC https origin — the only kind a provider
/// can fetch a CIMD document from. Localhost, loopback, private ranges,
/// link-local and dotless hostnames are not.
pub fn is_public_https(app_base: &str) -> bool {
    let Ok((origin, _)) = split_url(app_base) else {
        return false;
    };
    let Some(host) = origin.strip_prefix("https://") else {
        return false;
    };
    let host = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host == "::1" || !host.contains('.') {
        return false;
    }
    let octets: Vec<u8> = host
        .split('.')
        .map(|part| part.parse::<u8>())
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    if octets.len() == 4 {
        return !matches!(
            octets.as_slice(),
            [10, ..] | [127, ..] | [0, ..] | [192, 168, ..] | [169, 254, ..]
        ) && !(octets[0] == 172 && (16..=31).contains(&octets[1]));
    }
    !host.ends_with(".local") && !host.ends_with(".internal")
}

/// The CIMD client id for `app_base` (`<app_base>/api/mcp-oauth/client.json`).
pub fn cimd_client_id(app_base: &str) -> String {
    format!("{}{CIMD_PATH}", app_base.trim_end_matches('/'))
}

/// The cached RFC 7591 registration for one issuer.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct CachedClient {
    client_id: String,
    #[serde(default)]
    redirect_uris: Vec<String>,
}

fn loopback_family(redirect_uri: &str) -> bool {
    redirect_uri.starts_with("http://127.0.0.1") || redirect_uri.starts_with("http://localhost")
}

/// Which client id to use with `discovery`: CIMD when the provider supports
/// it AND `app_base` is public https; else dynamic registration (cached per
/// issuer in [`SecretKind::McpClient`], re-registered when the cached
/// client does not cover `redirect_uri`); else the provider needs a
/// pre-registered client we cannot supply.
pub fn choose_client_id(
    discovery: &Discovery,
    app_base: &str,
    store: &TokenStore,
    account_id: &str,
    redirect_uri: &str,
) -> Result<String, McpOauthError> {
    if discovery.cimd_supported && is_public_https(app_base) {
        return Ok(cimd_client_id(app_base));
    }
    let Some(registration_endpoint) = discovery.registration_endpoint.as_deref() else {
        return Err(err(
            "this provider requires a pre-registered OAuth client, which Exponential cannot supply",
        ));
    };
    let kind = SecretKind::McpClient {
        issuer: discovery.issuer.clone(),
    };
    if let Some(cached) = store
        .get(account_id, kind.clone())
        .and_then(|raw| serde_json::from_str::<CachedClient>(&raw).ok())
    {
        let covered = cached.redirect_uris.iter().any(|uri| uri == redirect_uri)
            || (loopback_family(redirect_uri)
                && cached.redirect_uris.iter().any(|uri| loopback_family(uri)));
        if covered && !cached.client_id.is_empty() {
            return Ok(cached.client_id);
        }
    }
    let mut redirect_uris: Vec<String> = vec![redirect_uri.to_string()];
    for loopback in LOOPBACK_REDIRECTS {
        if loopback != redirect_uri {
            redirect_uris.push(loopback.to_string());
        }
    }
    let body = serde_json::json!({
        "client_name": CLIENT_NAME,
        "redirect_uris": redirect_uris,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    let response = api::http::shared()
        .post(registration_endpoint)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .body(body.to_string())
        .send()
        .map_err(|e| err(format!("client registration failed: {}", scrub_reqwest(&e))))?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    let doc: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let client_id = doc
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            err(format!(
                "client registration failed (HTTP {}): {}",
                status.as_u16(),
                oauth_error_text(&doc)
            ))
        })?;
    let cached = CachedClient {
        client_id: client_id.to_string(),
        redirect_uris,
    };
    store
        .set(
            account_id,
            kind,
            &serde_json::to_string(&cached).unwrap_or_default(),
        )
        .map_err(|e| err(format!("could not cache the client registration: {e}")))?;
    Ok(cached.client_id)
}

/// `error_description` / `error` of an OAuth error body, else a stub.
fn oauth_error_text(doc: &Value) -> String {
    doc.get("error_description")
        .or_else(|| doc.get("error"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "no error body".to_string())
}

// ---------------------------------------------------------------------------
// PKCE, state, the authorize URL
// ---------------------------------------------------------------------------

/// A fresh PKCE S256 pair ([`api::login::generate_pkce`]: 64 hex chars, a
/// valid RFC 7636 verifier).
pub fn generate_pkce() -> api::login::PkcePair {
    api::login::generate_pkce()
}

/// A fresh opaque `state` for a device-initiated flow (the web mints its own
/// for hosted ones): two v4 UUIDs, URL-safe as-is.
pub fn random_state() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// RFC 3986 unreserved-only percent encoding (a copy of the api crate's
/// private codec — form bodies and query strings need it here).
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        if bytes[i] == b'%' {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `k=v&k2=v2` with every key and value percent-encoded.
pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Everything a flow needs between the authorize redirect and the code:
/// persisted as [`SecretKind::McpPending`] (the verifier IS a secret).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingFlow {
    pub server_id: String,
    pub state: String,
    pub code_verifier: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub issuer: String,
    pub redirect_uri: String,
    /// RFC 8707 `resource` — the MCP server URL.
    pub resource: String,
    pub created_at: u64,
}

impl PendingFlow {
    pub fn kind(&self) -> SecretKind {
        SecretKind::McpPending {
            server_id: self.server_id.clone(),
            state: self.state.clone(),
        }
    }

    pub fn save(&self, store: &TokenStore, account_id: &str) -> Result<(), McpOauthError> {
        store
            .set(
                account_id,
                self.kind(),
                &serde_json::to_string(self).unwrap_or_default(),
            )
            .map_err(|e| err(format!("could not persist the sign-in: {e}")))
    }

    pub fn load(
        store: &TokenStore,
        account_id: &str,
        server_id: &str,
        state: &str,
    ) -> Option<PendingFlow> {
        let raw = store.get(
            account_id,
            SecretKind::McpPending {
                server_id: server_id.to_string(),
                state: state.to_string(),
            },
        )?;
        serde_json::from_str(&raw).ok()
    }

    pub fn forget(&self, store: &TokenStore, account_id: &str) {
        store.delete(account_id, self.kind());
    }
}

/// The authorize URL: `response_type=code`, the client id, the redirect,
/// the PKCE challenge (S256), the state, the scope when there is one, and
/// RFC 8707 `resource` = the server URL.
pub fn authorize_url(
    discovery: &Discovery,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    scope: Option<&str>,
    resource: &str,
) -> String {
    let mut pairs: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
    ];
    if let Some(scope) = scope.filter(|scope| !scope.trim().is_empty()) {
        pairs.push(("scope", scope));
    }
    pairs.push(("resource", resource));
    let separator = if discovery.authorization_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    format!(
        "{}{separator}{}",
        discovery.authorization_endpoint,
        form_encode(&pairs)
    )
}

/// The scope to request: the row's scopes joined by space, else the
/// discovered `scopes_supported` joined, else none.
pub fn scope_for(row_scopes: &[String], discovery: &Discovery) -> Option<String> {
    let picked: Vec<&str> = if row_scopes.is_empty() {
        discovery
            .scopes_supported
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        row_scopes.iter().map(String::as_str).collect()
    };
    let joined = picked
        .iter()
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

// ---------------------------------------------------------------------------
// Token sets
// ---------------------------------------------------------------------------

/// The stored token set ([`SecretKind::McpOauth`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix seconds; `None` = the provider gave no `expires_in`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default = "bearer")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub token_endpoint: String,
    pub client_id: String,
    pub issuer: String,
}

fn bearer() -> String {
    "Bearer".to_string()
}

impl TokenSet {
    pub fn kind(server_id: &str) -> SecretKind {
        SecretKind::McpOauth {
            server_id: server_id.to_string(),
        }
    }

    pub fn load(store: &TokenStore, account_id: &str, server_id: &str) -> Option<TokenSet> {
        let raw = store.get(account_id, Self::kind(server_id))?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(
        &self,
        store: &TokenStore,
        account_id: &str,
        server_id: &str,
    ) -> Result<(), McpOauthError> {
        store
            .set(
                account_id,
                Self::kind(server_id),
                &serde_json::to_string(self).unwrap_or_default(),
            )
            .map_err(|e| err(format!("could not store the token: {e}")))
    }

    /// Seconds until expiry (`None` = no known expiry).
    pub fn seconds_left(&self, now: u64) -> Option<u64> {
        self.expires_at.map(|at| at.saturating_sub(now))
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|at| at <= now)
    }

    /// Whether a refresh is due: expiry known and inside `margin`.
    pub fn expires_within(&self, now: u64, margin: u64) -> bool {
        self.seconds_left(now).is_some_and(|left| left < margin)
    }

    /// The ISO expiry the readiness rows carry.
    pub fn expires_at_iso(&self) -> Option<String> {
        self.expires_at
            .and_then(|at| crate::agent_accounts::iso_from_unix_secs(at as i64))
    }
}

/// Parse a token endpoint's JSON into a [`TokenSet`]; `previous` supplies
/// the refresh token when a refresh response rotates none.
fn parse_token_response(
    doc: &Value,
    now: u64,
    token_endpoint: &str,
    client_id: &str,
    issuer: &str,
    previous_refresh: Option<&str>,
) -> Result<TokenSet, McpOauthError> {
    let access_token = doc
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| err(format!("token response had no access token: {}", oauth_error_text(doc))))?;
    let expires_at = doc
        .get("expires_in")
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
        .map(|expires_in| now + expires_in);
    Ok(TokenSet {
        access_token: access_token.to_string(),
        refresh_token: doc
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .or_else(|| previous_refresh.map(str::to_string)),
        expires_at,
        token_type: doc
            .get("token_type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(bearer),
        scope: doc
            .get("scope")
            .and_then(Value::as_str)
            .map(str::to_string),
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
        issuer: issuer.to_string(),
    })
}

fn post_token(token_endpoint: &str, body: String) -> Result<Value, McpOauthError> {
    let response = api::http::shared()
        .post(token_endpoint)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .body(body)
        .send()
        .map_err(|e| err(format!("token request failed: {}", scrub_reqwest(&e))))?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    let doc: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(err(format!(
            "token request refused (HTTP {}): {}",
            status.as_u16(),
            oauth_error_text(&doc)
        )));
    }
    Ok(doc)
}

/// Exchange the authorization `code` for a token set (public client: no
/// secret, PKCE verifier instead).
pub fn exchange_code(pending: &PendingFlow, code: &str, now: u64) -> Result<TokenSet, McpOauthError> {
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("client_id", &pending.client_id),
        ("code", code),
        ("redirect_uri", &pending.redirect_uri),
        ("code_verifier", &pending.code_verifier),
        ("resource", &pending.resource),
    ]);
    let doc = post_token(&pending.token_endpoint, body)?;
    parse_token_response(
        &doc,
        now,
        &pending.token_endpoint,
        &pending.client_id,
        &pending.issuer,
        None,
    )
}

/// Rotate `set` with its refresh token (`resource` = the server URL). The
/// old refresh token is kept when the provider returns none.
pub fn refresh(set: &TokenSet, resource: &str, now: u64) -> Result<TokenSet, McpOauthError> {
    let refresh_token = set
        .refresh_token
        .as_deref()
        .ok_or_else(|| err("no refresh token; sign in again"))?;
    let body = form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &set.client_id),
        ("resource", resource),
    ]);
    let doc = post_token(&set.token_endpoint, body)?;
    parse_token_response(
        &doc,
        now,
        &set.token_endpoint,
        &set.client_id,
        &set.issuer,
        Some(refresh_token),
    )
}

// ---------------------------------------------------------------------------
// Loopback listener + the paste fallback
// ---------------------------------------------------------------------------

/// The `code` + `state` a redirect carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedirectParams {
    pub code: String,
    pub state: String,
}

/// Pull `code`/`state` (or the provider's `error`) out of a redirect URL's
/// query — the loopback request line and the pasted-URL fallback share it.
pub fn parse_redirect(url: &str) -> Result<RedirectParams, McpOauthError> {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or(url)
        .split('#')
        .next()
        .unwrap_or_default();
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut description = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            "error_description" => description = Some(value),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Err(err(match description {
            Some(description) => format!("the provider refused: {error} ({description})"),
            None => format!("the provider refused: {error}"),
        }));
    }
    match (code, state) {
        (Some(code), Some(state)) if !code.is_empty() && !state.is_empty() => {
            Ok(RedirectParams { code, state })
        }
        _ => Err(err("that URL carries no authorization code and state")),
    }
}

/// A bound loopback listener (`127.0.0.1:<port>`), waiting for the
/// browser's redirect.
#[derive(Debug)]
pub struct Loopback {
    listener: TcpListener,
    port: u16,
}

const CALLBACK_PATH: &str = "/callback";

/// The page the browser lands on. Plain, self-contained, no script.
fn callback_page(ok: bool, detail: &str) -> String {
    let (title, body) = if ok {
        ("Signed in", "You can close this tab and go back to Exponential.")
    } else {
        ("Sign-in failed", detail)
    };
    let body = html_escape(body);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
<style>body{{font-family:system-ui,sans-serif;background:#18181b;color:#fafafa;display:flex;\
align-items:center;justify-content:center;height:100vh;margin:0}}main{{text-align:center}}\
p{{color:#a1a1aa}}</style></head><body><main><h1>{title}</h1><p>{body}</p></main></body></html>"
    )
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_response(stream: &mut TcpStream, status: &str, html: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

impl Loopback {
    /// Bind an ephemeral loopback port.
    pub fn bind() -> Result<Loopback, McpOauthError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| err(format!("could not open a loopback port: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| err(format!("loopback port: {e}")))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| err(format!("loopback port: {e}")))?;
        Ok(Loopback { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `http://127.0.0.1:<port>/callback`.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK_PATH}", self.port)
    }

    /// Block until the browser hits `/callback` (answering it with a page),
    /// or `timeout` passes. Stray requests (a favicon probe, a wrong path)
    /// get a 404 and the wait continues. `expected_state` guards against a
    /// foreign redirect landing on the port.
    pub fn wait(self, timeout: Duration, expected_state: &str) -> Result<RedirectParams, McpOauthError> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_nonblocking(false);
                    let Some(target) = read_request_target(&mut stream) else {
                        write_response(&mut stream, "400 Bad Request", &callback_page(false, "Bad request."));
                        continue;
                    };
                    let path = target.split('?').next().unwrap_or_default();
                    if path != CALLBACK_PATH {
                        write_response(&mut stream, "404 Not Found", &callback_page(false, "Not found."));
                        continue;
                    }
                    match parse_redirect(&target) {
                        Ok(params) if params.state == expected_state => {
                            write_response(&mut stream, "200 OK", &callback_page(true, ""));
                            return Ok(params);
                        }
                        Ok(_) => {
                            write_response(
                                &mut stream,
                                "400 Bad Request",
                                &callback_page(false, "This sign-in link does not match the one waiting here."),
                            );
                            continue;
                        }
                        Err(error) => {
                            write_response(&mut stream, "400 Bad Request", &callback_page(false, &error.0));
                            return Err(error);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(err("timed out waiting for the browser to come back"));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(err(format!("loopback listener failed: {e}"))),
            }
        }
    }
}

/// The request target of an HTTP request line (`GET /callback?x=y HTTP/1.1`).
fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(2).any(|w| w == b"\r\n") || buf.len() > 16 * 1024 {
            break;
        }
    }
    let line = String::from_utf8_lossy(&buf);
    let line = line.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    (method == "GET").then(|| target.to_string())
}

// ---------------------------------------------------------------------------
// One-call helpers the hosts use
// ---------------------------------------------------------------------------

/// Everything `mcp_oauth_start` needs beyond the discovery: where the
/// secrets go and which app base the CIMD document would live at.
pub struct StartContext<'a> {
    pub data_dir: &'a Path,
    pub account_id: &'a str,
    /// The instance URL (`trpc.base_url()`).
    pub app_base: &'a str,
}

/// Discover, pick a client id, mint PKCE, persist the pending flow and
/// build the authorize URL for `server_url` with `redirect_uri` + `state`.
pub fn begin(
    ctx: &StartContext<'_>,
    server_id: &str,
    server_url: &str,
    row_scopes: &[String],
    redirect_uri: &str,
    state: &str,
    now: u64,
) -> Result<(String, PendingFlow), McpOauthError> {
    let store = TokenStore::new(ctx.data_dir.to_path_buf());
    let discovery = discover(server_url)?;
    let client_id = choose_client_id(&discovery, ctx.app_base, &store, ctx.account_id, redirect_uri)?;
    let pkce = generate_pkce();
    let pending = PendingFlow {
        server_id: server_id.to_string(),
        state: state.to_string(),
        code_verifier: pkce.verifier,
        token_endpoint: discovery.token_endpoint.clone(),
        client_id: client_id.clone(),
        issuer: discovery.issuer.clone(),
        redirect_uri: redirect_uri.to_string(),
        resource: server_url.to_string(),
        created_at: now,
    };
    pending.save(&store, ctx.account_id)?;
    let scope = scope_for(row_scopes, &discovery);
    let url = authorize_url(
        &discovery,
        &client_id,
        redirect_uri,
        &pkce.challenge,
        state,
        scope.as_deref(),
        server_url,
    );
    Ok((url, pending))
}

/// Finish a flow: exchange the code with the pending flow's verifier, store
/// the token set under the server, drop the pending entry. Returns the set.
pub fn complete(
    data_dir: &Path,
    account_id: &str,
    pending: &PendingFlow,
    code: &str,
    now: u64,
) -> Result<TokenSet, McpOauthError> {
    let store = TokenStore::new(data_dir.to_path_buf());
    let set = exchange_code(pending, code, now)?;
    set.save(&store, account_id, &pending.server_id)?;
    pending.forget(&store, account_id);
    Ok(set)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// A tiny routed HTTP stub: `routes` maps `"METHOD /path"` to a canned
    /// (status, body); everything else 404s. Records every request's raw
    /// head+body. Lives until dropped.
    pub(crate) struct StubServer {
        pub base: String,
        pub requests: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
    }

    impl Drop for StubServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    impl StubServer {
        pub(crate) fn new(routes: Vec<(&'static str, u16, String)>) -> StubServer {
            Self::with(|_| routes)
        }

        /// [`StubServer::new`] whose routes may embed the stub's own base
        /// URL (discovery documents point back at the issuer).
        pub(crate) fn with(
            routes: impl FnOnce(&str) -> Vec<(&'static str, u16, String)>,
        ) -> StubServer {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let routes: HashMap<&'static str, (u16, String)> = routes(&base)
                .into_iter()
                .map(|(key, status, body)| (key, (status, body)))
                .collect();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let recorded = Arc::clone(&requests);
            let stop_flag = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(accepted) => accepted,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                    };
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (mut head_end, mut content_length) = (None::<usize>, 0usize);
                    while let Ok(n) = stream.read(&mut chunk) {
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        if head_end.is_none() {
                            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                head_end = Some(pos + 4);
                                let head = String::from_utf8_lossy(&buf[..pos]);
                                content_length = head
                                    .lines()
                                    .find_map(|line| {
                                        let (name, value) = line.split_once(':')?;
                                        name.eq_ignore_ascii_case("content-length")
                                            .then(|| value.trim().parse().ok())?
                                    })
                                    .unwrap_or(0);
                            }
                        }
                        if let Some(pos) = head_end {
                            if buf.len() >= pos + content_length {
                                break;
                            }
                        }
                    }
                    let raw = String::from_utf8_lossy(&buf).into_owned();
                    let line = raw.lines().next().unwrap_or_default();
                    let mut parts = line.split_whitespace();
                    let key = format!(
                        "{} {}",
                        parts.next().unwrap_or_default(),
                        parts.next().unwrap_or_default()
                    );
                    recorded.lock().unwrap().push(raw.clone());
                    let (status, body) = routes
                        .get(key.as_str())
                        .cloned()
                        .unwrap_or((404, "{}".to_string()));
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            StubServer {
                base,
                requests,
                stop,
            }
        }

        pub(crate) fn paths(&self) -> Vec<String> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .map(|raw| {
                    raw.lines()
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        }

        pub(crate) fn last_body(&self) -> String {
            self.requests
                .lock()
                .unwrap()
                .last()
                .and_then(|raw| raw.split_once("\r\n\r\n").map(|(_, body)| body.to_string()))
                .unwrap_or_default()
        }
    }

    pub(crate) fn as_metadata(base: &str) -> String {
        serde_json::json!({
            "issuer": base,
            "authorization_endpoint": format!("{base}/authorize"),
            "token_endpoint": format!("{base}/token"),
            "registration_endpoint": format!("{base}/register"),
            "client_id_metadata_document_supported": true,
            "scopes_supported": ["read", "write"],
            "code_challenge_methods_supported": ["S256"],
        })
        .to_string()
    }

    fn temp_dir(tag: &str) -> crate::test_support::TempDir {
        crate::test_support::temp_dir(&format!("mcp-oauth-{tag}"))
    }

    #[test]
    fn discovery_prefers_the_path_suffixed_prm_then_root() {
        // Sentry-style: PRM only at the suffixed path, AS metadata at the
        // issuer root.
        let stub = StubServer::with(|base| {
            vec![
                (
                    "GET /.well-known/oauth-protected-resource/mcp",
                    200,
                    serde_json::json!({
                        "resource": format!("{base}/mcp"),
                        "authorization_servers": [base],
                        "scopes_supported": ["org:read"],
                    })
                    .to_string(),
                ),
                ("GET /.well-known/oauth-authorization-server", 200, as_metadata(base)),
            ]
        });
        let base = stub.base.clone();
        let discovery = discover(&format!("{base}/mcp")).expect("discovered");
        assert_eq!(discovery.issuer, base);
        assert_eq!(discovery.authorization_endpoint, format!("{base}/authorize"));
        assert_eq!(discovery.scopes_supported, vec!["org:read".to_string()]);
        assert!(discovery.cimd_supported);
        assert_eq!(
            discovery.prm_url.as_deref(),
            Some(format!("{base}/.well-known/oauth-protected-resource/mcp").as_str())
        );
        let paths = stub.paths();
        assert_eq!(paths[0], "/.well-known/oauth-protected-resource/mcp");
        assert_eq!(paths[1], "/.well-known/oauth-authorization-server");
    }

    #[test]
    fn discovery_falls_back_to_root_prm_then_openid_configuration() {
        let stub = StubServer::with(|base| {
            vec![
                (
                    "GET /.well-known/oauth-protected-resource",
                    200,
                    serde_json::json!({
                        "resource": format!("{base}/mcp"),
                        "authorization_servers": [format!("{base}/auth")],
                    })
                    .to_string(),
                ),
                (
                    "GET /.well-known/openid-configuration/auth",
                    200,
                    serde_json::json!({
                        "issuer": format!("{base}/auth"),
                        "authorization_endpoint": format!("{base}/auth/authorize"),
                        "token_endpoint": format!("{base}/auth/token"),
                        "scopes_supported": ["default"],
                    })
                    .to_string(),
                ),
            ]
        });
        let base = stub.base.clone();
        let discovery = discover(&format!("{base}/mcp")).expect("discovered");
        assert_eq!(discovery.issuer, format!("{base}/auth"));
        assert!(!discovery.cimd_supported);
        assert!(discovery.registration_endpoint.is_none());
        // Notion-style: the AS's scopes fill in when the PRM lists none.
        assert_eq!(discovery.scopes_supported, vec!["default".to_string()]);
        let paths = stub.paths();
        assert_eq!(
            paths,
            vec![
                "/.well-known/oauth-protected-resource/mcp",
                "/.well-known/oauth-protected-resource",
                "/.well-known/oauth-authorization-server/auth",
                "/.well-known/oauth-authorization-server",
                "/.well-known/openid-configuration/auth",
            ]
        );
    }

    #[test]
    fn discovery_without_any_metadata_is_an_error() {
        let stub = StubServer::new(vec![]);
        let error = discover(&format!("{}/mcp", stub.base)).unwrap_err();
        assert!(error.0.contains("no OAuth authorization server"), "{error}");
    }

    #[test]
    fn public_https_excludes_local_and_private_hosts() {
        assert!(is_public_https("https://app.exponential.dev"));
        assert!(is_public_https("https://app.exponential.dev:8443/"));
        assert!(!is_public_https("http://app.exponential.dev"));
        assert!(!is_public_https("https://localhost:3000"));
        assert!(!is_public_https("https://127.0.0.1"));
        assert!(!is_public_https("https://10.0.0.5"));
        assert!(!is_public_https("https://192.168.178.111"));
        assert!(!is_public_https("https://172.20.0.1"));
        assert!(!is_public_https("https://homeserver"));
        assert!(!is_public_https("https://nas.local"));
        assert!(is_public_https("https://172.32.0.1"));
    }

    #[test]
    fn cimd_wins_only_on_a_public_app_base() {
        let dir = temp_dir("cimd");
        let store = TokenStore::new(dir.0.clone());
        let discovery = Discovery {
            issuer: "https://auth.example.com".into(),
            cimd_supported: true,
            registration_endpoint: Some("https://auth.example.com/register".into()),
            ..Default::default()
        };
        let id = choose_client_id(
            &discovery,
            "https://app.exponential.dev/",
            &store,
            "acct",
            "https://app.exponential.dev/api/mcp-oauth/callback",
        )
        .unwrap();
        assert_eq!(id, "https://app.exponential.dev/api/mcp-oauth/client.json");
        // No registration happened: nothing cached.
        assert!(store
            .get(
                "acct",
                SecretKind::McpClient {
                    issuer: "https://auth.example.com".into()
                }
            )
            .is_none());
        // Neither CIMD nor DCR possible → the pre-registered-client error.
        let bare = Discovery {
            issuer: "https://auth.example.com".into(),
            cimd_supported: true,
            ..Default::default()
        };
        let error = choose_client_id(&bare, "https://localhost:3000", &store, "acct", "loopback")
            .unwrap_err();
        assert!(error.0.contains("pre-registered"), "{error}");
    }

    #[test]
    fn dcr_registers_once_per_issuer_and_reuses_the_cache() {
        let dir = temp_dir("dcr");
        let store = TokenStore::new(dir.0.clone());
        let stub = StubServer::new(vec![(
            "POST /register",
            201,
            r#"{"client_id":"dyn-1","client_name":"Exponential"}"#.to_string(),
        )]);
        let discovery = Discovery {
            issuer: stub.base.clone(),
            cimd_supported: true, // irrelevant: the app base is localhost
            registration_endpoint: Some(format!("{}/register", stub.base)),
            ..Default::default()
        };
        let redirect = "http://127.0.0.1:4711/callback";
        let first = choose_client_id(&discovery, "https://localhost:3000", &store, "acct", redirect).unwrap();
        assert_eq!(first, "dyn-1");
        let body: Value = serde_json::from_str(&stub.last_body()).unwrap();
        assert_eq!(body["client_name"], "Exponential");
        assert_eq!(body["token_endpoint_auth_method"], "none");
        assert_eq!(body["redirect_uris"][0], redirect);
        assert_eq!(body["grant_types"][1], "refresh_token");
        // A second loopback port reuses the registration (RFC 8252 §7.3).
        let second = choose_client_id(
            &discovery,
            "https://localhost:3000",
            &store,
            "acct",
            "http://127.0.0.1:9999/callback",
        )
        .unwrap();
        assert_eq!(second, "dyn-1");
        assert_eq!(stub.paths().len(), 1, "one registration only");
    }

    #[test]
    fn pkce_challenge_is_s256_of_the_verifier() {
        let pair = generate_pkce();
        assert_eq!(pair.verifier.len(), 64);
        assert_eq!(pair.challenge, api::login::pkce_challenge(&pair.verifier));
        assert_ne!(pair.challenge, pair.verifier);
        assert!(!pair.challenge.contains(['+', '/', '=']));
    }

    #[test]
    fn authorize_url_carries_every_parameter_in_order() {
        let discovery = Discovery {
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            ..Default::default()
        };
        let url = authorize_url(
            &discovery,
            "https://app.exponential.dev/api/mcp-oauth/client.json",
            "https://app.exponential.dev/api/mcp-oauth/callback",
            "chal",
            "st",
            Some("read write"),
            "https://mcp.example.com/mcp",
        );
        assert_eq!(
            url,
            "https://auth.example.com/authorize?response_type=code&client_id=https%3A%2F%2Fapp.exponential.dev%2Fapi%2Fmcp-oauth%2Fclient.json&redirect_uri=https%3A%2F%2Fapp.exponential.dev%2Fapi%2Fmcp-oauth%2Fcallback&code_challenge=chal&code_challenge_method=S256&state=st&scope=read%20write&resource=https%3A%2F%2Fmcp.example.com%2Fmcp"
        );
        let without_scope = authorize_url(&discovery, "c", "r", "x", "s", None, "res");
        assert!(!without_scope.contains("scope="));
        assert!(without_scope.ends_with("&resource=res"));
    }

    #[test]
    fn scope_prefers_the_row_then_discovery_then_none() {
        let discovery = Discovery {
            scopes_supported: vec!["default".into()],
            ..Default::default()
        };
        assert_eq!(
            scope_for(&["a".into(), "b".into()], &discovery).as_deref(),
            Some("a b")
        );
        assert_eq!(scope_for(&[], &discovery).as_deref(), Some("default"));
        assert_eq!(scope_for(&[], &Discovery::default()), None);
    }

    #[test]
    fn exchange_posts_the_pkce_form_and_stores_the_set() {
        let dir = temp_dir("exchange");
        let stub = StubServer::new(vec![(
            "POST /token",
            200,
            r#"{"access_token":"at-1","token_type":"bearer","expires_in":3600,"refresh_token":"rt-1","scope":"read"}"#
                .to_string(),
        )]);
        let pending = PendingFlow {
            server_id: "srv".into(),
            state: "st".into(),
            code_verifier: "verifier".into(),
            token_endpoint: format!("{}/token", stub.base),
            client_id: "cid".into(),
            issuer: stub.base.clone(),
            redirect_uri: "http://127.0.0.1:1/callback".into(),
            resource: "https://mcp.example.com/mcp".into(),
            created_at: 1_000,
        };
        let store = TokenStore::new(dir.0.clone());
        pending.save(&store, "acct").unwrap();
        assert!(PendingFlow::load(&store, "acct", "srv", "st").is_some());
        let set = complete(&dir.0, "acct", &pending, "the-code", 5_000).unwrap();
        assert_eq!(
            stub.last_body(),
            "grant_type=authorization_code&client_id=cid&code=the-code&redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcallback&code_verifier=verifier&resource=https%3A%2F%2Fmcp.example.com%2Fmcp"
        );
        assert_eq!(set.access_token, "at-1");
        assert_eq!(set.refresh_token.as_deref(), Some("rt-1"));
        assert_eq!(set.expires_at, Some(8_600));
        assert_eq!(set.scope.as_deref(), Some("read"));
        assert_eq!(TokenSet::load(&store, "acct", "srv").unwrap(), set);
        assert!(PendingFlow::load(&store, "acct", "srv", "st").is_none(), "pending dropped");
        assert_eq!(set.expires_at_iso().as_deref(), Some("1970-01-01T02:23:20.000Z"));
    }

    #[test]
    fn exchange_refusal_names_the_oauth_error_without_the_code() {
        let stub = StubServer::new(vec![(
            "POST /token",
            400,
            r#"{"error":"invalid_grant","error_description":"code expired"}"#.to_string(),
        )]);
        let pending = PendingFlow {
            server_id: "srv".into(),
            state: "st".into(),
            code_verifier: "v".into(),
            token_endpoint: format!("{}/token", stub.base),
            client_id: "cid".into(),
            issuer: stub.base.clone(),
            redirect_uri: "r".into(),
            resource: "res".into(),
            created_at: 0,
        };
        let error = exchange_code(&pending, "secret-code", 1).unwrap_err();
        assert!(error.0.contains("HTTP 400"), "{error}");
        assert!(error.0.contains("code expired"), "{error}");
        assert!(!error.0.contains("secret-code"));
    }

    #[test]
    fn refresh_rotates_and_keeps_the_old_refresh_token_when_none_returned() {
        let stub = StubServer::new(vec![(
            "POST /token",
            200,
            r#"{"access_token":"at-2","expires_in":"1800"}"#.to_string(),
        )]);
        let set = TokenSet {
            access_token: "at-1".into(),
            refresh_token: Some("rt-1".into()),
            expires_at: Some(100),
            token_type: "Bearer".into(),
            scope: Some("read".into()),
            token_endpoint: format!("{}/token", stub.base),
            client_id: "cid".into(),
            issuer: stub.base.clone(),
        };
        assert!(set.expires_within(0, 600));
        assert!(set.is_expired(100));
        let rotated = refresh(&set, "https://mcp.example.com/mcp", 1_000).unwrap();
        assert_eq!(
            stub.last_body(),
            "grant_type=refresh_token&refresh_token=rt-1&client_id=cid&resource=https%3A%2F%2Fmcp.example.com%2Fmcp"
        );
        assert_eq!(rotated.access_token, "at-2");
        assert_eq!(rotated.refresh_token.as_deref(), Some("rt-1"));
        assert_eq!(rotated.expires_at, Some(2_800));
        assert_eq!(rotated.token_type, "Bearer");
        assert_eq!(rotated.client_id, "cid");
        let no_refresh = TokenSet {
            refresh_token: None,
            ..set
        };
        assert!(refresh(&no_refresh, "r", 0).unwrap_err().0.contains("sign in again"));
    }

    #[test]
    fn parse_redirect_reads_code_and_state_and_surfaces_errors() {
        let params = parse_redirect("http://127.0.0.1:5/callback?state=st%201&code=abc%2Fdef&x=1").unwrap();
        assert_eq!(params.code, "abc/def");
        assert_eq!(params.state, "st 1");
        let params = parse_redirect("/callback?code=c&state=s#frag").unwrap();
        assert_eq!((params.code.as_str(), params.state.as_str()), ("c", "s"));
        let refused = parse_redirect("http://x/callback?error=access_denied&error_description=nope&state=s")
            .unwrap_err();
        assert_eq!(refused.0, "the provider refused: access_denied (nope)");
        assert!(parse_redirect("http://x/callback?state=s").is_err());
        assert!(parse_redirect("garbage").is_err());
    }

    #[test]
    fn loopback_answers_the_callback_and_ignores_strays() {
        let loopback = Loopback::bind().unwrap();
        let port = loopback.port();
        assert_eq!(loopback.redirect_uri(), format!("http://127.0.0.1:{port}/callback"));
        let waiter = std::thread::spawn(move || loopback.wait(Duration::from_secs(10), "st"));
        let get = |target: &str| -> String {
            let mut stream = loop {
                if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                    break stream;
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            stream
                .write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                .unwrap();
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        };
        assert!(get("/favicon.ico").starts_with("HTTP/1.1 404"));
        assert!(get("/callback?code=c&state=other").starts_with("HTTP/1.1 400"));
        let ok = get("/callback?code=the-code&state=st");
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains("You can close this tab"));
        assert!(!ok.contains("the-code"), "the page never echoes the code");
        let params = waiter.join().unwrap().unwrap();
        assert_eq!(params.code, "the-code");
        assert_eq!(params.state, "st");
    }

    #[test]
    fn loopback_times_out() {
        let loopback = Loopback::bind().unwrap();
        let error = loopback.wait(Duration::from_millis(250), "st").unwrap_err();
        assert!(error.0.contains("timed out"), "{error}");
    }

    #[test]
    fn begin_persists_the_pending_flow_and_builds_the_url() {
        let dir = temp_dir("begin");
        let stub = StubServer::with(|base| {
            vec![
                ("GET /.well-known/oauth-authorization-server", 200, as_metadata(base)),
                ("POST /register", 201, r#"{"client_id":"dyn-9"}"#.to_string()),
            ]
        });
        let base = stub.base.clone();
        let ctx = StartContext {
            data_dir: &dir.0,
            account_id: "acct",
            app_base: "https://localhost:3000",
        };
        let (url, pending) = begin(
            &ctx,
            "srv",
            &format!("{base}/mcp"),
            &[],
            "http://127.0.0.1:7/callback",
            "st-9",
            42,
        )
        .unwrap();
        assert!(url.starts_with(&format!("{base}/authorize?response_type=code&client_id=dyn-9&")));
        assert!(url.contains("&scope=read%20write&"));
        assert!(url.contains("&state=st-9&"));
        assert_eq!(pending.token_endpoint, format!("{base}/token"));
        assert_eq!(pending.created_at, 42);
        let store = TokenStore::new(dir.0.clone());
        assert_eq!(
            PendingFlow::load(&store, "acct", "srv", "st-9").unwrap(),
            pending
        );
        assert!(!url.contains(&pending.code_verifier), "the verifier never leaves the store");
    }
}
