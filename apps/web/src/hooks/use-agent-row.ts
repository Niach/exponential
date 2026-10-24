import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { agentCollection } from "@/lib/collections"
import type { SyncedAgent } from "@/components/team/agents-section"

// NEW-2: one synced `agents` definition row (Settings → Agents, the agent's
// conversation page). null while absent; `isReady` once the snapshot is in.
export function useAgentRow(agentId: string | null | undefined): {
  agent: SyncedAgent | null
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      agentId
        ? query.from({ agents: agentCollection }).where(({ agents }) => eq(agents.id, agentId))
        : undefined,
    [agentId]
  )
  const agent = ((data ?? []) as SyncedAgent[])[0] ?? null
  return { agent, isReady: Boolean(agentId) && isReady }
}

/** #EXP-1383: ONE team's synced agent definitions (none while `teamId` is
 *  unknown): the assignee picker's agent rows and agent assignees. */
export function useAgentRows(teamId: string | null | undefined): SyncedAgent[] {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ agents: agentCollection }).where(({ agents }) => eq(agents.teamId, teamId))
        : undefined,
    [teamId ?? null]
  )
  return useMemo(() => (data ?? []) as SyncedAgent[], [data])
}

/** #EXP-1383: every synced agent (all member teams): what `issueActor`
 *  resolves an agent actor against, where the row carries no team. */
export function useAllAgentRows(): SyncedAgent[] {
  const { data } = useLiveQuery((query) => query.from({ agents: agentCollection }))
  return useMemo(() => (data ?? []) as SyncedAgent[], [data])
}
