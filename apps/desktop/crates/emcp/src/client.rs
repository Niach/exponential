//! A blocking MCP client over Streamable HTTP, on rmcp's transport (plain
//! JSON or SSE answers, `Mcp-Session-Id` echoed, `nextCursor` followed).
//! Header VALUES are the caller's secrets: never logged, never in an error
//! text ([`Upstream`]'s `Debug` prints names only).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, ClientConfig};
use rmcp::service::{RoleClient, RunningService, ServiceError};
use rmcp::transport::streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{Peer, ServiceExt};
use serde_json::{json, Value};

use crate::runtime::block_on;

/// One HTTP MCP server with its headers RESOLVED.
#[derive(Clone, PartialEq, Eq)]
pub struct Upstream {
    /// The name the caller knows the server by (`exponential`, `computer`,
    /// a team server's key).
    pub name: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        write!(f, "Upstream({} @ {}, headers {names:?})", self.name, self.url)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// A non-2xx status (401 = the credential was refused).
    Http(u16),
    /// Could not connect, read or parse.
    Transport(String),
    /// The server answered a JSON-RPC error.
    Rpc { code: i64, message: String },
    Timeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Http(status) => write!(f, "HTTP {status}"),
            ClientError::Transport(text) => write!(f, "{text}"),
            ClientError::Rpc { code, message } => write!(f, "{message} (code {code})"),
            ClientError::Timeout => write!(f, "timed out"),
        }
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    /// How a failed `tools/call` ended (#EXP-1517, eCore's verdict): a
    /// non-2xx status or a JSON-RPC error = the tool never ran; `None` =
    /// unknown (a timeout or a broken connection may have run it), so a
    /// blind retry could double its effect.
    pub fn delivery(&self) -> Option<ecore::DeliveryVerdict> {
        match self {
            ClientError::Http(_) | ClientError::Rpc { .. } => Some(ecore::DeliveryVerdict::NotDelivered),
            ClientError::Transport(_) | ClientError::Timeout => None,
        }
    }
}

/// The HTTP status buried in a transport error, if there is one.
fn http_status(error: &(dyn std::error::Error + 'static)) -> Option<u16> {
    let mut at: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = at {
        if let Some(http) = error.downcast_ref::<StreamableHttpError<reqwest::Error>>() {
            match http {
                StreamableHttpError::HttpStatus(status) => return Some(status.status),
                StreamableHttpError::AuthRequired(_) => return Some(401),
                StreamableHttpError::InsufficientScope(_) => return Some(403),
                StreamableHttpError::SessionExpired => return Some(404),
                // Every other non-2xx (401 without a challenge, 426, …)
                // arrives as `HTTP <status>: <body>`.
                StreamableHttpError::UnexpectedServerResponse(text) => {
                    if let Some(status) = text.strip_prefix("HTTP ").and_then(|rest| rest.get(..3)).and_then(|code| code.parse().ok()) {
                        return Some(status);
                    }
                }
                StreamableHttpError::Client(err) => {
                    if let Some(status) = err.status() {
                        return Some(status.as_u16());
                    }
                }
                _ => {}
            }
        }
        if let Some(status) = error.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status) {
            return Some(status.as_u16());
        }
        at = error.source();
    }
    None
}

fn transport_error(name: &str, error: &(dyn std::error::Error + 'static)) -> ClientError {
    match http_status(error) {
        Some(status) => ClientError::Http(status),
        // The URL carries no secret; headers never reach a Display.
        None => ClientError::Transport(format!("{name}: {error}")),
    }
}

fn service_error(name: &str, error: ServiceError) -> ClientError {
    match error {
        ServiceError::McpError(error) => ClientError::Rpc { code: error.code.0 as i64, message: error.message.to_string() },
        ServiceError::Timeout { .. } => ClientError::Timeout,
        ServiceError::TransportSend(error) => transport_error(name, &error),
        other => transport_error(name, &other),
    }
}

async fn timed<T>(timeout: Duration, future: impl std::future::Future<Output = Result<T, ClientError>>) -> Result<T, ClientError> {
    tokio::time::timeout(timeout, future).await.unwrap_or(Err(ClientError::Timeout))
}

fn client_config(client_name: &str) -> ClientConfig {
    serde_json::from_value(json!({
        "protocolVersion": rmcp::model::ProtocolVersion::LATEST,
        "capabilities": {},
        "clientInfo": { "name": client_name, "version": env!("CARGO_PKG_VERSION") },
    }))
    .expect("a client config")
}

/// A connected client (the handshake done). Cheap to share; dropping the
/// last clone closes the connection.
#[derive(Clone)]
pub struct Client {
    name: Arc<str>,
    peer: Peer<RoleClient>,
    // Keeps the service task alive while any clone lives.
    _service: Arc<RunningService<RoleClient, ClientConfig>>,
}

impl Client {
    /// `initialize` + `notifications/initialized` against `upstream`, its
    /// headers sent verbatim on every request. `client_name` is the
    /// `clientInfo.name` (`exponential-codemode`).
    pub fn connect(upstream: &Upstream, client_name: &str, timeout: Duration) -> Result<Client, ClientError> {
        let mut headers = HashMap::new();
        let mut auth = None;
        for (name, value) in &upstream.headers {
            if name.eq_ignore_ascii_case("authorization") {
                auth = Some(value.clone());
                continue;
            }
            let name = http::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ClientError::Transport(format!("{}: a header name is malformed", upstream.name)))?;
            let value = http::HeaderValue::from_str(value)
                .map_err(|_| ClientError::Transport(format!("{}: the {name} header is malformed", upstream.name)))?;
            headers.insert(name, value);
        }
        let mut config = StreamableHttpClientTransportConfig::with_uri(upstream.url.clone()).custom_headers(headers);
        // rmcp sends `Authorization: Bearer <auth_header>`. The value is
        // the host's credential (an `expu_` key, an OAuth token), not always
        // an eCore token, so it is split verbatim, never `parse_bearer`ed.
        if let Some(value) = auth {
            match value.strip_prefix("Bearer ") {
                Some(token) => config = config.auth_header(token.to_string()),
                None => {
                    let mut custom = config.custom_headers.clone();
                    custom.insert(
                        http::header::AUTHORIZATION,
                        http::HeaderValue::from_str(&value)
                            .map_err(|_| ClientError::Transport(format!("{}: the authorization header is malformed", upstream.name)))?,
                    );
                    config = config.custom_headers(custom);
                }
            }
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| ClientError::Transport(format!("{}: {err}", upstream.name)))?;
        Self::start(&upstream.name, client_name, move || StreamableHttpClientTransport::with_client(http, config), timeout)
    }

    /// The handshake over any rmcp client transport (the OAuth one). The
    /// transport is made on emcp's runtime: rmcp spawns its worker there.
    pub(crate) fn start<T, E, A>(
        name: &str,
        client_name: &str,
        transport: impl FnOnce() -> T + Send + 'static,
        timeout: Duration,
    ) -> Result<Client, ClientError>
    where
        T: rmcp::transport::IntoTransport<RoleClient, E, A> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
        A: Send + 'static,
    {
        let name: Arc<str> = name.into();
        let config = client_config(client_name);
        let label = name.clone();
        let service = block_on(async move {
            timed(timeout, async {
                config.serve(transport()).await.map_err(|err| match err {
                    rmcp::service::ClientInitializeError::JsonRpcError(error) => {
                        ClientError::Rpc { code: error.code.0 as i64, message: error.message.to_string() }
                    }
                    // Not a `source` of the init error: walked by hand.
                    rmcp::service::ClientInitializeError::TransportError { error, .. } => transport_error(&label, &error),
                    other => transport_error(&label, &other),
                })
            })
            .await
        })?;
        Ok(Client { name, peer: service.peer().clone(), _service: Arc::new(service) })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Every tool as its JSON object, following `nextCursor`.
    pub fn list_tools(&self, timeout: Duration) -> Result<Vec<Value>, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        block_on(async move {
            timed(timeout, async {
                let tools = peer.list_all_tools().await.map_err(|err| service_error(&name, err))?;
                Ok(tools.into_iter().filter_map(|tool| serde_json::to_value(tool).ok()).collect())
            })
            .await
        })
    }

    /// One `tools/call`; the MCP call result as JSON (`content`,
    /// `structuredContent`, `isError`, `_meta`).
    pub fn call(&self, tool: &str, arguments: Value, timeout: Duration) -> Result<Value, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        let params: CallToolRequestParams = serde_json::from_value(json!({ "name": tool, "arguments": arguments }))
            .map_err(|err| ClientError::Transport(format!("{tool}: the arguments are not an object ({err})")))?;
        block_on(async move {
            timed(timeout, async {
                let result = peer.call_tool(params).await.map_err(|err| service_error(&name, err))?;
                serde_json::to_value(result).map_err(|err| ClientError::Transport(format!("{name}: {err}")))
            })
            .await
        })
    }
}

/// One-shot: connect, call `tool`, drop the connection. For callers that
/// make a single call per process (the CLI) or per user action.
pub fn call_once(upstream: &Upstream, client_name: &str, tool: &str, arguments: Value, timeout: Duration) -> Result<Value, ClientError> {
    Client::connect(upstream, client_name, timeout)?.call(tool, arguments, timeout)
}
