//! Typed `workflows.*` client (EXP-981 — workflows: a picked set of issues of
//! ONE repository, planned as a DAG).
//!
//! Reading rides the Electric `workflows` + `workflow_nodes` shapes (the
//! nodes' `wave`/`lane`/`on_cycle` ARE the server-computed layout — nothing
//! here lays a graph out); this client is the WRITE path. Any team member may
//! call every proc. The server owns every rule — one repository, backlog-only
//! picks, draft-only re-planning, the runner device's `workflows` cap — and
//! answers a refusal with a human sentence, so this layer only shapes the
//! wire and lets [`ApiError`] carry that sentence to the usual error notice.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

/// `workflows.launch` — what every node's run starts with. Every field is
/// optional; an absent one falls back to the runner device's own defaults.
///
/// It serializes ONLY as `workflows.update`'s `launch`, and there the three
/// EXP-1002 phase pins are a TRI-STATE server-side: key absent = keep the
/// stored pin (a client that predates the keys must not wipe them), `null` =
/// clear, a string = set. This client knows them, so it ALWAYS writes all
/// three, `null` when unset — omitting one would make a cleared pin stick.
/// Decoding stays tolerant (`default`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLaunch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The model every node's run spawns on, unless its PHASE overrides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// EXP-1002: the model `contract` nodes run on. Absent = `model`.
    #[serde(default)]
    pub contract_model: Option<String>,
    /// EXP-1002: the model `integration` nodes run on. Absent = `model`.
    #[serde(default)]
    pub integration_model: Option<String>,
    /// EXP-1002: the model a `risk: high` node runs on, whatever its kind.
    /// Absent = the node's phase model.
    #[serde(default)]
    pub risk_model: Option<String>,
    /// Claude only: the model its SUBAGENTS run on — never the node run's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// An agent profile id on the runner device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<u32>,
    /// EXP-984: the model AGENT REVIEWS run on. Absent = the engine picks
    /// one (fable on claude, else the author's); a `risk: high` node is always reviewed on a model
    /// other than its author's, whatever this says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_model: Option<String>,
}

/// One `workflows` row as the wire carries it (the shape's column set;
/// `creator_id` is server-only).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub team_id: String,
    #[serde(default)]
    pub repository_id: Option<String>,
    pub name: String,
    /// contract `wfStatus` — kept a raw string so a newer server's status
    /// never fails decoding (the band rule maps an unknown one to Done).
    pub status: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub launch: WorkflowLaunch,
    #[serde(default)]
    pub start_on: Option<String>,
    #[serde(default)]
    pub integration_branch: Option<String>,
    #[serde(default)]
    pub metrics: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct WorkflowResponse {
    workflow: Workflow,
}

/// `workflows.create` — mutation, member-gated. `issue_ids` ride in DISPLAY
/// order; the server names the workflow after the first identifier when
/// `name` is omitted.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    issue_ids: &[String],
    name: Option<&str>,
) -> Result<Workflow, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        issue_ids: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
    }
    let response: WorkflowResponse = trpc.mutation(
        "workflows.create",
        &Input {
            team_id,
            issue_ids,
            name,
        },
    )?;
    Ok(response.workflow)
}

/// `workflows.update` input. Omitted fields stay unchanged; `device_id` is
/// the server's `.nullable().optional()` tri-state ([`Patch`]): `Null` unbinds
/// the runner machine. Everything but `name` is draft-only server-side.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub device_id: Patch<String>,
    /// A WHOLE-object replace (the server's `workflowLaunchSchema` is
    /// `.strict()`), so senders read the current launch first and edit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<WorkflowLaunch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_on: Option<String>,
}

impl WorkflowUpdate {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

/// `workflows.update` — mutation, member-gated.
pub fn update(trpc: &TrpcClient, input: &WorkflowUpdate) -> Result<Workflow, ApiError> {
    let response: WorkflowResponse = trpc.mutation("workflows.update", input)?;
    Ok(response.workflow)
}

/// `workflows.setIssues` — mutation: add or drop issues of a DRAFT. Returns
/// the re-derived metrics, which the synced row echoes moments later.
pub fn set_issues(
    trpc: &TrpcClient,
    id: &str,
    add_issue_ids: &[String],
    remove_issue_ids: &[String],
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        id: &'a str,
        add_issue_ids: &'a [String],
        remove_issue_ids: &'a [String],
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation(
        "workflows.setIssues",
        &Input {
            id,
            add_issue_ids,
            remove_issue_ids,
        },
    )?;
    Ok(())
}

/// `workflows.updateNode` input — addressed by ISSUE (a member's id resolves
/// to its compound node). `kind` and `touches` are draft-only server-side.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNodeUpdate {
    pub workflow_id: String,
    pub issue_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    /// A whole-array replace; `Some(vec![])` deliberately CLEARS the globs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touches: Option<Vec<String>>,
}

/// The `workflows` cap a device advertises once it can run the engine
/// (EXP-982) — the ONE place the literal lives, mirrored by the server's
/// `WORKFLOW_DEVICE_CAP`.
pub const WORKFLOWS_CAP: &str = "workflows";

impl WorkflowNodeUpdate {
    pub fn new(workflow_id: impl Into<String>, issue_id: impl Into<String>) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            issue_id: issue_id.into(),
            ..Self::default()
        }
    }
}

/// `workflows.updateNode` — mutation, member-gated.
pub fn update_node(trpc: &TrpcClient, input: &WorkflowNodeUpdate) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.updateNode", input)?;
    Ok(())
}

/// `workflows.replan` — mutation: re-derive the compound nodes, the layout
/// and the metrics now.
pub fn replan(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.replan", &Input { id })?;
    Ok(())
}

/// `workflows.delete` — mutation, member-gated. The server refuses a running
/// or paused workflow with a sentence ("Cancel the workflow before deleting
/// it").
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.delete", &Input { id })?;
    Ok(())
}

// ── Running a workflow (EXP-982) ────────────────────────────────────────────
// The server only flips intent; the deterministic ENGINE on the runner device
// ([`coding::workflows`]) does the work off the synced rows. Everything from
// `report_node` down is ENGINE-ONLY: the server checks that the caller owns
// the device row named by `workflows.device_id` and answers a refusal with a
// human sentence.

/// A member-gated status flip with no payload (`start`/`pause`/`resume`/
/// `cancel`) — the server owns every precondition.
fn workflow_command(trpc: &TrpcClient, proc: &str, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation(proc, &Input { id })?;
    Ok(())
}

/// `workflows.start` — draft → running. The server re-plans first and
/// refuses a cycle or a missing repository/device with the sentence
/// `domain::workflow_view::workflow_start_blocker` shows. EXP-983: every
/// `start_on` runs, so the mode is never a refusal.
pub fn start(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    workflow_command(trpc, "workflows.start", id)
}

/// `workflows.pause` — the engine starts and lands nothing new; live runs
/// finish.
pub fn pause(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    workflow_command(trpc, "workflows.pause", id)
}

pub fn resume(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    workflow_command(trpc, "workflows.resume", id)
}

/// `workflows.cancel` — the engine then ends the live runs and deletes the
/// integration branch. Nothing reached the default branch.
pub fn cancel(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    workflow_command(trpc, "workflows.cancel", id)
}

/// `workflows.approveNode` — the human gate. `approved: false` takes the
/// approval back while the node has not landed.
pub fn approve_node(trpc: &TrpcClient, node_id: &str, approved: bool) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        node_id: &'a str,
        approved: bool,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation(
        "workflows.approveNode",
        &Input { node_id, approved },
    )?;
    Ok(())
}

/// `workflows.resolveNode` — a person unsticks a node: `retry` gives it a
/// fresh attempt, `skip` takes it out so its dependents go on without it.
pub fn resolve_node(trpc: &TrpcClient, node_id: &str, action: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        node_id: &'a str,
        action: &'a str,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.resolveNode", &Input { node_id, action })?;
    Ok(())
}

/// EXP-984: `workflows.admitNode` — a follow-up filed mid-run arrived as a
/// `proposed` node. `admit: true` makes it part of the run (state `blocked`,
/// re-planned); `false` deletes it. Member-gated, never the engine's call.
pub fn admit_node(trpc: &TrpcClient, node_id: &str, admit: bool) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        node_id: &'a str,
        admit: bool,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.admitNode", &Input { node_id, admit })?;
    Ok(())
}

/// EXP-984: the counters only the DEVICE can see — one merge-in per upstream
/// movement actually delivered, one contract change per checkpointed node
/// whose branch moved again. Everything else is counted by the server.
pub const COUNTER_MERGE_INS: &str = "mergeIns";
pub const COUNTER_CONTRACT_CHANGES: &str = "contractChanges";

/// ENGINE: `workflows.reportMetrics` — add `deltas` to the synced
/// `workflows.metrics` counters, batched once per beat. A key the server does
/// not know is refused whole, so only the two constants above are ever sent.
pub fn report_metrics(
    trpc: &TrpcClient,
    id: &str,
    deltas: &std::collections::BTreeMap<String, u32>,
) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        deltas: &'a std::collections::BTreeMap<String, u32>,
    }
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.reportMetrics", &Input { id, deltas })?;
    Ok(())
}

/// `workflows.update({ decision })` — a dated line appended to the log every
/// node prompt carries, at ANY status. The server relays it to every sibling
/// parked on a question.
pub fn append_decision(trpc: &TrpcClient, id: &str, decision: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        decision: &'a str,
    }
    let _: WorkflowResponse = trpc.mutation("workflows.update", &Input { id, decision })?;
    Ok(())
}

/// ENGINE: `workflows.reportNode` input. Omitted fields stay unchanged; the
/// server NEVER makes or unmakes `landed`/`skipped` from here.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeReport {
    pub node_id: String,
    /// contract `wfNodeState`.
    pub state: String,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub session_id: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub base_branch: Patch<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<i64>,
    /// Why the node is `failed` / `waiting`, in one sentence.
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub note: Patch<String>,
    /// EXP-983: the SERIALIZATION edges a sibling collision produced — a
    /// whole-array replace, so the engine sends the union it computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_node_ids: Option<Vec<String>>,
}

impl NodeReport {
    pub fn new(node_id: impl Into<String>, state: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            state: state.into(),
            ..Self::default()
        }
    }
}

/// ENGINE: `workflows.reportNode` — a node's state moved.
pub fn report_node(trpc: &TrpcClient, input: &NodeReport) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct Ignored {}
    let _: Ignored = trpc.mutation("workflows.reportNode", input)?;
    Ok(())
}

/// What `workflows.landNode` answers. A refusal is an ANSWER, not an error:
/// `merged: false` with `reason` = GitHub would not merge (a conflict with
/// what landed before it), or a person still owes an approval.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LandOutcome {
    pub merged: bool,
    #[serde(default)]
    pub reason: Option<String>,
    /// EXP-983: the dependents whose LAST unlanded blocker this was. The
    /// server retargeted their pull request onto the integration branch and
    /// moved their `base_branch`; the engine then has them merge it in.
    #[serde(default)]
    pub retargeted: Vec<String>,
}

impl LandOutcome {
    /// Whether the refusal is the "wait, nothing is wrong" kind: the gate or
    /// a workflow that is no longer running. Anything else means the branch
    /// needs the trunk merged in.
    pub fn is_waiting(&self) -> bool {
        matches!(
            self.reason.as_deref(),
            Some("Waiting for a person to approve")
                | Some("The workflow is not running")
                // EXP-983: the train lands in topological order, so a
                // speculative node's turn simply has not come yet.
                | Some("Waiting for its blockers to land")
        )
    }
}

/// ENGINE: `workflows.landNode` — the merge train's one step. The GATE is
/// enforced server-side, never trusted from the device.
pub fn land_node(trpc: &TrpcClient, node_id: &str) -> Result<LandOutcome, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        node_id: &'a str,
    }
    trpc.mutation("workflows.landNode", &Input { node_id })
}

/// ENGINE: `workflows.openFinalPr` — the ONE final pull request, integration
/// branch → the repository's default branch. Idempotent; returns its url.
pub fn open_final_pr(trpc: &TrpcClient, id: &str) -> Result<String, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Response {
        url: String,
    }
    let response: Response = trpc.mutation("workflows.openFinalPr", &Input { id })?;
    Ok(response.url)
}

/// Hydrate the wire shape from a synced `workflows` row — what the detail's
/// configuration section edits before it sends a whole-object `launch` back.
pub fn from_row(row: &domain::rows::WorkflowRow) -> Workflow {
    let launch = row
        .launch
        .clone()
        .and_then(|value| serde_json::from_value::<WorkflowLaunch>(value).ok())
        .unwrap_or_default();
    Workflow {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        repository_id: row.repository_id.clone(),
        name: row.name.clone().unwrap_or_default(),
        status: row.status_wire().to_string(),
        device_id: row.device_id.clone(),
        launch,
        start_on: row.start_on.clone(),
        integration_branch: row.integration_branch.clone(),
        metrics: row.metrics.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn create_posts_the_picked_issues_in_order() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"workflow":{"id":"wf-1","teamId":"team-1",
                "repositoryId":"repo-1","name":"EXP-1 +2","status":"draft",
                "deviceId":null,"launch":{},"startOn":"contract",
                "integrationBranch":"exp/wf-abcdef12",
                "metrics":{"nodes":3,"edges":0,"depth":1,"width":3,"cycles":[]}},
                "txId":"1"}}}"#,
        );
        let issues = vec!["i-1".to_string(), "i-2".to_string(), "i-3".to_string()];
        let workflow = create(&client(&base), "team-1", &issues, None).unwrap();
        assert_eq!(workflow.id, "wf-1");
        assert_eq!(workflow.status, "draft");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/workflows.create HTTP/1.1"));
        assert!(request.contains(r#""issueIds":["i-1","i-2","i-3"]"#));
        // An omitted name lets the server derive one (zod .optional()).
        assert!(!request.contains(r#""name""#));
    }

    #[test]
    fn update_serializes_the_device_tristate_and_the_whole_launch() {
        let mut input = WorkflowUpdate::new("wf-1");
        input.device_id = Patch::Set("dev-1".to_string());
        input.launch = Some(WorkflowLaunch {
            agent: Some("claude".to_string()),
            model: Some("opus".to_string()),
            subagent_model: Some("sonnet".to_string()),
            max_parallel: Some(4),
            ..WorkflowLaunch::default()
        });
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""deviceId":"dev-1""#));
        assert!(json.contains(r#""subagentModel":"sonnet""#));
        assert!(json.contains(r#""maxParallel":4"#));
        // The launch object is `.strict()` server-side: absent fields stay off.
        assert!(!json.contains(r#""effort""#));
        assert!(!json.contains(r#""name""#));

        // Null unbinds the runner machine; Omit leaves it.
        let mut unbound = WorkflowUpdate::new("wf-1");
        unbound.device_id = Patch::Null;
        assert!(serde_json::to_string(&unbound)
            .unwrap()
            .contains(r#""deviceId":null"#));
        let untouched = WorkflowUpdate::new("wf-1");
        assert!(!serde_json::to_string(&untouched)
            .unwrap()
            .contains("deviceId"));
    }

    #[test]
    fn update_node_addresses_the_node_by_issue() {
        let mut input = WorkflowNodeUpdate::new("wf-1", "i-1");
        input.kind = Some("contract".to_string());
        input.risk = Some("high".to_string());
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""workflowId":"wf-1""#));
        assert!(json.contains(r#""issueId":"i-1""#));
        assert!(json.contains(r#""kind":"contract""#));
        assert!(!json.contains("touches"));
        // An empty array deliberately CLEARS the globs.
        let mut cleared = WorkflowNodeUpdate::new("wf-1", "i-1");
        cleared.touches = Some(Vec::new());
        assert!(serde_json::to_string(&cleared)
            .unwrap()
            .contains(r#""touches":[]"#));
    }

    /// EXP-983 — the engine's two new halves of the wire: the serialization
    /// edges it reports, and the dependents a land released.
    #[test]
    fn a_report_carries_the_serial_edges_and_a_land_its_retargets() {
        let mut report = NodeReport::new("n-1", "running");
        assert!(!serde_json::to_string(&report).unwrap().contains("afterNodeIds"));
        report.after_node_ids = Some(vec!["n-2".to_string()]);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains(r#""afterNodeIds":["n-2"]"#));
        // A whole-array replace: an empty array deliberately CLEARS them.
        report.after_node_ids = Some(Vec::new());
        assert!(serde_json::to_string(&report)
            .unwrap()
            .contains(r#""afterNodeIds":[]"#));

        let outcome: LandOutcome =
            serde_json::from_str(r#"{"merged":true,"reason":null,"retargeted":["n-2","n-3"]}"#)
                .unwrap();
        assert_eq!(outcome.retargeted, ["n-2", "n-3"]);
        // An older server's answer still decodes, with nothing retargeted.
        let narrow: LandOutcome = serde_json::from_str(r#"{"merged":false}"#).unwrap();
        assert!(narrow.retargeted.is_empty());
        // The train's topological refusal is a WAIT, not a conflict: the
        // node's turn simply has not come.
        let waiting = LandOutcome {
            merged: false,
            reason: Some("Waiting for its blockers to land".to_string()),
            retargeted: Vec::new(),
        };
        assert!(waiting.is_waiting());
    }

    /// EXP-1002: the three phase pins are ALWAYS on the wire, `null` when
    /// unset — the server keeps a stored pin whose key is absent, so an
    /// omitted key could never clear one.
    #[test]
    fn the_phase_pins_serialize_as_explicit_nulls() {
        let mut input = WorkflowUpdate::new("wf-1");
        input.launch = Some(WorkflowLaunch {
            model: Some("opus".to_string()),
            integration_model: Some("fable".to_string()),
            ..WorkflowLaunch::default()
        });
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json["launch"],
            serde_json::json!({
                "model": "opus",
                "contractModel": null,
                "integrationModel": "fable",
                "riskModel": null,
            }),
            "every other unset field stays off the strict launch object"
        );
        // An empty launch still names all three.
        let bare = serde_json::to_value(WorkflowLaunch::default()).unwrap();
        assert_eq!(
            bare,
            serde_json::json!({
                "contractModel": null,
                "integrationModel": null,
                "riskModel": null,
            })
        );
        // Decoding stays tolerant: absent and null both read as unset.
        for raw in [r#"{}"#, r#"{"contractModel":null,"riskModel":null}"#] {
            let launch: WorkflowLaunch = serde_json::from_str(raw).unwrap();
            assert_eq!(launch, WorkflowLaunch::default(), "{raw}");
        }
    }

    /// EXP-984 — the review model rides the launch object.
    #[test]
    fn the_review_model_serializes() {
        let mut input = WorkflowUpdate::new("wf-1");
        input.launch = Some(WorkflowLaunch {
            model: Some("opus".to_string()),
            review_model: Some("fable".to_string()),
            ..WorkflowLaunch::default()
        });
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains(r#""reviewModel":"fable""#));
    }

    #[test]
    fn from_row_hydrates_the_synced_row() {
        // jsonb columns arrive TEXT-stored (§5.5) — both must re-parse.
        let row: domain::rows::WorkflowRow = serde_json::from_value(serde_json::json!({
            "id": "wf-1",
            "team_id": "team-1",
            "repository_id": "repo-1",
            "name": "EXP-1 +2",
            "status": "draft",
            "device_id": "dev-1",
            "launch": r#"{"agent":"claude","subagentModel":"sonnet","maxParallel":5}"#,
            "start_on": "pr_open",
            "metrics": r#"{"nodes":3,"edges":2,"depth":2,"width":2,"cycles":[]}"#,
        }))
        .unwrap();
        let workflow = from_row(&row);
        assert_eq!(workflow.name, "EXP-1 +2");
        assert_eq!(workflow.device_id.as_deref(), Some("dev-1"));
        assert_eq!(workflow.launch.agent.as_deref(), Some("claude"));
        assert_eq!(workflow.launch.subagent_model.as_deref(), Some("sonnet"));
        assert_eq!(workflow.launch.max_parallel, Some(5));
        assert_eq!(row.max_parallel(), 5);
        assert_eq!(row.shape().nodes, 3);
        assert_eq!(row.shape().depth, 2);

        // A minimal row (a workflow the planner has not touched) still
        // hydrates: status defaults to draft, the shape reads empty.
        let bare: domain::rows::WorkflowRow =
            serde_json::from_value(serde_json::json!({ "id": "wf-2" })).unwrap();
        assert_eq!(bare.status_wire(), "draft");
        assert_eq!(bare.shape(), domain::workflow_view::WorkflowShape::default());
        assert!(bare.cycle_edges().is_empty());
        assert_eq!(
            bare.max_parallel(),
            domain::contract::WORKFLOW_MAX_PARALLEL_DEFAULT
        );
    }

    /// The nodes carry the SERVER's layout; an unknown/absent column degrades
    /// to wave 0 / lane 0 rather than dropping the node.
    #[test]
    fn node_rows_hydrate_the_server_layout_tolerantly() {
        let row: domain::rows::WorkflowNodeRow = serde_json::from_value(serde_json::json!({
            "id": "n-1",
            "workflow_id": "wf-1",
            "issue_id": "i-1",
            "member_issue_ids": r#"["i-2","i-3"]"#,
            "kind": "contract",
            "state": "ready",
            "risk": "high",
            "wave": "2",
            "lane": "1",
            "on_cycle": "t",
            "touches": "{apps/web/**,packages/ui/**}",
            // EXP-983: the contract stamp and the serialization edges.
            "checkpoint_at": "2026-09-19T10:00:00.000Z",
            "after_node_ids": r#"["n-2"]"#,
            // EXP-984: the review gate's counter + latest verdict, and the
            // budget — all three TEXT-stored like every jsonb column.
            "review_round": "2",
            "review": r#"{"verdict":"request_changes","findings":"src/a.rs:4 off by one",
                "oracle":{"command":"cargo test -p coding","passed":false},
                "model":"fable","round":2,"head":"0123abc","at":"2026-09-19T11:00:00.000Z"}"#,
        }))
        .unwrap();
        assert_eq!(row.review_count(), 2);
        let review = row.review_facts().expect("the verdict decodes");
        assert_eq!(review.verdict, "request_changes");
        assert_eq!(review.round, 2);
        assert_eq!(review.oracle_passed, Some(false));
        assert_eq!(review.oracle_command.as_deref(), Some("cargo test -p coding"));
        assert_eq!(review.model.as_deref(), Some("fable"));
        // The commit the verdict is tied to; a row from before the field
        // simply carries none.
        assert_eq!(review.head.as_deref(), Some("0123abc"));
        assert_eq!(row.member_ids(), vec!["i-2", "i-3"]);
        assert_eq!(row.after_ids(), vec!["n-2"]);
        assert!(row.checkpoint_at.is_some());
        assert_eq!(row.wave_index(), 2);
        assert_eq!(row.lane_index(), 1);
        assert!(row.is_on_cycle());
        assert_eq!(row.touches, vec!["apps/web/**", "packages/ui/**"]);

        let bare: domain::rows::WorkflowNodeRow =
            serde_json::from_value(serde_json::json!({ "id": "n-2" })).unwrap();
        assert_eq!(bare.kind_wire(), "leaf");
        assert_eq!(bare.state_wire(), "blocked");
        assert_eq!(bare.risk_wire(), "medium");
        assert_eq!(bare.wave_index(), 0);
        assert!(!bare.is_on_cycle());
        assert!(bare.member_ids().is_empty());
        assert!(bare.touches.is_empty());
        assert!(bare.after_ids().is_empty());
        assert_eq!(bare.checkpoint_at, None);
        // EXP-984: no review reads as exactly that.
        assert_eq!(bare.review_count(), 0);
        assert_eq!(bare.review_facts(), None);
    }
}
