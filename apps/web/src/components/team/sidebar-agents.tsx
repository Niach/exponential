import type * as React from "react"
import { Link } from "@tanstack/react-router"
import {
  ClaudeSpinner,
  conceptIcon,
  getActionIcon,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@exp/ui"
import { THREADS_LIST_COPY, type AgentRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// EXP-1293: the rail's AGENTS section, ABOVE Boards and drawn like them —
// one row per team agent (`listAgents`, fixture-locked: sortOrder, then
// name): its icon in its color + its name, a spinner while it works, an
// amber dot while it needs you. A row opens the agent's channel; owners get
// the gear to its settings, and the "+" files a new agent.

const UiAddIcon = conceptIcon(`ui-add`)
const NavSettingsIcon = conceptIcon(`nav-settings`)

/** An agent's glyph: the curated icon set boards and actions share. */
export function AgentIcon({
  agent,
  className,
}: {
  agent: Pick<AgentRow, `icon` | `color`>
  className?: string
}) {
  const Icon = getActionIcon({ icon: agent.icon })
  return <Icon className={cn(`size-4 shrink-0`, className)} style={{ color: agent.color }} />
}

/** The trailing state: the spinner while busy, else amber when it needs you. */
function AgentState({
  agent,
  placement,
  yieldsOnHover = false,
}: {
  agent: Pick<AgentRow, `busy` | `needsYou`>
  placement: `row` | `icon`
  /** The owner's hover gear takes the same corner. */
  yieldsOnHover?: boolean
}) {
  const position = cn(
    placement === `row` ? `right-2 top-1/2 -translate-y-1/2` : `right-1 top-1`,
    yieldsOnHover && `group-hover/menu-item:opacity-0`
  )
  if (agent.needsYou) {
    return (
      <span
        aria-hidden
        data-testid="agent-needs-you"
        className={cn(`pointer-events-none absolute size-2 rounded-full bg-amber-500`, position)}
      />
    )
  }
  if (agent.busy && placement === `row`) {
    return (
      <span aria-hidden className={cn(`pointer-events-none absolute flex`, position)}>
        <ClaudeSpinner className="size-3.5" />
      </span>
    )
  }
  return null
}

export function SidebarAgentsSection({
  teamSlug,
  agents,
  isOwner,
  activeAgentId,
}: {
  teamSlug: string
  agents: readonly AgentRow[]
  isOwner: boolean
  activeAgentId: string | null
}) {
  return (
    <SidebarGroup data-testid="sidebar-agents">
      <SidebarGroupLabel>{THREADS_LIST_COPY.agents}</SidebarGroupLabel>
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
        <SidebarMenu>
          {agents.map((agent) => (
            <SidebarMenuItem key={agent.agentId}>
              <SidebarMenuButton
                asChild
                density="compact"
                isActive={activeAgentId === agent.agentId}
              >
                <Link
                  to="/t/$teamSlug/agents/$agentId"
                  params={{ teamSlug, agentId: agent.agentId }}
                  data-testid="agents-row"
                >
                  <AgentIcon agent={agent} />
                  <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                </Link>
              </SidebarMenuButton>
              <AgentState agent={agent} placement="row" yieldsOnHover={isOwner} />
              {isOwner && (
                <SidebarMenuAction asChild showOnHover title="Agent settings">
                  <Link
                    to="/t/$teamSlug/settings/agents/$agentId"
                    params={{ teamSlug, agentId: agent.agentId }}
                    aria-label={`Settings for ${agent.name}`}
                  >
                    <NavSettingsIcon />
                  </Link>
                </SidebarMenuAction>
              )}
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** The compact rail's agent icons, above the board icons. */
export function SidebarAgentsIcons({
  teamSlug,
  agents,
  renderItem,
  separator,
}: {
  teamSlug: string
  agents: readonly AgentRow[]
  renderItem: (item: {
    key: string
    label: string
    link: { to: string; params: Record<string, string> }
    icon: React.ReactNode
  }) => React.ReactNode
  separator?: React.ReactNode
}) {
  if (agents.length === 0) return null
  return (
    <>
      {separator}
      {agents.map((agent) =>
        renderItem({
          key: agent.agentId,
          label: agent.name,
          link: {
            to: `/t/$teamSlug/agents/$agentId`,
            params: { teamSlug, agentId: agent.agentId },
          },
          icon: (
            <>
              <AgentIcon agent={agent} />
              <AgentState agent={agent} placement="icon" />
            </>
          ),
        })
      )}
    </>
  )
}
