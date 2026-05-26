import { useEffect, useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"
import {
  useWorkspaceBySlug,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import { WorkspaceGeneralSection } from "@/components/workspace/general-section"
import { WorkspaceInviteSection } from "@/components/workspace/invite-section"
import { WorkspaceLabelsSection } from "@/components/workspace/labels-section"
import { WorkspaceMembersSection } from "@/components/workspace/members-section"
import { WorkspaceAgentsSection } from "@/components/workspace/agents-section"
import { WorkspaceProjectsSection } from "@/components/workspace/projects-section"
import { WorkspaceBillingSection } from "@/components/workspace/billing-section"
import { Separator } from "@/components/ui/separator"
import {
  getRuntimeConfig,
  type RuntimeConfig,
} from "@/lib/runtime-config"

export const Route = createFileRoute(`/w/$workspaceSlug/settings/`)({
  beforeLoad: async () => {
    const result = await authClient.getSession()
    if (!result.data?.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: WorkspaceSettings,
})

function WorkspaceSettings() {
  const { workspaceSlug } = Route.useParams()
  const { data: session } = authClient.useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { members, userMap } = useWorkspaceUsers(workspace?.id)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then(setConfig)
  }, [])

  const currentMember = members.find(
    (member) => member.userId === session?.user?.id
  )
  const isOwner = currentMember?.role === `owner`

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <h1 className="text-2xl font-bold">Workspace Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage members, invites, and labels for {workspace?.name}
        </p>
      </div>

      <Separator />

      {workspace && isOwner && config?.isCloud && (
        <WorkspaceBillingSection
          workspaceId={workspace.id}
          proProductId={config.creemProProductId}
          businessProductId={config.creemBusinessProductId}
        />
      )}

      {workspace && isOwner && (
        <WorkspaceGeneralSection workspace={workspace} />
      )}

      {workspace && isOwner && (
        <WorkspaceInviteSection workspaceId={workspace.id} />
      )}

      {workspace && isOwner && (
        <WorkspaceAgentsSection workspaceId={workspace.id} />
      )}

      {workspace && isOwner && (
        <WorkspaceProjectsSection workspaceId={workspace.id} />
      )}

      <WorkspaceMembersSection
        members={members.filter((member) => member.role !== `agent`)}
        userMap={userMap}
        currentUserId={session?.user?.id}
        isOwner={isOwner}
      />

      <Separator />

      {workspace && <WorkspaceLabelsSection workspaceId={workspace.id} />}
    </div>
  )
}
