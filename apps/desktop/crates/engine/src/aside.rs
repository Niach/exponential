//! EXP-1330 — a `/btw` aside run: a forked claude run of its PARENT's
//! conversation (`--resume <parent> --fork-session`, read-only tools, the
//! parent's worktree), launched by `coding::aside`. The engine half: the
//! fork + tool posture on the adapter ([`ASIDE_DISALLOWED_TOOLS`] and NO MCP
//! servers, [`ASIDE_MCP_CONFIG`]) and the one-shot end — after its FIRST
//! reply the aside ends (the reply itself posts like any turn's), unless a
//! person spoke in it meanwhile: then it is an ordinary session.
//!
//! NEW-8: nothing launches an aside any more (`/btw` is gone and the pickup
//! never routes one); a run record left by an older build still resumes
//! under this posture, never with the full tool set.

// EXP-1426 part 4c: the aside's tool posture is the claude adapter's, so
// it moved with it into the agent runtime.
pub use exponential_agent_runtime::engine::launch::{ASIDE_DISALLOWED_TOOLS, ASIDE_MCP_CONFIG};

// EXP-1426 part 4d: the one-shot state moved to the agent runtime with the
// host that reads it.
pub use exponential_agent_runtime::engine::aside::{AsideEnd, AsideState};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aside_mcp_config_names_no_server() {
        let parsed: serde_json::Value = serde_json::from_str(ASIDE_MCP_CONFIG).expect("JSON");
        assert_eq!(parsed, serde_json::json!({ "mcpServers": {} }));
        assert!(!ASIDE_MCP_CONFIG.contains("exponential"));
    }
}
