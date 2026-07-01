import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, Lock, Plus, Star, Trash2, X } from "lucide-react"
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
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import type { Project } from "@/db/schema"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
type RepoRowData = RepoList[number]

export function WorkspaceRepositoriesSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const projects = useWorkspaceProjects(workspaceId)
  const visibleProjects = useMemo(
    () => projects.filter((p) => !p.archivedAt),
    [projects]
  )
  const projectMap = useMemo(
    () => new Map(visibleProjects.map((p) => [p.id, p])),
    [visibleProjects]
  )

  const [repos, setRepos] = useState<RepoList | null>(null)
  const [connectOpen, setConnectOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setRepos(await trpc.repositories.list.query({ workspaceId }))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [workspaceId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true)
    setError(null)
    try {
      await fn()
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const handleConnect = (picked: PickerRepo) =>
    run(async () => {
      await trpc.repositories.add.mutate({
        workspaceId,
        fullName: picked.fullName,
        defaultBranch: picked.defaultBranch,
        private: picked.private,
        installationId: picked.installationId,
      })
      setConnectOpen(false)
    })

  const count = repos?.length ?? 0

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            Repositories
            <Badge variant="secondary" className="text-xs font-normal">
              {count}
            </Badge>
          </CardTitle>
          <CardDescription>
            Connect GitHub repos so issues in this workspace can be coded on.
            Link a repo to a project to make it the clone target for
            &ldquo;Start coding&rdquo;.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div>
            <Button size="sm" onClick={() => setConnectOpen(true)}>
              <Github className="mr-1.5 h-3.5 w-3.5" />
              Connect repository
            </Button>
          </div>

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          {count === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No repositories connected yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {repos!.map((repo) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  projects={visibleProjects}
                  projectMap={projectMap}
                  busy={busy}
                  onRemove={() =>
                    run(() =>
                      trpc.repositories.remove.mutate({ repositoryId: repo.id })
                    )
                  }
                  onLink={(projectId) =>
                    run(() =>
                      trpc.repositories.linkProject.mutate({
                        projectId,
                        repositoryId: repo.id,
                      })
                    )
                  }
                  onUnlink={(projectId) =>
                    run(() =>
                      trpc.repositories.unlinkProject.mutate({
                        projectId,
                        repositoryId: repo.id,
                      })
                    )
                  }
                  onSetPrimary={(projectId) =>
                    run(() =>
                      trpc.repositories.setPrimary.mutate({
                        projectId,
                        repositoryId: repo.id,
                      })
                    )
                  }
                />
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={connectOpen} onOpenChange={setConnectOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Connect a repository</DialogTitle>
            <DialogDescription>
              Pick a repository the Exponential GitHub App is installed on. It
              becomes available to link to this workspace&apos;s projects.
            </DialogDescription>
          </DialogHeader>
          <GithubRepoPicker onSelect={handleConnect} />
        </DialogContent>
      </Dialog>
    </>
  )
}

function RepoRow({
  repo,
  projects,
  projectMap,
  busy,
  onRemove,
  onLink,
  onUnlink,
  onSetPrimary,
}: {
  repo: RepoRowData
  projects: Project[]
  projectMap: Map<string, Project>
  busy: boolean
  onRemove: () => void
  onLink: (projectId: string) => void
  onUnlink: (projectId: string) => void
  onSetPrimary: (projectId: string) => void
}) {
  const [linkOpen, setLinkOpen] = useState(false)
  const linkedIds = new Set(repo.projectLinks.map((l) => l.projectId))
  const unlinked = projects.filter((p) => !linkedIds.has(p.id))

  return (
    <div className="space-y-2 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {repo.fullName}
        </span>
        <Badge variant="outline" className="shrink-0 font-mono text-xs">
          {repo.defaultBranch}
        </Badge>
        {repo.private && (
          <Badge variant="secondary" className="shrink-0 gap-1 text-xs">
            <Lock className="h-3 w-3" />
            Private
          </Badge>
        )}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
          disabled={busy}
          onClick={onRemove}
          title="Remove repository"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-1.5 pl-6">
        {repo.projectLinks.length === 0 && (
          <span className="text-xs text-muted-foreground">
            No projects linked
          </span>
        )}
        {repo.projectLinks.map((link) => {
          const project = projectMap.get(link.projectId)
          return (
            <Badge
              key={link.projectId}
              variant="outline"
              className="gap-1 pl-1.5"
            >
              <button
                type="button"
                disabled={busy}
                onClick={() => onSetPrimary(link.projectId)}
                title={link.isPrimary ? `Primary repo` : `Make primary`}
                className="flex items-center"
              >
                <Star
                  className={
                    link.isPrimary
                      ? `h-3 w-3 fill-yellow-500 text-yellow-500`
                      : `h-3 w-3 text-muted-foreground/60`
                  }
                />
              </button>
              <span className="max-w-[10rem] truncate">
                {project?.name ?? `Unknown project`}
              </span>
              <button
                type="button"
                disabled={busy}
                onClick={() => onUnlink(link.projectId)}
                title="Unlink project"
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          )
        })}

        {unlinked.length > 0 && (
          <Popover open={linkOpen} onOpenChange={setLinkOpen}>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="h-6 gap-1 px-2 text-xs text-muted-foreground"
                disabled={busy}
              >
                <Plus className="h-3 w-3" />
                Link project
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-[14rem] p-0" align="start">
              <Command>
                <CommandInput placeholder="Search projects..." />
                <CommandList>
                  <CommandEmpty>No projects found.</CommandEmpty>
                  <CommandGroup>
                    {unlinked.map((project) => (
                      <CommandItem
                        key={project.id}
                        value={project.name}
                        onSelect={() => {
                          onLink(project.id)
                          setLinkOpen(false)
                        }}
                        className="flex items-center gap-2"
                      >
                        <span
                          className="h-2.5 w-2.5 shrink-0 rounded-full"
                          style={{ backgroundColor: project.color }}
                        />
                        <span className="truncate text-sm">{project.name}</span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
        )}
      </div>
    </div>
  )
}
