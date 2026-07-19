import { createFileRoute } from "@tanstack/react-router"
import { TeamMembersSection } from "@/components/team/members-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/members`)({
  component: SettingsMembers,
})

function SettingsMembers() {
  const { teamSlug } = Route.useParams()
  const { session, team, members, userMap, permissions, solo } =
    useSettingsPage(teamSlug)

  return (
    <TeamMembersSection
      members={members}
      userMap={userMap}
      currentUserId={session?.user?.id}
      canManageMembers={permissions.canManageMembers}
      teamId={team?.id}
      showInvite={permissions.canManageMembers}
      solo={solo}
    />
  )
}
