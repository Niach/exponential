//! tRPC-over-HTTP client (masterplan-v3 §3.1/§5.7) — the desktop mirror of
//! the proven iOS `TrpcClient.swift`.
//!
//! Wire format, verified against `apps/web/src/lib/trpc.ts`
//! (`initTRPC.context<Context>().create()` — **no transformer**, so plain
//! JSON, never superjson) and the fetch adapter mounted at
//! `apps/web/src/routes/api/trpc/$.ts`:
//!
//! - **Query** procedures are `GET /api/trpc/<proc>`; an input rides as
//!   `?input=<raw JSON, percent-encoded>` (the NON-batched form — the batched
//!   `{"0":{…}}` shape is never used). POSTing to a query returns 405.
//! - **Mutation** procedures are `POST /api/trpc/<proc>` with the raw JSON
//!   input as the body.
//! - Success envelope: `{"result":{"data":<output>}}`.
//! - Error envelope: `{"error":{"message":…,"code":…,"data":{…}}}` with the
//!   HTTP status carrying the mapped tRPC code (401 = UNAUTHORIZED).
//!
//! Every request reads the bearer through the [`TokenProvider`] **at call
//! time** (§5.7) so a re-login is picked up by the very next request. A 401
//! maps to [`ApiError::Unauthorized`] — the caller feeds that into
//! [`crate::AuthStore::handle_unauthorized`] (route to login,
//! never silently degrade).
//!
//! Later phases extend this with the typed routers they need; the `awaitTxId`
//! Electric-echo gate (§4.1/Phase 3) sits on top of the `txid` values many
//! mutation outputs carry — decode them via your output type.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use domain::client_version::{client_version_header_value, CLIENT_VERSION_HEADER};

use crate::encode::percent_encode;
use crate::error::{from_ureq_authed, ApiError};
use crate::login::normalize_instance_url;
use crate::TokenProvider;

#[derive(Deserialize)]
struct Envelope<T> {
    result: EnvelopeResult<T>,
}

#[derive(Deserialize)]
struct EnvelopeResult<T> {
    data: T,
}

/// Blocking tRPC client bound to one instance URL + one account's token
/// provider. Cheap to clone-per-account; share one per (account, app).
pub struct TrpcClient {
    agent: ureq::Agent,
    base_url: String,
    token_provider: Arc<dyn TokenProvider>,
}

impl TrpcClient {
    /// `instance_url` is normalized ([`normalize_instance_url`]); the token
    /// provider is evaluated per request (never captured once).
    pub fn new(instance_url: &str, token_provider: Arc<dyn TokenProvider>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        Self {
            agent,
            base_url: normalize_instance_url(instance_url),
            token_provider,
        }
    }

    /// The normalized instance base URL this client talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET an input-less `query` procedure (e.g. `users.listPersonalApiKeys`).
    pub fn query<O: DeserializeOwned>(&self, path: &str) -> Result<O, ApiError> {
        let url = format!("{}/api/trpc/{path}", self.base_url);
        let request = self.authorize(self.agent.get(&url).set("Accept", "application/json"));
        let response = request.call().map_err(from_ureq_authed)?;
        decode_envelope(response, path)
    }

    /// GET a `query` procedure with an input (`?input=<raw JSON>`,
    /// percent-encoded so the JSON delimiters survive — no `+`-as-space).
    pub fn query_with_input<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        input: &I,
    ) -> Result<O, ApiError> {
        let json = serde_json::to_string(input)
            .map_err(|e| ApiError::Decode(format!("{path} input: {e}")))?;
        let url = format!(
            "{}/api/trpc/{path}?input={}",
            self.base_url,
            percent_encode(&json)
        );
        let request = self.authorize(self.agent.get(&url).set("Accept", "application/json"));
        let response = request.call().map_err(from_ureq_authed)?;
        decode_envelope(response, path)
    }

    /// POST a `mutation` procedure with an input.
    pub fn mutation<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        input: &I,
    ) -> Result<O, ApiError> {
        let body = serde_json::to_string(input)
            .map_err(|e| ApiError::Decode(format!("{path} input: {e}")))?;
        self.mutation_raw(path, &body)
    }

    /// POST a `mutation` procedure that takes no input.
    pub fn mutation_no_input<O: DeserializeOwned>(&self, path: &str) -> Result<O, ApiError> {
        self.mutation_raw(path, "")
    }

    fn mutation_raw<O: DeserializeOwned>(&self, path: &str, body: &str) -> Result<O, ApiError> {
        let url = format!("{}/api/trpc/{path}", self.base_url);
        let request = self.authorize(
            self.agent
                .post(&url)
                .set("Accept", "application/json")
                .set("Content-Type", "application/json"),
        );
        let response = request.send_string(body).map_err(from_ureq_authed)?;
        decode_envelope(response, path)
    }

    /// Attach the bearer read **at call time** (§5.7). No token → the request
    /// goes out unauthenticated and the server answers 401 for authed procs —
    /// which correctly surfaces as [`ApiError::Unauthorized`].
    fn authorize(&self, request: ureq::Request) -> ureq::Request {
        // EXP-104: every request carries the client-version header so the
        // server can 426-gate stale builds (authed and unauthed alike).
        let request = request.set(CLIENT_VERSION_HEADER, &client_version_header_value());
        match self.token_provider.token() {
            Some(token) => request.set("Authorization", &format!("Bearer {token}")),
            None => request,
        }
    }
}

fn decode_envelope<O: DeserializeOwned>(
    response: ureq::Response,
    path: &str,
) -> Result<O, ApiError> {
    let body = response
        .into_string()
        .map_err(|e| ApiError::Transport(e.to_string()))?;
    let envelope: Envelope<O> = serde_json::from_str(&body)
        .map_err(|e| ApiError::Decode(format!("{path} envelope: {e}")))?;
    Ok(envelope.result.data)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// One-shot canned HTTP server: accepts a single connection, captures the
    /// full request (head + body), answers with `status` + `body`. Returns
    /// (base_url, captured-request receiver).
    pub(crate) fn one_shot_server(
        status: u16,
        body: &'static str,
    ) -> (String, flume::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = flume::bounded::<String>(1);
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut captured = Vec::new();
            let mut buf = [0u8; 4096];
            // Read the head, then any Content-Length body.
            let (mut head_end, mut content_length) = (None, 0usize);
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                captured.extend_from_slice(&buf[..n]);
                if head_end.is_none() {
                    if let Some(pos) = find_head_end(&captured) {
                        head_end = Some(pos);
                        let head = String::from_utf8_lossy(&captured[..pos]);
                        content_length = head
                            .lines()
                            .find_map(|l| {
                                let (name, value) = l.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().ok())?
                            })
                            .unwrap_or(0);
                    }
                }
                if let Some(pos) = head_end {
                    if captured.len() >= pos + content_length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            let _ = tx.send(String::from_utf8_lossy(&captured).into_owned());
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    fn find_head_end(bytes: &[u8]) -> Option<usize> {
        bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Widget {
        id: String,
        count: i64,
    }

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(crate::StaticToken("tok-1".to_string())))
    }

    #[test]
    fn query_decodes_result_data_envelope() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"w1","count":3}}}"#,
        );
        let out: Widget = client(&base).query("widgets.get").unwrap();
        assert_eq!(
            out,
            Widget {
                id: "w1".to_string(),
                count: 3
            }
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/widgets.get HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer tok-1"));
        // EXP-104: the client-version header rides every request.
        assert!(
            request.to_ascii_lowercase().contains("x-client-version: desktop/"),
            "missing client-version header: {request}"
        );
    }

    #[test]
    fn http_426_maps_to_upgrade_required() {
        // The min-version gate (EXP-104): a stale build is stopped, and this
        // must NOT be mistaken for Unauthorized (which would clear the token).
        let (base, _captured) = one_shot_server(
            426,
            r#"{"error":"client_upgrade_required","platform":"desktop","min":"0.9.0"}"#,
        );
        let result: Result<Widget, ApiError> = client(&base).query("widgets.get");
        assert!(matches!(result, Err(ApiError::UpgradeRequired)));
    }

    #[test]
    fn query_with_input_percent_encodes_raw_json() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":true}}"#);
        #[derive(Serialize)]
        struct Input {
            id: String,
        }
        let ok: bool = client(&base)
            .query_with_input(
                "issues.byId",
                &Input {
                    id: "a b".to_string(),
                },
            )
            .unwrap();
        assert!(ok);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // Raw JSON input (non-batched), RFC-3986 percent-encoded.
        assert!(
            request.starts_with(
                "GET /api/trpc/issues.byId?input=%7B%22id%22%3A%22a%20b%22%7D HTTP/1.1"
            ),
            "unexpected request line: {}",
            request.lines().next().unwrap_or_default()
        );
    }

    #[test]
    fn mutation_posts_raw_json_body() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"id":"w2","count":1}}}"#);
        let out: Widget = client(&base)
            .mutation("widgets.create", &serde_json::json!({"name":"x"}))
            .unwrap();
        assert_eq!(out.id, "w2");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/widgets.create HTTP/1.1"));
        assert!(request.contains("Content-Type: application/json"));
        assert!(request.ends_with(r#"{"name":"x"}"#), "body missing: {request}");
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        // The reauth signal, never an anonymous retry.
        let (base, _captured) = one_shot_server(
            401,
            r#"{"error":{"message":"UNAUTHORIZED","code":-32001,"data":{"code":"UNAUTHORIZED","httpStatus":401}}}"#,
        );
        let result: Result<Widget, ApiError> = client(&base).query("widgets.get");
        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }

    #[test]
    fn http_error_carries_trpc_message() {
        let (base, _captured) = one_shot_server(
            403,
            r#"{"error":{"message":"Owner role required","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#,
        );
        let result: Result<Widget, ApiError> = client(&base).query("widgets.get");
        match result {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 403);
                assert_eq!(message, "Owner role required");
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_envelope_is_decode_error() {
        let (base, _captured) = one_shot_server(200, r#"{"data":{"id":"w1","count":3}}"#);
        let result: Result<Widget, ApiError> = client(&base).query("widgets.get");
        assert!(matches!(result, Err(ApiError::Decode(_))));
    }

    #[test]
    fn no_token_sends_no_authorization_header() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":true}}"#);
        let provider: Arc<dyn TokenProvider> = Arc::new(|| None);
        let client = TrpcClient::new(&base, provider);
        let _: bool = client.query("public.ping").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!request.contains("Authorization:"));
    }
}
