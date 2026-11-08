import type * as React from "react"
import { Link } from "@tanstack/react-router"
import {
  AGENT_ROW_MENU_KIND,
  ClaudeSpinner,
  conceptIcon,
  ExponentialLogo,
  getActionIcon,
  menuProps,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuDot,
  SidebarMenuItem,
  SidebarMenuTrailing,
} from "@exp/ui"
import { CHATS_LIST_COPY, type AgentRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// EXP-1293: the rail's AGENTS section, ABOVE Boards and drawn like them —
// one row per team agent (`listAgents`, fixture-locked: sortOrder, then
// name): its icon in its color + its name, a spinner while it works, an
// yellow dot while it needs you. A row opens the agent's channel; owners get
// the gear to its settings, and the "+" files a new agent.

const UiAddIcon = conceptIcon(`ui-add`)
const NavSettingsIcon = conceptIcon(`nav-settings`)

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

/** The trailing state: the spinner while busy, else yellow when it needs you,
 *  in the row's trailing column (EXP-1477). */
function AgentState({
  agent,
  yieldsOnHover = false,
}: {
  agent: Pick<AgentRow, `busy` | `needsYou`>
  /** The owner's hover gear takes the same column. */
  yieldsOnHover?: boolean
}) {
  let mark: React.ReactNode = null
  if (agent.needsYou) {
    mark = (
      <SidebarMenuDot
        aria-hidden
        tone="needsYou"
        data-testid="agent-needs-you"
        className="pointer-events-none"
      />
    )
  } else if (agent.busy) {
    mark = <ClaudeSpinner className="size-3.5" />
  }
  if (!mark) return null
  return (
    <SidebarMenuTrailing
      aria-hidden
      className={cn(`pointer-events-none`, yieldsOnHover && `group-hover/menu-item:opacity-0`)}
    >
      {mark}
    </SidebarMenuTrailing>
  )
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
                  // Chat 58cb32df: THE agent menu on right-click / long-press.
                  {...menuProps(AGENT_ROW_MENU_KIND, agent.agentId)}
                >
                  <AgentIcon agent={agent} />
                  <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                </Link>
              </SidebarMenuButton>
              <AgentState agent={agent} yieldsOnHover={isOwner} />
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

