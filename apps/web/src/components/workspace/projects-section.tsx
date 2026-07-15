import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, GitBranch, Globe, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getProjectIcon } from "@/lib/project-types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
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
import {
  buildPublicBoardUrl,
  PublicBoardLinkRow,
} from "@/components/workspace/public-board-share"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import type { Project } from "@/db/schema"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>

export function WorkspaceProjectsSection({
  workspaceId,
  workspaceSlug,
}: {
  workspaceId: string
  workspaceSlug: string
}) {
  const projects = useWorkspaceProjects(workspaceId)
  const visibleProjects = projects.filter((p) => !p.archivedAt)

  // The workspace's connected repos — used to render each project's repo chip
  // (uuid → owner/name) and to feed the "Change repository…" dialog.
  const [repos, setRepos] = useState<RepoList | null>(null)
  const refreshRepos = useCallback(async () => {
    try {
      setRepos(await trpc.repositories.list.query({ workspaceId }))
    } catch {
      // The chips degrade to "No repository" if the list can't load.
    }
  }, [workspaceId])
  useEffect(() => {
    void refreshRepos()
  }, [refreshRepos])

  const repoMap = useMemo(
    () => new Map((repos ?? []).map((r) => [r.id, r])),
    [repos]
  )

  const [deleteTarget, setDeleteTarget] = useState<{
    id: string
    name: string
  } | null>(null)
  const [deleting, setDeleting] = useState(false)
  // Bumped on delete so the trash card refetches (restored projects re-appear
  // in the synced list on their own via Electric).
  const [trashRefreshKey, setTrashRefreshKey] = useState(0)
  const [repoTarget, setRepoTarget] = useState<Project | null>(null)
  const [publicTargetId, setPublicTargetId] = useState<string | null>(null)
  // Live row so toggle writes reflect immediately via Electric sync.
  const publicTarget =
    visibleProjects.find((p) => p.id === publicTargetId) ?? null

  const handleDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      await trpc.projects.delete.mutate({ projectId: deleteTarget.id })
      setTrashRefreshKey((key) => key + 1)
    } finally {
      setDeleting(false)
      setDeleteTarget(null)
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            Projects
            <Badge variant="secondary" className="text-xs font-normal">
              {visibleProjects.length}
            </Badge>
          </CardTitle>
          <CardDescription>
            Manage projects in this team.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {visibleProjects.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No projects in this team yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {visibleProjects.map((project) => {
                const repo = project.repositoryId
                  ? repoMap.get(project.repositoryId)
                  : undefined
                const TypeIcon = getProjectIcon(project)
                const isPublicBoard = project.isPublic
                return (
                  <div
                    key={project.id}
                    className="flex items-center gap-3 px-3 py-2.5"
                  >
                    <TypeIcon
                      className="h-4 w-4 shrink-0"
                      style={{ color: project.color }}
                    />
                    <span className="min-w-0 flex-1 truncate text-sm font-medium">
                      {project.name}
                    </span>
                    {repo && (
                      <Badge
                        variant="outline"
                        className="hidden max-w-[12rem] shrink-0 gap-1 sm:inline-flex"
                        title={repo?.fullName ?? `No repository`}
                      >
                        <Github className="h-3 w-3 shrink-0 text-muted-foreground" />
                        <span className="truncate">
                          {repo?.fullName ?? `No repository`}
                        </span>
                      </Badge>
                    )}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground"
                      title="Change repository"
                      onClick={() => setRepoTarget(project)}
                    >
                      <GitBranch className="h-3.5 w-3.5" />
                    </Button>
                    {isPublicBoard && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0 text-muted-foreground"
                        title="Public board settings"
                        onClick={() => setPublicTargetId(project.id)}
                      >
                        <Globe className="h-3.5 w-3.5" />
                      </Button>
                    )}
                    <Badge
                      variant="outline"
                      className="hidden shrink-0 font-mono text-xs sm:inline-flex"
                    >
                      {project.prefix}
                    </Badge>
                    {!project.isProtected && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                        title="Move to trash"
                        onClick={() =>
                          setDeleteTarget({
                            id: project.id,
                            name: project.name,
                          })
                        }
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <PendingDeletionCard
        workspaceId={workspaceId}
        refreshKey={trashRefreshKey}
      />

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Move project to trash</DialogTitle>
            <DialogDescription>
              Move{` `}
              <span className="font-semibold text-foreground">
                {deleteTarget?.name}
              </span>
              {` `}
              to the trash? It is kept for 48 hours — owners can restore it from
              this page — then permanently deleted with all its issues.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleting}
            >
              {deleting ? `Moving…` : `Move to trash`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ChangeRepositoryDialog
        project={repoTarget}
        workspaceId={workspaceId}
        onOpenChange={(open) => {
          if (!open) setRepoTarget(null)
        }}
        onChanged={() => void refreshRepos()}
      />

      <PublicBoardDialog
        project={publicTarget}
        workspaceSlug={workspaceSlug}
        onOpenChange={(open) => {
          if (!open) setPublicTargetId(null)
        }}
      />
    </>
  )
}

type TrashedProject = Awaited<
  ReturnType<typeof trpc.projects.listDeleted.query>
>[number]

// Dates cross the tRPC boundary as ISO strings (no transformer), so coerce.
function formatPurgeCountdown(purgeAt: Date | string | null): string {
  if (!purgeAt) return `Purges soon`
  const ms = new Date(purgeAt).getTime() - Date.now()
  if (ms <= 0) return `Purging soon`
  const hours = Math.ceil(ms / (60 * 60 * 1000))
  return `Purges in ~${hours}h`
}

// The workspace's trashed projects. Renders NOTHING when the trash is empty —
// the trash surface only exists while something is pending deletion.
function PendingDeletionCard({
  workspaceId,
  refreshKey,
}: {
  workspaceId: string
  refreshKey: number
}) {
  const [trashed, setTrashed] = useState<TrashedProject[] | null>(null)
  const [restoringId, setRestoringId] = useState<string | null>(null)
  // Bumped every 60s so the purge countdown re-renders while the page stays open.
  const [, setTick] = useState(0)

  const refresh = useCallback(async () => {
    try {
      setTrashed(await trpc.projects.listDeleted.query({ workspaceId }))
    } catch {
      setTrashed([])
    }
  }, [workspaceId])

  useEffect(() => {
    void refresh()
  }, [refresh, refreshKey])

  useEffect(() => {
    const id = setInterval(() => setTick((tick) => tick + 1), 60_000)
    return () => clearInterval(id)
  }, [])

  const handleRestore = async (id: string) => {
    setRestoringId(id)
    try {
      await trpc.projects.restore.mutate({ projectId: id })
    } finally {
      setRestoringId(null)
      // Refresh on success AND failure — a restore can fail because the row was
      // purged out from under us, in which case it should drop off the card.
      await refresh()
    }
  }

  if (!trashed || trashed.length === 0) return null

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Trash2 className="h-4 w-4" />
          Trash
        </CardTitle>
        <CardDescription>
          Deleted projects are kept for 48 hours, then permanently removed with
          all their issues.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="divide-y rounded-md border">
          {trashed.map((project) => {
            const TypeIcon = getProjectIcon(project)
            return (
              <div
                key={project.id}
                className="flex items-center gap-3 px-3 py-2.5"
              >
                <TypeIcon
                  className="h-4 w-4 shrink-0"
                  style={{ color: project.color }}
                />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {project.name}
                </span>
                <Badge
                  variant="outline"
                  className="hidden shrink-0 font-mono text-xs sm:inline-flex"
                >
                  {project.prefix}
                </Badge>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {formatPurgeCountdown(project.purgeAt)}
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 shrink-0"
                  disabled={restoringId === project.id}
                  onClick={() => void handleRestore(project.id)}
                >
                  {restoringId === project.id ? `Restoring…` : `Restore`}
                </Button>
              </div>
            )
          })}
        </div>
      </CardContent>
    </Card>
  )
}

// Owner controls for a feedback board's public surface. The board itself is
// always publicly readable (that's what a feedback board IS); these toggles
// govern what visitors see beyond the issues.
function PublicBoardDialog({
  project,
  workspaceSlug,
  onOpenChange,
}: {
  project: Project | null
  workspaceSlug: string
  onOpenChange: (open: boolean) => void
}) {
  const [busy, setBusy] = useState(false)

  const publicUrl = project
    ? buildPublicBoardUrl(workspaceSlug, project.slug)
    : ``

  const update = async (
    updates: Partial<{
      publicShowComments: boolean
      publicShowActivity: boolean
    }>
  ) => {
    if (!project) return
    setBusy(true)
    try {
      await trpc.projects.update.mutate({ id: project.id, ...updates })
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={project !== null} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Globe className="h-4 w-4" />
            Public board
          </DialogTitle>
          <DialogDescription>
            Anyone with the link can read{` `}
            <span className="font-medium text-foreground">
              {project?.name}
            </span>
            : issue titles, descriptions and @mentions are public. Visitors
            submit feedback through the embeddable widget.
          </DialogDescription>
        </DialogHeader>

        <PublicBoardLinkRow url={publicUrl} />

        <div className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0 flex-1">
              <Label className="text-sm">Show comments</Label>
              <p className="text-xs text-muted-foreground">
                Visitors see issue discussions (authors stay anonymized).
              </p>
            </div>
            <Switch
              checked={project?.publicShowComments ?? true}
              disabled={busy}
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
              checked={project?.publicShowActivity ?? false}
              disabled={busy}
              onCheckedChange={(checked) =>
                void update({ publicShowActivity: checked })
              }
            />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ChangeRepositoryDialog({
  project,
  workspaceId,
  onOpenChange,
  onChanged,
}: {
  project: Project | null
  workspaceId: string
  onOpenChange: (open: boolean) => void
  onChanged: () => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Reset transient state whenever the target project changes.
  useEffect(() => {
    setBusy(false)
    setError(null)
  }, [project?.id])

  const apply = async (repositoryId: string) => {
    if (!project) return
    setBusy(true)
    setError(null)
    try {
      await trpc.projects.setRepository.mutate(
        { projectId: project.id, repositoryId },
        { context: { skipErrorToast: true } }
      )
      onChanged()
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  // A brand-new repo: register it (idempotent upsert/un-archive) then point the
  // project at the returned repository id.
  const handleConnect = async (picked: PickerRepo) => {
    setBusy(true)
    setError(null)
    try {
      const { repository } = await trpc.repositories.add.mutate(
        {
          workspaceId,
          fullName: picked.fullName,
          defaultBranch: picked.defaultBranch,
          private: picked.private,
        },
        { context: { skipErrorToast: true } }
      )
      if (repository) {
        await apply(repository.id)
        return
      }
      setError(`Could not connect ${picked.fullName}.`)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={project !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Change repository</DialogTitle>
          <DialogDescription>
            Choose the repository{` `}
            <span className="font-medium text-foreground">
              {project?.name}
            </span>
            {` `}is coded on. New &ldquo;Start coding&rdquo; launches use it;
            existing worktrees keep working locally.
          </DialogDescription>
        </DialogHeader>

        {error && (
          <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        <ConnectedRepoPicker
          workspaceId={workspaceId}
          value={project?.repositoryId ?? null}
          disabled={busy}
          onSelectRegistry={(repo) => void apply(repo.id)}
          onConnectNew={(picked) => void handleConnect(picked)}
        />
      </DialogContent>
    </Dialog>
  )
}
