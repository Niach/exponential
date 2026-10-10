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
//! Limits: a script runs in-process on boa with no module loader, no fs,
//! no network; its deadline is observed at every await, call and native
//! (`runtime.rs`). What boa cannot interrupt is a synchronous loop that
//! never awaits: the caller still gets its timeout, but that one engine
//! thread keeps a core until boa's per-function loop ceiling ends it, or
//! until the host exits when the loop re-enters a helper per iteration.

mod catalog;
mod client;
mod runtime;
mod server;

use std::sync::{Arc, OnceLock};

pub use server::SERVER_NAME;

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
/// (the values are the run's secrets: this type never prints them).
#[derive(Clone, PartialEq, Eq)]
pub struct Upstream {
    /// The config key the agent knows the server by (`exponential`,
    /// `computer`, a team server's key); scripts spell `tools.<name>.<tool>`.
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

struct Host {
    hub: Arc<server::Hub>,
    url: String,
}

/// The process-wide host, started on the first grant. An `Err` (no free
/// port) is remembered: it will not change under a running process.
static HOST: OnceLock<Result<Host, String>> = OnceLock::new();

fn host() -> Result<&'static Host, String> {
    HOST.get_or_init(|| {
        let hub = Arc::new(server::Hub::new());
        let url = server::serve(hub.clone())?;
        log::info!("[codemode] serving code mode on {url}");
        Ok(Host { hub, url })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Admit the run `session_id`; a grant it held before stops working.
/// `label` names the run in logs; `upstreams` = the servers its scripts
/// reach; `direct_only` = the stdio servers named as unreachable.
pub fn grant(session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> Result<Grant, String> {
    let host = host()?;
    Ok(Grant { url: host.url.clone(), token: host.hub.grant(session_id, label, upstreams, direct_only) })
}

/// The run ended: its token is dead and a script still running is cancelled.
pub fn revoke(session_id: &str) {
    if let Some(Ok(host)) = HOST.get() {
        host.hub.revoke(session_id);
    }
}
