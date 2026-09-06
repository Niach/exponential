//! EXP-746 (D13) — ExternalAgent: a user-declared binary that already speaks
//! ACP over stdio. Owned by lane E4.
//!
//! Opt-in and settings-driven (`coding::Settings::external_agents`), offered
//! only when the command resolves on `terminal::pty::login_path()`, local
//! starts only — `coding_sessions.start` carries `agent: None` for one and it
//! is never remotely startable.
//!
//! `agent_client_protocol::AcpAgent` already IS a `ConnectTo<Client>`
//! subprocess transport, so there is no wire to write — but it must be
//! constructed from an explicit [`agent_client_protocol::AcpAgentConfig`].
//! NEVER `AcpAgent::claude_agent()` / `AcpAgent::codex()`: those shell out to
//! `npx -y @agentclientprotocol/...@latest`, i.e. network at runtime and an
//! unpinned supply chain.

use std::path::Path;

use agent_client_protocol::{Agent, Client, ConnectTo};

use super::AdapterSpec;
use crate::session::EngineError;

pub struct ExternalAgent {
    // EXP-746 E4: fill — the built `AcpAgent` plus the spec's label.
    #[allow(dead_code)]
    pub(crate) spec: AdapterSpec,
}

impl ExternalAgent {
    // EXP-746 E4: fill
    #[allow(unused_variables)]
    pub fn new(spec: AdapterSpec) -> Result<ExternalAgent, EngineError> {
        todo!("EXP-746 E4: build the external adapter")
    }
}

impl ConnectTo<Client> for ExternalAgent {
    // EXP-746 E4: fill
    #[allow(unused_variables)]
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            todo!("EXP-746 E4: delegate to the configured AcpAgent")
        }
    }
}

/// Build the ACP subprocess transport for one declared external agent.
///
/// CAVEAT for E4: `AcpAgentConfig` carries only command/args/env — there is
/// no working-directory setting, and the child would inherit the engine
/// process's cwd. The worktree must reach the binary some other way (an env
/// entry the spec declares, or the ACP `session/new { cwd }` the agent is
/// meant to honour); do not silently drop `cwd`.
// EXP-746 E4: fill
#[allow(unused_variables)]
pub fn external_adapter(
    spec: &coding::ExternalAgentSpec,
    cwd: &Path,
) -> Result<agent_client_protocol::AcpAgent, EngineError> {
    todo!("EXP-746 E4: AcpAgentConfig::new(command).args(..).envs(..) + the cwd decision")
}
