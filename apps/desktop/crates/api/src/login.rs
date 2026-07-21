//! Better Auth login mechanics (masterplan-v3 §5.7) — a straight port of the
//! proven iOS native flow (`ExpCore/Sources/API/AuthApi.swift` +
//! `HTTPClient.swift`):
//!
//! - `GET  /api/auth-config` — which methods the login view may show.
//! - `POST /api/auth/sign-in/email` — `{email, password}` → `{token, user}`
//!   (Better Auth bearer plugin; a `Set-Cookie` session fallback is parsed
//!   when the JSON token is absent, iOS-parity).
//! - `GET  /api/auth/get-session` — bearer session validation.
//! - `POST /api/auth/sign-out` — best-effort server-side revocation.
//! - OAuth via the system browser: start URLs for `/api/mobile-oauth-start`
//!   (carrying a PKCE S256 `code_challenge`, REV-13), plus the callback
//!   capture surfaces — the `exponential://oauth-return?code=…#code=…`
//!   deep-link parser (PRIMARY; a single-use code redeemed via
//!   `POST /api/mobile-oauth-exchange` with the in-memory verifier — legacy
//!   pre-PKCE servers still send `#token=…` with the raw session token).
//!
//! The login *view* (cloud button first) is §4/Phase-3 UI
//! territory; this module owns only the mechanics.

use domain::client_version::{client_version_header_value, CLIENT_VERSION_HEADER};
use serde::Deserialize;
use std::time::Duration;

use crate::encode::{base64url_nopad, percent_decode, percent_encode};
use crate::error::{from_ureq_unauthed, ApiError};

/// Tag a request with the client-version header (EXP-104) so the server can
/// 426-gate stale builds — applied to every `AuthClient` request, including
/// the unauthenticated auth-config / sign-in / oauth-exchange calls, for
/// uniformity. The server does NOT gate auth routes (only tRPC and shape
/// requests answer 426), so the blocking update screen latches once sync
/// starts, not at login.
fn versioned(request: ureq::Request) -> ureq::Request {
    request.set(CLIENT_VERSION_HEADER, &client_version_header_value())
}

/// Which auth methods the server offers (`GET /api/auth-config`, mirrors
/// `apps/web/src/lib/auth/config.ts`). Gate the login UI on this.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default = "default_true")]
    pub password_enabled: bool,
    #[serde(default)]
    pub password_reset_enabled: bool,
    #[serde(default)]
    pub oidc_providers: Vec<OidcProvider>,
    #[serde(default)]
    pub google_login_enabled: bool,
    #[serde(default)]
    pub apple_login_enabled: bool,
    #[serde(default)]
    pub github_enabled: bool,
}

fn default_true() -> bool {
    true
}

/// One configured OIDC provider (id feeds `oidc_oauth_start_url`).
#[derive(Clone, Debug, Deserialize)]
pub struct OidcProvider {
    pub id: String,
    pub name: String,
}

/// The signed-in user as Better Auth reports it (sign-in + get-session).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_admin: Option<bool>,
    /// better-auth additionalField (type date, input:false) — ISO string or
    /// null on session reads, exactly like the web onboarding gate.
    #[serde(default)]
    pub onboarding_completed_at: Option<String>,
}

/// Successful password sign-in: the portable session token + the user.
#[derive(Clone, Debug)]
pub struct SignInSuccess {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Deserialize)]
struct SignInResponseBody {
    token: Option<String>,
    user: Option<AuthUser>,
}

#[derive(Deserialize)]
struct SessionResponse {
    user: Option<AuthUser>,
}

/// Blocking Better Auth client. Cheap to construct; share one per app.
pub struct AuthClient {
    agent: ureq::Agent,
}

impl Default for AuthClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthClient {
    pub fn new() -> Self {
        // 30s overall — parity with the iOS URLSession config. Never used for
        // long-polls (sync owns its own 90s-read agent, §5.3).
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        Self { agent }
    }

    /// `GET /api/auth-config` — unauthenticated; call before any account exists.
    pub fn fetch_auth_config(&self, instance_url: &str) -> Result<AuthConfig, ApiError> {
        let base = normalize_instance_url(instance_url);
        let response = versioned(self.agent.get(&format!("{base}/api/auth-config")))
            .set("Accept", "application/json")
            .call()
            .map_err(from_ureq_unauthed)?;
        let body = response
            .into_string()
            .map_err(|e| ApiError::Transport(e.to_string()))?;
        serde_json::from_str(&body).map_err(|e| ApiError::Decode(format!("auth-config: {e}")))
    }

    /// `POST /api/auth/sign-in/email` → session token + user. The token is
    /// the portable credential (we ignore the cookie in favour of it); when
    /// the JSON omits it, fall back to the `Set-Cookie` session token
    /// (iOS-parity). Bad credentials come back as `ApiError::Http` with
    /// status 401 — NOT `ApiError::Unauthorized` (no bearer was presented).
    pub fn sign_in_with_password(
        &self,
        instance_url: &str,
        email: &str,
        password: &str,
    ) -> Result<SignInSuccess, ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "email": email, "password": password });
        let response = versioned(self.agent.post(&format!("{base}/api/auth/sign-in/email")))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            // Better Auth's CSRF check 403s POSTs without an Origin header
            // (MISSING_OR_NULL_ORIGIN); send the instance's own origin like a
            // same-origin browser request would.
            .set("Origin", &base)
            .send_string(&payload.to_string())
            .map_err(from_ureq_unauthed)?;

        let cookies: Vec<String> = response
            .all("set-cookie")
            .into_iter()
            .map(str::to_string)
            .collect();
        let body = response
            .into_string()
            .map_err(|e| ApiError::Transport(e.to_string()))?;
        let parsed: SignInResponseBody = serde_json::from_str(&body)
            .map_err(|e| ApiError::Decode(format!("sign-in response: {e}")))?;

        match (parsed.token, parsed.user) {
            // Better Auth bearer plugin returns { token, user }.
            (Some(token), Some(user)) => Ok(SignInSuccess { token, user }),
            // Fallback: extract the session token from Set-Cookie.
            (None, Some(user)) => {
                let token = cookies
                    .iter()
                    .find_map(|c| extract_session_token_cookie(c))
                    .ok_or_else(|| {
                        ApiError::Decode(
                            "sign-in succeeded but no session token returned".to_string(),
                        )
                    })?;
                Ok(SignInSuccess { token, user })
            }
            _ => Err(ApiError::Decode(
                "sign-in succeeded but no user returned".to_string(),
            )),
        }
    }

    /// `GET /api/auth/get-session` with the bearer. `Ok(Some(user))` = the
    /// session is alive; `Ok(None)` = the server answered but the token no
    /// longer resolves (dead session — route to login, §5.6b); `Err` =
    /// transport/HTTP failure (do NOT treat as signed-out; retry later).
    pub fn fetch_session(
        &self,
        instance_url: &str,
        token: &str,
    ) -> Result<Option<AuthUser>, ApiError> {
        let base = normalize_instance_url(instance_url);
        let result = versioned(self.agent.get(&format!("{base}/api/auth/get-session")))
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .call();
        let response = match result {
            Ok(r) => r,
            // A bearer that fails to resolve is an explicit 401 on some
            // Better Auth configs — that IS the dead-session answer.
            Err(ureq::Error::Status(401, _)) => return Ok(None),
            Err(e) => return Err(from_ureq_unauthed(e)),
        };
        let body = response
            .into_string()
            .map_err(|e| ApiError::Transport(e.to_string()))?;
        // Better Auth returns JSON `null` when there is no session.
        let session: Option<SessionResponse> = serde_json::from_str(&body)
            .map_err(|e| ApiError::Decode(format!("get-session: {e}")))?;
        Ok(session.and_then(|s| s.user))
    }

    /// `POST /api/mobile-oauth-exchange` — redeem an oauth-return PKCE code
    /// for the session token (REV-13). Unauthenticated; the code + verifier
    /// ARE the credentials. The server answers 400 `invalid_grant` for an
    /// unknown/expired/replayed code or a wrong verifier.
    pub fn exchange_oauth_code(
        &self,
        instance_url: &str,
        code: &str,
        code_verifier: &str,
    ) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct ExchangeResponse {
            token: Option<String>,
        }

        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "code": code, "code_verifier": code_verifier });
        let response = versioned(self.agent.post(&format!("{base}/api/mobile-oauth-exchange")))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string())
            .map_err(|e| match e {
                // invalid_grant — the code is single-use and short-TTL, so a
                // late/replayed callback lands here; not a transport problem.
                ureq::Error::Status(400, _) => {
                    ApiError::Decode("invalid or expired sign-in code".to_string())
                }
                other => from_ureq_unauthed(other),
            })?;
        let body = response
            .into_string()
            .map_err(|e| ApiError::Transport(e.to_string()))?;
        let parsed: ExchangeResponse = serde_json::from_str(&body)
            .map_err(|e| ApiError::Decode(format!("oauth-exchange response: {e}")))?;
        parsed
            .token
            .ok_or_else(|| ApiError::Decode("oauth-exchange returned no token".to_string()))
    }

    /// `POST /api/auth/sign-out` — best-effort server-side revocation. Local
    /// sign-out ([`crate::AuthStore::sign_out`]) must proceed even when this
    /// fails (offline sign-out is legal).
    pub fn sign_out(&self, instance_url: &str, token: &str) -> Result<(), ApiError> {
        let base = normalize_instance_url(instance_url);
        versioned(self.agent.post(&format!("{base}/api/auth/sign-out")))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .send_string("{}")
            .map_err(|e| match e {
                ureq::Error::Status(401, _) => ApiError::Unauthorized,
                other => from_ureq_unauthed(other),
            })?;
        Ok(())
    }
}

/// Normalize a user-typed instance URL (iOS `normalizeBaseUrl` parity): trim
/// whitespace, strip trailing slashes, default to `https://` when no scheme.
pub fn normalize_instance_url(input: &str) -> String {
    let mut trimmed = input.trim().to_string();
    while trimmed.ends_with('/') {
        trimmed.pop();
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        trimmed = format!("https://{trimmed}");
    }
    trimmed
}

// ---- OAuth via the system browser (§5.7) ----
//
// Flow: [`generate_pkce`] mints a verifier/challenge pair, then open the
// system browser at one of the start URLs below
// (crate::opener::open_in_browser) with the challenge attached. The server
// runs the OAuth dance and redirects to /api/mobile-oauth-return, which
// deep-links back as `exponential://oauth-return?code=…#code=…` — a
// single-use short-TTL code (REV-13; the raw session token never rides the
// deep link, so another app hijacking the xdg/HKCU scheme registration
// intercepts nothing usable). The app shell's on_open_urls channel (Phase 1,
// §3.6) delivers that URL to a foreground drain, which calls
// [`parse_oauth_callback`] and redeems the code via
// [`AuthClient::exchange_oauth_code`] with the held verifier. Legacy pre-PKCE
// servers (self-hosted lag) still deep-link `#token=<session-token>`; the
// parser surfaces both forms as [`OAuthCallback`].
//
// The v3-era loopback FALLBACK (an ephemeral 127.0.0.1 listener for
// environments where the scheme registration didn't take) was dropped: its
// server half (a `redirect=` param on /api/mobile-oauth-return) is not
// scheduled by any live plan, and unregistered dev builds degrade to the
// copyable-URL flow instead.

/// The custom URL scheme the app registers (macOS `CFBundleURLTypes`, Linux
/// `.desktop` `MimeType=x-scheme-handler/exponential;`, Windows
/// `HKCU\Software\Classes\exponential`) — the SINGLE source every functional
/// site derives from (EXP-41). Must match the packaging templates
/// (`assets/packaging/Info.plist`, `assets/packaging/exponential.desktop`,
/// `scripts/build-appimage.sh`) and the scheme the web server mints deep
/// links with.
pub const OAUTH_CALLBACK_SCHEME: &str = "exponential";

/// A PKCE verifier/challenge pair for one OAuth attempt (REV-13). The
/// verifier stays in memory (never persisted); the challenge rides the start
/// URL.
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

/// RFC 7636 §4.2: `challenge = base64url_no_pad(SHA-256(ASCII(verifier)))`.
pub fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_nopad(&digest)
}

/// Mint a fresh PKCE attempt. The verifier is two concatenated v4 UUIDs in
/// simple form — 64 hex chars, a valid RFC 7636 §4.1 charset/length (uuid is
/// already in the tree; no extra RNG dependency).
pub fn generate_pkce() -> PkcePair {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = pkce_challenge(&verifier);
    PkcePair {
        verifier,
        challenge,
    }
}

/// Browser start URL for Google sign-in (`provider=google` → Better Auth
/// `signInSocial`). `code_challenge` is base64url — URL-safe as-is.
pub fn google_oauth_start_url(instance_url: &str, code_challenge: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?provider=google&code_challenge={code_challenge}",
        normalize_instance_url(instance_url)
    )
}

/// Browser start URL for Apple sign-in (`provider=apple` → Better Auth
/// `signInSocial`).
pub fn apple_oauth_start_url(instance_url: &str, code_challenge: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?provider=apple&code_challenge={code_challenge}",
        normalize_instance_url(instance_url)
    )
}

/// Browser start URL for a generic OIDC provider (`providerId=…` → Better
/// Auth `signInWithOAuth2`).
pub fn oidc_oauth_start_url(instance_url: &str, provider_id: &str, code_challenge: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?providerId={}&code_challenge={code_challenge}",
        normalize_instance_url(instance_url),
        percent_encode(provider_id)
    )
}

/// What an OAuth callback URL carried (REV-13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OAuthCallback {
    /// A single-use PKCE code — redeem via [`AuthClient::exchange_oauth_code`]
    /// with the verifier held from [`generate_pkce`].
    Code(String),
    /// The raw session token (DEPRECATED legacy form — pre-PKCE servers).
    Token(String),
}

/// Extract the payload from an OAuth callback URL. Handles both capture
/// mechanisms of §5.7 — for each param the URL **fragment** wins over the
/// query (the fragment never leaves the client; the query survives the
/// browser→OS custom-scheme hop, EXP-21) — and both payload forms, `code`
/// (new PKCE flow) winning over `token` (legacy):
///
/// - PRIMARY custom scheme: `exponential://oauth-return?code=<c>#code=<c>`
///   (or legacy `…?token=<t>#token=<t>`).
/// - Query-only `…?token=<t>` URLs (no fragment) parse too — the wire shape
///   of the dropped v3 loopback fallback, kept for legacy tolerance.
///
/// Values are percent-decoded (the server `encodeURIComponent`s them; a PKCE
/// code is base64url and decode-inert).
pub fn parse_oauth_callback(url: &str) -> Option<OAuthCallback> {
    let fragment = url.split_once('#').map(|(_, fragment)| fragment);
    // Everything between '?' and '#'.
    let query = url
        .split('#')
        .next()
        .unwrap_or(url)
        .split_once('?')
        .map(|(_, query)| query);

    for key in ["code", "token"] {
        let value = fragment
            .and_then(|pairs| find_param(pairs, key))
            .or_else(|| query.and_then(|pairs| find_param(pairs, key)));
        if let Some(value) = value {
            return Some(match key {
                "code" => OAuthCallback::Code(value),
                _ => OAuthCallback::Token(value),
            });
        }
    }
    None
}

/// Find `key=` in a `k=v&k=v` pair list, percent-decoding the value.
fn find_param(pairs: &str, key: &str) -> Option<String> {
    for pair in pairs.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == key && !v.is_empty() {
            return Some(percent_decode(v));
        }
    }
    None
}

/// Parse a Better Auth session cookie out of one `Set-Cookie` header value
/// (matches the iOS regex `session_token=([^;]+)`: catches both
/// `better-auth.session_token` and `__Secure-better-auth.session_token`).
fn extract_session_token_cookie(set_cookie: &str) -> Option<String> {
    let start = set_cookie.find("session_token=")?;
    let value = &set_cookie[start + "session_token=".len()..];
    let value = value.split(';').next().unwrap_or(value);
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_instance_urls() {
        assert_eq!(
            normalize_instance_url("  app.exponential.at/ "),
            "https://app.exponential.at"
        );
        assert_eq!(
            normalize_instance_url("http://localhost:5173///"),
            "http://localhost:5173"
        );
        assert_eq!(
            normalize_instance_url("https://next.exponential.at"),
            "https://next.exponential.at"
        );
    }

    #[test]
    fn oauth_start_urls() {
        assert_eq!(
            google_oauth_start_url("app.exponential.at", "chal-1"),
            "https://app.exponential.at/api/mobile-oauth-start?provider=google&code_challenge=chal-1"
        );
        assert_eq!(
            apple_oauth_start_url("app.exponential.at", "chal-2"),
            "https://app.exponential.at/api/mobile-oauth-start?provider=apple&code_challenge=chal-2"
        );
        assert_eq!(
            oidc_oauth_start_url("https://app.exponential.at/", "authentik prod", "chal-3"),
            "https://app.exponential.at/api/mobile-oauth-start?providerId=authentik%20prod&code_challenge=chal-3"
        );
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_vector() {
        // RFC 7636 Appendix B — the same pair is asserted by the web, Android
        // and iOS tests so all four implementations provably agree.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_pkce_is_valid_and_consistent() {
        let pair = generate_pkce();
        // 64 hex chars — valid RFC 7636 §4.1 charset/length.
        assert_eq!(pair.verifier.len(), 64);
        assert!(pair.verifier.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(pair.challenge, pkce_challenge(&pair.verifier));
        assert_ne!(generate_pkce().verifier, pair.verifier);
    }

    #[test]
    fn parses_custom_scheme_code_callback() {
        // PRIMARY: single-use PKCE code, doubled into query + fragment.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?code=c-1#code=c-1"),
            Some(OAuthCallback::Code("c-1".to_string()))
        );
        // Fragment-dropped hop (Linux xdg): the query alone still parses.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?code=c-2"),
            Some(OAuthCallback::Code("c-2".to_string()))
        );
    }

    #[test]
    fn parses_custom_scheme_fragment_callback() {
        // LEGACY: token in the FRAGMENT, encodeURIComponent-encoded.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return#token=abc123%2Edef"),
            Some(OAuthCallback::Token("abc123.def".to_string()))
        );
    }

    #[test]
    fn parses_loopback_query_callback() {
        assert_eq!(
            parse_oauth_callback("http://127.0.0.1:49152/cb?token=tok-1&x=y"),
            Some(OAuthCallback::Token("tok-1".to_string()))
        );
        // Bare path form (a request-line-style path with query).
        assert_eq!(
            parse_oauth_callback("/cb?token=tok-2"),
            Some(OAuthCallback::Token("tok-2".to_string()))
        );
    }

    #[test]
    fn callback_without_payload_is_none() {
        assert_eq!(parse_oauth_callback("exponential://oauth-return"), None);
        assert_eq!(parse_oauth_callback("exponential://oauth-return#token="), None);
        assert_eq!(parse_oauth_callback("exponential://oauth-return?code="), None);
        assert_eq!(parse_oauth_callback("/favicon.ico"), None);
    }

    #[test]
    fn fragment_wins_over_query() {
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?token=query#token=frag"),
            Some(OAuthCallback::Token("frag".to_string()))
        );
    }

    #[test]
    fn code_wins_over_token() {
        // A mixed callback must never fall back to the raw-token path when a
        // redeemable code is present.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?code=c#token=t"),
            Some(OAuthCallback::Code("c".to_string()))
        );
    }

    #[test]
    fn extracts_session_token_from_set_cookie() {
        assert_eq!(
            extract_session_token_cookie(
                "__Secure-better-auth.session_token=abc.def; Path=/; HttpOnly; Secure"
            )
            .as_deref(),
            Some("abc.def")
        );
        assert_eq!(
            extract_session_token_cookie("better-auth.session_token=xyz").as_deref(),
            Some("xyz")
        );
        assert_eq!(extract_session_token_cookie("other=1; Path=/"), None);
    }

    #[test]
    fn auth_config_decodes_with_defaults() {
        // Full server shape.
        let full: AuthConfig = serde_json::from_str(
            r#"{"passwordEnabled":false,"passwordResetEnabled":false,
                "oidcProviders":[{"id":"authentik","name":"Authentik"}],
                "googleLoginEnabled":true,"appleLoginEnabled":true,"githubEnabled":true}"#,
        )
        .unwrap();
        assert!(!full.password_enabled);
        assert!(full.google_login_enabled);
        assert!(full.apple_login_enabled);
        assert_eq!(full.oidc_providers[0].id, "authentik");

        // Tolerant: unknown/missing fields degrade to defaults.
        let sparse: AuthConfig = serde_json::from_str(r#"{"futureField":1}"#).unwrap();
        assert!(sparse.password_enabled); // defaults true like iOS
        assert!(!sparse.apple_login_enabled);
        assert!(sparse.oidc_providers.is_empty());
    }
}
