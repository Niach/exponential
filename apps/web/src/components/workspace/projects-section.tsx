import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, GitBranch, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
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
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import type { Project } from "@/db/schema"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>

export function WorkspaceProjectsSection({
  workspaceId,
}: {
  workspaceId: string
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
  const [repoTarget, setRepoTarget] = useState<Project | null>(null)

  const handleDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      await trpc.projects.delete.mutate({ projectId: deleteTarget.id })
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
            Manage projects in this workspace.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {visibleProjects.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No projects in this workspace yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {visibleProjects.map((project) => {
                const repo = project.repositoryId
                  ? repoMap.get(project.repositoryId)
                  : undefined
                return (
                  <div
                    key={project.id}
                    className="flex items-center gap-3 px-3 py-2.5"
                  >
                    <div
                      className="h-2.5 w-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: project.color }}
                    />
                    <span className="min-w-0 flex-1 truncate text-sm font-medium">
                      {project.name}
                    </span>
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
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground"
                      title="Change repository"
                      onClick={() => setRepoTarget(project)}
                    >
                      <GitBranch className="h-3.5 w-3.5" />
                    </Button>
                    <Badge
                      variant="outline"
                      className="shrink-0 font-mono text-xs"
                    >
                      {project.prefix}
                    </Badge>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                      onClick={() =>
                        setDeleteTarget({ id: project.id, name: project.name })
                      }
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete project</DialogTitle>
            <DialogDescription>
              This will permanently delete{` `}
              <span className="font-semibold text-foreground">
                {deleteTarget?.name}
              </span>
              {` `}
              and all its issues. This cannot be undone.
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
              {deleting ? `Deleting...` : `Delete project`}
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
    </>
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
