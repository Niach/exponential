import { useMemo } from "react"
import { and, inArray, isNull, useLiveQuery } from "@tanstack/react-db"
import { railAgents } from "@exp/domain-contract/chats"
import { chatsBadge, sumBadges, teamBadge, type CountBadgeValue } from "@exp/domain-contract/nav-badges"
import { appCollection } from "@/lib/collections"
import { useMemberNeedsYouCounts } from "@/hooks/use-app-icon-badge"
import { useSessionReads, useChatsList } from "@/hooks/use-session-tree"
import { sidebarTeamSlice, useSidebarTeamsData } from "@/hooks/use-sidebar-teams-data"
import type { ChatsListRow } from "@/lib/session-tree"

/**
 * #FEED-232 (nav-badges.json): the Chats destination's count badge
 * (`chatsBadge`) for the phone's Chats tab; md+ reads the sidebar's own
 * list. `teamIds` = every member team.
 */
export function useChatsBadge(teamIds: string | readonly string[] | undefined): CountBadgeValue {
  const { groups } = useChatsList(teamIds)
  const reads = useSessionReads()
  return useMemo(() => chatsBadge(groups, reads), [groups, reads])
}

const NO_CHAT_ROWS: ReadonlyMap<string, ChatsListRow> = new Map()

/** nav-badges.json `teams-tab`: every team row's badge, folded, summed: its
 *  share of Needs you plus its agents' and placed sessions' unread. */
export function useTeamsTabBadge(teamIds: readonly string[], me: string | null | undefined): CountBadgeValue {
  const key = teamIds.join(`,`)
  const needsYou = useMemberNeedsYouCounts()
  const data = useSidebarTeamsData(teamIds, me, NO_CHAT_ROWS)
  const { data: apps } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ a: appCollection })
            .where(({ a }) => and(inArray(a.teamId, [...teamIds]), isNull(a.deletedAt)))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const appAgents = (apps ?? []).map((app) => ({ agentId: app.agentId ?? null }))
    return sumBadges(
      teamIds.map((teamId) => {
        const slice = sidebarTeamSlice(data, teamId)
        const ids = [
          ...railAgents(slice.agents, appAgents).map((agent) => agent.agentId),
          ...slice.sessions.map((session) => session.id),
        ]
        return teamBadge({
          needsYou: needsYou.byTeam.get(teamId) ?? 0,
          open: false,
          rows: ids.flatMap((id) => slice.badges.get(id) ?? []),
        })
      })
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, data, apps, needsYou])
}
