//! Shared error type for the auth + tRPC HTTP surfaces and the token store.

use std::fmt;

/// Errors surfaced by the `api` crate.
#[derive(Debug)]
pub enum ApiError {
    /// The server rejected the presented **bearer credential** (HTTP 401) on
    /// an authenticated endpoint. EXP-1 #13(b): terminal for that account's
    /// pipeline — clear the stored token ([`crate::AuthStore::handle_unauthorized`]),
    /// tear down, route to login. Never retry anonymously.
    ///
    /// Note: a failed *password sign-in* is NOT this variant — bad credentials
    /// on `/api/auth/sign-in/email` come back as [`ApiError::Http`] with
    /// status 401, because no session token was presented there.
    Unauthorized,
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

/// Map a `ureq` error on an **authenticated** request: 401 →
/// [`ApiError::Unauthorized`] (the EXP-1 #13(b) signal), other statuses →
/// [`ApiError::Http`], transport failures → [`ApiError::Transport`].
pub(crate) fn from_ureq_authed(err: ureq::Error) -> ApiError {
    match err {
        ureq::Error::Status(401, _) => ApiError::Unauthorized,
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            http_error(status, &body)
        }
        ureq::Error::Transport(t) => ApiError::Transport(t.to_string()),
    }
}

/// Map a `ureq` error on an **unauthenticated** request (no bearer was
/// presented, so a 401 means bad credentials, not a dead session).
pub(crate) fn from_ureq_unauthed(err: ureq::Error) -> ApiError {
    match err {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            http_error(status, &body)
        }
        ureq::Error::Transport(t) => ApiError::Transport(t.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_extracts_trpc_message() {
        let body = r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#;
        match http_error(403, body) {
            ApiError::Http { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "You are not a member of this workspace");
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
