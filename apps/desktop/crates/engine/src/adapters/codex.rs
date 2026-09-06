//! EXP-746 — CodexAgent: a LONG-LIVED `codex app-server` connection presented
//! as an ACP agent. Owned by lane E3.
//!
//! Shape (E3): `initialize` (with `optOutNotificationMethods` for the ~35
//! notifications we would parse and drop, `process/outputDelta` and
//! `command/exec/outputDelta` first) → `initialized` → `thread/start` whose
//! `config` carries the MCP server, the project trust level and the writable
//! roots — that object REPLACES today's `-c mcp_servers.*` argv — then one
//! `turn/start` per prompt with the `agent-full-access` preset (D3).
//! Non-obvious invariants: cancellation marks the turn stale BEFORE
//! `turn/interrupt` and auto-answers in-flight approvals `"cancel"`;
//! `item/fileChange` carries FULL FILE CONTENT for `add`/`delete` and only
//! `update` is a real diff; every enum over methods, item types, effort and
//! service tier needs a catch-all (the installed codex is older than the one
//! codex-acp targets).

use agent_client_protocol::{Agent, Client, ConnectTo};

use super::AdapterSpec;
use crate::session::EngineError;

pub struct CodexAgent {
    // EXP-746 E3: fill — the AppServer connection, the thread id, the current
    // turn id + stale fence, the item table.
    #[allow(dead_code)]
    pub(crate) spec: AdapterSpec,
}

impl CodexAgent {
    // EXP-746 E3: fill
    #[allow(unused_variables)]
    pub fn new(spec: AdapterSpec) -> Result<CodexAgent, EngineError> {
        todo!("EXP-746 E3: build the codex adapter")
    }
}

impl ConnectTo<Client> for CodexAgent {
    // EXP-746 E3: fill
    #[allow(unused_variables)]
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            todo!("EXP-746 E3: drive the codex app-server as an ACP agent")
        }
    }
}
