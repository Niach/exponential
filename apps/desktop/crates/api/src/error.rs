//! Shared error type for the auth + tRPC HTTP surfaces and the token store.

use std::fmt;

/// Errors surfaced by the `api` crate.
#[derive(Debug)]
pub enum ApiError {
    /// The server rejected the presented **bearer credential** (HTTP 401) on
    /// an authenticated endpoint — terminal for that account's
    /// pipeline — clear the stored token ([`crate::AuthStore::handle_unauthorized`]),
    /// tear down, route to login. Never retry anonymously.
    ///
    /// Note: a failed *password sign-in* is NOT this variant — bad credentials
    /// on `/api/auth/sign-in/email` come back as [`ApiError::Http`] with
    /// status 401, because no session token was presented there.
    Unauthorized,
    /// The server rejected this client build as too old (HTTP 426 Upgrade
    /// Required, EXP-104) — the app must show the blocking "Update required"
    /// view and stop syncing. NOTE: unlike [`ApiError::Unauthorized`] this
    /// NEVER clears the stored token; the session is fine, the binary is
    /// stale.
    UpgradeRequired,
    /// Any other non-2xx HTTP status. `message` is the server's error message
    /// when one could be extracted from the JSON body, else the raw body
    /// (truncated).
    Http { status: u16, message: String },
    /// DNS / TCP / TLS / timeout — transient; retry with backoff.
    Transport(String),
    /// The response body did not match the expected shape.
    Decode(String),
    /// A request URL could not be built from the inputs.
    InvalidUrl(String),
    /// Secret storage failed (the 0600-file store).
    TokenStore(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "unauthorized (session token rejected)"),
            ApiError::UpgradeRequired => {
                write!(f, "client upgrade required (server rejected this build)")
            }
            ApiError::Http { status, message } => write!(f, "HTTP {status}: {message}"),
            ApiError::Transport(msg) => write!(f, "transport error: {msg}"),
            ApiError::Decode(msg) => write!(f, "decode error: {msg}"),
            ApiError::InvalidUrl(msg) => write!(f, "invalid URL: {msg}"),
            ApiError::TokenStore(msg) => write!(f, "token store error: {msg}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// Build an [`ApiError::Http`] from a non-2xx response, extracting the
/// human-readable message from the two JSON error envelopes this backend
/// speaks: tRPC (`{"error":{"message":…}}`) and Better Auth
/// (`{"message":…}`). Falls back to the truncated raw body.
///
/// Callers that presented a bearer token map 401 → [`ApiError::Unauthorized`]
/// **before** calling this.
pub(crate) fn http_error(status: u16, body: &str) -> ApiError {
    #[derive(serde::Deserialize)]
    struct TrpcErrorEnvelope {
        error: TrpcErrorBody,
    }
    #[derive(serde::Deserialize)]
    struct TrpcErrorBody {
        message: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct FlatMessage {
        message: Option<String>,
    }

    let message = serde_json::from_str::<TrpcErrorEnvelope>(body)
        .ok()
        .and_then(|e| e.error.message)
        .or_else(|| {
            serde_json::from_str::<FlatMessage>(body)
                .ok()
                .and_then(|m| m.message)
        })
        .unwrap_or_else(|| {
            let mut raw = body.trim().to_string();
            if raw.len() > 300 {
                raw.truncate(300);
                raw.push('…');
            }
            raw
        });

    ApiError::Http { status, message }
}

/// Map a transport-level `reqwest` failure (DNS / TCP / TLS / timeout). Unlike
/// `ureq`, reqwest returns `Ok(response)` for non-2xx statuses, so an `Err`
/// here is always transport — status mapping lives in [`status_error_authed`]
/// / [`status_error_unauthed`], which the response path calls explicitly.
pub(crate) fn transport_error(err: reqwest::Error) -> ApiError {
    ApiError::Transport(err.to_string())
}

/// Map a non-2xx status on an **authenticated** request: 401 →
/// [`ApiError::Unauthorized`] (the reauth signal), 426 →
/// [`ApiError::UpgradeRequired`], everything else → [`ApiError::Http`].
pub(crate) fn status_error_authed(status: u16, body: &str) -> ApiError {
    match status {
        401 => ApiError::Unauthorized,
        426 => ApiError::UpgradeRequired,
        other => http_error(other, body),
    }
}

/// Map a non-2xx status on an **unauthenticated** request (no bearer was
/// presented, so a 401 means bad credentials, not a dead session).
pub(crate) fn status_error_unauthed(status: u16, body: &str) -> ApiError {
    match status {
        // The min-version gate (EXP-104) rejects even unauthenticated calls
        // (auth-config, sign-in) so a stale build is stopped before login.
        426 => ApiError::UpgradeRequired,
        other => http_error(other, body),
    }
}

/// Read a response body, mapping a read failure to [`ApiError::Transport`] —
/// with reqwest the body arrives separately from the status, so this is the
/// one place that conversion needs to happen.
pub(crate) fn read_body(response: reqwest::blocking::Response) -> Result<String, ApiError> {
    response.text().map_err(transport_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_extracts_trpc_message() {
        let body = r#"{"error":{"message":"You are not a member of this team","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#;
        match http_error(403, body) {
            ApiError::Http { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "You are not a member of this team");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn http_error_extracts_better_auth_message() {
        let body = r#"{"message":"Invalid email or password"}"#;
        match http_error(401, body) {
            ApiError::Http { status, message } => {
                assert_eq!(status, 401);
                assert_eq!(message, "Invalid email or password");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn authed_maps_426_to_upgrade_required() {
        // EXP-104: a 426 on an authed request is the min-version gate, NOT a
        // dead session — it must never be mistaken for Unauthorized (which
        // clears the token).
        let err = status_error_authed(426, "{}");
        assert!(matches!(err, ApiError::UpgradeRequired), "got {err:?}");
    }

    #[test]
    fn authed_maps_401_to_unauthorized() {
        // The reauth signal: a presented bearer the server rejected. This is
        // the one status that clears the stored token, so it must not drift
        // into the generic Http bucket.
        let err = status_error_authed(401, "{}");
        assert!(matches!(err, ApiError::Unauthorized), "got {err:?}");
    }

    #[test]
    fn unauthed_maps_426_to_upgrade_required() {
        // The gate also fires before login (auth-config / sign-in).
        let err = status_error_unauthed(426, "{}");
        assert!(matches!(err, ApiError::UpgradeRequired), "got {err:?}");
    }

    #[test]
    fn unauthed_401_is_bad_credentials_not_a_dead_session() {
        // No bearer was presented, so a 401 means "wrong email/password" and
        // must stay an Http error — mapping it to Unauthorized would clear a
        // perfectly good token belonging to another account.
        let err = status_error_unauthed(401, r#"{"message":"Invalid email or password"}"#);
        match err {
            ApiError::Http { status, message } => {
                assert_eq!(status, 401);
                assert_eq!(message, "Invalid email or password");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn http_error_falls_back_to_raw_body() {
        match http_error(500, "Internal Server Error") {
            ApiError::Http { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message, "Internal Server Error");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }
}
