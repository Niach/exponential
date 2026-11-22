import { useMemo, useRef } from "react"
import { agentChatRows } from "@exp/domain-contract/chats"
import { rowBadge, type CountBadgeValue } from "@exp/domain-contract/nav-badges"
import type { AgentRow, ChatsListRow } from "@/lib/session-tree"
import { useTeamAgentRows } from "@/hooks/use-agent-row"
import {
  useAgents,
  useDigestMessages,
  useSessionReads,
  useTeamChatTitles,
  useTeamSessions,
} from "@/hooks/use-session-tree"

// #PERF-13: the every-team sidebar (#EXP-2027) used to read every list PER
// TEAM SECTION (its agents, their badges, its sessions, its chat titles:
// about 7 session live queries and 2–3 digest passes per team, per synced
// row). It reads them ONCE here, over every member team, and hands each
// section its own slice; a slice whose rows did not move keeps its identity,
// so a memoised section skips the render.

/** What a placed session's row reads of its Chats row. */
export type SidebarChatMark = Pick<ChatsListRow, `needsYou` | `busy` | `unread` | `title`>

/** One team section's data. */
export interface SidebarTeamSlice {
  /** The team's agents (`listAgents` order), the viewer's Assistant aside. */
  agents: readonly AgentRow[]
  /** #FEED-232: each agent row's badge. */
  badges: ReadonlyMap<string, CountBadgeValue>
  /** The team's synced sessions (`sidebarTeamItems`' input). */
  sessions: readonly { id: string; teamId: string; parentId: string | null }[]
  /** The team's root sessions' chat titles. */
  titles: ReadonlyMap<string, string>
  /** The team's sessions' Chats marks. */
  chatRows: ReadonlyMap<string, SidebarChatMark>
}

const EMPTY_SLICE: SidebarTeamSlice = {
  agents: [],
  badges: new Map(),
  sessions: [],
  titles: new Map(),
  chatRows: new Map(),
}

/** Every listed team's slice, read once (see the module comment). */
export function useSidebarTeamsData(
  teamIds: readonly string[],
  me: string | null | undefined,
  chatRows: ReadonlyMap<string, ChatsListRow>
): ReadonlyMap<string, SidebarTeamSlice> {
  const key = teamIds.join(`,`)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const ids = useMemo(() => [...teamIds], [key])
  // The viewer's global Assistant's conversation lives in the personal team.
  const agentDefs = useTeamAgentRows(ids, me)
  const personal = agentDefs.find((agent) => !ids.includes(agent.teamId))?.teamId ?? null
  const scope = useMemo(() => (personal ? [...ids, personal] : ids), [ids, personal])
  const { agents } = useAgents(ids, me)
  const { rows, tree } = useTeamSessions(scope)
  const { messages } = useDigestMessages({ teamIds: scope })
  const reads = useSessionReads()
  const titles = useTeamChatTitles(ids)
  const badges = useMemo(() => {
    const out = agentChatRows({
      agents,
      sessions: tree,
      messages,
      reads,
      me: me ?? null,
      now: new Date(0).toISOString(),
      utcOffsetMinutes: 0,
    })
    return new Map(out.map((row) => [row.agentId, rowBadge(row)]))
  }, [agents, tree, messages, reads, me])

  const previous = useRef(new Map<string, { signature: string; slice: SidebarTeamSlice }>())
  return useMemo(() => {
    const byTeam = new Map<string, SidebarTeamSlice>()
    const kept = new Map<string, { signature: string; slice: SidebarTeamSlice }>()
    for (const teamId of ids) {
      const teamAgents = agents.filter((agent) => agent.teamId === teamId)
      const teamBadges = new Map<string, CountBadgeValue>()
      for (const agent of teamAgents) {
        const badge = badges.get(agent.agentId)
        if (badge) teamBadges.set(agent.agentId, badge)
      }
      const sessions = rows
        .filter((row) => row.teamId === teamId)
        .map((row) => ({ id: row.id, teamId: row.teamId, parentId: row.parentId ?? null }))
      const teamTitles = new Map<string, string>()
      const teamRows = new Map<string, SidebarChatMark>()
      for (const session of sessions) {
        if (session.parentId) continue
        const title = titles.get(session.id)
        if (title !== undefined) teamTitles.set(session.id, title)
        const row = chatRows.get(session.id)
        if (row) teamRows.set(session.id, { needsYou: row.needsYou, busy: row.busy, unread: row.unread, title: row.title })
      }
      const signature = JSON.stringify([teamAgents, [...teamBadges], sessions, [...teamTitles], [...teamRows]])
      const before = previous.current.get(teamId)
      const slice =
        before && before.signature === signature
          ? before.slice
          : { agents: teamAgents, badges: teamBadges, sessions, titles: teamTitles, chatRows: teamRows }
      kept.set(teamId, { signature, slice })
      byTeam.set(teamId, slice)
    }
    previous.current = kept
    return byTeam
  }, [ids, agents, badges, rows, titles, chatRows])
}

/** A team with no slice yet (its first render). */
export function sidebarTeamSlice(
  data: ReadonlyMap<string, SidebarTeamSlice>,
  teamId: string
): SidebarTeamSlice {
  return data.get(teamId) ?? EMPTY_SLICE
}
