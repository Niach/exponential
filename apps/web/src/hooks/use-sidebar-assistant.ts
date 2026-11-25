import { useMemo } from "react"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"
import type { Team } from "@/db/schema"
import { useAgents } from "@/hooks/use-session-tree"
import { useAgentBadges } from "@/components/team/sidebar-agents"

// #EXP-2124 (sidebar-tree.json `root`, sidebar-menus.json): the viewer's
// Assistant as the sidebar's first row and the new-tab menu read it: its
// name, its chat (under the layout's team, the FAB's own read) and its
// conversation's count badge.

export interface SidebarAssistant {
  agentId: string
  name: string
  /** The team its chat route (and its tab) runs under. */
  teamId: string
  teamSlug: string
  /** Its chat. */
  href: string
  badge?: CountBadgeValue
}

export function useSidebarAssistant(
  team: Pick<Team, `id`> | null | undefined,
  teamSlug: string,
  me: string | null | undefined
): SidebarAssistant | null {
  const { agents } = useAgents(team?.id, me)
  const assistant = useMemo(() => agents.find((agent) => agent.assistant) ?? null, [agents])
  const rows = useMemo(() => (assistant ? [assistant] : []), [assistant])
  const badge = useAgentBadges(team?.id, rows, me).get(assistant?.agentId ?? ``)
  const teamId = team?.id ?? null
  return useMemo(
    () =>
      assistant && teamId
        ? {
            agentId: assistant.agentId,
            name: assistant.name,
            teamId,
            teamSlug,
            href: `/t/${encodeURIComponent(teamSlug)}/agents/${encodeURIComponent(assistant.agentId)}`,
            badge,
          }
        : null,
    [assistant, teamId, teamSlug, badge]
  )
}
