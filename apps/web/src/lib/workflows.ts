import { and, eq, inArray, or, sql } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  boards,
  issueRelations,
  issues,
  workflowNodes,
  workflows,
} from "@/db/schema"
import type { WorkflowMetricsJson } from "@exp/db-schema/domain"
import { boardVisible } from "@/lib/board-visibility"
import { layoutWorkflow, type LayoutEdge } from "@/lib/workflow-layout"

// EXP-981: the server half of a workflow's GRAPH. The edges are the `blocks`
// relations among the covered issues and are never copied; what is stored is
// the derived picture — each node's `wave`/`lane`/`on_cycle` and the
// workflow's `metrics` — recomputed HERE every time the graph can have moved
// (create, a membership change, a `blocks`/`parent` relation written or
// removed on a covered issue). Clients draw it; none of them lays out.

type Tx = Parameters<Parameters<typeof database.transaction>[0]>[0]
type Executor = typeof database | Tx

/** `exp/wf-<id8>`: the integration branch of a workflow. */
export function workflowIntegrationBranch(workflowId: string): string {
  return `exp/wf-${workflowId.replace(/-/g, ``).slice(0, 8)}`
}

const CLOSED_ANCHORS = new Set<string>([`done`, `cancelled`, `duplicate`])

/** The statuses a workflow still re-shapes itself in. A running workflow's
 *  node SET is the engine's business (dynamic admission, P5); its layout is
 *  still refreshed. */
const RESHAPED = new Set<string>([`draft`])

export interface WorkflowGraphNode {
  id: string
  issueId: string
  memberIssueIds: string[]
}

/**
 * Fold the picked issues into NODES: an issue whose ancestor (via `parent`)
 * is also picked joins that ancestor's compound node, and the OPEN sub-issues
 * of a picked issue join it even when nobody picked them (the planner splits
 * an issue after the workflow exists). Pure; `parentOf` is child → parent.
 */
export function foldCompoundNodes(
  pickedIds: readonly string[],
  parentOf: ReadonlyMap<string, string>,
  openIds: ReadonlySet<string>
): Array<{ issueId: string; memberIssueIds: string[] }> {
  const picked = new Set(pickedIds)
  const rootOf = (id: string): string => {
    let root = id
    const seen = new Set<string>([id])
    let cursor = parentOf.get(id)
    while (cursor !== undefined && !seen.has(cursor)) {
      seen.add(cursor)
      if (picked.has(cursor)) root = cursor
      cursor = parentOf.get(cursor)
    }
    // A parent CYCLE (legacy rows; EXP-980 refuses new ones) folds nothing:
    // each member would otherwise claim the other.
    if (cursor === id) return id
    return root
  }
  const members = new Map<string, Set<string>>()
  for (const id of pickedIds) {
    const root = rootOf(id)
    if (!members.has(root)) members.set(root, new Set())
    if (root !== id) members.get(root)!.add(id)
  }
  // Unpicked open descendants of a node's issue.
  const childrenOf = new Map<string, string[]>()
  for (const [child, parent] of parentOf) {
    childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), child])
  }
  for (const [root, set] of members) {
    const stack = [root, ...set]
    const seen = new Set(stack)
    while (stack.length > 0) {
      for (const child of childrenOf.get(stack.pop()!) ?? []) {
        if (seen.has(child)) continue
        seen.add(child)
        // Another node's own subtree stays its own.
        if (members.has(child)) continue
        if (!openIds.has(child) && !picked.has(child)) continue
        set.add(child)
        stack.push(child)
      }
    }
  }
  return [...members].map(([issueId, set]) => ({
    issueId,
    memberIssueIds: [...set].sort(),
  }))
}

/** The `blocks` edges among nodes: a relation between ANY two covered issues
 *  of two different nodes. */
export function nodeEdges(
  nodes: readonly WorkflowGraphNode[],
  blocks: ReadonlyArray<{ issueId: string; relatedIssueId: string }>
): LayoutEdge[] {
  const nodeOf = new Map<string, string>()
  for (const node of nodes) {
    nodeOf.set(node.issueId, node.id)
    for (const member of node.memberIssueIds) nodeOf.set(member, node.id)
  }
  const edges: LayoutEdge[] = []
  for (const relation of blocks) {
    const from = nodeOf.get(relation.issueId)
    const to = nodeOf.get(relation.relatedIssueId)
    if (!from || !to || from === to) continue
    edges.push([from, to])
  }
  return edges
}

/** All descendants (any depth) of `rootIds` via `parent` rows, as child →
 *  parent. Bounded: a sub-issue tree is shallow, the walk stops at 12 levels. */
async function loadParentMap(
  executor: Executor,
  rootIds: readonly string[]
): Promise<Map<string, string>> {
  const parentOf = new Map<string, string>()
  let frontier = [...new Set(rootIds)]
  for (let depth = 0; depth < 12 && frontier.length > 0; depth += 1) {
    const rows = await executor
      .select({
        parent: issueRelations.issueId,
        child: issueRelations.relatedIssueId,
      })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.type, `parent`),
          or(
            inArray(issueRelations.issueId, frontier),
            inArray(issueRelations.relatedIssueId, frontier)
          )
        )
      )
    const next: string[] = []
    for (const row of rows) {
      if (parentOf.has(row.child)) continue
      parentOf.set(row.child, row.parent)
      next.push(row.child)
    }
    frontier = next
  }
  return parentOf
}

/**
 * Re-derive one workflow's picture and write what changed. For a DRAFT the
 * node set is re-folded first (compound nodes follow the `parent` relations);
 * for every status the layout + metrics are refreshed. Returns the metrics.
 */
export async function replanWorkflow(
  tx: Tx,
  workflowId: string
): Promise<WorkflowMetricsJson | null> {
  const [workflow] = await tx
    .select({
      id: workflows.id,
      teamId: workflows.teamId,
      status: workflows.status,
    })
    .from(workflows)
    .where(eq(workflows.id, workflowId))
    .limit(1)
  if (!workflow) return null

  let nodes = await tx
    .select({
      id: workflowNodes.id,
      issueId: workflowNodes.issueId,
      memberIssueIds: workflowNodes.memberIssueIds,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.workflowId, workflowId))

  if (RESHAPED.has(workflow.status) && nodes.length > 0) {
    // Everything the workflow covers is re-folded from scratch, so a sub-issue
    // whose `parent` link was REMOVED comes back as a node of its own instead
    // of silently leaving the workflow with its old compound node.
    const picked = [
      ...new Set(nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])),
    ]
    const parentOf = await loadParentMap(tx, picked)
    const candidates = [...new Set([...parentOf.keys(), ...parentOf.values()])]
    const open = candidates.length
      ? await tx
          .select({ id: issues.id, status: issues.status })
          .from(issues)
          .innerJoin(boards, eq(boards.id, issues.boardId))
          .where(and(inArray(issues.id, candidates), boardVisible()))
      : []
    const openIds = new Set(
      open.filter((row) => !CLOSED_ANCHORS.has(row.status)).map((row) => row.id)
    )
    const folded = foldCompoundNodes(picked, parentOf, openIds)
    const keep = new Map(folded.map((node) => [node.issueId, node]))
    const gone = nodes.filter((node) => !keep.has(node.issueId))
    if (gone.length > 0) {
      await tx.delete(workflowNodes).where(
        inArray(
          workflowNodes.id,
          gone.map((node) => node.id)
        )
      )
    }
    nodes = nodes.filter((node) => keep.has(node.issueId))
    for (const node of nodes) {
      const next = keep.get(node.issueId)!
      if (next.memberIssueIds.join() === [...node.memberIssueIds].sort().join()) continue
      await tx
        .update(workflowNodes)
        .set({ memberIssueIds: next.memberIssueIds })
        .where(eq(workflowNodes.id, node.id))
      node.memberIssueIds = next.memberIssueIds
    }
    const have = new Set(nodes.map((node) => node.issueId))
    const fresh = folded.filter((node) => !have.has(node.issueId))
    if (fresh.length > 0) {
      const inserted = await tx
        .insert(workflowNodes)
        .values(
          fresh.map((node) => ({
            workflowId,
            teamId: workflow.teamId,
            issueId: node.issueId,
            memberIssueIds: node.memberIssueIds,
          }))
        )
        .returning({
          id: workflowNodes.id,
          issueId: workflowNodes.issueId,
          memberIssueIds: workflowNodes.memberIssueIds,
        })
      nodes = [...nodes, ...inserted]
    }
  }

  const covered = [
    ...new Set(nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])),
  ]
  const [blocks, identifiers] = covered.length
    ? await Promise.all([
        tx
          .select({
            issueId: issueRelations.issueId,
            relatedIssueId: issueRelations.relatedIssueId,
          })
          .from(issueRelations)
          .where(
            and(
              eq(issueRelations.type, `blocks`),
              inArray(issueRelations.issueId, covered),
              inArray(issueRelations.relatedIssueId, covered)
            )
          ),
        tx
          .select({ id: issues.id, identifier: issues.identifier })
          .from(issues)
          .where(inArray(issues.id, covered)),
      ])
    : [[], []]
  const identifierOf = new Map(identifiers.map((row) => [row.id, row.identifier]))

  const layout = layoutWorkflow(
    nodes.map((node) => ({
      id: node.id,
      key: identifierOf.get(node.issueId) ?? node.issueId,
    })),
    nodeEdges(nodes, blocks)
  )

  const current = await tx
    .select({
      id: workflowNodes.id,
      wave: workflowNodes.wave,
      lane: workflowNodes.lane,
      onCycle: workflowNodes.onCycle,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.workflowId, workflowId))
  for (const row of current) {
    const placement = layout.placements.get(row.id)
    if (!placement) continue
    if (
      placement.wave === row.wave &&
      placement.lane === row.lane &&
      placement.cycle === row.onCycle
    ) {
      continue
    }
    await tx
      .update(workflowNodes)
      .set({ wave: placement.wave, lane: placement.lane, onCycle: placement.cycle })
      .where(eq(workflowNodes.id, row.id))
  }

  const metrics: WorkflowMetricsJson = {
    ...layout.metrics,
    cycleEdges: [...layout.cycleEdges].sort(),
  }
  // Counters the engine wrote (P3+) survive a re-layout: shape keys win,
  // everything else is kept.
  await tx
    .update(workflows)
    .set({
      metrics: sql`${workflows.metrics} || ${JSON.stringify(metrics)}::jsonb`,
    })
    .where(eq(workflows.id, workflowId))
  return metrics
}

/**
 * A `blocks`/`parent` relation on these issues was written or removed:
 * re-derive every live workflow that covers one of them. Cheap when none
 * does (one indexed probe).
 */
export async function replanWorkflowsForIssues(
  tx: Tx,
  issueIds: readonly string[]
): Promise<void> {
  const ids = [...new Set(issueIds)]
  if (ids.length === 0) return
  const rows = await tx
    .select({ workflowId: workflowNodes.workflowId })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`draft`, `running`, `paused`]),
        or(
          inArray(workflowNodes.issueId, ids),
          sql`${workflowNodes.memberIssueIds} ?| ${sql.param(ids)}::text[]`
        )
      )
    )
  for (const workflowId of new Set(rows.map((row) => row.workflowId))) {
    await replanWorkflow(tx, workflowId)
  }
}

/**
 * EXP-982: is this issue covered by a node of a workflow that is still
 * running (or paused)? Such an issue's PR merges into the workflow's
 * integration branch, so the merge must not move it to the team's PR-merge
 * status yet — the final PR's merge does.
 */
export async function issueLandsInLiveWorkflow(
  executor: Executor,
  issueId: string
): Promise<boolean> {
  const rows = await executor
    .select({ id: workflowNodes.id })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`running`, `paused`]),
        or(
          eq(workflowNodes.issueId, issueId),
          sql`${workflowNodes.memberIssueIds} ? ${issueId}`
        )
      )
    )
    .limit(1)
  return rows.length > 0
}

/** The live workflow node covering an issue, with what `pr_open` needs to
 *  derive the base: agents pass nothing new. */
export async function liveWorkflowBaseForIssue(
  executor: Executor,
  issueId: string
): Promise<{ workflowId: string; nodeId: string; base: string } | null> {
  const [row] = await executor
    .select({
      workflowId: workflows.id,
      nodeId: workflowNodes.id,
      baseBranch: workflowNodes.baseBranch,
      integrationBranch: workflows.integrationBranch,
    })
    .from(workflowNodes)
    .innerJoin(workflows, eq(workflows.id, workflowNodes.workflowId))
    .where(
      and(
        inArray(workflows.status, [`running`, `paused`]),
        or(
          eq(workflowNodes.issueId, issueId),
          sql`${workflowNodes.memberIssueIds} ? ${issueId}`
        )
      )
    )
    .limit(1)
  if (!row) return null
  return {
    workflowId: row.workflowId,
    nodeId: row.nodeId,
    base: row.baseBranch ?? row.integrationBranch,
  }
}
