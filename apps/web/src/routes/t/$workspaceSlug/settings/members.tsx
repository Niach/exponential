import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceMembersSection } from "@/components/workspace/members-section"
import { useSettingsPage } from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/members`)({
  component: SettingsMembers,
})

function SettingsMembers() {
  const { workspaceSlug } = Route.useParams()
  const { session, workspace, members, userMap, permissions, solo } =
    useSettingsPage(workspaceSlug)

  return (
    <WorkspaceMembersSection
      members={members}
      userMap={userMap}
      currentUserId={session?.user?.id}
      canManageMembers={permissions.canManageMembers}
      workspaceId={workspace?.id}
      showInvite={permissions.canManageMembers}
      solo={solo}
    />
  )
}
