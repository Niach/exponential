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
//! - OAuth via the system browser: start URLs for `/api/mobile-oauth-start`,
//!   plus the callback capture surfaces — the `exp://oauth-return#token=…`
//!   deep-link parser (PRIMARY; token in the URL *fragment*) and the
//!   `127.0.0.1` loopback listener (FALLBACK; token as `?token=` query).
//!
//! The login *view* (cloud button first) is §4/Phase-3 UI
//! territory; this module owns only the mechanics.

use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::encode::{percent_decode, percent_encode};
use crate::error::{from_ureq_unauthed, ApiError};

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
        let response = self
            .agent
            .get(&format!("{base}/api/auth-config"))
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
        let response = self
            .agent
            .post(&format!("{base}/api/auth/sign-in/email"))
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
        let result = self
            .agent
            .get(&format!("{base}/api/auth/get-session"))
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

    /// `POST /api/auth/sign-out` — best-effort server-side revocation. Local
    /// sign-out ([`crate::AuthStore::sign_out`]) must proceed even when this
    /// fails (offline sign-out is legal).
    pub fn sign_out(&self, instance_url: &str, token: &str) -> Result<(), ApiError> {
        let base = normalize_instance_url(instance_url);
        self.agent
            .post(&format!("{base}/api/auth/sign-out"))
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
// Flow: open the system browser at one of the start URLs below
// (crate::opener::open_in_browser). The server runs the OAuth dance and
// redirects to /api/mobile-oauth-return, which deep-links back as
// `exp://oauth-return#token=<session-token>`. The app shell's on_open_urls
// channel (Phase 1, §3.6) delivers that URL to a foreground drain, which
// calls [`parse_oauth_callback`] and then signs the account in.
//
// TODO(v3 §5.7 / Phase 3): the full Google/OIDC login flow (browser round
// trip wired into the login view) lands with the Phase-3 auth UI. The
// loopback FALLBACK additionally needs a NEW server-side `redirect=` param on
// /api/mobile-oauth-return (127.0.0.1-bound, single-use, short-lived token as
// `?token=` query) — a coordinated server change that has not landed yet;
// [`LoopbackListener`] below is the ready client half.

/// The custom URL scheme the app registers (macOS `CFBundleURLTypes`, Linux
/// `.desktop` `MimeType=x-scheme-handler/exp;`).
pub const OAUTH_CALLBACK_SCHEME: &str = "exp";

/// Browser start URL for Google sign-in (`provider=google` → Better Auth
/// `signInSocial`).
pub fn google_oauth_start_url(instance_url: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?provider=google",
        normalize_instance_url(instance_url)
    )
}

/// Browser start URL for Apple sign-in (`provider=apple` → Better Auth
/// `signInSocial`).
pub fn apple_oauth_start_url(instance_url: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?provider=apple",
        normalize_instance_url(instance_url)
    )
}

/// Browser start URL for a generic OIDC provider (`providerId=…` → Better
/// Auth `signInWithOAuth2`).
pub fn oidc_oauth_start_url(instance_url: &str, provider_id: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?providerId={}",
        normalize_instance_url(instance_url),
        percent_encode(provider_id)
    )
}

/// Extract the session token from an OAuth callback URL. Handles both
/// capture mechanisms of §5.7:
///
/// - PRIMARY custom scheme: `exp://oauth-return#token=<t>` — the token is in
///   the URL **fragment** (never sent to any server; parsed app-locally).
/// - FALLBACK loopback: `http://127.0.0.1:<port>/cb?token=<t>` — the token is
///   a **query** param (fragments are never sent to servers, so the loopback
///   listener could not see one).
///
/// The value is percent-decoded (the server `encodeURIComponent`s it).
pub fn parse_oauth_callback(url: &str) -> Option<String> {
    // Fragment first (primary form).
    if let Some((_, fragment)) = url.split_once('#') {
        if let Some(token) = find_param(fragment, "token") {
            return Some(token);
        }
    }
    // Query (loopback form) — everything between '?' and '#'.
    let without_fragment = url.split('#').next().unwrap_or(url);
    if let Some((_, query)) = without_fragment.split_once('?') {
        if let Some(token) = find_param(query, "token") {
            return Some(token);
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

// ---- Loopback fallback listener (§5.7) ----

/// Ephemeral `127.0.0.1` HTTP listener that captures ONE OAuth callback of
/// the form `GET /cb?token=…` — the fallback for environments where the
/// `exp://` scheme registration didn't take. Server-side support (the
/// `redirect=` param on `/api/mobile-oauth-return`) is a coordinated change
/// that hasn't landed yet; see the module TODO above.
pub struct LoopbackListener {
    port: u16,
    token_rx: flume::Receiver<String>,
    stop: Arc<AtomicBool>,
}

impl LoopbackListener {
    /// Bind `127.0.0.1:0` (ephemeral port) and start the accept thread.
    pub fn bind() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let (token_tx, token_rx) = flume::bounded::<String>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        std::thread::Builder::new()
            .name("oauth-loopback".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    match handle_loopback_connection(stream) {
                        Some(token) => {
                            let _ = token_tx.send(token);
                            break;
                        }
                        None => continue, // favicon probes etc. — keep listening
                    }
                }
            })?;

        Ok(Self {
            port,
            token_rx,
            stop,
        })
    }

    /// The bound ephemeral port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The redirect URI to hand the server once the `redirect=` param lands:
    /// `http://127.0.0.1:{port}/cb`.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/cb", self.port)
    }

    /// Block up to `timeout` for the callback token.
    pub fn recv_token(&self, timeout: Duration) -> Option<String> {
        self.token_rx.recv_timeout(timeout).ok()
    }
}

impl Drop for LoopbackListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the accept() so the thread observes the stop flag and exits.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// Read one HTTP request head; if it is `GET …?token=…`, answer with a tiny
/// "return to the app" page and yield the token. Anything else gets a 404.
fn handle_loopback_connection(stream: TcpStream) -> Option<String> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    // Drain the rest of the head so the client sees a clean HTTP exchange.
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    // "GET /cb?token=abc HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let token = parse_oauth_callback(path);

    let mut stream = reader.into_inner();
    let (status, body) = if token.is_some() {
        (
            "200 OK",
            "<html><body style=\"font-family:sans-serif\"><p>Signed in — you can return to Exponential.</p></body></html>",
        )
    } else {
        ("404 Not Found", "")
    };
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    token
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
            google_oauth_start_url("app.exponential.at"),
            "https://app.exponential.at/api/mobile-oauth-start?provider=google"
        );
        assert_eq!(
            apple_oauth_start_url("app.exponential.at"),
            "https://app.exponential.at/api/mobile-oauth-start?provider=apple"
        );
        assert_eq!(
            oidc_oauth_start_url("https://app.exponential.at/", "authentik prod"),
            "https://app.exponential.at/api/mobile-oauth-start?providerId=authentik%20prod"
        );
    }

    #[test]
    fn parses_exp_scheme_fragment_callback() {
        // PRIMARY: token in the FRAGMENT, encodeURIComponent-encoded.
        assert_eq!(
            parse_oauth_callback("exp://oauth-return#token=abc123%2Edef").as_deref(),
            Some("abc123.def")
        );
    }

    #[test]
    fn parses_loopback_query_callback() {
        assert_eq!(
            parse_oauth_callback("http://127.0.0.1:49152/cb?token=tok-1&x=y").as_deref(),
            Some("tok-1")
        );
        // Bare path form (what the listener sees on the request line).
        assert_eq!(
            parse_oauth_callback("/cb?token=tok-2").as_deref(),
            Some("tok-2")
        );
    }

    #[test]
    fn callback_without_token_is_none() {
        assert_eq!(parse_oauth_callback("exp://oauth-return"), None);
        assert_eq!(parse_oauth_callback("exp://oauth-return#token="), None);
        assert_eq!(parse_oauth_callback("/favicon.ico"), None);
    }

    #[test]
    fn fragment_wins_over_query() {
        assert_eq!(
            parse_oauth_callback("exp://oauth-return?token=query#token=frag").as_deref(),
            Some("frag")
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

    #[test]
    fn loopback_listener_captures_token() {
        let listener = LoopbackListener::bind().unwrap();
        let uri = listener.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:"));

        // Simulate the browser hitting the redirect target.
        let port = listener.port();
        std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .write_all(b"GET /cb?token=loop-tok HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .unwrap();
            // Read the response so the exchange completes.
            let mut buf = Vec::new();
            use std::io::Read;
            let _ = stream.read_to_end(&mut buf);
            assert!(String::from_utf8_lossy(&buf).contains("200 OK"));
        });

        let token = listener.recv_token(Duration::from_secs(5));
        assert_eq!(token.as_deref(), Some("loop-tok"));
    }
}
