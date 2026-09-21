import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { CodingSession, Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import {
  codingSessionCollection,
  workflowCollection,
  workflowNodeCollection,
} from "@/lib/collections"
import type { WorkflowNodeRun } from "@/lib/workflow-run"
import {
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"

// EXP-981: the two synced workflow shapes, read the way every other list
// reads its collection. `wave`/`lane`/`on_cycle` on a node ARE the layout —
// nothing here sorts a graph, it only orders the LIST (newest first) and the
// nodes by the server's (wave, lane).

/** The team's workflows, newest first. `undefined` skips the query. */
export function useTeamWorkflows(teamId: string | undefined): SyncedWorkflow[] {
  const { data: rows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ w: workflowCollection })
            .where(({ w }) => eq(w.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(
    () =>
      [...((rows ?? []) as SyncedWorkflow[])].sort(
        (left, right) =>
          new Date(right.createdAt).getTime() -
          new Date(left.createdAt).getTime()
      ),
    [rows]
  )
}

/** One workflow's row, or null while it has not synced (or is gone). */
export function useWorkflow(
  workflowId: string | undefined
): SyncedWorkflow | null {
  const { data: rows } = useLiveQuery(
    (query) =>
      workflowId
        ? query
            .from({ w: workflowCollection })
            .where(({ w }) => eq(w.id, workflowId))
        : undefined,
    [workflowId]
  )
  return ((rows ?? []) as SyncedWorkflow[])[0] ?? null
}

/** One workflow's nodes in the server's reading order (wave, then lane). */
export function useWorkflowNodes(
  workflowId: string | undefined
): WorkflowNode[] {
  const { data: rows } = useLiveQuery(
    (query) =>
      workflowId
        ? query
            .from({ n: workflowNodeCollection })
            .where(({ n }) => eq(n.workflowId, workflowId))
        : undefined,
    [workflowId]
  )
  return useMemo(
    () =>
      [...((rows ?? []) as WorkflowNode[])].sort(
        (left, right) => left.wave - right.wave || left.lane - right.lane
      ),
    [rows]
  )
}

/** Each node's synced coding session, by node id — what the graph paints its
 *  live dot from and what the Running strip links into. A node whose session
 *  row has not synced is simply absent. */
export function useWorkflowNodeRuns(
  teamId: string | undefined,
  nodes: readonly WorkflowNode[],
  issueById: ReadonlyMap<string, Issue>
): ReadonlyMap<string, WorkflowNodeRun> {
  const { data: rows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(() => {
    const sessionById = new Map(
      ((rows ?? []) as CodingSession[]).map((session) => [session.id, session])
    )
    const runs = new Map<string, WorkflowNodeRun>()
    for (const node of nodes) {
      const session = node.sessionId ? sessionById.get(node.sessionId) : undefined
      if (!session) continue
      const prState = issueById.get(node.issueId)?.prState
      // `live` is the row's status alone — a run the engine lost is reported
      // by the NODE's own state, never by a stale dot. The same rule ×4.
      const live = session.status === `running` || session.status === `in_review`
      runs.set(node.id, {
        sessionId: session.id,
        live,
        state: sessionDisplayState(session, prState),
        working: sessionRowIsWorking(session, prState),
      })
    }
    return runs
  }, [rows, nodes, issueById])
}
