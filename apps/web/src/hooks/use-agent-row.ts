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
