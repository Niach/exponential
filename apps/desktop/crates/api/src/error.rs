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
    /// DNS / TCP / TLS / timeout — transient; retry with backoff. `offline`
    /// (EXP-533) marks the subset that means "this machine cannot reach the
    /// network at all" (DNS/connect/timeout), so [`ApiError::user_message`]
    /// can say so in plain English instead of leaking reqwest's
    /// `error sending request for url …`.
    Transport { message: String, offline: bool },
    /// The response body did not match the expected shape.
    Decode(String),
    /// A request URL could not be built from the inputs.
    InvalidUrl(String),
    /// A LOCAL filesystem write failed while materializing a downloaded body
    /// (EXP-511 steer-image localization). Nothing server-side to retry — the
    /// caller degrades locally.
    Io(String),
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
            ApiError::Transport { message, .. } => write!(f, "transport error: {message}"),
            ApiError::Decode(msg) => write!(f, "decode error: {msg}"),
            ApiError::InvalidUrl(msg) => write!(f, "invalid URL: {msg}"),
            ApiError::Io(msg) => write!(f, "local file error: {msg}"),
            ApiError::TokenStore(msg) => write!(f, "token store error: {msg}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// EXP-533: the ONE offline sentence, byte-identical across web, iOS, Android
/// and desktop. Never say `Failed to fetch` / `Unable to resolve host` /
/// `error sending request for url` to a user.
pub const OFFLINE_MESSAGE: &str = "You're offline. Check your connection and try again.";

impl ApiError {
    /// Build a transport failure whose offline-ness is already known (the
    /// reqwest path uses [`transport_error`]; tests and non-reqwest callers
    /// use this). Defaults to NOT offline — a message we could not classify
    /// must not claim the user's connection is down.
    pub fn transport(message: impl Into<String>) -> ApiError {
        ApiError::Transport {
            message: message.into(),
            offline: false,
        }
    }

    /// The machine could not reach the server at all (DNS/connect/timeout).
    pub fn is_offline(&self) -> bool {
        matches!(self, ApiError::Transport { offline: true, .. })
    }

    /// EXP-533: a REAL content conflict — the only failure a "Fix merge
    /// conflicts" recovery run can do anything about. The server answers a
    /// genuine unmergeable PR with tRPC `CONFLICT` (HTTP 409); everything
    /// else (stale base, branch protection, no GitHub App) is 412.
    pub fn is_conflict(&self) -> bool {
        matches!(self, ApiError::Http { status: 409, .. })
    }

    /// What a USER should read. Server messages are already user-facing and
    /// win; transport/decode failures get a plain sentence instead of a
    /// library's internal text. Logging keeps [`Display`](fmt::Display).
    pub fn user_message(&self) -> String {
        match self {
            ApiError::Transport { offline: true, .. } => OFFLINE_MESSAGE.to_string(),
            ApiError::Transport { .. } => "Couldn't reach the server. Try again.".to_string(),
            ApiError::Unauthorized => "Your session expired. Sign in again.".to_string(),
            ApiError::UpgradeRequired => {
                "This app version is out of date. Update to continue.".to_string()
            }
            ApiError::Decode(_) => "The server sent an unexpected response.".to_string(),
            ApiError::Http { status, message } if message.trim().is_empty() => {
                format!("The server returned an error (HTTP {status}).")
            }
            ApiError::Http { message, .. } => message.clone(),
            // Local faults (bad URL, filesystem, secret store) carry their own
            // actionable text.
            other => other.to_string(),
        }
    }
}

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
    ApiError::Transport {
        message: err.to_string(),
        offline: is_offline_failure(err.is_connect(), err.is_timeout(), err.is_request()),
    }
}

/// EXP-533: which reqwest transport failures mean "this machine cannot reach
/// the server" (as opposed to a body/redirect/builder fault). Pure so the
/// classification is testable without forging a `reqwest::Error`.
///
/// * `is_connect` — DNS/TCP/TLS never got established: offline.
/// * `is_timeout` — nothing answered in time: indistinguishable from offline
///   for a user, and the honest advice is the same.
/// * `is_request` — reqwest's catch-all for "the request never completed";
///   DNS failures land here on some platforms without `is_connect`, so it
///   counts too. Body/decode/redirect faults are none of the three.
pub(crate) fn is_offline_failure(is_connect: bool, is_timeout: bool, is_request: bool) -> bool {
    is_connect || is_timeout || is_request
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

    // -- EXP-533 ------------------------------------------------------------

    #[test]
    fn offline_classification_table() {
        // (is_connect, is_timeout, is_request) → offline?
        assert!(is_offline_failure(true, false, false), "connect refused");
        assert!(is_offline_failure(false, true, false), "read timeout");
        assert!(is_offline_failure(false, false, true), "DNS / send failure");
        assert!(is_offline_failure(true, true, true));
        // A body/decode/redirect fault reached the server — never claim the
        // user's connection is down.
        assert!(!is_offline_failure(false, false, false));
    }

    #[test]
    fn user_message_never_leaks_a_transport_string() {
        let offline = ApiError::Transport {
            message: "error sending request for url (https://app.exponential.at/api/trpc)"
                .to_string(),
            offline: true,
        };
        assert_eq!(offline.user_message(), OFFLINE_MESSAGE);
        assert!(offline.is_offline());

        let other = ApiError::transport("body read: incomplete message");
        assert_eq!(other.user_message(), "Couldn't reach the server. Try again.");
        assert!(!other.is_offline());
        // Every arm is a sentence, not a library dump.
        for err in [
            ApiError::Unauthorized,
            ApiError::UpgradeRequired,
            ApiError::Decode("missing field `id`".into()),
        ] {
            let message = err.user_message();
            assert!(!message.contains("error sending request"), "{message}");
            assert!(message.ends_with('.'), "{message}");
        }
    }

    #[test]
    fn user_message_prefers_the_servers_message() {
        let err = ApiError::Http {
            status: 403,
            message: "Only team owners can delete a team".to_string(),
        };
        assert_eq!(err.user_message(), "Only team owners can delete a team");
        // …but an EMPTY server message is not a message.
        let empty = ApiError::Http {
            status: 502,
            message: "  ".to_string(),
        };
        assert_eq!(
            empty.user_message(),
            "The server returned an error (HTTP 502)."
        );
    }

    #[test]
    fn only_a_real_conflict_is_a_conflict() {
        assert!(ApiError::Http {
            status: 409,
            message: "Pull request is not mergeable".to_string(),
        }
        .is_conflict());
        // A 412 is the server saying "not a conflict" (stale base, branch
        // protection, no GitHub App) — no recovery run may be offered.
        assert!(!ApiError::Http {
            status: 412,
            message: "Head branch changed on GitHub. Refresh and try again.".to_string(),
        }
        .is_conflict());
        // Not even when a 412 quotes the conflict wording.
        assert!(!ApiError::Http {
            status: 412,
            message: "This branch has merge conflicts with main that must be resolved".to_string(),
        }
        .is_conflict());
        assert!(!ApiError::transport("refused").is_conflict());
        assert!(!ApiError::Unauthorized.is_conflict());
    }
}
