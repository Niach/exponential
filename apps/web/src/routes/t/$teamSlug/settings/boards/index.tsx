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

// #EXP-1381 / #NEW-37: the Boards page (settings-nav.json `boards`): ONE list
// of the team's boards, a row opens that board's settings (owners), "New
// board", and the Archived / Trash bands for owners. Every member sees the live
// list (settings-nav.json `boards` is `visible: all`). The phones' Team page
// row lands here; on md+ it is the sidebar's All boards entry.
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
