//! agent-core — the shared agent loop for the Exponential desktop apps.
//!
//! Ported (incrementally) from `apps/companion/src`, this crate is built as a
//! `cdylib`/`staticlib` and consumed over a C ABI by the macOS app (Swift) and
//! the Linux app (Zig). The C surface lives in [`ffi`]; the header it mirrors is
//! `include/agent_core.h`.
//!
//! M0 is a dependency-free stub: it stands up the C ABI lifecycle and emits a
//! periodic `log` event so both GUIs can verify they link and receive events.
//! Later milestones fill in `electric`, `dispatcher`, `pipeline`, `mcp`, `trpc`,
//! `github`, `git`, `pr_poll`, `state`, `agent_run`, and `mcp_config`.

#![allow(clippy::missing_safety_doc)]

/// Canonical enum string constants, generated from
/// `packages/domain-contract/contract.json` by its `generate.ts`.
mod domain_contract;

pub mod ffi;

/// The pure pipeline "brain" — stage decision + plan-output parsing + prompt
/// builders, ported from `apps/companion/src/pipeline.ts`.
pub mod pipeline;

/// Local agent state store (rusqlite), ported from `state.ts`.
pub mod state;

/// `companion.*` tRPC client (ureq), ported from `exponential-api.ts`.
pub mod trpc;

/// Electric assigned-issues long-poll, ported from `event-source.ts`.
pub mod electric;

/// MCP client for /api/mcp (tools/call), ported from `exponential-mcp-client.ts`.
pub mod mcp;

/// Threaded dispatcher (queue/concurrency/dedup/gating), ported from `dispatcher.ts`.
pub mod dispatcher;

/// git clone/fetch/worktree/push, ported from `repo-manager.ts` + `worktree.ts`.
pub mod git;

/// GitHub REST (repo + PRs), ported from `github-api.ts`.
pub mod github;

/// On-disk MCP config for the agent CLI, ported from the driver helpers.
pub mod mcp_config;

/// The agent-run handshake (run_request → host runs CLI → submit_result).
pub mod agent_run;

/// The per-issue pipeline I/O stages (plan / code), ported from `pipeline.ts`.
pub mod run_pipeline;

/// PR reconcile loop, ported from `pr-poll-loop.ts`.
pub mod pr_poll;

#[cfg(test)]
mod domain_contract_tests {
    use crate::domain_contract as dc;

    #[test]
    fn contract_constants_present() {
        assert!(dc::ISSUE_STATUS_VALUES.contains(&"in_progress"));
        assert_eq!(dc::WORKSPACE_ROLE_AGENT, "agent");
        assert_eq!(dc::COMMENT_KIND_PLAN, "plan");
        assert!(dc::RECURRENCE_INTERVALS.contains(&30));
    }
}
