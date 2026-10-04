//! Better Auth login mechanics (masterplan-v3 §5.7) — a straight port of the
//! proven iOS native flow (`ExpCore/Sources/API/AuthApi.swift` +
//! `HTTPClient.swift`):
//!
//! - `GET  /api/auth-config` — which methods the login view may show.
//! - `POST /api/auth/sign-in/email` — `{email, password}` → `{token, user}`
//!   (Better Auth bearer plugin; a `Set-Cookie` session fallback is parsed
//!   when the JSON token is absent, iOS-parity).
//! - EXP-857 passwordless one-time code:
//!   `POST /api/auth/email-otp/send-verification-otp` mails a 6-digit code
//!   (200 even for an unknown address — existence never leaks) and
//!   `POST /api/auth/sign-in/email-otp` redeems it for the same
//!   `{token, user}` a password sign-in returns.
//! - `GET  /api/auth/get-session` — bearer session validation.
//! - `POST /api/auth/sign-out` — best-effort server-side revocation.
//! - OAuth via the system browser: start URLs for `/api/mobile-oauth-start`
//!   (carrying a PKCE S256 `code_challenge`, REV-13), plus the callback
//!   capture surfaces — the `exponential://oauth-return?code=…#code=…`
//!   deep-link parser (PRIMARY; a single-use code redeemed via
//!   `POST /api/mobile-oauth-exchange` with the in-memory verifier — legacy
//!   pre-PKCE servers still send `#token=…` with the raw session token, and a
//!   failed hop comes back as `?error=…#error=…`, REV2-53).
//!
//! The login *view* (cloud button first) is §4/Phase-3 UI
//! territory; this module owns only the mechanics.

use domain::client_version::{client_version_header_value, CLIENT_VERSION_HEADER};
use serde::Deserialize;

use crate::encode::{base64url_nopad, percent_decode, percent_encode};
use crate::error::{read_body, status_error_unauthed, transport_error, ApiError};
use crate::http;

/// Tag a request with the client-version header (EXP-104) so the server can
/// 426-gate stale builds — applied to every `AuthClient` request, including
/// the unauthenticated auth-config / sign-in / oauth-exchange calls, for
/// uniformity. The server does NOT gate auth routes (only tRPC and shape
/// requests answer 426), so the blocking update screen latches once sync
/// starts, not at login.
fn versioned(request: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
    request.header(CLIENT_VERSION_HEADER, client_version_header_value())
}

/// A completed auth request, split the way reqwest hands it over: the status
/// separate from the body. Every caller below needs both, and reqwest returns
/// `Ok` for non-2xx (unlike ureq), so the status check is always explicit.
struct AuthResponse {
    status: u16,
    body: String,
    /// `Set-Cookie` values, kept for the sign-in token fallback.
    cookies: Vec<String>,
}

impl AuthResponse {
    /// Fail with the UNAUTHENTICATED status policy unless the status is 2xx.
    fn ok_or_status(self) -> Result<Self, ApiError> {
        if (200..300).contains(&self.status) {
            Ok(self)
        } else {
            Err(status_error_unauthed(self.status, &self.body))
        }
    }
}

fn send(request: reqwest::blocking::RequestBuilder) -> Result<AuthResponse, ApiError> {
    let response = request
        .timeout(http::DEFAULT_TIMEOUT)
        .send()
        .map_err(transport_error)?;
    let status = response.status().as_u16();
    let cookies = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect();
    let body = read_body(response)?;
    Ok(AuthResponse {
        status,
        body,
        cookies,
    })
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
    /// RFC 8628 device-code login (EXP-403). Defaults FALSE: an older
    /// self-hosted instance without the field lacks the endpoints — the CLI
    /// then falls back to password login.
    #[serde(default)]
    pub device_flow_enabled: bool,
    /// EXP-857: passwordless email one-time-code login (Better Auth
    /// `email-otp`) is offered — i.e. the instance has a mail transport.
    /// Defaults FALSE: an older server without the field has no
    /// `/api/auth/email-otp/*` routes.
    #[serde(default)]
    pub email_otp_enabled: bool,
    /// EXP-857: passkey (WebAuthn) login is offered. Defaults FALSE for the
    /// same reason.
    #[serde(default)]
    pub passkey_enabled: bool,
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
    // EXP-589: the EXP-470 `gettingStartedDismissedAt` additionalField is
    // gone server-side; this struct never denies unknown fields, so an older
    // server still sending it parses fine.
}

/// Successful password sign-in: the portable session token + the user.
#[derive(Clone, Debug)]
pub struct SignInSuccess {
    pub token: String,
    pub user: AuthUser,
}

/// The OAuth `client_id` the CLI presents on the device-code grant
/// (EXP-403). The server does not restrict client ids today; the value
/// exists for auditability and future `validateClient` policy.
pub const DEVICE_CLIENT_ID: &str = "exponential-cli";

/// `POST /api/auth/device/code` response (RFC 8628 §3.2, snake_case wire).
#[derive(Clone, Debug, Deserialize)]
pub struct DeviceCodeGrant {
    pub device_code: String,
    /// What the user types on `/auth/device` (charset excludes 0/1/I/O).
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Seconds until the codes expire.
    pub expires_in: u64,
    /// Minimum poll spacing in seconds — polling faster answers `slow_down`.
    #[serde(default = "default_device_interval")]
    pub interval: u64,
}

fn default_device_interval() -> u64 {
    5
}

/// One `POST /api/auth/device/token` poll outcome.
#[derive(Clone, Debug)]
pub enum DevicePoll {
    /// Approved: `token` is a regular Better Auth session token.
    Authorized { token: String },
    /// Still waiting for the user — poll again after `interval`.
    Pending,
    /// Polled too fast — back off (the server does not bump the interval).
    SlowDown,
    /// The codes expired — restart the flow.
    Expired,
    /// The user denied the request.
    Denied,
}

/// What the server said to one approver-side device-code call (EXP-1169:
/// the server card's code field). A refusal is a RESULT, never an `Err`;
/// only transport failures error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceCodeAnswer {
    /// 2xx. `status` is the code's state on the claim GET
    /// (`pending`/`approved`/`denied`), `None` on the approve POST.
    Accepted { status: Option<String> },
    /// Non-2xx: the body's RFC 8628 `error` (empty when there was none).
    Refused { error: String },
}

impl DeviceCodeAnswer {
    fn from_response(response: AuthResponse) -> Self {
        #[derive(Deserialize)]
        struct Body {
            #[serde(default)]
            status: Option<String>,
            #[serde(default)]
            error: Option<String>,
        }
        let body: Option<Body> = serde_json::from_str(&response.body).ok();
        if (200..300).contains(&response.status) {
            DeviceCodeAnswer::Accepted {
                status: body.and_then(|body| body.status),
            }
        } else {
            DeviceCodeAnswer::Refused {
                error: body.and_then(|body| body.error).unwrap_or_default(),
            }
        }
    }
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
pub struct AuthClient {}

impl Default for AuthClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthClient {
    pub fn new() -> Self {
        Self {}
    }

    /// The process-wide client (EXP-304), read per request so a rebuilt one
    /// (FEED-69) is picked up: 30s overall — parity with the iOS URLSession
    /// config — and the same connection pool as everything else. Never used
    /// for long-polls (sync overrides the per-request budget to 90s, §5.3).
    fn client(&self) -> reqwest::blocking::Client {
        http::shared()
    }

    /// `GET /api/auth-config` — unauthenticated; call before any account exists.
    pub fn fetch_auth_config(&self, instance_url: &str) -> Result<AuthConfig, ApiError> {
        let base = normalize_instance_url(instance_url);
        let response = send(
            versioned(self.client().get(format!("{base}/api/auth-config")))
                .header("Accept", "application/json")
                // EXP-418: the login card shows a spinner until this settles
                // — an offline first start must fall back to the password
                // form after seconds, not the client-wide 30s.
                .timeout(std::time::Duration::from_secs(8)),
        )?
        .ok_or_status()?;
        serde_json::from_str(&response.body)
            .map_err(|e| ApiError::Decode(format!("auth-config: {e}")))
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
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/sign-in/email")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                // Better Auth's CSRF check 403s POSTs without an Origin header
                // (MISSING_OR_NULL_ORIGIN); send the instance's own origin like
                // a same-origin browser request would.
                .header("Origin", &base)
                .body(payload.to_string()),
        )?
        .ok_or_status()?;

        parse_sign_in_success(response)
    }

    /// EXP-857 `POST /api/auth/email-otp/send-verification-otp` — mail a
    /// 6-digit sign-in code (Better Auth `email-otp`, valid 10 minutes).
    /// Answers 200 even for an unknown address (never leaks existence), so a
    /// success here only means "the request was accepted".
    pub fn send_sign_in_code(&self, instance_url: &str, email: &str) -> Result<(), ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "email": email, "type": "sign-in" });
        let response = send(
            versioned(
                self.client()
                    .post(format!("{base}/api/auth/email-otp/send-verification-otp")),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            // Better Auth's CSRF check 403s POSTs without an Origin header.
            .header("Origin", &base)
            .body(payload.to_string()),
        )?;
        if !(200..300).contains(&response.status) {
            return Err(otp_response_error(response.status, &response.body));
        }
        Ok(())
    }

    /// EXP-857 `POST /api/auth/sign-in/email-otp` — redeem the mailed code for
    /// a session. Same `{token, user}` shape (and the same `Set-Cookie`
    /// fallback) as the password path; a wrong/expired/burnt code comes back
    /// as `ApiError::Http` carrying the contract's copy (see
    /// [`otp_error_message`]).
    ///
    /// EXP-1026: the request opts in to the name step (`X-Exp-Ask-Name: 1`).
    /// A NEW address sent without a `name` answers 400 `NAME_REQUIRED` with
    /// the code left intact — see [`is_name_required`] — and the caller
    /// resubmits the SAME code with the typed `name`.
    pub fn sign_in_with_email_code(
        &self,
        instance_url: &str,
        email: &str,
        code: &str,
        name: Option<&str>,
    ) -> Result<SignInSuccess, ApiError> {
        let base = normalize_instance_url(instance_url);
        let mut payload = serde_json::json!({ "email": email, "otp": code });
        if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
            payload["name"] = serde_json::Value::String(name.to_string());
        }
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/sign-in/email-otp")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Origin", &base)
                .header(ASK_NAME_HEADER, "1")
                .body(payload.to_string()),
        )?;
        if !(200..300).contains(&response.status) {
            return Err(otp_response_error(response.status, &response.body));
        }
        parse_sign_in_success(response)
    }

    /// EXP-1126 `POST /api/auth/email-otp/request-email-change` — mail a
    /// 6-digit code to `new_email` for the SIGNED-IN account (bearer + the
    /// Origin Better Auth's CSRF check wants). Failures carry the contract's
    /// copy ([`otp_error_message`]: `INVALID_EMAIL`, or the server's own
    /// "Email is the same").
    pub fn request_email_change(
        &self,
        instance_url: &str,
        token: &str,
        new_email: &str,
    ) -> Result<(), ApiError> {
        let payload = serde_json::json!({ "newEmail": new_email });
        self.post_authed_otp(
            instance_url,
            token,
            "/api/auth/email-otp/request-email-change",
            payload,
        )
    }

    /// EXP-1126 `POST /api/auth/email-otp/change-email` — redeem the mailed
    /// code: the users row takes `new_email` (verified). The caller re-reads
    /// the session ([`Self::fetch_session`]) to pick up the new address.
    pub fn change_email(
        &self,
        instance_url: &str,
        token: &str,
        new_email: &str,
        otp: &str,
    ) -> Result<(), ApiError> {
        let payload = serde_json::json!({ "newEmail": new_email, "otp": otp });
        self.post_authed_otp(
            instance_url,
            token,
            "/api/auth/email-otp/change-email",
            payload,
        )
    }

    /// A bearer-authenticated Better Auth email-otp POST whose failures map
    /// through [`otp_response_error`].
    fn post_authed_otp(
        &self,
        instance_url: &str,
        token: &str,
        path: &str,
        payload: serde_json::Value,
    ) -> Result<(), ApiError> {
        let base = normalize_instance_url(instance_url);
        let response = send(
            versioned(self.client().post(format!("{base}{path}")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                // Better Auth's CSRF check 403s POSTs without an Origin header.
                .header("Origin", &base)
                .body(payload.to_string()),
        )?;
        if !(200..300).contains(&response.status) {
            return Err(otp_response_error(response.status, &response.body));
        }
        Ok(())
    }

    /// `POST /api/auth/device/code` — start the RFC 8628 device-code grant
    /// (EXP-403 CLI login). Unauthenticated. The caller shows
    /// `verification_uri` + `user_code`, then polls [`Self::poll_device_token`]
    /// every `interval` seconds until the user approves on `/auth/device`.
    pub fn request_device_code(&self, instance_url: &str) -> Result<DeviceCodeGrant, ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "client_id": DEVICE_CLIENT_ID });
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/device/code")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Origin", &base)
                .body(payload.to_string()),
        )?
        .ok_or_status()?;
        serde_json::from_str(&response.body)
            .map_err(|e| ApiError::Decode(format!("device/code: {e}")))
    }

    /// `POST /api/auth/device/token` — one poll of the device-code grant.
    /// The RFC error vocabulary (`authorization_pending` / `slow_down` /
    /// `expired_token` / `access_denied`) comes back as HTTP 400 and is a
    /// RESULT here, never an `Err`; only transport/decode/unexpected-status
    /// failures error. On approval the `access_token` is a regular Better
    /// Auth session token (usable exactly like a password sign-in's).
    pub fn poll_device_token(
        &self,
        instance_url: &str,
        device_code: &str,
    ) -> Result<DevicePoll, ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            "device_code": device_code,
            "client_id": DEVICE_CLIENT_ID,
        });
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/device/token")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Origin", &base)
                .body(payload.to_string()),
        )?;
        if response.status == 400 {
            #[derive(Deserialize)]
            struct OauthError {
                #[serde(default)]
                error: String,
            }
            let parsed: OauthError = serde_json::from_str(&response.body)
                .map_err(|e| ApiError::Decode(format!("device/token error body: {e}")))?;
            return Ok(match parsed.error.as_str() {
                "authorization_pending" => DevicePoll::Pending,
                "slow_down" => DevicePoll::SlowDown,
                "expired_token" => DevicePoll::Expired,
                "access_denied" => DevicePoll::Denied,
                other => {
                    return Err(ApiError::Http {
                        status: 400,
                        message: format!("device/token: {other}"),
                    })
                }
            });
        }
        let response = response.ok_or_status()?;
        #[derive(Deserialize)]
        struct TokenBody {
            access_token: String,
        }
        let parsed: TokenBody = serde_json::from_str(&response.body)
            .map_err(|e| ApiError::Decode(format!("device/token: {e}")))?;
        Ok(DevicePoll::Authorized {
            token: parsed.access_token,
        })
    }

    /// EXP-1169, the APPROVER side of the grant above:
    /// `GET /api/auth/device?user_code=…` with the signed-in bearer claims a
    /// code the CLI printed for this session (approve refuses unclaimed
    /// codes) and reports its `status`. The web `/auth/device` page's first
    /// call.
    pub fn claim_device_code(
        &self,
        instance_url: &str,
        token: &str,
        user_code: &str,
    ) -> Result<DeviceCodeAnswer, ApiError> {
        let base = normalize_instance_url(instance_url);
        let response = send(
            versioned(self.client().get(format!(
                "{base}/api/auth/device?user_code={}",
                percent_encode(user_code)
            )))
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {token}"))
            .header("Origin", &base),
        )?;
        Ok(DeviceCodeAnswer::from_response(response))
    }

    /// EXP-1169: `POST /api/auth/device/approve` `{userCode}` with the
    /// signed-in bearer approves a claimed code; the CLI's next
    /// [`Self::poll_device_token`] then gets its session token.
    pub fn approve_device_code(
        &self,
        instance_url: &str,
        token: &str,
        user_code: &str,
    ) -> Result<DeviceCodeAnswer, ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "userCode": user_code });
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/device/approve")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                // Better Auth's CSRF check 403s POSTs without an Origin header.
                .header("Origin", &base)
                .body(payload.to_string()),
        )?;
        Ok(DeviceCodeAnswer::from_response(response))
    }

    /// EXP-1111: `POST /api/cli/install-token/redeem` — trade the web "Add
    /// device" dialog's one-time `expi_…` token (15-min TTL, single use,
    /// scoped to the user who minted it) for a Better Auth session token: the
    /// SAME kind [`Self::poll_device_token`] yields on approval, so the
    /// caller persists it exactly like a device-code login. Unauthenticated.
    /// `400 {"error":"invalid_token"}` (unknown, used or expired) comes back
    /// as [`ApiError::Http`] with a sentence that says what to do.
    pub fn redeem_install_token(&self, instance_url: &str, token: &str) -> Result<String, ApiError> {
        let base = normalize_instance_url(instance_url);
        let payload = serde_json::json!({ "token": token.trim() });
        let response = send(
            versioned(self.client().post(format!("{base}/api/cli/install-token/redeem")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Origin", &base)
                .body(payload.to_string()),
        )?;
        if response.status == 400 {
            #[derive(Deserialize)]
            struct RedeemError {
                #[serde(default)]
                error: String,
            }
            let code = serde_json::from_str::<RedeemError>(&response.body)
                .map(|parsed| parsed.error)
                .unwrap_or_default();
            return Err(ApiError::Http {
                status: 400,
                message: install_token_error_message(&code),
            });
        }
        let response = response.ok_or_status()?;
        #[derive(Deserialize)]
        struct TokenBody {
            access_token: String,
        }
        let parsed: TokenBody = serde_json::from_str(&response.body)
            .map_err(|e| ApiError::Decode(format!("install-token/redeem: {e}")))?;
        Ok(parsed.access_token)
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
        let response = send(
            versioned(self.client().get(format!("{base}/api/auth/get-session")))
                .header("Accept", "application/json")
                .header("Authorization", format!("Bearer {token}")),
        )?;
        // A bearer that fails to resolve is an explicit 401 on some Better Auth
        // configs — that IS the dead-session answer, not an error.
        if response.status == 401 {
            return Ok(None);
        }
        let response = response.ok_or_status()?;
        // Better Auth returns JSON `null` when there is no session.
        let session: Option<SessionResponse> = serde_json::from_str(&response.body)
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
        let response = send(
            versioned(self.client().post(format!("{base}/api/mobile-oauth-exchange")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .body(payload.to_string()),
        )?;
        // invalid_grant — the code is single-use and short-TTL, so a
        // late/replayed callback lands here; not a transport problem.
        if response.status == 400 {
            return Err(ApiError::Decode(
                "invalid or expired sign-in code".to_string(),
            ));
        }
        let response = response.ok_or_status()?;
        let parsed: ExchangeResponse = serde_json::from_str(&response.body)
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
        let response = send(
            versioned(self.client().post(format!("{base}/api/auth/sign-out")))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {token}"))
                .body("{}"),
        )?;
        if response.status == 401 {
            return Err(ApiError::Unauthorized);
        }
        response.ok_or_status()?;
        Ok(())
    }
}

/// EXP-1111: the sentence for a refused install-token redeem (pure, tested).
pub fn install_token_error_message(code: &str) -> String {
    match code {
        "invalid_token" => "The install token is invalid, already used or expired (they last 15 minutes). \
             Create a new one with Add device in the web app and run the command again."
            .to_string(),
        "" => "The server refused the install token.".to_string(),
        other => format!("The server refused the install token ({other})."),
    }
}

/// The shared `{token, user}` reader behind every sign-in endpoint
/// (`/sign-in/email`, `/sign-in/email-otp`): the bearer plugin's JSON token
/// wins, and when it is absent the session token is lifted out of
/// `Set-Cookie` (iOS-parity).
fn parse_sign_in_success(response: AuthResponse) -> Result<SignInSuccess, ApiError> {
    let cookies = response.cookies;
    let parsed: SignInResponseBody = serde_json::from_str(&response.body)
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
                    ApiError::Decode("sign-in succeeded but no session token returned".to_string())
                })?;
            Ok(SignInSuccess { token, user })
        }
        _ => Err(ApiError::Decode(
            "sign-in succeeded but no user returned".to_string(),
        )),
    }
}

/// EXP-1026: the header an email-code sign-in sends to opt in to the name
/// step (web `lib/auth/sign-up-copy.ts` `ASK_NAME_HEADER`).
pub const ASK_NAME_HEADER: &str = "X-Exp-Ask-Name";
/// EXP-1026: the Better Auth error `code` a name-less first sign-in answers.
pub const NAME_REQUIRED_CODE: &str = "NAME_REQUIRED";
/// The resolved message of a `NAME_REQUIRED` refusal — the marker
/// [`is_name_required`] reads (never shown: the caller shows the name step).
pub const NAME_REQUIRED_MESSAGE: &str = "A name is required to create this account.";

/// EXP-1026: whether a [`AuthClient::sign_in_with_email_code`] refusal means
/// "new account, ask for a name" (the code was NOT consumed).
pub fn is_name_required(err: &ApiError) -> bool {
    matches!(err, ApiError::Http { status: 400, message } if message == NAME_REQUIRED_MESSAGE)
}

/// A failed one-time-code request as an [`ApiError`], with the message already
/// resolved to the contract's copy. 426 stays the EXP-104 upgrade gate — it is
/// not an OTP failure.
fn otp_response_error(status: u16, body: &str) -> ApiError {
    if status == 426 {
        return ApiError::UpgradeRequired;
    }
    ApiError::Http {
        status,
        message: otp_error_message(status, body),
    }
}

/// EXP-857: the user-facing sentence for a failed one-time-code call. Pure so
/// the mapping is testable without a server. Better Auth answers
/// `{"code": "...", "message": "..."}`; the three code-specific sentences are
/// the contract's preferred copy, anything else falls back to the server's own
/// message, then to a generic line.
pub fn otp_error_message(status: u16, body: &str) -> String {
    #[derive(Deserialize)]
    struct OtpError {
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        message: Option<String>,
    }

    let parsed = serde_json::from_str::<OtpError>(body).ok();
    match parsed.as_ref().and_then(|e| e.code.as_deref()) {
        Some(NAME_REQUIRED_CODE) => return NAME_REQUIRED_MESSAGE.to_string(),
        Some("INVALID_OTP") => {
            return "That code is not right. Check the email and try again.".to_string()
        }
        Some("OTP_EXPIRED") => return "That code expired. Request a new one.".to_string(),
        Some("TOO_MANY_ATTEMPTS") => {
            return "Too many attempts. Request a new code.".to_string()
        }
        Some("INVALID_EMAIL") => return "Enter a valid email address.".to_string(),
        _ => {}
    }
    if let Some(message) = parsed
        .and_then(|e| e.message)
        .filter(|message| !message.trim().is_empty())
    {
        return message;
    }
    if status == 429 {
        return "Too many requests. Wait a minute and try again.".to_string();
    }
    format!("Something went wrong (HTTP {status}). Try again.")
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

/// EXP-857 browser handoff (`provider=browser`): the server sets its state
/// cookie and redirects to the web login page, where the user may finish with
/// ANY method (passkey, code, Google, Apple). The hop ends on the same
/// `/api/mobile-oauth-return` deep link as the social providers, so the whole
/// PKCE + callback + exchange path is reused unchanged. This is how the
/// desktop signs in with a passkey (no native WebAuthn ceremony).
pub fn browser_login_start_url(instance_url: &str, code_challenge: &str) -> String {
    format!(
        "{}/api/mobile-oauth-start?provider=browser&code_challenge={code_challenge}",
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

/// EXP-1126 browser handoff in LINK mode: attach `provider` (`google`,
/// `apple` or an OIDC id) to the signed-in account named by `ticket`
/// (`users.mintSignInLinkTicket`). Social providers ride `provider=`, OIDC
/// ones `providerId=`, exactly like the sign-in start URLs. The route still
/// requires a PKCE challenge, but link mode ends on
/// `exponential://oauth-return?linked=<id>` (no code to redeem).
pub fn link_start_url(
    instance_url: &str,
    ticket: &str,
    provider: &str,
    code_challenge: &str,
) -> String {
    let base = normalize_instance_url(instance_url);
    let ticket = percent_encode(ticket);
    match provider {
        // The social providers go by `provider=`; SLOP-7 made GitHub one of
        // them (the GitHub App's OAuth client), so connecting GitHub for
        // repositories is this same link hop.
        "google" | "apple" | "github" => format!(
            "{base}/api/mobile-oauth-start?link={ticket}&provider={provider}&code_challenge={code_challenge}"
        ),
        _ => format!(
            "{base}/api/mobile-oauth-start?link={ticket}&providerId={}&code_challenge={code_challenge}",
            percent_encode(provider)
        ),
    }
}

/// EXP-1126: human copy for a failed LINK hop's `error` slug — byte-equal to
/// the web `oauthLinkErrorMessage` (apps/web/src/lib/deep-link.ts), so a
/// reason reads the same on every client; an unknown slug gets the generic
/// line, never the raw value.
pub fn link_error_message(reason: &str) -> &'static str {
    match reason {
        "access_denied" => "Linking was cancelled.",
        "link_ticket_invalid" | "state_missing" | "state_invalid" | "state_mismatch"
        | "state_not_found" | "please_restart_the_process" => {
            "That link request expired. Please try again."
        }
        "account_already_linked_to_different_user" => {
            "That account is already linked to a different user."
        }
        _ => "Couldn't link that account. Please try again.",
    }
}

/// What an OAuth callback URL carried (REV-13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OAuthCallback {
    /// A single-use PKCE code — redeem via [`AuthClient::exchange_oauth_code`]
    /// with the verifier held from [`generate_pkce`].
    Code(String),
    /// The raw session token (DEPRECATED legacy form — pre-PKCE servers).
    Token(String),
    /// The FAILURE handoff (REV2-53):
    /// `exponential://oauth-return?error=<reason>#error=<reason>`. Every
    /// failing branch of the web hop deep-links back this way instead of
    /// stranding the user on an https page the app never sees, so the login
    /// surface can end the attempt. The payload is a short slug the server
    /// clamps (`normalizeOauthErrorReason`, apps/web/src/lib/deep-link.ts) —
    /// map it to human copy, never render it raw.
    Error(String),
    /// EXP-1126 LINK-mode success:
    /// `exponential://oauth-return?linked=<providerId>#linked=<providerId>` —
    /// a provider was attached to the SIGNED-IN account. Carries no
    /// credential; it only tells the settings surface to refresh.
    Linked(String),
}

/// Extract the payload from an OAuth callback URL. Handles both capture
/// mechanisms of §5.7 — for each param the URL **fragment** wins over the
/// query (the fragment never leaves the client; the query survives the
/// browser→OS custom-scheme hop, EXP-21) — and all three payload forms,
/// `error` (the REV2-53 failure handoff) winning over `linked` (the EXP-1126
/// link-mode success) winning over `code` (new PKCE flow) winning over
/// `token` (legacy):
///
/// - PRIMARY custom scheme: `exponential://oauth-return?code=<c>#code=<c>`
///   (or legacy `…?token=<t>#token=<t>`).
/// - FAILURE: `exponential://oauth-return?error=<reason>#error=<reason>`.
/// - Query-only `…?token=<t>` URLs (no fragment) parse too — the wire shape
///   of the dropped v3 loopback fallback, kept for legacy tolerance.
///
/// `error` is scanned first so a callback that somehow carries both never
/// tries to redeem a payload the server already declared failed — the same
/// precedence iOS (`LoginViewModel`) and Android (`handleOauthReturn`) apply.
///
/// Values are percent-decoded (the server `encodeURIComponent`s them; a PKCE
/// code is base64url and decode-inert).
pub fn parse_oauth_callback(url: &str) -> Option<OAuthCallback> {
    // EXP-368: on our own scheme only the `oauth-return` host is an OAuth
    // callback — other exponential:// deep links (github-connected, invite,
    // issue) carry query params this scanner would otherwise mis-adopt.
    // Non-scheme URLs (the legacy 127.0.0.1 loopback wire shape above) stay
    // host-agnostic.
    if let Some(rest) = url.strip_prefix(&format!("{OAUTH_CALLBACK_SCHEME}://")) {
        // EXACT host match (same shape as `parse_github_connected_deep_link`):
        // only the bare host, `/`, `?query` or `#fragment` may follow, so a
        // different host sharing the prefix (`oauth-returnish?code=…`) is not
        // an OAuth callback.
        let Some(rest) = rest.strip_prefix("oauth-return") else {
            return None;
        };
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if !(rest.is_empty() || rest.starts_with('?') || rest.starts_with('#')) {
            return None;
        }
    }
    let fragment = url.split_once('#').map(|(_, fragment)| fragment);
    // Everything between '?' and '#'.
    let query = url
        .split('#')
        .next()
        .unwrap_or(url)
        .split_once('?')
        .map(|(_, query)| query);

    for key in ["error", "linked", "code", "token"] {
        let value = fragment
            .and_then(|pairs| find_param(pairs, key))
            .or_else(|| query.and_then(|pairs| find_param(pairs, key)));
        if let Some(value) = value {
            return Some(match key {
                "error" => OAuthCallback::Error(value),
                "linked" => OAuthCallback::Linked(value),
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
        // EXP-857: the passkey / any-method browser handoff.
        assert_eq!(
            browser_login_start_url("app.exponential.at", "chal-4"),
            "https://app.exponential.at/api/mobile-oauth-start?provider=browser&code_challenge=chal-4"
        );
        assert_eq!(
            browser_login_start_url("http://localhost:3000/", "chal-5"),
            "http://localhost:3000/api/mobile-oauth-start?provider=browser&code_challenge=chal-5"
        );
    }

    #[test]
    fn otp_error_messages_follow_the_contract() {
        // The three code-specific sentences win over the server's own text.
        assert_eq!(
            otp_error_message(400, r#"{"code":"INVALID_OTP","message":"Invalid OTP"}"#),
            "That code is not right. Check the email and try again."
        );
        assert_eq!(
            otp_error_message(400, r#"{"code":"OTP_EXPIRED","message":"OTP expired"}"#),
            "That code expired. Request a new one."
        );
        assert_eq!(
            otp_error_message(400, r#"{"code":"TOO_MANY_ATTEMPTS","message":"nope"}"#),
            "Too many attempts. Request a new code."
        );
        // An unknown code falls back to the server's message…
        assert_eq!(
            otp_error_message(400, r#"{"code":"SOMETHING_NEW","message":"Something new"}"#),
            "Something new"
        );
        // …a body without one to a generic line (429 says what to do).
        assert_eq!(
            otp_error_message(429, "{}"),
            "Too many requests. Wait a minute and try again."
        );
        assert_eq!(
            otp_error_message(500, "<html>502 Bad Gateway</html>"),
            "Something went wrong (HTTP 500). Try again."
        );
        assert_eq!(
            otp_error_message(400, r#"{"code":"X","message":"   "}"#),
            "Something went wrong (HTTP 400). Try again."
        );
    }

    /// EXP-1026: `NAME_REQUIRED` is recognisable, every other refusal is not.
    #[test]
    fn name_required_refusal_is_recognised() {
        let err = otp_response_error(
            400,
            r#"{"code":"NAME_REQUIRED","message":"A name is required to create this account."}"#,
        );
        assert!(is_name_required(&err));
        assert!(!is_name_required(&otp_response_error(400, r#"{"code":"INVALID_OTP"}"#)));
    }

    /// EXP-1026: the sign-in opts in with the header and carries the name.
    #[test]
    fn email_code_sign_in_sends_the_ask_name_header_and_name() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) = one_shot_server(
            400,
            r#"{"code":"NAME_REQUIRED","message":"A name is required to create this account."}"#,
        );
        let err = AuthClient::new()
            .sign_in_with_email_code(&base, "new@example.com", "123456", Some(" Ada "))
            .unwrap_err();
        assert!(is_name_required(&err));
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(request.to_ascii_lowercase().contains("x-exp-ask-name: 1"), "{request}");
        assert!(request.contains(r#""name":"Ada""#), "{request}");
    }

    #[test]
    fn email_change_sends_bearer_origin_and_body() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) = one_shot_server(200, r#"{"success":true}"#);
        AuthClient::new()
            .change_email(&base, "tok-1", "new@example.com", "123456")
            .unwrap();
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with("POST /api/auth/email-otp/change-email HTTP/1.1"),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer tok-1"), "{request}");
        assert!(lower.contains(&format!("origin: {base}")), "{request}");
        assert!(
            request.ends_with(r#"{"newEmail":"new@example.com","otp":"123456"}"#),
            "{request}"
        );
    }

    #[test]
    fn device_code_claim_sends_bearer_and_reads_the_status() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) =
            one_shot_server(200, r#"{"user_code":"ZP3H-V7HK","status":"pending"}"#);
        let answer = AuthClient::new()
            .claim_device_code(&base, "tok-1", "ZP3H-V7HK")
            .unwrap();
        assert_eq!(
            answer,
            DeviceCodeAnswer::Accepted { status: Some("pending".to_string()) }
        );
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with("GET /api/auth/device?user_code=ZP3H-V7HK HTTP/1.1"),
            "{request}"
        );
        assert!(request.to_ascii_lowercase().contains("authorization: bearer tok-1"), "{request}");
    }

    #[test]
    fn device_code_approve_posts_the_code_and_reads_refusals() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) = one_shot_server(
            400,
            r#"{"error":"expired_token","error_description":"Device code has expired"}"#,
        );
        let answer = AuthClient::new()
            .approve_device_code(&base, "tok-1", "ZP3H-V7HK")
            .unwrap();
        assert_eq!(
            answer,
            DeviceCodeAnswer::Refused { error: "expired_token".to_string() }
        );
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/auth/device/approve HTTP/1.1"), "{request}");
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer tok-1"), "{request}");
        assert!(lower.contains(&format!("origin: {base}")), "{request}");
        assert!(request.ends_with(r#"{"userCode":"ZP3H-V7HK"}"#), "{request}");
    }

    #[test]
    fn email_change_request_maps_refusals_to_copy() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) =
            one_shot_server(400, r#"{"code":"INVALID_EMAIL","message":"Invalid email"}"#);
        let err = AuthClient::new()
            .request_email_change(&base, "tok-1", "nope")
            .unwrap_err();
        assert_eq!(err.user_message(), "Enter a valid email address.");
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(request
            .starts_with("POST /api/auth/email-otp/request-email-change HTTP/1.1"));
        assert!(request.ends_with(r#"{"newEmail":"nope"}"#));
    }

    #[test]
    fn otp_response_error_keeps_the_upgrade_gate() {
        // EXP-104: a 426 on the OTP routes is the stale-build gate, never an
        // OTP failure (it must not read as "that code is wrong").
        assert!(matches!(
            otp_response_error(426, "{}"),
            ApiError::UpgradeRequired
        ));
        match otp_response_error(400, r#"{"code":"OTP_EXPIRED"}"#) {
            ApiError::Http { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "That code expired. Request a new one.");
            }
            other => panic!("expected Http, got {other:?}"),
        }
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
    fn parses_error_callback() {
        // REV2-53 failure handoff, doubled into query + fragment.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?error=access_denied#error=access_denied"),
            Some(OAuthCallback::Error("access_denied".to_string()))
        );
        // Fragment-dropped hop (Linux xdg): the query alone still parses.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?error=state_missing"),
            Some(OAuthCallback::Error("state_missing".to_string()))
        );
        // Fragment-only (macOS keeps the whole URL).
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return#error=no_session"),
            Some(OAuthCallback::Error("no_session".to_string()))
        );
        // Percent-decoded like every other payload.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?error=oauth%5Ffailed"),
            Some(OAuthCallback::Error("oauth_failed".to_string()))
        );
    }

    #[test]
    fn error_wins_over_code_and_token() {
        // A declared failure must never be redeemed as a payload.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?error=access_denied&code=c#token=t"),
            Some(OAuthCallback::Error("access_denied".to_string()))
        );
    }

    #[test]
    fn parses_linked_callback() {
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?linked=google#linked=google"),
            Some(OAuthCallback::Linked("google".to_string()))
        );
        // An OIDC id is percent-decoded like every other payload.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?linked=my%20idp"),
            Some(OAuthCallback::Linked("my idp".to_string()))
        );
    }

    #[test]
    fn linked_fragment_wins_over_query() {
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return?linked=apple#linked=google"),
            Some(OAuthCallback::Linked("google".to_string()))
        );
    }

    #[test]
    fn error_wins_over_linked() {
        assert_eq!(
            parse_oauth_callback(
                "exponential://oauth-return?linked=google&error=access_denied#linked=google"
            ),
            Some(OAuthCallback::Error("access_denied".to_string()))
        );
    }

    #[test]
    fn linked_on_another_host_is_not_a_callback() {
        assert_eq!(
            parse_oauth_callback("exponential://github-connected?linked=google"),
            None
        );
    }

    #[test]
    fn link_start_urls() {
        assert_eq!(
            link_start_url("https://x.test/", "t.1", "google", "chal"),
            "https://x.test/api/mobile-oauth-start?link=t.1&provider=google&code_challenge=chal"
        );
        assert_eq!(
            link_start_url("https://x.test", "t.1", "apple", "chal"),
            "https://x.test/api/mobile-oauth-start?link=t.1&provider=apple&code_challenge=chal"
        );
        assert_eq!(
            link_start_url("https://x.test", "t.1", "github", "chal"),
            "https://x.test/api/mobile-oauth-start?link=t.1&provider=github&code_challenge=chal"
        );
        assert_eq!(
            link_start_url("https://x.test", "t.1", "my idp", "chal"),
            "https://x.test/api/mobile-oauth-start?link=t.1&providerId=my%20idp&code_challenge=chal"
        );
    }

    #[test]
    fn link_error_copy_matches_the_web() {
        assert_eq!(link_error_message("access_denied"), "Linking was cancelled.");
        for reason in [
            "link_ticket_invalid",
            "state_missing",
            "state_invalid",
            "state_mismatch",
            "state_not_found",
            "please_restart_the_process",
        ] {
            assert_eq!(
                link_error_message(reason),
                "That link request expired. Please try again.",
                "{reason}"
            );
        }
        assert_eq!(
            link_error_message("account_already_linked_to_different_user"),
            "That account is already linked to a different user."
        );
        for reason in ["unable_to_link_account", "oauth_failed", ""] {
            assert_eq!(
                link_error_message(reason),
                "Couldn't link that account. Please try again.",
                "{reason}"
            );
        }
    }

    #[test]
    fn invalid_email_gets_the_contract_copy() {
        assert_eq!(
            otp_error_message(400, r#"{"code":"INVALID_EMAIL","message":"Invalid email"}"#),
            "Enter a valid email address."
        );
        assert_eq!(
            otp_error_message(400, r#"{"message":"Email is the same"}"#),
            "Email is the same"
        );
    }

    #[test]
    fn callback_without_payload_is_none() {
        assert_eq!(parse_oauth_callback("exponential://oauth-return"), None);
        assert_eq!(parse_oauth_callback("exponential://oauth-return#token="), None);
        assert_eq!(parse_oauth_callback("exponential://oauth-return?code="), None);
        assert_eq!(parse_oauth_callback("exponential://oauth-return?error="), None);
        assert_eq!(parse_oauth_callback("/favicon.ico"), None);
    }

    #[test]
    fn other_scheme_hosts_are_not_oauth_callbacks() {
        // EXP-368: github-connected (and any other exponential:// deep link)
        // must never scan as an OAuth callback, even when it carries an
        // `error=` the host-agnostic param scan would otherwise adopt.
        assert_eq!(
            parse_oauth_callback("exponential://github-connected?error=session"),
            None
        );
        assert_eq!(parse_oauth_callback("exponential://github-connected"), None);
        assert_eq!(
            parse_oauth_callback("exponential://invite/tok?code=x"),
            None
        );
        // The host match is EXACT — a host merely PREFIXED by `oauth-return`
        // is a different host, not a callback.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-returnish?code=c#code=c"),
            None
        );
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return.evil.example?token=t"),
            None
        );
        // …while the legitimate shapes (bare host, trailing slash, query,
        // fragment) all still parse.
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return/?code=c"),
            Some(OAuthCallback::Code("c".to_string()))
        );
        assert_eq!(
            parse_oauth_callback("exponential://oauth-return#code=c"),
            Some(OAuthCallback::Code("c".to_string()))
        );
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
                "googleLoginEnabled":true,"appleLoginEnabled":true,"githubEnabled":true,
                "emailOtpEnabled":true,"passkeyEnabled":true}"#,
        )
        .unwrap();
        assert!(!full.password_enabled);
        assert!(full.google_login_enabled);
        assert!(full.apple_login_enabled);
        assert_eq!(full.oidc_providers[0].id, "authentik");
        assert!(full.email_otp_enabled);
        assert!(full.passkey_enabled);

        // Tolerant: unknown/missing fields degrade to defaults.
        let sparse: AuthConfig = serde_json::from_str(r#"{"futureField":1}"#).unwrap();
        assert!(sparse.password_enabled); // defaults true like iOS
        assert!(!sparse.apple_login_enabled);
        assert!(sparse.oidc_providers.is_empty());
        // EXP-857: an older server without the fields offers neither method.
        assert!(!sparse.email_otp_enabled);
        assert!(!sparse.passkey_enabled);
    }
    /// EXP-1111: the redeem posts the pinned body and returns the session
    /// token; `invalid_token` reads as the actionable sentence.
    #[test]
    fn install_token_redeem_returns_the_session_token() {
        use crate::trpc::tests::one_shot_server;
        let (base, captured) =
            one_shot_server(200, r#"{"access_token":"sess-1","token_type":"Bearer"}"#);
        let token = AuthClient::new().redeem_install_token(&base, " expi_abc ").unwrap();
        assert_eq!(token, "sess-1");
        let request = captured.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/cli/install-token/redeem HTTP/1.1"), "{request}");
        assert!(request.ends_with(r#"{"token":"expi_abc"}"#), "{request}");
    }

    #[test]
    fn install_token_refusal_names_the_fix() {
        use crate::trpc::tests::one_shot_server;
        let (base, _captured) = one_shot_server(400, r#"{"error":"invalid_token"}"#);
        match AuthClient::new().redeem_install_token(&base, "expi_used") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 400);
                assert!(message.starts_with("The install token is invalid, already used or expired"), "{message}");
            }
            other => panic!("expected the refusal, got {other:?}"),
        }
        assert_eq!(install_token_error_message(""), "The server refused the install token.");
    }
}
