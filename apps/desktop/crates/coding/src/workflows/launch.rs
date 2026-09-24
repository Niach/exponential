//! EXP-1029 contract — the ONE place a workflow run's models come from, the
//! Rust mirror of web `lib/workflow-launch.ts` (same three functions, same
//! test names), so the desktop engine (`ui::workflow_host`) and the CLI
//! daemon pick models from ONE rule.
//!
//! A workflow's `launch` jsonb is read through [`normalize_workflow_launch`]
//! into the strict [`WorkflowLaunch`]: two models, no more. `model` is the
//! CHEAP one (leaf nodes, and the `Task` subagents inside every node run),
//! `strong_model` the capable one (contract nodes, integration nodes,
//! `risk: high` nodes and EVERY agent review). The EXP-1002 per-phase pins,
//! `subagentModel` and `reviewModel` are deprecated (folded in here); the
//! gate choice is gone (the agent reviews every node, the one human review
//! is the final PR); `start_on` is fixed to `contract`.
//!
//! EXP-1014 owns the implementations and the wiring on both hosts; the
//! ignored tests below are the acceptance table it un-ignores.

use domain::contract::{
    WORKFLOW_LAUNCH_CLAUDE_MODEL, WORKFLOW_LAUNCH_CLAUDE_STRONG_MODEL,
    WORKFLOW_LAUNCH_CODEX_MODEL, WORKFLOW_LAUNCH_CODEX_STRONG_MODEL,
};

/// The agents a workflow may run on (contract `workflowLaunch.agents`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowLaunchAgent {
    Claude,
    Codex,
}

impl WorkflowLaunchAgent {
    /// The contract `codingAgent` id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    /// Contract `workflowLaunch` defaults: `(model, strong_model)`.
    pub fn default_models(self) -> (&'static str, &'static str) {
        match self {
            Self::Claude => (WORKFLOW_LAUNCH_CLAUDE_MODEL, WORKFLOW_LAUNCH_CLAUDE_STRONG_MODEL),
            Self::Codex => (WORKFLOW_LAUNCH_CODEX_MODEL, WORKFLOW_LAUNCH_CODEX_STRONG_MODEL),
        }
    }
}

/// THE workflow launch — what every node run and every agent review reads
/// (`WorkflowLaunch` in `@exp/db-schema/domain`). Distinct from the wire
/// struct `api::workflows::WorkflowLaunch`, which is the stored jsonb of any
/// vintage; this is what [`normalize_workflow_launch`] makes of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowLaunch {
    pub agent: WorkflowLaunchAgent,
    /// An agent profile id on the runner device; `None` = its active login.
    pub account: Option<String>,
    /// The cheap model: leaves, and the `Task` subagents inside every run.
    pub model: String,
    /// The capable model: contract, integration and `risk: high` nodes, and
    /// every agent review.
    pub strong_model: String,
}

/// The stored jsonb keys that fold into `strong_model`, in precedence order:
/// the first one set wins.
pub const STRONG_MODEL_LEGACY_KEYS: [&str; 4] =
    ["reviewModel", "riskModel", "contractModel", "integrationModel"];

/// The stored `workflows.launch` (any vintage, or garbage) → the strict
/// launch every run reads. Rules (the ignored table below):
/// - `agent`: `claude` or `codex`; anything else → claude.
/// - `account`: a non-empty string stays, anything else is `None`.
/// - `model`: the stored `model` when set, else the agent's default model.
/// - `strong_model`: the stored `strongModel` when set; else the first set
///   of [`STRONG_MODEL_LEGACY_KEYS`]; else the agent's default strong model.
/// - `subagentModel`, `effort`, `maxParallel` are dropped.
pub fn normalize_workflow_launch(raw: &serde_json::Value) -> WorkflowLaunch {
    let _ = raw;
    todo!("EXP-1014 implements coding::workflows::launch")
}

/// The model ONE node's run spawns on: `strong_model` for a `contract` or
/// `integration` node (contract `wfNodeKind`) and for any `risk: high` node
/// (contract `wfRisk`), else `model`. The `Task` subagents inside the run
/// always take `model`.
pub fn model_for_node(launch: &WorkflowLaunch, kind: &str, risk: &str) -> String {
    let _ = (launch, kind, risk);
    todo!("EXP-1014 implements coding::workflows::launch")
}

/// The model EVERY agent review runs on: `strong_model`, whatever the node.
pub fn review_model_for(launch: &WorkflowLaunch) -> String {
    let _ = launch;
    todo!("EXP-1014 implements coding::workflows::launch")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude() -> WorkflowLaunch {
        WorkflowLaunch {
            agent: WorkflowLaunchAgent::Claude,
            account: None,
            model: "opus".into(),
            strong_model: "fable".into(),
        }
    }

    #[test]
    fn the_contract_defaults_are_the_claude_and_codex_pairs() {
        assert_eq!(WorkflowLaunchAgent::Claude.default_models(), ("opus", "fable"));
        assert_eq!(
            WorkflowLaunchAgent::Codex.default_models(),
            ("gpt-5.6-sol", "gpt-5.6-luna")
        );
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn reads_the_new_shape_verbatim() {
        let launch = normalize_workflow_launch(&json!({
            "agent": "claude", "account": "p-1", "model": "sonnet", "strongModel": "opus"
        }));
        assert_eq!(launch.agent, WorkflowLaunchAgent::Claude);
        assert_eq!(launch.account.as_deref(), Some("p-1"));
        assert_eq!(launch.model, "sonnet");
        assert_eq!(launch.strong_model, "opus");
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn fills_an_empty_row_from_the_claude_defaults() {
        assert_eq!(normalize_workflow_launch(&json!({})), claude());
        assert_eq!(normalize_workflow_launch(&serde_json::Value::Null), claude());
        assert_eq!(normalize_workflow_launch(&json!("garbage")), claude());
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn fills_a_codex_row_from_the_codex_defaults() {
        let launch = normalize_workflow_launch(&json!({ "agent": "codex" }));
        assert_eq!(launch.agent, WorkflowLaunchAgent::Codex);
        assert_eq!(launch.model, "gpt-5.6-sol");
        assert_eq!(launch.strong_model, "gpt-5.6-luna");
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn degrades_an_unknown_agent_to_claude() {
        assert_eq!(
            normalize_workflow_launch(&json!({ "agent": "pi" })).agent,
            WorkflowLaunchAgent::Claude
        );
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn folds_an_old_rows_pins_into_strong_model_review_model_first() {
        assert_eq!(
            normalize_workflow_launch(&json!({ "model": "opus", "contractModel": "sonnet" }))
                .strong_model,
            "sonnet"
        );
        assert_eq!(
            normalize_workflow_launch(&json!({ "riskModel": "sonnet", "reviewModel": "opus" }))
                .strong_model,
            "opus"
        );
        assert_eq!(
            normalize_workflow_launch(&json!({ "integrationModel": "sonnet" })).strong_model,
            "sonnet"
        );
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn lets_a_stored_strong_model_win_over_every_legacy_pin() {
        assert_eq!(
            normalize_workflow_launch(&json!({ "strongModel": "opus", "contractModel": "sonnet" }))
                .strong_model,
            "opus"
        );
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn drops_subagent_model_effort_and_max_parallel() {
        assert_eq!(
            normalize_workflow_launch(&json!({
                "agent": "claude", "subagentModel": "sonnet", "effort": "high", "maxParallel": 5
            })),
            claude()
        );
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements normalize_workflow_launch"]
    fn drops_a_blank_account() {
        assert_eq!(normalize_workflow_launch(&json!({ "account": "" })), claude());
        assert_eq!(normalize_workflow_launch(&json!({ "account": null })), claude());
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements model_for_node"]
    fn runs_a_leaf_on_the_cheap_model() {
        assert_eq!(model_for_node(&claude(), "leaf", "low"), "opus");
        assert_eq!(model_for_node(&claude(), "leaf", "medium"), "opus");
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements model_for_node"]
    fn runs_contract_and_integration_nodes_on_the_strong_model() {
        assert_eq!(model_for_node(&claude(), "contract", "low"), "fable");
        assert_eq!(model_for_node(&claude(), "integration", "low"), "fable");
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements model_for_node"]
    fn runs_a_high_risk_node_on_the_strong_model_whatever_its_kind() {
        assert_eq!(model_for_node(&claude(), "leaf", "high"), "fable");
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1014 implements review_model_for"]
    fn reviews_every_node_on_the_strong_model() {
        assert_eq!(review_model_for(&claude()), "fable");
    }
}
