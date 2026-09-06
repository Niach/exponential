//! EXP-746 — ClaudeAgent: the user's own `claude` CLI in stream-json mode,
//! presented to the engine as an ACP agent. Owned by lane E2.
//!
//! Shape (E2): spawn [`claude_wire::claude_argv`] over
//! [`crate::transport::spawn_lines`], answer `initialize` from
//! `system/init.capabilities` (never a version check), and translate both
//! ways — `can_use_tool` → `session/request_permission`, `AskUserQuestion` →
//! `elicitation/create`, `stream_event`/`assistant`/`user`/`result` →
//! `session/update`. Non-obvious invariants: `--permission-prompt-tool stdio`
//! is MANDATORY or no `can_use_tool` ever arrives and the whole
//! permission/steering surface silently dies; `keep_alive` is dropped and
//! NEVER answered; the argv keeps `--settings <claude-hooks/<pid>/<sid>>` as
//! the reaper's only process-selection anchor even though the file is `{}`.

use agent_client_protocol::{Agent, Client, ConnectTo};

use super::AdapterSpec;
use crate::session::EngineError;

pub struct ClaudeAgent {
    // EXP-746 E2: fill — the spec, the child, the control-request registry,
    // the tool table and the subagent (`task_id`) edges.
    #[allow(dead_code)]
    pub(crate) spec: AdapterSpec,
}

impl ClaudeAgent {
    // EXP-746 E2: fill
    #[allow(unused_variables)]
    pub fn new(spec: AdapterSpec) -> Result<ClaudeAgent, EngineError> {
        todo!("EXP-746 E2: build the claude adapter")
    }
}

impl ConnectTo<Client> for ClaudeAgent {
    // EXP-746 E2: fill
    #[allow(unused_variables)]
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        async move {
            todo!("EXP-746 E2: drive claude's stream-json control protocol as an ACP agent")
        }
    }
}
