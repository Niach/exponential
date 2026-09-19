// EXP-981: what every client SAYS about a workflow. The graph's geometry is
// the server's (`wave`/`lane` on the synced nodes); this is the rest — bands,
// captions, the edges between nodes — mirrored ×4 (iOS `ExpCore/Sources/
// Domain/WorkflowView.swift`, Android `domain/WorkflowView.kt`, desktop
// `domain::workflow_view`) and locked by the contract fixture
// `domain-contract/fixtures/workflow-view.json`. All strings byte-identical.

export type WorkflowBand = `running` | `draft` | `done`

/** The list's three bands, in order. Flat rows under each, no row buttons. */
export const WORKFLOW_BANDS: ReadonlyArray<{ key: WorkflowBand; title: string }> = [
  { key: `running`, title: `Running` },
  { key: `draft`, title: `Draft` },
  { key: `done`, title: `Done` },
]

export const WORKFLOWS_TITLE = `Workflows`
export const WORKFLOWS_EMPTY_TITLE = `No workflows yet`
export const WORKFLOWS_EMPTY_BODY = `Select backlog issues on a board and choose Create workflow to plan them as one parallel run.`
export const PLAN_WORKFLOW_LABEL = `Plan`
export const DELETE_WORKFLOW_LABEL = `Delete workflow`
/** The bulk bar's play menu (EXP-981), in order. */
export const START_AS_BATCH_LABEL = `Start as batch`
export const START_AS_STACK_LABEL = `Start as stack`
export const CREATE_WORKFLOW_LABEL = `Create workflow…`

/** `paused` is a running workflow someone held; `cancelled` is over. An
 *  unknown status (a newer server) lands in Done rather than vanishing. */
export function workflowBand(status: string): WorkflowBand {
  if (status === `running` || status === `paused`) return `running`
  if (status === `draft`) return `draft`
  return `done`
}

export interface WorkflowShape {
  nodes: number
  depth: number
  width: number
  cycles: string[][]
}

/** `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`. */
export function workflowShapeLine(metrics: WorkflowShape): string {
  const nodes = metrics.nodes === 1 ? `1 node` : `${metrics.nodes} nodes`
  return `${nodes} · depth ${metrics.depth} · width ${metrics.width}`
}

/** Null while the workflow could start; else the cycles spelled out. */
export function workflowCycleNote(metrics: WorkflowShape): string | null {
  if (metrics.cycles.length === 0) return null
  const spelled = metrics.cycles.map((keys) => keys.join(`, `)).join(`; `)
  return `These issues block each other in a cycle: ${spelled}. Remove one relation to start.`
}

const STATE_LABELS: Record<string, string> = {
  proposed: `Proposed`,
  blocked: `Blocked`,
  ready: `Ready`,
  running: `Running`,
  waiting: `Waiting`,
  in_review: `In review`,
  updating: `Updating`,
  landed: `Landed`,
  failed: `Failed`,
  skipped: `Skipped`,
  paused: `Paused`,
}

const KIND_LABELS: Record<string, string> = {
  contract: `Contract`,
  leaf: `Leaf`,
  integration: `Integration`,
}

export function workflowNodeStateLabel(state: string): string {
  return STATE_LABELS[state] ?? state
}

export function workflowNodeKindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind
}

/** The tone a node's state paints in. `waiting` is the ONLY amber one (and
 *  the only one that pushes): amber means "a person is needed". */
export type WorkflowNodeTone = `muted` | `active` | `amber` | `success` | `danger`

export function workflowNodeTone(state: string): WorkflowNodeTone {
  if (state === `waiting`) return `amber`
  if (state === `failed`) return `danger`
  if (state === `landed`) return `success`
  if (state === `running` || state === `updating` || state === `in_review`) {
    return `active`
  }
  return `muted`
}

export interface CaptionNode {
  kind: string
  state: string
  risk: string
}

/**
 * The ONE caption under a node. A draft has no states worth reading yet, so
 * it names the plan (`Contract`, `Leaf · high risk`); a started workflow names
 * the state, prefixed by the kind only for the two special nodes
 * (`Contract · Running`, `In review`).
 */
export function workflowNodeCaption(node: CaptionNode, workflowStatus: string): string {
  if (workflowStatus === `draft`) {
    const kind = workflowNodeKindLabel(node.kind)
    return node.risk === `high` ? `${kind} · high risk` : kind
  }
  const state = workflowNodeStateLabel(node.state)
  return node.kind === `leaf` ? state : `${workflowNodeKindLabel(node.kind)} · ${state}`
}

/** `EXP-14 +3` for a compound node (a parent run as one batch with its
 *  sub-issues), the bare identifier otherwise. */
export function workflowNodeTitle(identifier: string, memberCount: number): string {
  return memberCount > 0 ? `${identifier} +${memberCount}` : identifier
}

export interface EdgeNode {
  id: string
  issueId: string
  memberIssueIds: readonly string[]
}

export interface EdgeRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

export interface WorkflowEdge {
  from: string
  to: string
  /** Inside a blocking cycle (`metrics.cycleEdges`): drawn red. */
  cycle: boolean
}

/**
 * The edges between a workflow's nodes, from the synced `blocks` relations: a
 * relation between ANY two covered issues of two different nodes (the server's
 * `nodeEdges`). One edge per node pair, ordered by (from, to) node id.
 * `cycleEdges` = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
 */
export function workflowEdges(
  nodes: readonly EdgeNode[],
  relations: readonly EdgeRelation[],
  cycleEdges: readonly string[] = []
): WorkflowEdge[] {
  const nodeOf = new Map<string, string>()
  for (const node of nodes) {
    nodeOf.set(node.issueId, node.id)
    for (const member of node.memberIssueIds) nodeOf.set(member, node.id)
  }
  const onCycle = new Set(cycleEdges)
  const seen = new Set<string>()
  const edges: WorkflowEdge[] = []
  for (const relation of relations) {
    if (relation.type !== `blocks`) continue
    const from = nodeOf.get(relation.issueId)
    const to = nodeOf.get(relation.relatedIssueId)
    if (!from || !to || from === to) continue
    const key = `${from}\n${to}`
    if (seen.has(key)) continue
    seen.add(key)
    edges.push({ from, to, cycle: onCycle.has(key) })
  }
  return edges.sort((a, b) =>
    a.from < b.from ? -1 : a.from > b.from ? 1 : a.to < b.to ? -1 : a.to > b.to ? 1 : 0
  )
}

// ── Running a workflow (EXP-982) ────────────────────────────────────────────

export const START_WORKFLOW_LABEL = `Start`
export const PAUSE_WORKFLOW_LABEL = `Pause`
export const RESUME_WORKFLOW_LABEL = `Resume`
export const CANCEL_WORKFLOW_LABEL = `Cancel workflow`
export const CANCEL_WORKFLOW_CONFIRM = `Its live runs end and its branch is deleted. Nothing reached the default branch.`
export const APPROVE_NODE_LABEL = `Approve and land`
export const WITHDRAW_APPROVAL_LABEL = `Withdraw approval`
export const MERGE_TRAIN_TITLE = `Merge train`
export const MERGE_TRAIN_EMPTY = `Nothing is waiting to land.`
export const FINAL_PR_TITLE = `Final pull request`

export interface StartableWorkflow {
  status: string
  deviceId: string | null
  repositoryId: string | null
  startOn: string
}

/**
 * Why Start is disabled, or null when the draft can start. One reason, the
 * most fundamental first; the server refuses with the same sentences.
 */
export function workflowStartBlocker(
  workflow: StartableWorkflow,
  metrics: WorkflowShape
): string | null {
  if (workflow.status !== `draft`) return `The workflow has already started.`
  if (metrics.nodes === 0) return `The workflow has no issues.`
  const cycle = workflowCycleNote(metrics)
  if (cycle) return cycle
  if (!workflow.repositoryId) return `The workflow's repository is gone.`
  if (!workflow.deviceId) return `Pick the device that runs this workflow first.`
  if (workflow.startOn !== `landed`) {
    return `Only "When landed" starts are available yet.`
  }
  return null
}

/** A node lands without a person only when the workflow has no gate AND it is
 *  not the contract (always human-gated). Mirrors the server. */
export function workflowNodeNeedsApproval(gate: string, kind: string): boolean {
  return kind === `contract` || gate !== `none`
}

export interface TrainNode {
  id: string
  kind: string
  state: string
  wave: number
  lane: number
  approvedAt: string | Date | null
}

export type TrainStep = `next` | `queued` | `needs-approval` | `updating`

export interface TrainEntry {
  id: string
  step: TrainStep
}

/**
 * The merge train: every node whose PR is up (`in_review`, or `updating`
 * while it merges the trunk in), in landing order (wave, then lane). The
 * FIRST node that is cleared to land is `next`; cleared ones behind it are
 * `queued`; one still waiting for a person says so.
 */
export function workflowMergeTrain(
  nodes: readonly TrainNode[],
  gate: string
): TrainEntry[] {
  const waiting = nodes
    .filter((node) => node.state === `in_review` || node.state === `updating`)
    .sort((a, b) => a.wave - b.wave || a.lane - b.lane || (a.id < b.id ? -1 : 1))
  let nextTaken = false
  return waiting.map((node) => {
    if (node.state === `updating`) return { id: node.id, step: `updating` }
    if (workflowNodeNeedsApproval(gate, node.kind) && !node.approvedAt) {
      return { id: node.id, step: `needs-approval` }
    }
    if (nextTaken) return { id: node.id, step: `queued` }
    nextTaken = true
    return { id: node.id, step: `next` }
  })
}

const TRAIN_STEP_LABELS: Record<TrainStep, string> = {
  next: `Landing next`,
  queued: `Queued`,
  "needs-approval": `Needs approval`,
  updating: `Merging the trunk in`,
}

export function workflowTrainStepLabel(step: TrainStep): string {
  return TRAIN_STEP_LABELS[step]
}

/**
 * The final-PR node's caption, or null while the node is not drawn: it
 * appears once every node landed (or was skipped), after the last wave.
 */
export function workflowFinalPrCaption(
  nodes: ReadonlyArray<{ state: string }>,
  finalPrState: string | null,
  finalPrNumber: number | null
): string | null {
  if (nodes.length === 0) return null
  const allIn = nodes.every((node) => node.state === `landed` || node.state === `skipped`)
  if (!allIn && finalPrNumber === null) return null
  if (finalPrNumber === null) return `Opening the pull request`
  const label =
    finalPrState === `merged` ? `Merged` : finalPrState === `closed` ? `Closed` : `Open`
  return `#${finalPrNumber} · ${label}`
}

export const RETRY_NODE_LABEL = `Retry`
export const SKIP_NODE_LABEL = `Skip`
export const SKIP_NODE_CONFIRM = `Its dependents go on without it. The node's work is not part of the final pull request.`
