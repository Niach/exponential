// EXP-1029 contract — the ONE place a workflow run's models come from.
//
// A workflow's `launch` jsonb is read through `normalizeWorkflowLaunch` into
// the strict `WorkflowLaunch` (`@exp/db-schema/domain`): two models, no
// more. `model` is the CHEAP one (leaf nodes, and the `Task` subagents inside
// every node run), `strongModel` the capable one (contract nodes, integration
// nodes, `risk: high` nodes and EVERY agent review). The per-phase pins of
// EXP-1002, `subagentModel` and `reviewModel` are deprecated; the gate choice
// is gone (the agent reviews every node, the one human review is the final
// PR); `startOn` is fixed to `contract`.
//
// Mirrored in Rust as `coding::workflows::launch` (the same three functions,
// same test names), so the desktop engine and the CLI daemon pick models from
// ONE rule. EXP-1014 owns the implementations and the wiring on both hosts;
// the contract tests below are the acceptance table it un-skips.
import {
  WORKFLOW_LAUNCH_DEFAULTS,
  workflowLaunchAgentValues,
  type WfNodeKind,
  type WfRisk,
  type WorkflowLaunch,
  type WorkflowLaunchStored,
} from "@exp/db-schema/domain"

export type {
  WorkflowLaunch,
  WorkflowLaunchAgent,
  WorkflowLaunchStored,
} from "@exp/db-schema/domain"
export { WORKFLOW_LAUNCH_DEFAULTS, workflowLaunchAgentValues }

/** The stored jsonb keys that fold into `strongModel`, in precedence order:
 *  the first one set wins. */
export const STRONG_MODEL_LEGACY_KEYS = [
  `reviewModel`,
  `riskModel`,
  `contractModel`,
  `integrationModel`,
] as const satisfies readonly (keyof WorkflowLaunchStored)[]

const TODO = `EXP-1014 implements lib/workflow-launch.ts`

/**
 * The stored `workflows.launch` (any vintage, or garbage) → the strict
 * launch every run reads.
 *
 * Rules (the skipped table in `workflow-launch.test.ts`):
 * - `agent`: `claude` or `codex`; anything else → `claude`.
 * - `account`: a non-empty string stays, anything else is absent.
 * - `model`: the stored `model` when set, else that agent's
 *   `WORKFLOW_LAUNCH_DEFAULTS` model.
 * - `strongModel`: the stored `strongModel` when set; else the first set of
 *   `STRONG_MODEL_LEGACY_KEYS` (an old row's pins); else that agent's
 *   default strong model.
 * - `subagentModel`, `effort`, `maxParallel` are dropped.
 */
export function normalizeWorkflowLaunch(raw: unknown): WorkflowLaunch {
  void raw
  throw new Error(TODO)
}

/**
 * The model ONE node's run spawns on: `strongModel` for a `contract` or
 * `integration` node and for any `risk: high` node, else `model`. The
 * `Task` subagents inside the run always take `model`.
 */
export function modelForNode(
  launch: WorkflowLaunch,
  kind: WfNodeKind,
  risk: WfRisk
): string {
  void launch
  void kind
  void risk
  throw new Error(TODO)
}

/** The model EVERY agent review runs on: `strongModel`, whatever the node. */
export function reviewModelFor(launch: WorkflowLaunch): string {
  void launch
  throw new Error(TODO)
}
