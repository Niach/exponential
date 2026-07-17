import { useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import { LifeBuoy } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { getProjectIconName } from "@/lib/project-types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  OWNER_ONLY_PUBLIC_HINT,
  ProjectIconColorFields,
  ProjectNameField,
  ProjectPublicSection,
} from "@/components/project-form-fields"
import {
  buildPublicBoardUrl,
  PublicBoardLinkRow,
} from "@/components/workspace/public-board-share"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import type { Project, Workspace } from "@/db/schema"

const PROTECTED_VISIBILITY_HINT = `This project is protected — its visibility can't be changed.`
const PROTECTED_REPO_HINT = `This project is protected — its repository can't be changed.`

// Consolidated per-project settings (EXP-159): everything the create dialog
// offers, editable after creation — name, icon, color, repository, publicness
// (+ the public-visitor toggles), helpdesk. Receives the LIVE Electric row so
// every toggle write reflects via sync; a concurrently-trashed project closes
// the dialog (project becomes null).
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
  const { isOwner, canManageRepos } = useWorkspacePermissions(workspace)

  // Name is the one deferred write (save on blur / close) — swapping it live
  // under the user's caret would fight typing. Everything else mutates
  // immediately off the live row.
  const [name, setName] = useState(``)
  const [busyPublic, setBusyPublic] = useState(false)
  const [busyHelpdesk, setBusyHelpdesk] = useState(false)
  const [busyRepo, setBusyRepo] = useState(false)
  const [helpdeskError, setHelpdeskError] = useState<string | null>(null)
  const [repoError, setRepoError] = useState<string | null>(null)

  useEffect(() => {
    setName(project?.name ?? ``)
    setBusyPublic(false)
    setBusyHelpdesk(false)
    setBusyRepo(false)
    setHelpdeskError(null)
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

  const update = async (
    updates: Partial<{
      isPublic: boolean
      publicShowComments: boolean
      publicShowActivity: boolean
    }>
  ) => {
    if (!project) return
    setBusyPublic(true)
    try {
      await trpc.projects.update.mutate({ id: project.id, ...updates })
    } finally {
      setBusyPublic(false)
    }
  }

  const toggleHelpdesk = async (enabled: boolean) => {
    if (!project) return
    setBusyHelpdesk(true)
    setHelpdeskError(null)
    try {
      await trpc.projects.update.mutate({
        id: project.id,
        helpdeskEnabled: enabled,
      })
    } catch (err) {
      setHelpdeskError(
        isPlanLimitError(err)
          ? `The helpdesk is available on Pro and Business plans.`
          : `Could not update the helpdesk setting.`
      )
    } finally {
      setBusyHelpdesk(false)
    }
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

  const publicHint = project?.isProtected
    ? PROTECTED_VISIBILITY_HINT
    : !isOwner
      ? OWNER_ONLY_PUBLIC_HINT
      : undefined

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

            <ProjectPublicSection
              checked={project.isPublic}
              onCheckedChange={(checked) => void update({ isPublic: checked })}
              disabled={busyPublic || !isOwner || project.isProtected}
              hint={publicHint}
              showWarning={project.isPublic}
            />

            {project.isPublic && (
              <>
                <PublicBoardLinkRow
                  url={buildPublicBoardUrl(workspace.slug, project.slug)}
                />
                {isOwner && (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <Label className="text-sm">Show comments</Label>
                        <p className="text-xs text-muted-foreground">
                          Visitors see issue discussions (authors stay
                          anonymized).
                        </p>
                      </div>
                      <Switch
                        checked={project.publicShowComments}
                        disabled={busyPublic}
                        onCheckedChange={(checked) =>
                          void update({ publicShowComments: checked })
                        }
                      />
                    </div>
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <Label className="text-sm">Show activity</Label>
                        <p className="text-xs text-muted-foreground">
                          Status and label changes appear on public issues.
                        </p>
                      </div>
                      <Switch
                        checked={project.publicShowActivity}
                        disabled={busyPublic}
                        onCheckedChange={(checked) =>
                          void update({ publicShowActivity: checked })
                        }
                      />
                    </div>
                  </div>
                )}
              </>
            )}

            {isOwner && (
              <div className="space-y-2 rounded-md border border-border px-3 py-2">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <Label className="flex items-center gap-1.5 text-sm">
                      <LifeBuoy className="h-3.5 w-3.5 text-muted-foreground" />
                      Helpdesk
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Widget support conversations land in the team&apos;s
                      Support inbox. Pro and Business plans.
                    </p>
                  </div>
                  <Switch
                    checked={project.helpdeskEnabled}
                    disabled={busyHelpdesk}
                    onCheckedChange={(checked) => void toggleHelpdesk(checked)}
                  />
                </div>
                {helpdeskError && (
                  <p className="text-xs text-destructive">{helpdeskError}</p>
                )}
                {project.helpdeskEnabled && (
                  <Button variant="outline" size="sm" asChild className="w-fit">
                    <Link
                      to="/t/$workspaceSlug/support"
                      params={{ workspaceSlug: workspace.slug }}
                    >
                      Open Support inbox
                    </Link>
                  </Button>
                )}
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
