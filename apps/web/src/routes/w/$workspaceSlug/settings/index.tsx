import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import {
  useShowWorkspaceChrome,
  useWorkspaceBySlug,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import { WorkspaceGeneralSection } from "@/components/workspace/general-section"
import { WorkspaceLabelsSection } from "@/components/workspace/labels-section"
import { WorkspaceMembersSection } from "@/components/workspace/members-section"
import { WorkspaceAgentsSection } from "@/components/workspace/agents-section"
import { WorkspaceProjectsSection } from "@/components/workspace/projects-section"
import { WorkspaceBillingSection } from "@/components/workspace/billing-section"
import { Separator } from "@/components/ui/separator"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { trpc } from "@/lib/trpc-client"
import {
  getRuntimeConfig,
  type RuntimeConfig,
} from "@/lib/runtime-config"

export const Route = createFileRoute(`/w/$workspaceSlug/settings/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
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
  const { data: session } = useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { members, userMap } = useWorkspaceUsers(workspace?.id)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  const navigate = useNavigate()

  const [showDeleteWorkspace, setShowDeleteWorkspace] = useState(false)
  const [deleteConfirmation, setDeleteConfirmation] = useState(``)
  const [deletingWorkspace, setDeletingWorkspace] = useState(false)

  useEffect(() => {
    void getRuntimeConfig().then(setConfig)
  }, [])

  const currentMember = members.find(
    (member) => member.userId === session?.user?.id
  )
  const isOwner = currentMember?.role === `owner`
  const showChrome = useShowWorkspaceChrome(workspace?.id, session?.user?.id)
  const solo = !showChrome

  const handleDeleteWorkspace = async () => {
    if (!workspace || deleteConfirmation !== workspace.name) return
    setDeletingWorkspace(true)
    try {
      await trpc.workspaces.delete.mutate({ workspaceId: workspace.id })
      void navigate({ to: `/` })
    } catch {
      setDeletingWorkspace(false)
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <h1 className="text-2xl font-bold">
          {solo ? `Settings` : `Workspace Settings`}
        </h1>
        <p className="text-sm text-muted-foreground">
          {solo
            ? `Manage your projects, labels, and billing.`
            : `Manage members, invites, and labels for ${workspace?.name ?? ``}`}
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
        <WorkspaceGeneralSection workspace={workspace} solo={solo} />
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
        workspaceId={workspace?.id}
        showInvite={isOwner}
        solo={solo}
      />

      <Separator />

      {workspace && <WorkspaceLabelsSection workspaceId={workspace.id} />}

      {workspace && isOwner && !workspace.isPublic && !solo && (
        <>
          <Separator />
          <Card className="border-destructive/50">
            <CardHeader>
              <CardTitle className="text-base text-destructive">
                Danger Zone
              </CardTitle>
              <CardDescription>
                Permanently delete this workspace and all its data.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Button
                variant="destructive"
                onClick={() => setShowDeleteWorkspace(true)}
              >
                Delete workspace
              </Button>
            </CardContent>
          </Card>

          <Dialog
            open={showDeleteWorkspace}
            onOpenChange={(open) => {
              if (!open) {
                setShowDeleteWorkspace(false)
                setDeleteConfirmation(``)
              }
            }}
          >
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Delete workspace</DialogTitle>
                <DialogDescription>
                  This will permanently delete{` `}
                  <span className="font-semibold text-foreground">
                    {workspace.name}
                  </span>
                  {` `}
                  and all its projects, issues, and data. This cannot be undone.
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-2 py-2">
                <Label htmlFor="delete-confirm">
                  Type{` `}
                  <span className="font-semibold">{workspace.name}</span>
                  {` `}to confirm
                </Label>
                <Input
                  id="delete-confirm"
                  value={deleteConfirmation}
                  onChange={(e) => setDeleteConfirmation(e.target.value)}
                  placeholder={workspace.name}
                />
              </div>
              <DialogFooter>
                <Button
                  variant="outline"
                  onClick={() => {
                    setShowDeleteWorkspace(false)
                    setDeleteConfirmation(``)
                  }}
                  disabled={deletingWorkspace}
                >
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  onClick={handleDeleteWorkspace}
                  disabled={
                    deleteConfirmation !== workspace.name || deletingWorkspace
                  }
                >
                  {deletingWorkspace
                    ? `Deleting...`
                    : `Delete workspace`}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </>
      )}
    </div>
  )
}
