import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { getProjectIconName } from "@/lib/project-types"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  ProjectIconColorFields,
  ProjectNameField,
} from "@/components/project-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import type { Project, Workspace } from "@/db/schema"

const PROTECTED_REPO_HINT = `This project is protected — its repository can't be changed.`

// Consolidated per-project settings (EXP-159): everything the create dialog
// offers, editable after creation — name, icon, color, repository. Receives
// the LIVE Electric row so every write reflects via sync; a concurrently-
// trashed project closes the dialog (project becomes null).
export function ProjectSettingsDialog({
  project,
  workspace,
  onOpenChange,
  onRepoChanged,
}: {
  project: Project | null
  workspace: Workspace
  onOpenChange: (open: boolean) => void
  onRepoChanged: () => void
}) {
  const { canManageRepos } = useWorkspacePermissions(workspace)

  // Name is the one deferred write (save on blur / close) — swapping it live
  // under the user's caret would fight typing. Everything else mutates
  // immediately off the live row.
  const [name, setName] = useState(``)
  const [busyRepo, setBusyRepo] = useState(false)
  const [repoError, setRepoError] = useState<string | null>(null)

  useEffect(() => {
    setName(project?.name ?? ``)
    setBusyRepo(false)
    setRepoError(null)
    // Reset keyed on the target project only — remote edits while the dialog
    // is open deliberately don't stomp a local in-progress rename.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.id])

  const saveName = (target: Project) => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === target.name) return
    void trpc.projects.update.mutate({ id: target.id, name: trimmed })
  }

  const applyRepo = async (repositoryId: string) => {
    if (!project) return
    setBusyRepo(true)
    setRepoError(null)
    try {
      await trpc.projects.setRepository.mutate(
        { projectId: project.id, repositoryId },
        { context: { skipErrorToast: true } }
      )
      onRepoChanged()
    } catch (err) {
      setRepoError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyRepo(false)
    }
  }

  // A brand-new repo: register it (idempotent upsert/un-archive) then point
  // the project at the returned repository id.
  const handleConnect = async (picked: PickerRepo) => {
    if (!project) return
    setBusyRepo(true)
    setRepoError(null)
    try {
      const { repository } = await trpc.repositories.add.mutate(
        {
          workspaceId: workspace.id,
          fullName: picked.fullName,
          defaultBranch: picked.defaultBranch,
          private: picked.private,
        },
        { context: { skipErrorToast: true } }
      )
      if (repository) {
        await applyRepo(repository.id)
        return
      }
      setRepoError(`Could not connect ${picked.fullName}.`)
    } catch (err) {
      setRepoError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyRepo(false)
    }
  }

  return (
    <Dialog
      open={project !== null}
      onOpenChange={(open) => {
        // Blur doesn't reliably fire on unmount — flush a pending rename
        // before the dialog goes away.
        if (!open && project) saveName(project)
        onOpenChange(open)
      }}
    >
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Project settings</DialogTitle>
          <DialogDescription>
            Same settings as project creation — changes apply to{` `}
            <span className="font-medium text-foreground">
              {project?.name}
            </span>
            {` `}immediately.
          </DialogDescription>
        </DialogHeader>

        {project && (
          <div className="space-y-4">
            <ProjectNameField
              value={name}
              onChange={setName}
              onBlur={() => saveName(project)}
            />

            <div className="space-y-2">
              <Label>Prefix</Label>
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="font-mono text-xs">
                  {project.prefix}
                </Badge>
                <p className="text-xs text-muted-foreground">
                  The prefix can&apos;t be changed after creation.
                </p>
              </div>
            </div>

            <ProjectIconColorFields
              icon={getProjectIconName(project)}
              onIconChange={(icon) =>
                void trpc.projects.update.mutate({ id: project.id, icon })
              }
              color={project.color}
              onColorChange={(color) =>
                void trpc.projects.update.mutate({ id: project.id, color })
              }
            />

            {canManageRepos && (
              <div className="space-y-2">
                <Label>Repository</Label>
                {project.isProtected ? (
                  <p className="rounded-md border px-3 py-2 text-xs text-muted-foreground">
                    {PROTECTED_REPO_HINT}
                  </p>
                ) : (
                  <>
                    <ConnectedRepoPicker
                      workspaceId={workspace.id}
                      value={project.repositoryId}
                      disabled={busyRepo}
                      onSelectRegistry={(repo) => void applyRepo(repo.id)}
                      onConnectNew={(picked) => void handleConnect(picked)}
                    />
                    <p className="text-xs text-muted-foreground">
                      New &ldquo;Start coding&rdquo; launches use the selected
                      repo; existing worktrees keep working locally.
                    </p>
                  </>
                )}
                {repoError && (
                  <p className="text-xs text-destructive">{repoError}</p>
                )}
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
