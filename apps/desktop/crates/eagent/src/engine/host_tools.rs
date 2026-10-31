//! EXP-1426 (part 4b): what a host knows about ITS OWN tools — the seam the
//! mapper asks instead of a product's contract tables.
//!
//! The core maps any agent's tool calls without knowing whose MCP servers a
//! run carries. A host that ships its own tools (Exponential's MCP surface and
//! its in-process code-mode server) answers these lookups so a settled call of
//! one of THOSE publishes its subject, its preview refs and its script log;
//! [`NoHostTools`] is the host with none (every call renders generically).

/// The code-mode server's MCP name: its `exec` tool is a `script` row
/// (contract `script-card.json` `toolNameSuffix`). Pinned against the
/// `codemode` crate by Exponential's engine.
pub const CODEMODE_SERVER_NAME: &str = "codemode";

/// One finished code-mode `exec`, as the activity feed shows it — the host's
/// in-process code-mode server files it, the mapper takes it off when the
/// exec call settles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptReport {
    /// Every call the script made (the log stops at its cap, this does not).
    pub count: usize,
    pub elapsed_ms: u64,
    pub calls: Vec<ScriptCall>,
}

/// One upstream call a script made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptCall {
    /// `<server>.<tool>` as the script called it.
    pub tool: String,
    /// What the call was about.
    pub subject: Option<String>,
    /// The call answered (a refusal, a transport error, a deadline = false).
    pub ok: bool,
    /// The upstream call's wall time; `None` for one that never ran.
    pub ms: Option<u64>,
}

/// The host's own tools, looked up by the ACP title an adapter gives a call.
/// Every method answers `None` for a tool that is not the host's.
pub trait HostTools: Send + Sync {
    /// The display row a title belongs to (Exponential: the contract's
    /// `expToolDisplay` row, `issues_create`), whatever namespace an adapter
    /// put in front of it.
    fn row(&self, _title: &str) -> Option<&'static str> {
        None
    }

    /// The INPUT field naming the call's subject (an issue create's `title`),
    /// when the row has one.
    fn subject_key(&self, _title: &str) -> Option<&'static str> {
        None
    }

    /// The row's preview spec — whitespace-separated `kind@path` terms
    /// ([`super::exp_tool_refs::exp_tool_refs`] parses them); `Some("")` for
    /// one of the host's tools whose answer previews nothing.
    fn preview_spec(&self, _title: &str) -> Option<&'static str> {
        None
    }

    /// The log of the run's oldest unclaimed code-mode exec of exactly
    /// `script`, removed.
    fn take_script_report(&self, _session_seed: &str, _script: &str) -> Option<ScriptReport> {
        None
    }
}

/// A host with no tools of its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHostTools;

impl HostTools for NoHostTools {}
