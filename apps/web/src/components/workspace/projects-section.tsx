import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, Pencil, Plus, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getProjectIcon } from "@/lib/project-types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
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
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { ProjectSettingsDialog } from "@/components/workspace/project-settings-dialog"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import type { Workspace } from "@/db/schema"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>

export function WorkspaceProjectsSection({
  workspace,
}: {
  workspace: Workspace
}) {
  const workspaceId = workspace.id
  const projects = useWorkspaceProjects(workspaceId)
  const visibleProjects = projects.filter((p) => !p.archivedAt)

  // The workspace's connected repos — used to render each project's repo chip
  // (uuid → owner/name) and to feed the settings dialog's repo picker.
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
  const [createOpen, setCreateOpen] = useState(false)
  // Live row so edit-dialog toggle writes reflect immediately via Electric
  // sync (and a concurrently-trashed target closes the dialog).
  const [editTargetId, setEditTargetId] = useState<string | null>(null)
  const editTarget =
    visibleProjects.find((p) => p.id === editTargetId) ?? null

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
          <CardAction>
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              <Plus className="mr-1 h-3.5 w-3.5" />
              New project
            </Button>
          </CardAction>
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
                return (
                  <div
                    key={project.id}
                    className="flex cursor-pointer items-center gap-3 px-3 py-2.5 transition-colors hover:bg-accent/40"
                    onClick={() => setEditTargetId(project.id)}
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
                    <Badge
                      variant="outline"
                      className="hidden shrink-0 font-mono text-xs sm:inline-flex"
                    >
                      {project.prefix}
                    </Badge>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground"
                      title="Project settings"
                      onClick={(e) => {
                        e.stopPropagation()
                        setEditTargetId(project.id)
                      }}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    {!project.isProtected && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                        title="Move to trash"
                        onClick={(e) => {
                          e.stopPropagation()
                          setDeleteTarget({
                            id: project.id,
                            name: project.name,
                          })
                        }}
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

      <CreateProjectDialog
        open={createOpen}
        onOpenChange={(open) => {
          setCreateOpen(open)
          // An inline-connected repo should show up as a chip right away.
          if (!open) void refreshRepos()
        }}
        workspace={workspace}
      />

      <ProjectSettingsDialog
        project={editTarget}
        workspace={workspace}
        onOpenChange={(open) => {
          if (!open) setEditTargetId(null)
        }}
        onRepoChanged={() => void refreshRepos()}
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
