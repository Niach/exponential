//! The loopback MCP server: rmcp's Streamable HTTP service, STATELESS with
//! JSON responses (a `GET` stream is refused, which the transport allows),
//! on `127.0.0.1:<free port>/mcp`. One listener per [`Server`]; each run
//! reaches it with its own bearer token ([`Server::grant`]), which is what
//! names the run: an unknown token is a 401 before rmcp sees the request,
//! and a revoked one reads why on its next `tools/call` (#EXP-1918). The
//! tools are a [`ToolProvider`]'s, called on the blocking
//! pool with the run as [`Caller`].
//!
//! [`RawRoute`]s share the listener for what is not an MCP server of ours
//! (codemode's gateway, a transparent proxy): they own their auth.
//!
//! #EXP-1517: the token, the session id and the refusal are eCore's
//! ([`Token`], [`SessionId`], [`Failure`]).

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use ecore::{ErrorCode, Failure, SessionId, Token};
use http_body_util::{combinators::BoxBody, BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, InitializeRequestParams, InitializeResult, ListToolsResult, PaginatedRequestParams,
    ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};

use crate::runtime::runtime;

/// A raw route's request body cap; an MCP POST's is rmcp's (4 MiB).
const RAW_BODY_MAX_BYTES: usize = 4 * 1024 * 1024;
/// How long a failed accept waits before the next one.
const ACCEPT_RETRY: std::time::Duration = std::time::Duration::from_millis(100);
/// How many revoked tokens are remembered, the newest kept.
const ENDED_KEPT: usize = 256;

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
    /// EXP-1507: the `tools/list` of ONE run (a provider whose list depends
    /// on the caller's grant, the [`crate::hub`]); the default is
    /// [`Self::tools`].
    fn tools_for(&self, _caller: &Caller) -> Result<Vec<Value>, String> {
        self.tools()
    }
    /// EXP-1507: [`Self::call`], or a protocol-level refusal (MCP's
    /// unknown-tool error for a name the run may not see); the default
    /// always calls.
    fn try_call(&self, caller: &Caller, name: &str, arguments: Value) -> Result<Value, Refusal> {
        Ok(self.call(caller, name, arguments))
    }
    /// #EDE-126: what ONE run reads at `initialize`; `None` (the default)
    /// = the server's own text ([`ServerInfo::instructions`]).
    fn instructions_for(&self, _caller: &Caller) -> Option<String> {
        None
    }
    /// The run's grant ended ([`Server::revoke`]).
    fn ended(&self, _session_id: &SessionId) {}
}

/// A call a provider refuses as a JSON-RPC error rather than as content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a tool of this run: `-32602`, the text never hinting it exists.
    UnknownTool,
}

/// MCP's unknown-tool error code (JSON-RPC invalid params).
pub const UNKNOWN_TOOL_CODE: i32 = -32602;

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
    /// The tokens [`Server::revoke`] ended, oldest first: their run's next
    /// call is told so instead of a bare 401.
    ended: Mutex<VecDeque<Token>>,
}

impl Grants {
    fn get(&self, token: &Token) -> Option<Caller> {
        self.runs.lock().unwrap().get(token).cloned()
    }

    /// Whether `session_id` holds a grant.
    fn admits(&self, session_id: &SessionId) -> bool {
        self.runs.lock().unwrap().values().any(|run| run.session_id == *session_id)
    }

    /// Take the runs `ends` names out; their tokens are remembered as ended.
    fn end(&self, ends: impl Fn(&Caller) -> bool) -> Vec<Caller> {
        let mut gone = Vec::new();
        self.runs.lock().unwrap().retain(|token, run| {
            let ended = ends(run);
            if ended {
                gone.push((token.clone(), run.clone()));
            }
            !ended
        });
        let mut ended = self.ended.lock().unwrap();
        let mut runs = Vec::new();
        for (token, run) in gone {
            ended.push_back(token);
            runs.push(run);
        }
        while ended.len() > ENDED_KEPT {
            ended.pop_front();
        }
        runs
    }

    /// Whether `token` was a run's until it was revoked.
    fn ended(&self, token: &Token) -> bool {
        self.ended.lock().unwrap().contains(token)
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
        let listener = Arc::new(listener);
        let grants = Arc::new(Grants::default());
        let handler = Handler { info: Arc::new(info), provider: provider.clone(), grants: grants.clone() };
        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None);
        let service = Arc::new(StreamableHttpService::new(
            move || Ok(handler.clone()),
            Arc::new(NeverSessionManager::default()),
            config,
        ));
        let state = Arc::new(Listener { grants: grants.clone(), service, routes, path: path.to_string(), name: name.clone() });
        runtime().spawn(accept_loop(
            name,
            move || {
                let listener = listener.clone();
                async move { listener.accept().await.map(|(stream, _)| stream) }
            },
            move |stream| {
                let state = state.clone();
                tokio::spawn(async move {
                    let io = hyper_util::rt::TokioIo::new(stream);
                    let service = hyper::service::service_fn(move |request| {
                        let state = state.clone();
                        async move { Ok::<_, Infallible>(state.route(request).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, service).await;
                });
            },
        ));
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
        self.grants.end(|run| run.session_id == *session_id);
        self.provider.ended(session_id);
    }

    /// Every run ends (the host is shutting down).
    pub fn revoke_all(&self) {
        for run in self.grants.end(|_| true) {
            self.provider.ended(&run.session_id);
        }
    }

    /// The run a token admits.
    pub fn admitted(&self, token: &Token) -> Option<Caller> {
        self.grants.get(token)
    }
}

/// The listener's loop: every accepted connection goes to `serve`. A failed
/// accept (the process is out of file descriptors: the connection stays
/// pending, so the next accept fails at once) waits [`ACCEPT_RETRY`] and is
/// logged once per run of failures.
async fn accept_loop<S, F>(name: String, mut accept: impl FnMut() -> F, mut serve: impl FnMut(S))
where
    F: std::future::Future<Output = std::io::Result<S>>,
{
    let mut failing = false;
    loop {
        match accept().await {
            Ok(stream) => {
                failing = false;
                serve(stream);
            }
            Err(err) => {
                if !std::mem::replace(&mut failing, true) {
                    log::warn!("[emcp] {name}: accept failed, trying again every {ACCEPT_RETRY:?}: {err}");
                }
                tokio::time::sleep(ACCEPT_RETRY).await;
            }
        }
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
    /// The server's name, for what a revoked run reads.
    name: String,
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
        let Some(caller) = token.as_ref().and_then(|token| self.grants.get(token)) else {
            return match token.is_some_and(|token| self.grants.ended(&token)) {
                true => ended(&self.name, request).await,
                false => status(401),
            };
        };
        let (mut parts, body) = request.into_parts();
        // rmcp hands the request's parts to the handler: the caller rides in
        // their extensions.
        parts.extensions.insert(caller);
        self.service.handle(Request::from_parts(parts, body)).await
    }
}

/// #EXP-1918: the answer to a run whose grant was revoked (its switch went
/// off, or the session ended). An MCP client reads a bare 401 as "sign in"
/// and the model never learns why its tools stopped: a `tools/call` gets a
/// result the agent reads; anything else stays a 401, the reason its body.
async fn ended(name: &str, request: Request<Incoming>) -> Response<Body> {
    let text = format!(
        "The {name} server ended this session's access: it was switched off on this device, or the session ended. Its tools stay unavailable; do not retry."
    );
    let body = Limited::new(request.into_body(), RAW_BODY_MAX_BYTES).collect().await.ok().map(|collected| collected.to_bytes());
    let message: Option<Value> = body.and_then(|body| serde_json::from_slice(&body).ok());
    let call_id = message
        .filter(|message| message.get("method").and_then(Value::as_str) == Some("tools/call"))
        .and_then(|message| message.get("id").cloned());
    let Some(id) = call_id else {
        let mut response = Response::new(Full::new(Bytes::from(text)).boxed());
        *response.status_mut() = StatusCode::UNAUTHORIZED;
        return response;
    };
    let answer = json!({ "jsonrpc": "2.0", "id": id, "result": error_result(text) });
    let mut response = Response::new(Full::new(Bytes::from(answer.to_string())).boxed());
    response.headers_mut().insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static("application/json"));
    response
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
    grants: Arc<Grants>,
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

/// The run a request belongs to (the listener put it in the parts).
fn caller_of(context: &RequestContext<RoleServer>) -> Result<Caller, ErrorData> {
    context
        .extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Caller>())
        .cloned()
        .ok_or_else(|| refusal(Failure::new(ErrorCode::Unauthorized, "This session's grant ended.")))
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

    /// rmcp's own `initialize`, with the run's text in place of the
    /// server's when its provider has one ([`ToolProvider::instructions_for`]).
    async fn initialize(&self, request: InitializeRequestParams, context: RequestContext<RoleServer>) -> Result<InitializeResult, ErrorData> {
        context.peer.set_peer_info(request.clone());
        let mut result = self.negotiate_initialize(&request)?;
        if let Some(own) = caller_of(&context).ok().and_then(|caller| self.provider.instructions_for(&caller)) {
            result.instructions = Some(own);
        }
        Ok(result)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let provider = self.provider.clone();
        let caller = caller_of(&context)?;
        let tools = tokio::task::spawn_blocking(move || provider.tools_for(&caller))
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
        let caller = caller_of(&context)?;
        let provider = self.provider.clone();
        let name = request.name.to_string();
        let arguments = request.arguments.map(Value::Object).unwrap_or_else(|| json!({}));
        let called = name.clone();
        let session_id = caller.session_id.clone();
        let result = tokio::task::spawn_blocking(move || provider.try_call(&caller, &called, arguments))
            .await
            .map_err(|err| ErrorData::internal_error(err.to_string(), None))?
            .map_err(|Refusal::UnknownTool| {
                ErrorData::new(rmcp::model::ErrorCode(UNKNOWN_TOOL_CODE), format!("Unknown tool: {name}"), None)
            })?;
        // #EXP-1894: the run ended while its call ran: what the call saw
        // or did is not its to read any more.
        if !self.grants.admits(&session_id) {
            return Err(refusal(Failure::new(ErrorCode::Unauthorized, "This session's grant ended.")));
        }
        let result = serde_json::from_value::<CallToolResult>(result)
            .unwrap_or_else(|err| error_result(format!("The tool answered a malformed result: {err}")));
        Ok(CallToolResponse::Complete(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A failing accept (the process is out of file descriptors) is tried
    /// again after a pause, never in a busy loop.
    #[test]
    fn a_failing_accept_waits_before_the_next_one() {
        let tries = Arc::new(Mutex::new(Vec::<Instant>::new()));
        let seen = tries.clone();
        let accept = move || {
            let seen = seen.clone();
            async move {
                let nth = {
                    let mut seen = seen.lock().unwrap();
                    seen.push(Instant::now());
                    seen.len()
                };
                if nth > 3 {
                    std::future::pending::<()>().await;
                }
                Err::<(), _>(std::io::Error::from_raw_os_error(24))
            }
        };
        let task = runtime().spawn(accept_loop("test".to_string(), accept, |_| {}));
        let deadline = Instant::now() + Duration::from_secs(10);
        while tries.lock().unwrap().len() < 4 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        task.abort();
        let tries = tries.lock().unwrap().clone();
        assert_eq!(tries.len(), 4);
        for pair in tries.windows(2) {
            assert!(pair[1] - pair[0] >= Duration::from_millis(50), "accept was tried again after {:?}", pair[1] - pair[0]);
        }
    }
}
