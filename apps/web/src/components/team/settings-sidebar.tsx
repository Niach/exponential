// EXP-456: the settings navigation as a sidebar panel. EXP-870: it occupies
// the 17rem panel slot beside the compact rail while any /settings route is
// active — so the settings pages no longer
// carry their own desktop nav column (the in-page nav remains mobile-only).
import { Fragment, useEffect, useState } from "react"
import { Link, useRouterState } from "@tanstack/react-router"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamBoards, useTeamBySlug } from "@/hooks/use-team-data"
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
  NEW_BOARD_LABEL,
  SETTINGS_BOARDS_GROUP,
  SETTINGS_NAV,
  type SettingsNavContext,
  type SettingsNavItem,
} from "@/routes/t/$teamSlug/settings/-shared"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

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
            <SidebarMenuSubItem key={child.label}>
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
  // like the pages behind it.
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  const showBoards = permissions.isOwner
  const [createOpen, setCreateOpen] = useState(false)

  return (
    <>
      <SidebarBackRow label="Settings" onBack={onBack} />

      <SidebarContent>
        {SETTINGS_NAV.map((group) => {
          const items = group.items.filter((item) =>
            item.visible(permissions, navContext)
          )
          const withBoards =
            group.group === SETTINGS_BOARDS_GROUP && showBoards
          if (items.length === 0 && !withBoards) return null
          return (
            <SidebarGroup key={group.group}>
              <SidebarGroupLabel>{group.group}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {withBoards && (
                    <Fragment>
                      {boards.map((board) => (
                        <SidebarMenuItem key={board.id}>
                          <SidebarMenuButton asChild>
                            <Link
                              to="/t/$teamSlug/settings/boards/$boardId"
                              params={{ teamSlug, boardId: board.id }}
                            >
                              <BoardGlyph board={board} />
                              <span>{board.name}</span>
                            </Link>
                          </SidebarMenuButton>
                        </SidebarMenuItem>
                      ))}
                      {team && (
                        <SidebarMenuItem>
                          <SidebarMenuButton
                            className="text-muted-foreground"
                            onClick={() => setCreateOpen(true)}
                          >
                            <AddIcon className="h-4 w-4" />
                            <span>{NEW_BOARD_LABEL}</span>
                          </SidebarMenuButton>
                        </SidebarMenuItem>
                      )}
                    </Fragment>
                  )}
                  {items.map((item) =>
                    item.children ? (
                      <NavItemWithChildren
                        key={item.label}
                        item={item}
                        teamSlug={teamSlug}
                        permissions={permissions}
                        navContext={navContext}
                      />
                    ) : (
                      <SidebarMenuItem key={item.label}>
                        <SidebarMenuButton asChild>
                          <Link to={item.to} params={{ teamSlug }}>
                            <item.icon className="h-4 w-4" />
                            <span>{item.label}</span>
                          </Link>
                        </SidebarMenuButton>
                      </SidebarMenuItem>
                    )
                  )}
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
