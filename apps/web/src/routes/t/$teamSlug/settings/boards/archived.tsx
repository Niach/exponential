import { createFileRoute } from "@tanstack/react-router"
import { BoardsTrashPage } from "@/components/team/boards-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(
  `/t/$teamSlug/settings/boards/archived`
)({
  head: () => ({
    meta: [{ title: pageTitle(`Archived boards`, `Settings`) }],
  }),
  component: SettingsArchivedBoards,
})

// EXP-862: archived boards and the 48h trash, the two board states that are
// deliberately out of every synced shape (EXP-500).
function SettingsArchivedBoards() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <BoardsTrashPage teamId={team.id} />}
    </SettingsSectionGuard>
  )
}
