// EXP-456: the settings navigation as a sidebar panel. It occupies the same
// 17rem slot as the main team nav — TeamSidebar slides it in over the main
// panel while any /settings route is active — so the settings pages no longer
// carry their own desktop nav column (the in-page nav remains mobile-only).
import { Fragment, useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamBoards, useTeamBySlug } from "@/hooks/use-team-data"
import { getRuntimeConfigCached, type RuntimeConfig } from "@/lib/runtime-config"
import { conceptIcon } from "@/lib/icons.generated"
import {
  NEW_BOARD_LABEL,
  SETTINGS_BOARDS_GROUP,
  SETTINGS_NAV,
} from "@/routes/t/$teamSlug/settings/-shared"
import { BoardGlyph } from "@/components/board-glyph"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

interface SettingsSidebarProps {
  teamSlug: string
  // Computed once by TeamSidebar — this panel is mounted on every team route
  // (off-screen until a settings route activates), so running its own
  // useTeamPermissions here would duplicate the billing fetch (EXP-457).
  permissions: TeamPermissions
  onBack: () => void
}

const AddIcon = conceptIcon(`ui-add`)

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
                  {items.map((item) => (
                    <SidebarMenuItem key={item.label}>
                      <SidebarMenuButton asChild>
                        <Link to={item.to} params={{ teamSlug }}>
                          <item.icon className="h-4 w-4" />
                          <span>{item.label}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
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
