// EXP-456: the settings navigation as a sidebar panel. EXP-870: it occupies
// the 17rem panel slot beside the compact rail while any /settings route is
// active — so the settings pages no longer
// carry their own desktop nav column (the in-page nav remains mobile-only).
import { Fragment, useEffect, useState, type ReactNode } from "react"
import { Link, useRouterState } from "@tanstack/react-router"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamBoards, useTeamBySlug } from "@/hooks/use-team-data"
import { useAgents } from "@/hooks/use-session-tree"
import { useApps } from "@/hooks/use-apps"
import { useNewApp } from "@/hooks/use-new-app"
import { useSession } from "@/hooks/use-session"
import { AGENT_SETTINGS_TEST_IDS } from "@/components/team/agents-section"
import { getRuntimeConfigCached, type RuntimeConfig } from "@/lib/runtime-config"
import {
  conceptIcon,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  BoardGlyph,
} from "@exp/ui"
import {
  ASSISTANT_LABEL,
  NEW_AGENT_LABEL,
  NEW_BOARD_LABEL,
  SETTINGS_AGENTS_GROUP,
  SETTINGS_APPS_GROUP,
  NEW_APP_LABEL,
  SETTINGS_NAV,
  settingsNavListVisible,
  type SettingsNavContext,
  type SettingsNavItem,
} from "@/routes/t/$teamSlug/settings/-shared"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"
import { AssistantMark } from "@/components/team/assistant-mark"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"

interface SettingsSidebarProps {
  teamSlug: string
  // Computed once by TeamSidebar — this panel is mounted on every team route
  // (off-screen until a settings route activates), so running its own
  // useTeamPermissions here would duplicate the billing fetch (EXP-457).
  permissions: TeamPermissions
  onBack: () => void
}

const AddIcon = conceptIcon(`ui-add`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

// The trailing path of a nav entry (`/settings/labels`) — what the current
// location ends with while that page is open.
function sectionPath(item: SettingsNavItem): string {
  return item.to.replace(`/t/$teamSlug`, ``)
}

// EXP-630: an entry with sub-pages (Issues → Labels, Statuses). The row
// itself navigates to the parent page; the chevron folds the sub-pages,
// which start unfolded while the parent or one of them is open.
function NavItemWithChildren({
  item,
  teamSlug,
  permissions,
  navContext,
}: {
  item: SettingsNavItem
  teamSlug: string
  permissions: TeamPermissions
  navContext: SettingsNavContext
}) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const children = (item.children ?? []).filter((child) =>
    child.visible(permissions, navContext)
  )
  const within = [item, ...children].some((entry) =>
    pathname.endsWith(sectionPath(entry))
  )
  const [toggled, setToggled] = useState<boolean | null>(null)
  const open = toggled ?? within
  const Chevron = open ? ChevronDownIcon : ChevronRightIcon
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <Link to={item.to} params={{ teamSlug }}>
          <item.icon className="h-4 w-4" />
          <span>{item.label}</span>
        </Link>
      </SidebarMenuButton>
      {children.length > 0 && (
        <SidebarMenuAction
          aria-label={open ? `Collapse ${item.label}` : `Expand ${item.label}`}
          aria-expanded={open}
          onClick={() => setToggled(!open)}
        >
          <Chevron />
        </SidebarMenuAction>
      )}
      {open && children.length > 0 && (
        <SidebarMenuSub>
          {children.map((child) => (
            <SidebarMenuSubItem key={child.id}>
              <SidebarMenuSubButton asChild>
                <Link to={child.to} params={{ teamSlug }}>
                  <child.icon className="h-4 w-4" />
                  <span>{child.label}</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          ))}
        </SidebarMenuSub>
      )}
    </SidebarMenuItem>
  )
}

export function SettingsSidebar({
  teamSlug,
  permissions,
  onBack,
}: SettingsSidebarProps) {
  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  useEffect(() => {
    void getRuntimeConfigCached().then(setConfig)
  }, [])
  const navContext = { isCloud: Boolean(config?.isCloud) }

  // EXP-862: the Boards group lists every board as its own entry (the desktop
  // IDE's EXP-288 nav), so this panel needs the live board list. Owner-gated
  // like the board pages behind it.
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  const [createOpen, setCreateOpen] = useState(false)
  // EXP-1293: the Agents group lists the team's agents the same way. Every
  // member sees them (their pages read-only); "New agent" is owner-only.
  // S-3: `useAgents` drops other members' Assistants and puts the viewer's
  // first; the Assistant is its own `@assistant` row (created on first use
  // while a settings page is open).
  const { data: authSession } = useSession()
  const { agents } = useAgents(team?.id, authSession?.user?.id)
  const teamAgents = agents.filter((agent) => !agent.assistant)
  const { apps } = useApps(team?.id)
  const newApp = useNewApp(team?.id, teamSlug)
  const onSettings = useRouterState({ select: (state) => state.location.pathname.includes(`/settings`) })
  useEnsureAssistant(onSettings ? team?.id : null)

  const listRows = (list: string): ReactNode => {
    if (!settingsNavListVisible(list, permissions)) return null
    switch (list) {
      case `@boards`:
        if (!permissions.isOwner) return null
        return boards.map((board) => (
          <SidebarMenuItem key={board.id}>
            <SidebarMenuButton asChild>
              <Link to="/t/$teamSlug/settings/boards/$boardId" params={{ teamSlug, boardId: board.id }}>
                <BoardGlyph board={board} />
                <span>{board.name}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))
      case `@new-board`:
        return team ? (
          <SidebarMenuItem key={list}>
            <SidebarMenuButton className="text-muted-foreground" onClick={() => setCreateOpen(true)}>
              <AddIcon className="h-4 w-4" />
              <span>{NEW_BOARD_LABEL}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ) : null
      case `@assistant`:
        return (
          <SidebarMenuItem key={list}>
            <SidebarMenuButton asChild>
              <Link to="/t/$teamSlug/settings/assistant" params={{ teamSlug }} data-testid="settings-assistant">
                <AssistantMark />
                <span>{ASSISTANT_LABEL}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      case `@agents`:
        return teamAgents.map((agent) => (
          <SidebarMenuItem key={agent.agentId}>
            <SidebarMenuButton asChild>
              <Link to="/t/$teamSlug/settings/agents/$agentId" params={{ teamSlug, agentId: agent.agentId }}>
                <BoardGlyph board={{ icon: agent.icon, color: agent.color }} />
                <span>{agent.name}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))
      case `@new-agent`:
        return (
          <SidebarMenuItem key={list}>
            <SidebarMenuButton asChild className="text-muted-foreground">
              <Link to="/t/$teamSlug/settings/agents/new" params={{ teamSlug }}>
                <AddIcon className="h-4 w-4" />
                <span>{NEW_AGENT_LABEL}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      // VAPP-110: the team's apps + "New app". VAPP-177: an app opens its
      // editor on the Settings tab.
      case `@apps`:
        if (!permissions.isMember) return null
        return apps.map((app) => (
          <SidebarMenuItem key={app.id}>
            <SidebarMenuButton asChild>
              <Link to="/t/$teamSlug/apps/$appSlug" params={{ teamSlug, appSlug: app.slug }} search={{ tab: `settings` }}>
                <BoardGlyph board={{ icon: app.icon, color: app.color }} />
                <span>{app.name}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))
      case `@new-app`:
        return (
          <SidebarMenuItem key={list}>
            {/* VAPP-176: a draft and its editor, no create page. */}
            <SidebarMenuButton className="text-muted-foreground" onClick={() => void newApp.create()} disabled={newApp.pending}>
              <AddIcon className="h-4 w-4" />
              <span>{NEW_APP_LABEL}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      default:
        return null
    }
  }

  return (
    <>
      <SidebarBackRow label="Settings" onBack={onBack} />

      <SidebarContent>
        {SETTINGS_NAV.map((group) => {
          // S-3: the entries in settings-nav.json order, injected lists in
          // place (Boards: rows, New board, Archived boards; Agents: the
          // Assistant, the team agents, New agent, MCP servers).
          const rows = group.entries.map((entry) => {
            if (entry.kind === `list`) {
              const drawn = listRows(entry.list)
              return Array.isArray(drawn) && drawn.length === 0 ? null : drawn
            }
            const item = entry.item
            if (!item.visible(permissions, navContext)) return null
            return item.children ? (
              <NavItemWithChildren
                key={item.id}
                item={item}
                teamSlug={teamSlug}
                permissions={permissions}
                navContext={navContext}
              />
            ) : (
              <SidebarMenuItem key={item.id}>
                <SidebarMenuButton asChild>
                  <Link to={item.to} params={{ teamSlug }}>
                    <item.icon className="h-4 w-4" />
                    <span>{item.label}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            )
          })
          if (rows.every((row) => row === null)) return null
          const isAgentsGroup = group.group === SETTINGS_AGENTS_GROUP
          const isAppsGroup = group.group === SETTINGS_APPS_GROUP
          return (
            <SidebarGroup
              key={group.id}
              data-testid={isAgentsGroup ? AGENT_SETTINGS_TEST_IDS.list : undefined}
            >
              {isAgentsGroup ? (
                // NEW-2: the label opens the Agents page (the list + Events).
                <SidebarGroupLabel asChild className="hover:text-sidebar-foreground">
                  <Link
                    to="/t/$teamSlug/settings/agents"
                    params={{ teamSlug }}
                    activeOptions={{ exact: true }}
                    activeProps={{ className: `text-sidebar-foreground` }}
                    data-testid="settings-agents-index"
                  >
                    {group.group}
                  </Link>
                </SidebarGroupLabel>
              ) : isAppsGroup ? (
                <SidebarGroupLabel asChild className="hover:text-sidebar-foreground">
                  <Link
                    to="/t/$teamSlug/settings/apps"
                    params={{ teamSlug }}
                    activeOptions={{ exact: true }}
                    activeProps={{ className: `text-sidebar-foreground` }}
                    data-testid="settings-apps-index"
                  >
                    {group.group}
                  </Link>
                </SidebarGroupLabel>
              ) : (
                <SidebarGroupLabel>{group.group}</SidebarGroupLabel>
              )}
              <SidebarGroupContent>
                <SidebarMenu>
                  {rows.map((row, index) => (
                    <Fragment key={index}>{row}</Fragment>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )
        })}
      </SidebarContent>

      {team && (
        <CreateBoardDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          team={team}
        />
      )}
    </>
  )
}
