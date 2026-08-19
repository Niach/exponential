import { createFileRoute } from "@tanstack/react-router"
import { TeamRepositoriesSection } from "@/components/team/repositories-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(
  `/t/$teamSlug/settings/repositories`
)({
  component: SettingsRepositories,
})

// Member-visible since EXP-557 (per-user sharing): every member manages their
// own GitHub connection and the repos they shared here. Row-level rights
// (remove, branch pin, stale-account disconnect) are gated inside the section.
function SettingsRepositories() {
  const { teamSlug } = Route.useParams()
  const { session, team, permissions } = useSettingsPage(teamSlug)

  return (
    <>
      {team && (
        <TeamRepositoriesSection
          teamId={team.id}
          currentUserId={session?.user?.id}
          isOwner={permissions.isOwner}
        />
      )}
    </>
  )
}
