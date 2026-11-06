//! [`AcpAgent`]: the user's own Claude Code / Codex over the Agent Client
//! Protocol, as an [`AgentProvider`]. Underneath it is exactly
//! [`LocalAgent::start`] (the same spawn, adapters, mapper and
//! [`crate::engine::NoBackend`]); this file only turns a [`Brief`] and a
//! [`HubGrant`] into its [`RunRequest`]. The run's tools are the hub, mounted
//! as ONE MCP server whose bearer token travels as a `${VAR}` reference into
//! the spawn env, never on argv or disk.

use std::sync::Arc;

use super::{AgentProvider, Brief, ErrorCode, EventSink, Failure, HubGrant, RunControl, RunHandle};
use crate::agent::CodingAgent;
use crate::engine::EngineError;
use crate::local::{LocalAgent, LocalRun, RunRequest};
use crate::mcp::{McpServerWire, McpWireTransport};

/// The config key the hub is mounted under in the CLI's MCP config.
pub const HUB_SERVER_NAME: &str = "hub";

/// The spawn env var carrying the run's hub token.
pub const HUB_TOKEN_ENV: &str = "EXP_HUB_TOKEN";

/// Claude Code or Codex, spawned locally (local agent runs, #EDE-58).
pub struct AcpAgent {
    agent: LocalAgent,
    runtime: Option<tokio::runtime::Handle>,
}

impl AcpAgent {
    /// `agent`'s default binary on the login shell's PATH.
    pub fn new(agent: CodingAgent) -> AcpAgent {
        AcpAgent { agent: LocalAgent::new(agent), runtime: None }
    }

    /// An explicit binary (a name on PATH or an absolute path).
    pub fn with_program(agent: CodingAgent, program: impl Into<String>) -> AcpAgent {
        AcpAgent { agent: LocalAgent::with_program(agent, program), runtime: None }
    }

    /// Drive every run's connection on the host's tokio runtime (`None` =
    /// each run owns one).
    pub fn runtime(mut self, handle: tokio::runtime::Handle) -> Self {
        self.runtime = Some(handle);
        self
    }

    /// The facade underneath (detect, login, history).
    pub fn local(&self) -> &LocalAgent {
        &self.agent
    }

    /// The hub as the run's one extra MCP server.
    pub fn hub_server(tools: &HubGrant) -> McpServerWire {
        McpServerWire {
            id: HUB_SERVER_NAME.to_string(),
            name: HUB_SERVER_NAME.to_string(),
            transport: McpWireTransport::Http { url: tools.endpoint.trim().to_string() },
            headers: vec![("Authorization".to_string(), format!("Bearer ${{{HUB_TOKEN_ENV}}}"))],
            token_env: Some(HUB_TOKEN_ENV.to_string()),
            env: Vec::new(),
            actor: None,
        }
    }

    /// The run request a start hands [`LocalAgent::start`].
    pub fn request(&self, brief: Brief, tools: &HubGrant, sink: Arc<dyn EventSink>) -> Result<RunRequest, Failure> {
        if let Some(refusal) = tools.refusal() {
            return Err(refusal);
        }
        let Some(cwd) = brief.cwd else {
            return Err(Failure::new(ErrorCode::Invalid, "an agent run needs a working directory"));
        };
        let mut request = RunRequest::new(cwd)
            .mcp_server(Self::hub_server(tools))
            .env(HUB_TOKEN_ENV, tools.token.as_str())
            .system_append(brief.system_append)
            .sink(sink);
        if !brief.prompt.trim().is_empty() {
            request = request.prompt(brief.prompt);
        }
        if let Some(resume) = brief.resume {
            request = request.resume(resume);
        }
        if let Some(handle) = &self.runtime {
            request = request.runtime(handle.clone());
        }
        request.options = brief.options;
        Ok(request)
    }
}

/// A start error as eCore's failure shape.
pub fn engine_failure(err: &EngineError) -> Failure {
    let code = match err {
        EngineError::Spawn(_) | EngineError::Handshake(_) => ErrorCode::Unavailable,
        EngineError::Api(_) => ErrorCode::Internal,
        EngineError::Unsupported(_) => ErrorCode::Invalid,
    };
    Failure::new(code, &err.to_string())
}

impl AgentProvider for AcpAgent {
    fn id(&self) -> &str {
        self.agent.agent().id()
    }

    fn start(&self, brief: Brief, tools: HubGrant, sink: Arc<dyn EventSink>) -> Result<RunHandle, Failure> {
        let session_id = brief.session_id.clone();
        let request = self.request(brief, &tools, sink)?;
        let run = self.agent.start(request).map_err(|err| engine_failure(&err))?;
        Ok(RunHandle::new(self.id(), session_id, AcpRun(run)))
    }
}

struct AcpRun(LocalRun);

impl RunControl for AcpRun {
    fn stop(&self) {
        self.0.stop();
    }

    fn is_done(&self) -> bool {
        self.0.is_done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RecordingSink;
    use crate::provider::tests::{grant, session, token};

    fn brief() -> Brief {
        Brief::new(session(), "fix the bug").cwd("/tmp/work").system_append("be brief").resume("native-1")
    }

    #[test]
    fn the_id_is_the_agents_own() {
        assert_eq!(AcpAgent::new(CodingAgent::Claude).id(), "claude");
        assert_eq!(AcpAgent::with_program(CodingAgent::Codex, "/opt/codex").id(), "codex");
    }

    /// The brief and the grant land on the unchanged run request: the hub is
    /// the run's one extra server, its token only in the spawn env.
    #[test]
    fn a_brief_and_a_grant_become_the_run_request() {
        let sink: Arc<dyn EventSink> = RecordingSink::new();
        let request = AcpAgent::new(CodingAgent::Claude).request(brief(), &grant(), sink.clone()).unwrap();
        assert_eq!(request.cwd, std::path::PathBuf::from("/tmp/work"));
        assert_eq!(request.prompt.as_deref(), Some("fix the bug"));
        assert_eq!(request.resume.as_deref(), Some("native-1"));
        assert_eq!(request.system_append, "be brief");
        assert_eq!(request.servers, vec![AcpAgent::hub_server(&grant())]);
        assert_eq!(request.env, vec![(HUB_TOKEN_ENV.to_string(), token().as_str().to_string())]);
        assert!(Arc::ptr_eq(request.sink.as_ref().unwrap(), &sink), "events go to the caller's sink");

        let claude: serde_json::Value = serde_json::from_str(
            &crate::engine::adapters::claude_wire::servers_mcp_config(&request.servers),
        )
        .unwrap();
        assert_eq!(claude["mcpServers"]["hub"]["headers"]["Authorization"], "Bearer ${EXP_HUB_TOKEN}");
        assert!(!claude.to_string().contains(token().as_str()), "the token never lands in the config");
        let entries = crate::mcp::codex_server_entries(&request.servers);
        let table = crate::engine::adapters::codex_wire::mcp_servers_table(&entries);
        assert_eq!(table["hub"]["bearer_token_env_var"], HUB_TOKEN_ENV);
    }

    #[test]
    fn a_blank_prompt_sends_nothing_and_no_cwd_is_refused() {
        let agent = AcpAgent::new(CodingAgent::Codex);
        let request = agent
            .request(Brief::new(session(), "  ").cwd("/tmp/work"), &grant(), RecordingSink::new())
            .unwrap();
        assert!(request.prompt.is_none());
        let refused = agent.request(Brief::new(session(), "hi"), &grant(), RecordingSink::new()).err().unwrap();
        assert_eq!(refused.code, ErrorCode::Invalid);
        let mut bad = grant();
        bad.endpoint = String::new();
        let refused = agent.request(brief(), &bad, RecordingSink::new()).err().unwrap();
        assert_eq!(refused.code, ErrorCode::Invalid);
    }

    #[test]
    fn start_errors_map_onto_ecore_failures() {
        let missing = engine_failure(&EngineError::Spawn(std::io::Error::from(std::io::ErrorKind::NotFound)));
        assert_eq!(missing.code, ErrorCode::Unavailable);
        assert!(missing.retryable);
        assert_eq!(engine_failure(&EngineError::Handshake("eof".into())).code, ErrorCode::Unavailable);
        assert_eq!(engine_failure(&EngineError::Api("boom".into())).code, ErrorCode::Internal);
        let unsupported = engine_failure(&EngineError::Unsupported("no adapter"));
        assert_eq!(unsupported.code, ErrorCode::Invalid);
        assert_eq!(unsupported.message, "no adapter");
    }
}
