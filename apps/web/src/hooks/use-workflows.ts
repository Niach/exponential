import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { workflowCollection, workflowNodeCollection } from "@/lib/collections"

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
