import { createFileRoute } from "@tanstack/react-router"
import { useTeamBoardsWithReady } from "@/hooks/use-team-data"
import { BoardsListBand } from "@/components/team/boards-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/boards/`)({
  head: () => ({ meta: [{ title: pageTitle(`Boards`, `Settings`) }] }),
  component: SettingsBoardsIndex,
})

// #EXP-1381: the Boards page (settings-nav.json `boards`): the team's boards,
// a row opens that board's settings (owners), "New board" for owners.
// Every member sees the list (settings-nav.json `boards` is `visible: all`). The phones'
// Team page row lands here; on md+ the settings sidebar lists the boards too.
function SettingsBoardsIndex() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)

  return (
    <SettingsSectionGuard resolved={resolved} allowed>
      {team && boardsReady && (
        <BoardsListBand team={team} boards={boards} canManage={permissions.isOwner} />
      )}
    </SettingsSectionGuard>
  )
}
