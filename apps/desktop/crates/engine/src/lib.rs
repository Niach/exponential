//! The ACP session engine (EXP-746).
//!
//! Hosts an `agent-client-protocol` client in-process, drives the user's own
//! agent CLI through one of the four adapters, maps every `SessionUpdate` into
//! the steer wire vocabulary, and owns the run lifecycle (heartbeat, publisher,
//! diff snapshots, kill feed, end sequence) that the desktop IDE and the
//! headless CLI daemon used to duplicate by hand.
//!
//! Scaffold only: the module tree lands with the engine lanes.

/// Placeholder so the crate compiles before its modules exist.
pub fn crate_name() -> &'static str {
    "engine"
}
