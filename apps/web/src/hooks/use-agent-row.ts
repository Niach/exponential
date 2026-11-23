import { useMemo } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { agentChatOf, agentsInTeam } from "@exp/domain-contract/agent-settings"
import { agentCollection, sessionCollection, teamCollection, userCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
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

/** #EXP-1714 (agent-settings.json `assistant.teamAgentsCases`): the agents
 *  `teamIds` list: each team's own rows plus the viewer's global Assistant
 *  (its team is none of the synced teams: a personal team never syncs). */
export function useTeamAgentRows(teamIds: readonly string[], me: string | null | undefined): SyncedAgent[] {
  const all = useAllAgentRows()
  const { data: teams } = useLiveQuery((query) => query.from({ teams: teamCollection }))
  const key = teamIds.join(`,`)
  return useMemo(() => {
    const synced = (teams ?? []).map((team) => team.id)
    const seen = new Set<string>()
    return teamIds
      .flatMap((teamId) => agentsInTeam(all, teamId, synced, me))
      .filter((agent) => !seen.has(agent.id) && Boolean(seen.add(agent.id)))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [all, teams, key, me])
}

/** #EXP-1990 (agent-settings.json `groupChat`): the roots that are a team
 *  agent's group chat (every synced agent): they count for every member. */
export function useGroupChatIds(): string[] {
  const all = useAllAgentRows()
  return useMemo(() => all.flatMap((agent) => (agent.groupChatId ? [agent.groupChatId] : [])).sort(), [all])
}

/** The viewer's chat with an agent (`agentChatOf`): a team agent's GROUP
 *  chat (#EXP-1990, `agents.groupChatId`), else the root with its `agentId`
 *  they started, in ANY team: #EXP-1714, the global Assistant's lives in
 *  the personal team. */
export function useMyAgentConversation(agentId: string | null | undefined, me: string | null | undefined): SessionRow | null {
  const { agent } = useAgentRow(agentId)
  const groupChatId = agent?.groupChatId ?? null
  const { data } = useLiveQuery(
    (query) =>
      groupChatId
        ? query.from({ sessions: sessionCollection }).where(({ sessions }) => eq(sessions.id, groupChatId))
        : agentId && me
          ? query
              .from({ sessions: sessionCollection })
              .where(({ sessions }) => and(eq(sessions.agentId, agentId), eq(sessions.userId, me)))
          : undefined,
    [agentId ?? null, me ?? null, groupChatId]
  )
  return useMemo(
    () =>
      agentId
        ? agentChatOf({ agent: { agentId, groupChatId }, sessions: (data ?? []) as SessionRow[], me })
        : null,
    [data, agentId, groupChatId, me]
  )
}

/** #EXP-1990 (chats.json `chatPreview`): every synced member's name by id:
 *  a teammate's line in a chat list reads under their name. */
export function useMemberNames(): Readonly<Record<string, string>> {
  const { data } = useLiveQuery((query) => query.from({ users: userCollection }))
  return useMemo(
    () => Object.fromEntries(((data ?? []) as { id: string; name?: string | null }[]).flatMap((user) => (user.name ? [[user.id, user.name]] : []))),
    [data]
  )
}
