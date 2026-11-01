//! EXP-1236: code mode for coding runs. The host process (the IDE or the
//! `exponential` daemon) serves ONE loopback MCP endpoint with two tools:
//! `exec` runs a script the agent wrote, in which every MCP tool of the run
//! is `await tools.<server>.<tool>(args)` and `Promise.all` runs calls at
//! once; `describe` hands out input schemas. The launcher admits a run with
//! a [`Grant`] carrying the run's bearer, and tells the host which upstream
//! servers that run's scripts may reach, with the run's OWN credentials
//! (the `expu_` key, the computer-use token, the team servers' resolved
//! headers) already substituted.
//!
//! Why a server of our own and not the agents' native code modes: Codex's
//! is experimental and needs a second binary the managed download does not
//! fetch, Claude Code has none, and ONE implementation gives both agents the
//! same tool, the same prompt section and the same limits. Stdio team
//! servers stay direct-call only (a second instance of a stdio server is
//! not the same server); every HTTP upstream is reachable.
//!
//! Limits: a script runs on boa with no module loader, no fs, no network;
//! its deadline is observed at every await, call and native (`runtime.rs`).
//! EXP-1287: each script runs in a WORKER PROCESS (`worker.rs`: this same
//! executable re-run as `__codemode-worker`, the host keeps the
//! credentials and runs the calls), killed at the deadline, at a revoke and
//! at a resident-memory ceiling — so a synchronous loop boa cannot
//! interrupt, or one that allocates without end, dies with its process
//! instead of keeping a core and growing until the host exits.

mod catalog;
mod gateway;
mod reports;
mod runtime;
mod server;
pub mod worker;

use std::sync::{Arc, OnceLock};

pub use gateway::{GatewayTransform, GatewayUpstream, ROUTE as GATEWAY_ROUTE};
pub use reports::{publish as file_report, take_report, ScriptReport};
pub use runtime::{CallRecord, LOG_CAP};
pub use server::{META_CALLS_KEY, META_COUNT_KEY, META_ELAPSED_KEY, SERVER_NAME};

/// The env var the run's token rides in; the MCP config only ever says
/// `Bearer ${EXP_CODEMODE_TOKEN}`.
pub const TOKEN_ENV: &str = "EXP_CODEMODE_TOKEN";

/// What a run needs to reach the endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Grant({}, <redacted>)", self.url)
    }
}

/// One HTTP MCP server a run's scripts may call, with its headers RESOLVED
/// (the values are the run's secrets: its `Debug` never prints them).
/// `name` is the config key the agent knows the server by (`exponential`,
/// `computer`, a team server's key); scripts spell `tools.<name>.<tool>`.
pub use emcp::Upstream;

struct Host {
    hub: Arc<server::Hub>,
    server: emcp::Server,
}

/// The process-wide host, started on the first grant. An `Err` (no free
/// port) is remembered: it will not change under a running process.
static HOST: OnceLock<Result<Host, String>> = OnceLock::new();

fn host() -> Result<&'static Host, String> {
    HOST.get_or_init(|| {
        let hub = Arc::new(server::Hub::new());
        let server = server::serve(hub.clone())?;
        log::info!("[codemode] serving code mode on {}", server.url());
        Ok(Host { hub, server })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Admit the run `session_id`; a grant it held before stops working.
/// `label` names the run in logs; `upstreams` = the servers its scripts
/// reach; `direct_only` = the stdio servers named as unreachable.
pub fn grant(session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> Result<Grant, String> {
    let host = host()?;
    host.hub.admit(session_id, label, upstreams, direct_only);
    Ok(Grant { url: host.server.url().to_string(), token: host.server.grant(session_id, label) })
}

/// EXP-1458: admit the run `session_id` to the MCP GATEWAY: the agent
/// reaches `upstream` (Exponential's `/api/mcp` with the run's real
/// headers) through `format!("{}/api/mcp", grant.url)` with `grant.token`
/// as its bearer, every `tools/call` passing through `transform`. A gateway
/// grant the run held before stops working; [`revoke`] drops it too.
pub fn gateway_grant(
    session_id: &str,
    label: &str,
    upstream: Upstream,
    transform: Arc<dyn GatewayTransform>,
) -> Result<Grant, String> {
    let host = host()?;
    Ok(Grant {
        url: format!("{}{}", host.server.base(), gateway::ROUTE),
        token: host.hub.gateways.grant(session_id, label, upstream, transform),
    })
}

/// The run ended: its token is dead, a script still running is cancelled
/// and the call logs its feed never claimed are dropped.
pub fn revoke(session_id: &str) {
    if let Some(Ok(host)) = HOST.get() {
        // The server's revoke reaches the hub (`ToolProvider::ended`): the
        // run is cancelled and its gateway token dropped.
        host.server.revoke(session_id);
    }
    reports::forget(session_id);
}
