import { useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { WorkspaceGeneralSection } from "@/components/workspace/general-section"
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
import { useWorkspaceMemberships } from "@/hooks/use-workspace-data"
import { trpc } from "@/lib/trpc-client"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/general`)({
  component: SettingsGeneral,
})

function SettingsGeneral() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const { session, workspace, permissions, solo, resolved } =
    useSettingsPage(workspaceSlug)

  const [showDeleteWorkspace, setShowDeleteWorkspace] = useState(false)
  const [deleteConfirmation, setDeleteConfirmation] = useState(``)
  const [deletingWorkspace, setDeletingWorkspace] = useState(false)

  const { myWorkspaces } = useWorkspaceMemberships(session?.user?.id)
  // Deleting your LAST personal workspace is server-refused (EXP-82) — the
  // bootstrap feedback workspace (slug `feedback`) never counts as one.
  // Empty-while-loading biases to disabled, the safe default.
  const isOnlyWorkspace =
    myWorkspaces.filter((w) => w.slug !== `feedback`).length <= 1

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
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageWorkspace}
    >
      <div className="space-y-6">
        {workspace && (
          <WorkspaceGeneralSection workspace={workspace} solo={solo} />
        )}

        {workspace && !solo && (
          <>
            <Card className="border-destructive/50">
              <CardHeader>
                <CardTitle className="text-base text-destructive">
                  Danger Zone
                </CardTitle>
                <CardDescription>
                  Permanently delete this team and all its data.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                <Button
                  variant="destructive"
                  disabled={isOnlyWorkspace}
                  onClick={() => setShowDeleteWorkspace(true)}
                >
                  Delete team
                </Button>
                {isOnlyWorkspace && (
                  <p className="text-sm text-muted-foreground">
                    This is your only workspace, so it can't be deleted.
                  </p>
                )}
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
                  <DialogTitle>Delete team</DialogTitle>
                  <DialogDescription>
                    This will permanently delete{` `}
                    <span className="font-semibold text-foreground">
                      {workspace.name}
                    </span>
                    {` `}
                    and all its projects, issues, and data. This cannot be
                    undone.
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
                      deleteConfirmation !== workspace.name ||
                      deletingWorkspace
                    }
                  >
                    {deletingWorkspace ? `Deleting...` : `Delete team`}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </>
        )}
      </div>
    </SettingsSectionGuard>
  )
}
