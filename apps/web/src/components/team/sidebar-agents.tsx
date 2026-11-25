import { useMemo, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { agentChatRows } from "@exp/domain-contract/chats"
import { rowBadge, type CountBadgeValue } from "@exp/domain-contract/nav-badges"
import {
  Badge,
  ExponentialLogo,
  agentGlyph,
  BoardGlyph,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuTrailing,
  type MenuTargetProps,
} from "@exp/ui"
import type { AgentRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"
import { useTeamAgentRows } from "@/hooks/use-agent-row"
import type { SidebarRowItemProps } from "@/components/team/sidebar-folders"
import { useDigestMessages, useSessionReads, useTeamSessions } from "@/hooks/use-session-tree"

// EXP-1293 / #EXP-2027: an AGENT's row, drawn like a board's inside its
// team's list: its icon in its color + its name, and (#FEED-232) THE count
// badge `rowBadge` of the viewer's conversation with it (unread agent
// messages, yellow while it needs you; busy alone = no badge). A row opens the
// agent's channel; its right-click / long-press is THE sidebar table's
// `agent` menu (#EXP-2124, sidebar-menus.json; #FEED-239: no gear). #FEED-239 / #EXP-1837: the host lists
// `railAgents(agents, apps)`: an app's agent is listed as its app only, and
// the viewer's Assistant is THE FAB (`assistant-fab.tsx`), never a row here.

/** An agent's glyph: the curated icon set boards and actions share; S-3:
 *  the viewer's Assistant = the Exponential brand mark in the foreground
 *  colour (never its stored icon/color). */
export function AgentIcon({
  agent,
  className,
}: {
  agent: Pick<AgentRow, `icon` | `color`> & { assistant?: boolean }
  className?: string
}) {
  if (agent.assistant) {
    return (
      <ExponentialLogo variant="light" className={cn(`size-4 shrink-0 text-foreground`, className)} />
    )
  }
  return <BoardGlyph board={agentGlyph(agent)} className={className} />
}

/** #FEED-232: each agent row's count badge (`rowBadge` over the viewer's
 *  conversation's unread agent messages, `agentChatRows`), keyed by agent.
 *  The scope adds the viewer's global Assistant's personal team. */
export function useAgentBadges(
  teamId: string | undefined,
  agents: readonly AgentRow[],
  me: string | null | undefined
): ReadonlyMap<string, CountBadgeValue> {
  const teamAgents = useTeamAgentRows(teamId ? [teamId] : [], me)
  const personal = teamAgents.find((agent) => agent.teamId !== teamId)?.teamId ?? null
  const scope = useMemo(() => (teamId ? (personal ? [teamId, personal] : [teamId]) : []), [teamId, personal])
  const { tree } = useTeamSessions(scope)
  const { messages } = useDigestMessages({ teamIds: scope })
  const reads = useSessionReads()
  return useMemo(() => {
    const rows = agentChatRows({
      agents,
      sessions: tree,
      messages,
      reads,
      me: me ?? null,
      now: new Date(0).toISOString(),
      utcOffsetMinutes: 0,
    })
    return new Map(rows.map((row) => [row.agentId, rowBadge(row)]))
  }, [agents, tree, messages, reads, me])
}

/** The trailing count badge, in the row's trailing column (EXP-1477). */
function AgentBadge({ badge }: { badge: CountBadgeValue | undefined }) {
  if (!badge || badge.count <= 0) return null
  return (
    <SidebarMenuTrailing className="pointer-events-none">
      <Badge
        count={badge.count}
        tone={badge.tone}
        data-testid={badge.tone === `attention` ? `agent-needs-you` : `agent-unread`}
      />
    </SidebarMenuTrailing>
  )
}

/** #EXP-2027: an agent's row in its team's list (sidebar-tree.json `teamItems`):
 *  its icon in its color + its name opening its channel, THE table's agent
 *  menu on right-click, its count badge trailing. */
export function SidebarAgentRow({
  agent,
  teamSlug,
  menu,
  active,
  badge,
  itemProps,
  guides,
}: {
  agent: AgentRow
  teamSlug: string
  /** #EXP-2124: the row's context-menu target (`menuProps`). */
  menu: MenuTargetProps
  active: boolean
  /** #FEED-232: `useAgentBadges`'s value for this agent. */
  badge: CountBadgeValue | undefined
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  return (
    <SidebarMenuItem key={agent.agentId} {...itemProps}>
      {guides}
      <SidebarMenuButton asChild density="compact" isActive={active}>
        <Link
          to="/t/$teamSlug/agents/$agentId"
          params={{ teamSlug, agentId: agent.agentId }}
          data-testid="agents-row"
          {...menu}
        >
          <AgentIcon agent={agent} />
          <span className="min-w-0 flex-1 truncate">{agent.name}</span>
        </Link>
      </SidebarMenuButton>
      <AgentBadge badge={badge} />
    </SidebarMenuItem>
  )
}
