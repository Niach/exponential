import { createFileRoute } from "@tanstack/react-router"
import { TeamRepositoriesSection } from "@/components/team/repositories-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(
  `/t/$teamSlug/settings/repositories`
)({
  component: SettingsRepositories,
})

function SettingsRepositories() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageRepos}
    >
      {team && (
        <TeamRepositoriesSection
          teamId={team.id}
          // The bootstrap feedback team (slug `feedback`) holds the
          // protected dogfood GitHub connection — its unlink is server-refused.
          isFeedbackTeam={team.slug === `feedback`}
        />
      )}
    </SettingsSectionGuard>
  )
}
