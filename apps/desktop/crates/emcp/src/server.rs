//! The loopback MCP server: rmcp's Streamable HTTP service, STATELESS with
//! JSON responses (a `GET` stream is refused, which the transport allows),
//! on `127.0.0.1:<free port>/mcp`. One listener per [`Server`]; each run
//! reaches it with its own bearer token ([`Server::grant`]), which is what
//! names the run: an unknown or revoked token is a 401 before rmcp sees
//! the request. The tools are a [`ToolProvider`]'s, called on the blocking
//! pool with the run as [`Caller`].
//!
//! [`RawRoute`]s share the listener for what is not an MCP server of ours
//! (codemode's gateway, a transparent proxy): they own their auth.
//!
//! #EXP-1517: the token, the session id and the refusal are eCore's
//! ([`Token`], [`SessionId`], [`Failure`]).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use ecore::{ErrorCode, Failure, SessionId, Token};
use http_body_util::{combinators::BoxBody, BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use rmcp::model::{CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult, PaginatedRequestParams, ServerConfig, Tool};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};

use crate::runtime::runtime;

/// A raw route's request body cap; an MCP POST's is rmcp's (4 MiB).
const RAW_BODY_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Who `initialize` says this server is.
#[derive(Clone, Debug)]
pub struct ServerInfo {
    /// The server name (`serverInfo.name`): also the config key agents know
    /// it by (`mcp__<name>__<tool>`).
    pub name: String,
    pub version: String,
    /// What the agent reads at `initialize`.
    pub instructions: Option<String>,
}

/// The run a call belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub session_id: SessionId,
    /// The run's name, for logs and banners.
    pub label: String,
}

/// What the server asks of its host.
pub trait ToolProvider: Send + Sync + 'static {
    /// The `tools/list` tools, as MCP tool objects (`name`, `description`,
    /// `inputSchema`, …). An `Err` answers `tools/list` with an internal
    /// error carrying the text.
    fn tools(&self) -> Result<Vec<Value>, String>;
    /// One `tools/call`, answered as an MCP call result (`content`,
    /// `structuredContent`, `isError`, `_meta`); a refusal is content the
    /// agent must read, never a protocol error.
    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value;
    /// The run's grant ended ([`Server::revoke`]).
    fn ended(&self, _session_id: &SessionId) {}
}

/// A request to a [`RawRoute`].
#[derive(Clone, Debug)]
pub struct RawRequest {
    pub method: String,
    /// The path and query (`/gateway/api/mcp`).
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// `None` when the body could not be read whole (over the cap).
    pub body: Option<Vec<u8>>,
}

impl RawRequest {
    /// A header's value, its name matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    /// The `Authorization: Bearer` token, verbatim: a raw route's tokens
    /// are its own (codemode's gateway mints `gw…`), not eCore [`Token`]s.
    pub fn bearer(&self) -> Option<&str> {
        self.header("authorization").and_then(|value| value.strip_prefix("Bearer "))
    }
}

/// A [`RawRoute`]'s answer.
#[derive(Clone, Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawResponse {
    pub fn empty(status: u16) -> Self {
        RawResponse { status, headers: Vec::new(), body: Vec::new() }
    }
}

/// Something on the listener that is not this server's MCP endpoint,
/// called on the blocking pool for every request under its prefix.
pub trait RawRoute: Send + Sync + 'static {
    fn handle(&self, request: RawRequest) -> RawResponse;
}

/// Token → run.
#[derive(Default)]
struct Grants {
    runs: Mutex<HashMap<Token, Caller>>,
}

impl Grants {
    fn get(&self, token: &Token) -> Option<Caller> {
        self.runs.lock().unwrap().get(token).cloned()
    }
}

/// A fresh bearer token: 64 hex characters of two v4 UUIDs (eCore does no
/// randomness, so the minting stays here).
pub fn new_token() -> Token {
    Token::new(format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()))
        .expect("two simple v4 UUIDs are 64 lowercase hex")
}

/// A running loopback server. It lives for the process (hosts keep it in a
/// `OnceLock`); dropping it stops nothing.
pub struct Server {
    url: String,
    base: String,
    grants: Arc<Grants>,
    provider: Arc<dyn ToolProvider>,
}

impl Server {
    /// Bind `127.0.0.1:0` and serve `provider`.
    pub fn serve(info: ServerInfo, provider: Arc<dyn ToolProvider>) -> Result<Server, String> {
        Self::serve_with(info, provider, Vec::new())
    }

    /// [`Self::serve`] plus raw routes, each owning every path under its
    /// prefix (`/gateway`).
    pub fn serve_with(
        info: ServerInfo,
        provider: Arc<dyn ToolProvider>,
        routes: Vec<(String, Arc<dyn RawRoute>)>,
    ) -> Result<Server, String> {
        Self::serve_at(info, provider, routes, "/mcp")
    }

    /// [`Self::serve_with`] with the MCP endpoint at `path` (`/api/mcp`
    /// for a stand-in of the instance's own endpoint).
    pub fn serve_at(
        info: ServerInfo,
        provider: Arc<dyn ToolProvider>,
        routes: Vec<(String, Arc<dyn RawRoute>)>,
        path: &str,
    ) -> Result<Server, String> {
        let name = info.name.clone();
        let listener = runtime()
            .block_on(async { tokio::net::TcpListener::bind("127.0.0.1:0").await })
            .map_err(|err| format!("Could not start the {name} server: {err}"))?;
        let port = listener.local_addr().map_err(|err| format!("The {name} server has no address: {err}"))?.port();
        let grants = Arc::new(Grants::default());
        let handler = Handler { info: Arc::new(info), provider: provider.clone() };
        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None);
        let service = Arc::new(StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(NeverSessionManager::default()),
            config,
        ));
        let state = Arc::new(Listener { grants: grants.clone(), service, routes, path: path.to_string() });
        runtime().spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { continue };
                let state = state.clone();
                tokio::spawn(async move {
                    let io = hyper_util::rt::TokioIo::new(stream);
                    let service = hyper::service::service_fn(move |request| {
                        let state = state.clone();
                        async move { Ok::<_, Infallible>(state.route(request).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, service).await;
                });
            }
        });
        Ok(Server { url: format!("http://127.0.0.1:{port}{path}"), base: format!("http://127.0.0.1:{port}"), grants, provider })
    }

    /// `http://127.0.0.1:<port>/mcp`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// `http://127.0.0.1:<port>`, for a raw route's URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// A fresh token for `session_id`; an earlier one of the same run dies.
    pub fn grant(&self, session_id: &SessionId, label: &str) -> Token {
        let token = new_token();
        let mut runs = self.grants.runs.lock().unwrap();
        runs.retain(|_, run| run.session_id != *session_id);
        runs.insert(token.clone(), Caller { session_id: session_id.clone(), label: label.to_string() });
        token
    }

    /// The run's token dies and the provider hears it ended.
    pub fn revoke(&self, session_id: &SessionId) {
        self.grants.runs.lock().unwrap().retain(|_, run| run.session_id != *session_id);
        self.provider.ended(session_id);
    }

    /// Every run ends (the host is shutting down).
    pub fn revoke_all(&self) {
        let runs = std::mem::take(&mut *self.grants.runs.lock().unwrap());
        for run in runs.values() {
            self.provider.ended(&run.session_id);
        }
    }

    /// The run a token admits.
    pub fn admitted(&self, token: &Token) -> Option<Caller> {
        self.grants.get(token)
    }
}

type Body = BoxBody<Bytes, Infallible>;

fn status(code: u16) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::new()).boxed());
    *response.status_mut() = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response
}

struct Listener {
    grants: Arc<Grants>,
    service: Arc<StreamableHttpService<Handler, NeverSessionManager>>,
    routes: Vec<(String, Arc<dyn RawRoute>)>,
    path: String,
}

impl Listener {
    async fn route(&self, request: Request<Incoming>) -> Response<Body> {
        let path = request.uri().path().to_string();
        if let Some((_, route)) = self.routes.iter().find(|(prefix, _)| path.starts_with(prefix.as_str())) {
            return raw(route.clone(), request).await;
        }
        if path != self.path {
            return status(404);
        }
        let token = request
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(Token::from_header);
        let Some(caller) = token.and_then(|token| self.grants.get(&token)) else {
            return status(401);
        };
        let (mut parts, body) = request.into_parts();
        // rmcp hands the request's parts to the handler: the caller rides in
        // their extensions.
        parts.extensions.insert(caller);
        self.service.handle(Request::from_parts(parts, body)).await
    }
}

async fn raw(route: Arc<dyn RawRoute>, request: Request<Incoming>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let body = Limited::new(body, RAW_BODY_MAX_BYTES).collect().await.ok().map(|collected| collected.to_bytes().to_vec());
    let request = RawRequest {
        method: parts.method.as_str().to_string(),
        path: parts.uri.path_and_query().map(|p| p.as_str().to_string()).unwrap_or_default(),
        headers: parts
            .headers
            .iter()
            .filter_map(|(name, value)| Some((name.as_str().to_string(), value.to_str().ok()?.to_string())))
            .collect(),
        body,
    };
    let Ok(answer) = tokio::task::spawn_blocking(move || route.handle(request)).await else {
        return status(500);
    };
    let mut response = Response::new(Full::new(Bytes::from(answer.body)).boxed());
    *response.status_mut() = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in answer.headers {
        if let (Ok(name), Ok(value)) =
            (http::HeaderName::from_bytes(name.as_bytes()), http::HeaderValue::from_str(&value))
        {
            response.headers_mut().insert(name, value);
        }
    }
    response
}

/// The rmcp handler: a thin adapter onto the provider.
#[derive(Clone)]
struct Handler {
    info: Arc<ServerInfo>,
    provider: Arc<dyn ToolProvider>,
}

/// A tool object as rmcp's type; a missing `inputSchema` is an empty object
/// schema (MCP requires one).
pub(crate) fn tool_from_value(mut tool: Value) -> Option<Tool> {
    if let Some(object) = tool.as_object_mut() {
        object.entry("inputSchema").or_insert_with(|| json!({ "type": "object" }));
    }
    serde_json::from_value(tool).ok()
}

/// A refusal at the JSON-RPC level (the caller is not admitted, so no tool
/// result reaches it): the server error code, the failure's message.
fn refusal(failure: Failure) -> ErrorData {
    debug_assert!(failure.is_refusal());
    ErrorData::new(rmcp::model::ErrorCode(-32000), failure.message, None)
}

pub(crate) fn error_result(text: String) -> CallToolResult {
    serde_json::from_value(json!({ "content": [{ "type": "text", "text": text }], "isError": true }))
        .expect("a text error result")
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        let mut info = json!({
            "protocolVersion": rmcp::model::ProtocolVersion::LATEST,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": self.info.name, "version": self.info.version },
        });
        if let Some(instructions) = &self.info.instructions {
            info["instructions"] = json!(instructions);
        }
        serde_json::from_value(info).expect("a server info")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let provider = self.provider.clone();
        let tools = tokio::task::spawn_blocking(move || provider.tools())
            .await
            .map_err(|err| ErrorData::internal_error(err.to_string(), None))?
            .map_err(|text| ErrorData::internal_error(text, None))?;
        let tools: Vec<Tool> = tools.into_iter().filter_map(tool_from_value).collect();
        Ok(serde_json::from_value(json!({ "tools": tools })).expect("a tools list"))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let caller = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Caller>())
            .cloned()
            .ok_or_else(|| refusal(Failure::new(ErrorCode::Unauthorized, "This session's grant ended.")))?;
        let provider = self.provider.clone();
        let name = request.name.to_string();
        let arguments = request.arguments.map(Value::Object).unwrap_or_else(|| json!({}));
        let result = tokio::task::spawn_blocking(move || provider.call(&caller, &name, arguments))
            .await
            .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;
        let result = serde_json::from_value::<CallToolResult>(result)
            .unwrap_or_else(|err| error_result(format!("The tool answered a malformed result: {err}")));
        Ok(CallToolResponse::Complete(result))
    }
}
