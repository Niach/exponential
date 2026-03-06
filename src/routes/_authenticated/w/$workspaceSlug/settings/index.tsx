import { createFileRoute } from "@tanstack/react-router"
import { authClient } from "@/lib/auth-client"
import { useWorkspaceBySlug, useWorkspaceUsers } from "@/hooks/use-workspace-data"
import { WorkspaceInviteSection } from "@/components/workspace-invite-section"
import { WorkspaceMembersSection } from "@/components/workspace-members-section"
import { Separator } from "@/components/ui/separator"

export const Route = createFileRoute(
  `/_authenticated/w/$workspaceSlug/settings/`
)({
  component: WorkspaceSettings,
})

function WorkspaceSettings() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = authClient.useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { members, userMap } = useWorkspaceUsers(workspace?.id)

  const currentMember = members.find((member) => member.userId === session?.user?.id)
  const isOwner = currentMember?.role === `owner`

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <h1 className="text-2xl font-bold">Workspace Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage members and invites for {workspace?.name}
        </p>
      </div>

      <Separator />

      {workspace && isOwner && <WorkspaceInviteSection workspaceId={workspace.id} />}

      <WorkspaceMembersSection
        members={members}
        userMap={userMap}
        currentUserId={session?.user?.id}
        isOwner={isOwner}
      />
    </div>
  )
}
