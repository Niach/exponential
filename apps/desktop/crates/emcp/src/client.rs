//! A blocking MCP client over Streamable HTTP, on rmcp's transport (plain
//! JSON or SSE answers, `Mcp-Session-Id` echoed, `nextCursor` followed).
//! Header VALUES are the caller's secrets: never logged, never in an error
//! text ([`Upstream`]'s `Debug` prints names only).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CancelTaskParams, ClientConfig, GetSkillRequestParams, GetTaskParams, ReadResourceRequestParams,
    ReadResourceResponse, UpdateTaskParams,
};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt, RoleClient, RunningService, ServiceError};
use rmcp::transport::streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{Peer, ServiceExt};
use serde_json::{json, Map, Value};

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

/// VAPP-239: one stdio MCP server, a command the device spawns with its env
/// RESOLVED (the member's secret among it). Env VALUES are secrets: `Debug`
/// prints names only.
#[derive(Clone, PartialEq, Eq)]
pub struct StdioUpstream {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for StdioUpstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        write!(f, "StdioUpstream({}: {} {:?}, env {names:?})", self.name, self.command, self.args)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// A non-2xx status other than 401.
    Http(u16),
    /// #NEW-148: HTTP 401, the credential was refused (the hub asks its
    /// host for fresh credentials and retries once). Displays `HTTP 401`.
    Unauthorized,
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
            ClientError::Unauthorized => write!(f, "HTTP 401"),
            ClientError::Transport(text) => write!(f, "{text}"),
            ClientError::Rpc { code, message } => write!(f, "{message} (code {code})"),
            ClientError::Timeout => write!(f, "timed out"),
        }
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    /// A non-2xx status as its error: 401 = [`ClientError::Unauthorized`].
    pub fn from_status(status: u16) -> ClientError {
        match status {
            401 => ClientError::Unauthorized,
            status => ClientError::Http(status),
        }
    }

    /// How a failed `tools/call` ended (#EXP-1517, eCore's verdict): a
    /// non-2xx status or a JSON-RPC error = the tool never ran; `None` =
    /// unknown (a timeout or a broken connection may have run it), so a
    /// blind retry could double its effect.
    pub fn delivery(&self) -> Option<ecore::DeliveryVerdict> {
        match self {
            ClientError::Http(_) | ClientError::Unauthorized | ClientError::Rpc { .. } => Some(ecore::DeliveryVerdict::NotDelivered),
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
        Some(status) => ClientError::from_status(status),
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

/// The client's config. `capabilities` is what EVERY request declares:
/// rmcp stamps it into each request's `_meta`
/// `io.modelcontextprotocol/clientCapabilities` on 2026-07-28 (and into
/// `initialize` for an older server).
fn client_config(client_name: &str, capabilities: &Value) -> ClientConfig {
    serde_json::from_value(json!({
        "protocolVersion": rmcp::model::ProtocolVersion::LATEST,
        "capabilities": capabilities,
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
    // VAPP-239: a stdio server's process, killed when the last clone drops.
    _child: Option<Arc<std::sync::Mutex<tokio::process::Child>>>,
}

impl Client {
    /// `initialize` + `notifications/initialized` against `upstream`, its
    /// headers sent verbatim on every request. `client_name` is the
    /// `clientInfo.name` (`exponential-codemode`).
    pub fn connect(upstream: &Upstream, client_name: &str, timeout: Duration) -> Result<Client, ClientError> {
        Self::connect_as(upstream, client_name, None, timeout)
    }

    /// #NEW-148: [`Self::connect`] as an MCP 2026-07-28 client declaring
    /// `capabilities` (the hub's `hub_client::client_capabilities()`) on
    /// every request: `server/discover` first, an older server falls back
    /// to `initialize` (which declares them once).
    pub fn connect_with(upstream: &Upstream, client_name: &str, capabilities: &Value, timeout: Duration) -> Result<Client, ClientError> {
        Self::connect_as(upstream, client_name, Some(capabilities), timeout)
    }

    fn connect_as(upstream: &Upstream, client_name: &str, capabilities: Option<&Value>, timeout: Duration) -> Result<Client, ClientError> {
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
        let http = crate::runtime::http_client()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| ClientError::Transport(format!("{}: {err}", upstream.name)))?;
        Self::start(&upstream.name, client_name, capabilities, move || StreamableHttpClientTransport::with_client(http, config), timeout)
    }

    /// VAPP-239: spawn a stdio server (`command args`, its env added to the
    /// device's own, stderr discarded) and do the handshake over its
    /// stdin/stdout. The process dies with the last clone.
    pub fn spawn(upstream: &StdioUpstream, client_name: &str, timeout: Duration) -> Result<Client, ClientError> {
        Self::spawn_as(upstream, client_name, None, timeout)
    }

    /// #NEW-148: [`Self::spawn`] as a 2026-07-28 client declaring
    /// `capabilities` ([`Self::connect_with`]).
    pub fn spawn_with(upstream: &StdioUpstream, client_name: &str, capabilities: &Value, timeout: Duration) -> Result<Client, ClientError> {
        Self::spawn_as(upstream, client_name, Some(capabilities), timeout)
    }

    fn spawn_as(upstream: &StdioUpstream, client_name: &str, capabilities: Option<&Value>, timeout: Duration) -> Result<Client, ClientError> {
        let spec = upstream.clone();
        let spawned = block_on(async move {
            let mut command = tokio::process::Command::new(&spec.command);
            command
                .args(&spec.args)
                .envs(spec.env.iter().map(|(name, value)| (name.as_str(), value.as_str())))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);
            let mut child = command.spawn().map_err(|err| ClientError::Transport(format!("{}: could not start {} ({err})", spec.name, spec.command)))?;
            let stdin = child.stdin.take().ok_or_else(|| ClientError::Transport(format!("{}: no stdin", spec.name)))?;
            let stdout = child.stdout.take().ok_or_else(|| ClientError::Transport(format!("{}: no stdout", spec.name)))?;
            Ok::<_, ClientError>((child, stdout, stdin))
        })?;
        let (child, stdout, stdin) = spawned;
        let mut client = Self::start(&upstream.name, client_name, capabilities, move || (stdout, stdin), timeout)?;
        client._child = Some(Arc::new(std::sync::Mutex::new(child)));
        Ok(client)
    }

    /// The handshake over any rmcp client transport (the OAuth one). The
    /// transport is made on emcp's runtime: rmcp spawns its worker there.
    /// `capabilities` = `None`: the legacy `initialize` declaring none (as
    /// every host but the hub); `Some`: #NEW-148's 2026-07-28 lifecycle
    /// (`server/discover`, falling back to `initialize`), rmcp stamping the
    /// capabilities into every request's `_meta`.
    pub(crate) fn start<T, E, A>(
        name: &str,
        client_name: &str,
        capabilities: Option<&Value>,
        transport: impl FnOnce() -> T + Send + 'static,
        timeout: Duration,
    ) -> Result<Client, ClientError>
    where
        T: rmcp::transport::IntoTransport<RoleClient, E, A> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
        A: Send + 'static,
    {
        let name: Arc<str> = name.into();
        let config = client_config(client_name, capabilities.unwrap_or(&json!({})));
        let lifecycle = capabilities.map(|_| ClientLifecycleMode::Auto {
            preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            legacy_version: Some(rmcp::model::ProtocolVersion::LATEST_WITH_INITIALIZE),
        });
        let label = name.clone();
        let service = block_on(async move {
            timed(timeout, async {
                let started = match lifecycle {
                    Some(lifecycle) => config.serve_with_lifecycle(transport(), lifecycle).await,
                    None => config.serve(transport()).await,
                };
                started.map_err(|err| match err {
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
        Ok(Client { name, peer: service.peer().clone(), _service: Arc::new(service), _child: None })
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

    /// #NEW-148: ONE `tools/call` round (MRTR): `input_responses` and the
    /// echoed `request_state` on a retry. The answer as it came: complete,
    /// `input_required` (its requests by key, as JSON) or a task.
    pub fn call_once(
        &self,
        tool: &str,
        arguments: Value,
        input_responses: Option<Map<String, Value>>,
        request_state: Option<String>,
        timeout: Duration,
    ) -> Result<CallOutcome, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        let mut params: CallToolRequestParams = serde_json::from_value(json!({ "name": tool, "arguments": arguments }))
            .map_err(|err| ClientError::Transport(format!("{tool}: the arguments are not an object ({err})")))?;
        params.input_responses = input_responses.map(|responses| responses.into_iter().collect());
        params.request_state = request_state;
        block_on(async move {
            timed(timeout, async {
                let json = |value: Result<Value, serde_json::Error>| value.map_err(|err| ClientError::Transport(format!("{name}: {err}")));
                match peer.call_tool_once(params).await.map_err(|err| service_error(&name, err))? {
                    CallToolResponse::Complete(result) => Ok(CallOutcome::Complete(json(serde_json::to_value(result))?)),
                    CallToolResponse::InputRequired(result) => {
                        let state = result.request_state.clone();
                        let value = json(serde_json::to_value(&result))?;
                        let requests = value
                            .get("inputRequests")
                            .and_then(Value::as_object)
                            .map(|requests| requests.iter().map(|(key, request)| (key.clone(), request.clone())).collect())
                            .unwrap_or_default();
                        Ok(CallOutcome::InputRequired { requests, state })
                    }
                    CallToolResponse::Task(task) => Ok(CallOutcome::Task(json(serde_json::to_value(task))?)),
                    _ => Err(ClientError::Transport(format!("{name}: an unknown tools/call answer"))),
                }
            })
            .await
        })
    }

    /// #NEW-148: `tasks/get`: the task with its payload (`status`,
    /// `statusMessage`, `pollIntervalMs`, `inputRequests`, `result`,
    /// `error`) as JSON.
    pub fn get_task(&self, task_id: &str, timeout: Duration) -> Result<Value, ClientError> {
        let (peer, name, params) = (self.peer.clone(), self.name.clone(), GetTaskParams::new(task_id));
        block_on(async move {
            timed(timeout, async {
                let task = peer.get_task(params).await.map_err(|err| service_error(&name, err))?;
                serde_json::to_value(task).map_err(|err| ClientError::Transport(format!("{name}: {err}")))
            })
            .await
        })
    }

    /// #NEW-148: `tasks/update {taskId, inputResponses}`.
    pub fn update_task(&self, task_id: &str, input_responses: Map<String, Value>, timeout: Duration) -> Result<(), ClientError> {
        let params = UpdateTaskParams::new(task_id, input_responses.into_iter().collect::<BTreeMap<_, _>>());
        let (peer, name) = (self.peer.clone(), self.name.clone());
        block_on(async move { timed(timeout, async { peer.update_task(params).await.map_err(|err| service_error(&name, err)) }).await })
    }

    /// #NEW-148: `tasks/cancel` (cooperative).
    pub fn cancel_task(&self, task_id: &str, timeout: Duration) -> Result<(), ClientError> {
        let (peer, name, params) = (self.peer.clone(), self.name.clone(), CancelTaskParams::new(task_id));
        block_on(async move { timed(timeout, async { peer.cancel_task(params).await.map_err(|err| service_error(&name, err)) }).await })
    }

    /// #NEW-148: whether the server declared the skills extension.
    pub fn supports_skills(&self) -> bool {
        self.peer.peer_info().is_some_and(|info| info.capabilities.supports_skills())
    }

    /// #NEW-148: `skills/list`, every page; each skill as JSON (`uri`,
    /// `frontmatter`, `resources`).
    pub fn list_skills(&self, timeout: Duration) -> Result<Vec<Value>, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        block_on(async move {
            timed(timeout, async {
                let skills = peer.list_all_skills().await.map_err(|err| service_error(&name, err))?;
                Ok(skills.into_iter().filter_map(|skill| serde_json::to_value(skill).ok()).collect())
            })
            .await
        })
    }

    /// #NEW-148: `skills/get {uri}`: the skill entry as JSON.
    pub fn get_skill(&self, uri: &str, timeout: Duration) -> Result<Value, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        let params: GetSkillRequestParams =
            serde_json::from_value(json!({ "uri": uri })).map_err(|err| ClientError::Transport(format!("{name}: {err}")))?;
        block_on(async move {
            timed(timeout, async {
                let result = peer.get_skill(params).await.map_err(|err| service_error(&name, err))?;
                serde_json::to_value(result.skill).map_err(|err| ClientError::Transport(format!("{name}: {err}")))
            })
            .await
        })
    }

    /// #NEW-148: `resources/read {uri}`: the bytes of its first content
    /// (a text's UTF-8, a blob's base64 decoded).
    pub fn read_resource(&self, uri: &str, timeout: Duration) -> Result<Vec<u8>, ClientError> {
        let (peer, name) = (self.peer.clone(), self.name.clone());
        let params: ReadResourceRequestParams =
            serde_json::from_value(json!({ "uri": uri })).map_err(|err| ClientError::Transport(format!("{name}: {err}")))?;
        let uri = uri.to_string();
        block_on(async move {
            timed(timeout, async {
                let result = match peer.read_resource_once(params).await.map_err(|err| service_error(&name, err))? {
                    ReadResourceResponse::Complete(result) => result,
                    _ => return Err(ClientError::Transport(format!("{name}: {uri} asked for input"))),
                };
                let value = serde_json::to_value(result).map_err(|err| ClientError::Transport(format!("{name}: {err}")))?;
                let content = value.pointer("/contents/0").ok_or_else(|| ClientError::Transport(format!("{name}: {uri} is empty")))?;
                if let Some(text) = content.get("text").and_then(Value::as_str) {
                    return Ok(text.as_bytes().to_vec());
                }
                use base64::Engine;
                let blob = content.get("blob").and_then(Value::as_str).unwrap_or_default();
                base64::engine::general_purpose::STANDARD
                    .decode(blob)
                    .map_err(|err| ClientError::Transport(format!("{name}: {uri} is not base64 ({err})")))
            })
            .await
        })
    }
}

/// #NEW-148: one `tools/call` round's answer ([`Client::call_once`]).
#[derive(Clone, Debug, PartialEq)]
pub enum CallOutcome {
    /// The call result (`content`, `isError`, …).
    Complete(Value),
    /// MRTR: the requests to fulfil by key (`{method, params}`), and the
    /// `requestState` to echo.
    InputRequired { requests: BTreeMap<String, Value>, state: Option<String> },
    /// The task the server made (`taskId`, `status`, `pollIntervalMs`, …).
    Task(Value),
}

/// One-shot: connect, call `tool`, drop the connection. For callers that
/// make a single call per process (the CLI) or per user action.
pub fn call_once(upstream: &Upstream, client_name: &str, tool: &str, arguments: Value, timeout: Duration) -> Result<Value, ClientError> {
    Client::connect(upstream, client_name, timeout)?.call(tool, arguments, timeout)
}
