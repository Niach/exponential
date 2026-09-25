// EXP-1082 §1 — session MEMBERSHIP, the shared rule.
//
// Which workflow and node a coding session belongs to, and as what
// (`coding_sessions.workflow_id` / `workflow_node_id` / `workflow_role`),
// plus the tree fields a resume carries (`started_reason`,
// `parent_session_id`). PURE: the callers load the inputs —
// `codingSessions.start` (explicit host values, the resumed predecessor,
// the team's draft/running workflow nodes) and MCP
// `exponential_sessions_start` (the parent, after the poll). Stamped ONCE on
// the server; the session tree groups by `workflow_id` first and never
// guesses again. Rules, in this order:
//
//   (a) EXPLICIT host values win — only ever passed after the caller proved
//       they come from the workflow's runner device (`workflows.device_id`).
//   (b) A RESUME (the predecessor) inherits workflow_id, workflow_node_id,
//       workflow_role, started_reason and parent_session_id, whoever calls.
//       Its started_reason is NEVER re-branded to `agent`: the predecessor's
//       reason wins whenever it has one; the frame's reason only fills a
//       NULL (a person's run resumed by an agent reads `agent`, EXP-906).
//   (c) A CHILD started from inside a workflow run (`sessions_start`,
//       `ask_parent`) inherits workflow_id + workflow_node_id from the
//       parent; its role is its own: `author` with an issue subject, else
//       null. started_reason stays the frame's (`agent`).
//   (d) A person's FRESH run whose subject is a node's issue, or a member
//       issue of a compound node, of a `draft`/`running` workflow joins that
//       node as `author` (EXP-1062: yes, once, on the server). A batch run
//       joins only when EVERY covered issue belongs to the SAME node.
//       Several candidates: a `running` workflow beats a `draft`, then the
//       newest workflow (`workflowCreatedAt`), then the lowest workflow id,
//       then the lowest node id — deterministic.
//   Otherwise the run belongs to no workflow.
//
// Membership falls through: a resume or child whose source carries NO
// workflow still gets (d) — only a source that HAS one decides.

export interface WorkflowMembershipIds {
  workflowId: string | null
  workflowNodeId: string | null
  workflowRole: string | null
}

export interface WorkflowMembershipNode {
  workflowId: string
  nodeId: string
  issueId: string
  memberIssueIds: readonly string[]
  /** The node's workflow's `status` (contract `wfStatus`). */
  workflowStatus: string
  /** The workflow's `created_at`, the tie-break between two candidates. */
  workflowCreatedAt?: Date | string | null
}

export interface WorkflowMembershipInput {
  /** ONLY values the caller verified came from the workflow's runner device. */
  explicit?: {
    workflowId: string
    workflowNodeId?: string | null
    workflowRole?: string | null
  } | null
  /** The resumed run (resumeSessionId / an account switch). */
  predecessor?: {
    workflowId: string | null
    workflowNodeId: string | null
    workflowRole: string | null
    startedReason: string | null
    parentSessionId: string | null
  } | null
  /** The run that started this one (sessions_start / ask_parent children). */
  parent?: { workflowId: string | null; workflowNodeId: string | null } | null
  /** What the frame asked for. */
  startedReason?: string | null
  /** The run's subject: [issueId], a batch's covered issues, else []. */
  issueIds: readonly string[]
  /** The team's workflow nodes with their workflow's status. */
  nodes?: readonly WorkflowMembershipNode[]
}

export interface WorkflowMembership extends WorkflowMembershipIds {
  startedReason: string | null
  parentSessionId: string | null
}

/** The workflow statuses a person's fresh run may join (rule d). */
export const JOINABLE_WORKFLOW_STATUSES = [`draft`, `running`] as const

const STATUS_RANK: Record<string, number> = { running: 0, draft: 1 }

function createdMs(value: Date | string | null | undefined): number {
  if (!value) return 0
  const ms = new Date(value).getTime()
  return Number.isFinite(ms) ? ms : 0
}

function compareCandidates(a: WorkflowMembershipNode, b: WorkflowMembershipNode): number {
  return (
    (STATUS_RANK[a.workflowStatus] ?? 9) - (STATUS_RANK[b.workflowStatus] ?? 9) ||
    createdMs(b.workflowCreatedAt) - createdMs(a.workflowCreatedAt) ||
    (a.workflowId < b.workflowId ? -1 : a.workflowId > b.workflowId ? 1 : 0) ||
    (a.nodeId < b.nodeId ? -1 : a.nodeId > b.nodeId ? 1 : 0)
  )
}

/** Rule (d): the node a fresh run's subject belongs to, or null. */
export function matchWorkflowNode(
  issueIds: readonly string[],
  nodes: readonly WorkflowMembershipNode[]
): WorkflowMembershipNode | null {
  if (issueIds.length === 0) return null
  const candidates = nodes.filter((node) => {
    if (STATUS_RANK[node.workflowStatus] === undefined) return false
    const covered = new Set([node.issueId, ...node.memberIssueIds])
    return issueIds.every((id) => covered.has(id))
  })
  if (candidates.length === 0) return null
  return [...candidates].sort(compareCandidates)[0]!
}

const NONE: WorkflowMembershipIds = {
  workflowId: null,
  workflowNodeId: null,
  workflowRole: null,
}

export function resolveWorkflowMembership(
  input: WorkflowMembershipInput
): WorkflowMembership {
  const { explicit, predecessor, parent } = input
  const frameReason = input.startedReason ?? null
  // The tree fields: a resume keeps its place; the frame fills only a gap.
  const tree = predecessor
    ? {
        startedReason: predecessor.startedReason ?? frameReason,
        parentSessionId: predecessor.parentSessionId ?? null,
      }
    : { startedReason: frameReason, parentSessionId: null }

  let ids: WorkflowMembershipIds = NONE
  if (explicit?.workflowId) {
    // (a)
    ids = {
      workflowId: explicit.workflowId,
      workflowNodeId: explicit.workflowNodeId ?? null,
      workflowRole: explicit.workflowRole ?? null,
    }
  } else if (predecessor?.workflowId) {
    // (b)
    ids = {
      workflowId: predecessor.workflowId,
      workflowNodeId: predecessor.workflowNodeId ?? null,
      workflowRole: predecessor.workflowRole ?? null,
    }
  } else if (parent?.workflowId) {
    // (c)
    ids = {
      workflowId: parent.workflowId,
      workflowNodeId: parent.workflowNodeId ?? null,
      workflowRole: input.issueIds.length > 0 ? `author` : null,
    }
  } else {
    // (d)
    const node = matchWorkflowNode(input.issueIds, input.nodes ?? [])
    if (node) {
      ids = { workflowId: node.workflowId, workflowNodeId: node.nodeId, workflowRole: `author` }
    }
  }
  return { ...ids, ...tree }
}
