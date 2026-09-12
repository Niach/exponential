//! EXP-746 — the ACP adapters.
//!
//! Each one presents the user's UNMODIFIED agent CLI as an ACP `Agent` to the
//! engine's `Client`, in-process: `Client.builder().connect_with(adapter,
//! main_fn)` creates the `Channel` pair itself and drives both halves in ONE
//! `Send` future, so there is no serialization at the seam and no second
//! runtime. The adapters are the ONLY place that knows an agent's private
//! wire; everything above them speaks ACP and nothing else.
//!
//! This module is landed COMPLETE by the foundation lane (every variant
//! stubbed) precisely so no adapter lane has to edit it.

pub mod claude;
pub mod claude_wire;
pub mod codex;
pub mod codex_wire;

use std::path::PathBuf;

use agent_client_protocol::{Agent, Client, ConnectTo};

use crate::host::ChildExitLink;
use crate::session::{EngineError, ResumeHandle};

/// Which adapter drives a run. Distinct from `coding::CodingAgent` only
/// because `steer::SessionAgent` is the WIRE vocabulary and this is the
/// engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    Claude,
    Codex,
}

impl AdapterKind {
    pub fn from_agent(agent: coding::CodingAgent) -> AdapterKind {
        match agent {
            coding::CodingAgent::Claude => AdapterKind::Claude,
            coding::CodingAgent::Codex => AdapterKind::Codex,
        }
    }

    /// The relay-side identity.
    pub fn session_agent(self) -> steer::SessionAgent {
        match self {
            AdapterKind::Claude => steer::SessionAgent::Claude,
            AdapterKind::Codex => steer::SessionAgent::Codex,
        }
    }

    /// Log/telemetry id.
    pub fn id(self) -> &'static str {
        match self {
            AdapterKind::Claude => "claude",
            AdapterKind::Codex => "codex",
        }
    }
}

/// Everything an adapter needs to spawn and configure its CLI. Built by the
/// engine from `coding::PreparedLaunch` + its `AcpLaunch`, so an ACP launch
/// inherits the exact program/cwd/env the PTY launch would have used.
pub struct AdapterSpec {
    pub kind: AdapterKind,
    pub agent: coding::CodingAgent,
    /// Program, cwd and env from `PreparedLaunch::spawn`; `args` is EMPTY on
    /// the Acp arm — the adapter composes the ACP argv itself.
    pub spawn: terminal::pty::SpawnSpec,
    pub options: coding::LaunchOptions,
    /// The per-agent MCP posture `wire_agent_mcp` already prepared.
    pub mcp: coding::AgentMcp,
    /// EXP-792: the launch's team MCP servers beside `exponential` — claude
    /// renders them into its inline `--mcp-config`, codex into the
    /// `thread/start` config. Values are `${VAR}` references, never
    /// credentials.
    pub servers: Vec<coding::McpServerWire>,
    /// The worktree (`session/new { cwd }`).
    pub cwd: PathBuf,
    /// The `coding_sessions` row id — rides the MCP `X-Exp-Session-Id` header.
    pub session_id: String,
    /// The seed prompt as TEXT: the engine sends it as `session/prompt`.
    /// PROMPT.md delivery is a TUI affordance and never happens here.
    pub prompt: Option<String>,
    pub resume: Option<ResumeHandle>,
    /// Read-only transcript replay (`EngineSession::open_transcript`): the
    /// session is loaded to be READ and never prompted. A live resume takes
    /// the same `session/load` route with this `false`, and an adapter that
    /// has to do more than read history to become steerable (codex:
    /// `thread/resume` + its notification pumps) keys on it.
    pub replay: bool,
    /// The `expu_` key — the codex MCP bearer.
    pub personal_key: Option<String>,
    /// The claude `--settings <path>` reaper anchor (`{}`, no hooks). `None`
    /// for every other agent, which the reaper never anchored either.
    pub reaper_settings_path: Option<PathBuf>,
    /// Where a stdio adapter reports its child's exit (`ChildLines::forward_exit`),
    /// so the run's bye is `exit:<code>`. An adapter that owns no child of
    /// ours never records, and the run ends as `ended`.
    pub exit: ChildExitLink,
}

/// Every adapter behind one type, so the host holds a single field.
pub enum Adapter {
    Claude(claude::ClaudeAgent),
    Codex(codex::CodexAgent),
}

impl Adapter {
    /// Build the adapter `spec.kind` names. Errors here are start-time
    /// errors and REFUSE the launch — EXP-773 left nothing to fall back to.
    ///
    /// Claude spawns lazily (at `session/new`), so a missing `claude` surfaces
    /// as a handshake failure through `EngineExit`; codex spawns HERE, so it
    /// comes back as `EngineError::Spawn` before anything was registered.
    pub fn new(spec: AdapterSpec) -> Result<Adapter, EngineError> {
        Ok(match spec.kind {
            AdapterKind::Claude => Adapter::Claude(claude::ClaudeAgent::new(spec)?),
            AdapterKind::Codex => Adapter::Codex(codex::CodexAgent::new(spec)?),
        })
    }

    pub fn kind(&self) -> AdapterKind {
        match self {
            Adapter::Claude(_) => AdapterKind::Claude,
            Adapter::Codex(_) => AdapterKind::Codex,
        }
    }
}

impl ConnectTo<Client> for Adapter {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            match self {
                Adapter::Claude(adapter) => adapter.connect_to(client).await,
                Adapter::Codex(adapter) => adapter.connect_to(client).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_kind_maps_to_an_adapter_and_a_wire_agent() {
        assert_eq!(
            AdapterKind::from_agent(coding::CodingAgent::Claude),
            AdapterKind::Claude
        );
        assert_eq!(
            AdapterKind::from_agent(coding::CodingAgent::Codex),
            AdapterKind::Codex
        );
        assert_eq!(
            AdapterKind::Claude.session_agent(),
            steer::SessionAgent::Claude
        );
        assert_eq!(
            AdapterKind::Codex.session_agent(),
            steer::SessionAgent::Codex
        );
    }
}
