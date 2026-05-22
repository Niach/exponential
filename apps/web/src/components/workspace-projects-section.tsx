import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, Link2, Loader2, X } from "lucide-react"
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"

type CompanionAgentList = Awaited<
  ReturnType<typeof trpc.companion.list.query>
>[`agents`]

interface GithubRepoMinimal {
  fullName: string
  defaultBranch: string
  private: boolean
}

function parseRepos(value: unknown): GithubRepoMinimal[] {
  if (!Array.isArray(value)) return []
  const out: GithubRepoMinimal[] = []
  for (const item of value) {
    if (
      item &&
      typeof item === `object` &&
      typeof (item as { fullName?: unknown }).fullName === `string` &&
      typeof (item as { defaultBranch?: unknown }).defaultBranch === `string` &&
      typeof (item as { private?: unknown }).private === `boolean`
    ) {
      out.push(item as GithubRepoMinimal)
    }
  }
  return out
}

const NOT_LINKED_VALUE = `__none__`

export function WorkspaceProjectsSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const projects = useWorkspaceProjects(workspaceId)
  const [agents, setAgents] = useState<CompanionAgentList>([])
  const [loadingAgents, setLoadingAgents] = useState(true)
  const [busyProjectId, setBusyProjectId] = useState<string | null>(null)

  const refreshAgents = useCallback(async () => {
    const result = await trpc.companion.list.query({ workspaceId })
    setAgents(result.agents)
    setLoadingAgents(false)
  }, [workspaceId])

  useEffect(() => {
    setLoadingAgents(true)
    void refreshAgents()
  }, [refreshAgents])

  // Union of repos across all agents in this workspace, deduplicated by
  // fullName and sorted A-Z.
  const availableRepos = useMemo<GithubRepoMinimal[]>(() => {
    const map = new Map<string, GithubRepoMinimal>()
    for (const agent of agents) {
      for (const repo of parseRepos(agent.githubRepos)) {
        if (!map.has(repo.fullName)) map.set(repo.fullName, repo)
      }
    }
    return Array.from(map.values()).sort((a, b) =>
      a.fullName.localeCompare(b.fullName)
    )
  }, [agents])

  const hasAnyAgent = agents.length > 0
  const hasAnyGithubLogin = agents.some((a) => !!a.githubUserLogin)
  const visibleProjects = projects.filter((p) => !p.archivedAt)

  const setRepoForProject = async (projectId: string, value: string) => {
    setBusyProjectId(projectId)
    try {
      if (value === NOT_LINKED_VALUE) {
        await trpc.projects.unlinkGithubRepo.mutate({ projectId })
      } else {
        await trpc.projects.linkGithubRepo.mutate({ projectId, repo: value })
      }
      // No local optimistic update needed — projectCollection is Electric-synced.
    } finally {
      setBusyProjectId(null)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Github className="h-4 w-4" />
          Project Repositories
        </CardTitle>
        <CardDescription>
          Link each project to a GitHub repository so agents can work on
          assigned issues. The list of available repos comes from your
          companion&rsquo;s GitHub login.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {!hasAnyAgent ? (
          <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
            Add an agent member above first, then run{` `}
            <code className="rounded bg-muted px-1 py-0.5 text-xs">
              companion github login
            </code>
            {` `}
            on the daemon host.
          </div>
        ) : !hasAnyGithubLogin ? (
          <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
            No agent has authenticated against GitHub yet. Run{` `}
            <code className="rounded bg-muted px-1 py-0.5 text-xs">
              companion github login
            </code>
            {` `}
            on the daemon host.
          </div>
        ) : null}

        {loadingAgents ? (
          <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Loading agents
          </div>
        ) : visibleProjects.length === 0 ? (
          <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
            No projects in this workspace yet.
          </div>
        ) : (
          visibleProjects.map((project) => {
            const linked = project.githubRepo
            const pickerValue = linked ?? NOT_LINKED_VALUE
            return (
              <div
                key={project.id}
                className="flex flex-col gap-2 rounded-md border px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge
                      variant="outline"
                      className="font-mono text-xs"
                      style={{ borderColor: project.color }}
                    >
                      {project.prefix}
                    </Badge>
                    <span className="break-all text-sm font-medium">
                      {project.name}
                    </span>
                  </div>
                  {linked ? (
                    <a
                      href={`https://github.com/${linked}`}
                      target="_blank"
                      rel="noreferrer"
                      className="mt-1 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                    >
                      <Link2 className="h-3 w-3" />
                      {linked}
                    </a>
                  ) : (
                    <div className="mt-1 text-xs text-muted-foreground">
                      Not linked
                    </div>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-2 sm:max-w-sm sm:flex-1 sm:justify-end">
                  <Select
                    value={pickerValue}
                    onValueChange={(v) => void setRepoForProject(project.id, v)}
                    disabled={busyProjectId === project.id || !hasAnyGithubLogin}
                  >
                    <SelectTrigger className="h-9 w-full text-sm sm:max-w-xs">
                      <SelectValue placeholder="Pick a repo" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={NOT_LINKED_VALUE}>
                        Not linked
                      </SelectItem>
                      {availableRepos.length === 0 ? (
                        <SelectItem value="__empty__" disabled>
                          No repos available
                        </SelectItem>
                      ) : (
                        availableRepos.map((repo) => (
                          <SelectItem key={repo.fullName} value={repo.fullName}>
                            {repo.fullName}
                            {repo.private ? ` 🔒` : ``}
                          </SelectItem>
                        ))
                      )}
                    </SelectContent>
                  </Select>
                  {linked && busyProjectId !== project.id && (
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() =>
                        void setRepoForProject(project.id, NOT_LINKED_VALUE)
                      }
                      aria-label="Unlink"
                    >
                      <X className="h-4 w-4" />
                    </Button>
                  )}
                  {busyProjectId === project.id && (
                    <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                  )}
                </div>
              </div>
            )
          })
        )}
      </CardContent>
    </Card>
  )
}
