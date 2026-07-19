import { createFileRoute } from "@tanstack/react-router"
import { TeamBoardsSection } from "@/components/team/boards-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/boards`)({
  component: SettingsBoards,
})

function SettingsBoards() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <TeamBoardsSection team={team} />}
    </SettingsSectionGuard>
  )
}
