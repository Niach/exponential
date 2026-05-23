import { useState, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Input } from "@/components/ui/input"
import { issueCollection } from "@/lib/collections"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { StatusIcon } from "@/components/status-dropdown"
import { Search } from "lucide-react"
import type { Issue, Project } from "@/db/schema"

interface IssueSearchSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspaceId: string
  workspaceSlug: string
}

export function IssueSearchSheet({
  open,
  onOpenChange,
  workspaceId,
  workspaceSlug,
}: IssueSearchSheetProps) {
  const [query, setQuery] = useState(``)
  const navigate = useNavigate()
  const projects = useWorkspaceProjects(workspaceId)
  const projectIds = useMemo(
    () => projects.map((p: Project) => p.id),
    [projects]
  )
  const projectMap = useMemo(
    () => new Map<string, Project>(projects.map((p: Project) => [p.id, p])),
    [projects]
  )

  const { data: issues } = useLiveQuery(
    (q) =>
      projectIds.length > 0
        ? q
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.projectId, projectIds))
        : undefined,
    [projectIds.join(`,`)]
  )

  const results = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return [] as Issue[]
    return (issues ?? [])
      .filter((i: Issue) => i.title.toLowerCase().includes(q))
      .slice(0, 30)
  }, [issues, query])

  const handlePick = (issue: Issue) => {
    const project = projectMap.get(issue.projectId)
    if (!project) return
    onOpenChange(false)
    setQuery(``)
    void navigate({
      to: `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
      params: {
        workspaceSlug,
        projectSlug: project.slug,
        issueIdentifier: issue.identifier,
      },
    })
  }

  return (
    <Sheet
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o)
        if (!o) setQuery(``)
      }}
    >
      <SheetContent
        side="bottom"
        showCloseButton={false}
        className="top-0 h-[100dvh] p-0 gap-0 flex flex-col"
      >
        <SheetTitle className="sr-only">Search issues</SheetTitle>
        <div className="flex items-center gap-2 px-3 py-3 border-b border-border/50">
          <Search className="size-4 text-muted-foreground shrink-0" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search issues..."
            autoFocus
            className="border-none shadow-none focus-visible:ring-0 h-9 text-base"
          />
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            className="text-sm text-muted-foreground px-2"
          >
            Cancel
          </button>
        </div>
        <div className="flex-1 overflow-y-auto">
          {query.trim() === `` && (
            <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
              <Search className="size-8 mb-3 opacity-50" />
              <p className="text-sm">Type to search issues</p>
            </div>
          )}
          {query.trim() !== `` && results.length === 0 && (
            <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
              <p className="text-sm">No issues match "{query}"</p>
            </div>
          )}
          {results.map((issue) => {
            const project = projectMap.get(issue.projectId)
            return (
              <button
                key={issue.id}
                type="button"
                onClick={() => handlePick(issue)}
                className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-accent active:bg-accent/70 border-b border-border/30"
              >
                <StatusIcon status={issue.status} className="size-4 shrink-0" />
                <div className="flex flex-col flex-1 min-w-0">
                  <span className="text-sm truncate">{issue.title}</span>
                  {project && (
                    <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <span
                        className="h-1.5 w-1.5 rounded-full shrink-0"
                        style={{ backgroundColor: project.color }}
                      />
                      <span className="truncate">
                        {project.name} · {issue.identifier}
                      </span>
                    </span>
                  )}
                </div>
              </button>
            )
          })}
        </div>
      </SheetContent>
    </Sheet>
  )
}
