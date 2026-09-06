//! EXP-746 — PiAgent: `pi --mode rpc` presented as an ACP agent. Owned by
//! lane E4.
//!
//! Shape (E4): `pi --mode rpc [--model] [--thinking] [--session <file>] -e
//! ./.exp-pi-mcp.ts`. The observer and plan extensions are DROPPED on this
//! path (the rpc stream is the observation channel); the MCP bridge stays
//! because pi has no native MCP. Non-obvious invariants: the stream is
//! strict LF-only JSONL and mixes four shapes discriminated only by `type`,
//! unknown types are ignorable rather than fatal; `agent_settled` is the true
//! idle edge; pi is the one agent that reports COST; and pi is FILE-PATH
//! keyed, not id-keyed, so its resume handle is a path.
//!
//! Plan mode is deliberately absent here: pi's plan mode IS the injected
//! `.exp-pi-plan.ts` extension, so `coding::resolve_transport` sends a
//! plan-mode pi launch down the terminal transport instead.

use agent_client_protocol::{Agent, Client, ConnectTo};

use super::AdapterSpec;
use crate::session::EngineError;

pub struct PiAgent {
    // EXP-746 E4: fill — the child, the response registry, the tool table.
    #[allow(dead_code)]
    pub(crate) spec: AdapterSpec,
}

impl PiAgent {
    // EXP-746 E4: fill
    #[allow(unused_variables)]
    pub fn new(spec: AdapterSpec) -> Result<PiAgent, EngineError> {
        todo!("EXP-746 E4: build the pi adapter")
    }
}

impl ConnectTo<Client> for PiAgent {
    // EXP-746 E4: fill
    #[allow(unused_variables)]
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            todo!("EXP-746 E4: drive pi's rpc mode as an ACP agent")
        }
    }
}
