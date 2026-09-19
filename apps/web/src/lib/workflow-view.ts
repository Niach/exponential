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
