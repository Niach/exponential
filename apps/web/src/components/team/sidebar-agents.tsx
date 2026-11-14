import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { agentChatRows } from "@exp/domain-contract/chats"
import { rowBadge, type CountBadgeValue } from "@exp/domain-contract/nav-badges"
import {
  AGENT_ROW_MENU_KIND,
  Badge,
  conceptIcon,
  ExponentialLogo,
  getActionIcon,
  menuProps,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuTrailing,
} from "@exp/ui"
import { CHATS_LIST_COPY, type AgentRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"
import { useTeamAgentRows } from "@/hooks/use-agent-row"
import { SidebarFolderList } from "@/components/team/sidebar-folders"
import { useDigestMessages, useSessionReads, useTeamSessions } from "@/hooks/use-session-tree"

// EXP-1293: the rail's AGENTS section, ABOVE Boards and drawn like them —
// one row per team agent (`listAgents`, fixture-locked: sortOrder, then
// name): its icon in its color + its name, and (#FEED-232) THE count badge
// `rowBadge` of the viewer's conversation with it (unread agent messages,
// yellow while it needs you; busy alone = no badge). A row opens the agent's channel; its
// right-click / long-press is THE session screen menu (#FEED-239: no gear;
// Agent settings opens that chat with its settings), and the "+" files a
// new agent. #FEED-239 / #EXP-1837: the host passes `railAgents(agents,
// apps)`: an app's agent is listed under Apps only, and the viewer's
// Assistant is THE FAB (`assistant-fab.tsx`), never a row here.

const UiAddIcon = conceptIcon(`ui-add`)

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
  const Icon = getActionIcon({ icon: agent.icon })
  return <Icon className={cn(`size-4 shrink-0`, className)} style={{ color: agent.color }} />
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

export function SidebarAgentsSection({
  teamId,
  teamSlug,
  agents,
  isOwner,
  activeAgentId,
  badges,
}: {
  /** #EXP-1832: the team the member's folders and order belong to. */
  teamId: string | undefined
  teamSlug: string
  agents: readonly AgentRow[]
  /** #FEED-232: `useAgentBadges`, by agent id. */
  badges?: ReadonlyMap<string, CountBadgeValue>
  isOwner: boolean
  activeAgentId: string | null
}) {
  // #EXP-1832: the default order is the list's (listAgents); the member's
  // folders and drag order sit on top (sidebar-folders.json).
  const ids = useMemo(() => agents.map((agent) => agent.agentId), [agents])
  const byId = useMemo(() => new Map(agents.map((agent) => [agent.agentId, agent])), [agents])
  return (
    <SidebarGroup data-testid="sidebar-agents">
      <SidebarGroupLabel>{CHATS_LIST_COPY.agents}</SidebarGroupLabel>
      {isOwner && (
        <SidebarGroupAction asChild title="New agent">
          <Link
            to="/t/$teamSlug/settings/agents/new"
            params={{ teamSlug }}
            aria-label="New agent"
          >
            <UiAddIcon className="h-4 w-4" />
          </Link>
        </SidebarGroupAction>
      )}
      <SidebarGroupContent>
        <SidebarFolderList
          teamId={teamId}
          section="agent"
          ids={ids}
          renderItem={(id, itemProps, guides) => {
            const agent = byId.get(id)
            if (!agent) return null
            return (
              <SidebarMenuItem key={agent.agentId} {...itemProps}>
                {guides}
                <SidebarMenuButton
                  asChild
                  density="compact"
                  isActive={activeAgentId === agent.agentId}
                >
                  <Link
                    to="/t/$teamSlug/agents/$agentId"
                    params={{ teamSlug, agentId: agent.agentId }}
                    data-testid="agents-row"
                    // Chat 58cb32df: THE agent menu on right-click / long-press.
                    {...menuProps(AGENT_ROW_MENU_KIND, agent.agentId)}
                  >
                    <AgentIcon agent={agent} />
                    <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                  </Link>
                </SidebarMenuButton>
                <AgentBadge badge={badges?.get(agent.agentId)} />
              </SidebarMenuItem>
            )
          }}
        />
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

