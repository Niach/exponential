import { useState, useMemo } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLiveQuery, eq, and } from "@tanstack/react-db"
import {
  workspaceCollection,
  projectCollection,
  issueCollection,
  labelCollection,
  issueLabelCollection,
} from "@/lib/collections"
import { IssueList } from "@/components/issue-list"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import { EditIssueDialog } from "@/components/edit-issue-dialog"
import type { Issue, Label, IssueLabel } from "@/db/schema"
import type { IssueFilters } from "@/lib/filters"
import { emptyFilters, matchesFilters } from "@/lib/filters"

export const Route = createFileRoute(
  `/_authenticated/w/$workspaceSlug/projects/$projectSlug/`
)({
  component: ProjectPage,
})

const statusOrder = [`in_progress`, `todo`, `backlog`, `done`, `cancelled`]

function ProjectPage() {
  const { workspaceSlug, projectSlug } = Route.useParams()
  const [createIssueOpen, setCreateIssueOpen] = useState(false)
  const [defaultStatus, setDefaultStatus] = useState<string | undefined>()
  const [filters, setFilters] = useState<IssueFilters>(emptyFilters)
  const [editingIssueId, setEditingIssueId] = useState<string | null>(null)

  const { data: workspaces } = useLiveQuery((q) =>
    q
      .from({ workspaces: workspaceCollection })
      .where(({ workspaces }) => eq(workspaces.slug, workspaceSlug))
  )
  const workspace = workspaces?.[0]

  const { data: projects } = useLiveQuery(
    (q) =>
      workspace
        ? q
            .from({ projects: projectCollection })
            .where(({ projects }) =>
              and(
                eq(projects.workspaceId, workspace.id),
                eq(projects.slug, projectSlug)
              )
            )
        : undefined,
    [workspace?.id, projectSlug]
  )
  const project = projects?.[0]

  const { data: issues } = useLiveQuery(
    (q) =>
      project
        ? q
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.projectId, project.id))
            .orderBy(({ issues }) => issues.createdAt)
        : undefined,
    [project?.id]
  )

  const { data: labels } = useLiveQuery(
    (q) =>
      workspace
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )

  const { data: issueLabelsData } = useLiveQuery(
    (q) =>
      project ? q.from({ issueLabels: issueLabelCollection }) : undefined,
    [project?.id]
  )

  // Build a map of issueId -> Label[]
  const issueLabelMap = useMemo(() => {
    const map = new Map<string, Label[]>()
    if (!issueLabelsData || !labels) return map

    const labelMap = new Map(labels.map((l: Label) => [l.id, l]))
    for (const il of issueLabelsData as IssueLabel[]) {
      const label = labelMap.get(il.labelId)
      if (label) {
        const existing = map.get(il.issueId) ?? []
        existing.push(label)
        map.set(il.issueId, existing)
      }
    }
    return map
  }, [issueLabelsData, labels])

  // Build a map of issueId -> labelId[] for filtering
  const issueLabelIdsMap = useMemo(() => {
    const map = new Map<string, string[]>()
    if (!issueLabelsData) return map
    for (const il of issueLabelsData as IssueLabel[]) {
      const existing = map.get(il.issueId) ?? []
      existing.push(il.labelId)
      map.set(il.issueId, existing)
    }
    return map
  }, [issueLabelsData])

  // Filter issues
  const filteredIssues = useMemo(() => {
    const issueList = issues ?? []
    return issueList.filter((issue: Issue) =>
      matchesFilters(issue, issueLabelIdsMap.get(issue.id) ?? [], filters)
    )
  }, [issues, issueLabelIdsMap, filters])

  // Group filtered issues by status, hiding empty groups
  const visibleGroups = useMemo(() => {
    const allGroups = statusOrder.map((status) => ({
      status,
      issues: filteredIssues.filter((i: Issue) => i.status === status),
    }))
    if (filters.statuses.length > 0) {
      return allGroups.filter((g) => filters.statuses.includes(g.status))
    }
    return allGroups.filter((g) => g.issues.length > 0)
  }, [filteredIssues, filters.statuses])

  // Derive editing issue from live data
  const editingIssue = useMemo(() => {
    if (!editingIssueId || !issues) return null
    return (issues as Issue[]).find((i) => i.id === editingIssueId) ?? null
  }, [editingIssueId, issues])

  const editingIssueLabelIds = useMemo(() => {
    if (!editingIssueId || !issueLabelsData) return []
    return (issueLabelsData as IssueLabel[])
      .filter((il) => il.issueId === editingIssueId)
      .map((il) => il.labelId)
  }, [editingIssueId, issueLabelsData])

  const handleNewIssue = (status?: string) => {
    setDefaultStatus(status)
    setCreateIssueOpen(true)
  }

  if (!project) {
    return (
      <div className="text-muted-foreground text-sm p-6">
        Loading project...
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <IssueFilterBar
        filters={filters}
        onFiltersChange={setFilters}
        labels={(labels as Label[]) ?? []}
        onNewIssue={() => handleNewIssue()}
      />

      <div className="flex-1 overflow-auto">
        <IssueList
          groups={visibleGroups}
          issueLabelMap={issueLabelMap}
          onNewIssue={handleNewIssue}
          onIssueClick={(issue) => setEditingIssueId(issue.id)}
        />
      </div>

      <CreateIssueDialog
        open={createIssueOpen}
        onOpenChange={setCreateIssueOpen}
        projectId={project.id}
        projectPrefix={project.prefix}
        projectColor={project.color}
        workspaceId={workspace!.id}
        defaultStatus={defaultStatus}
      />

      {editingIssue && (
        <EditIssueDialog
          open={!!editingIssue}
          onOpenChange={(open) => {
            if (!open) setEditingIssueId(null)
          }}
          issue={editingIssue}
          projectPrefix={project.prefix}
          projectColor={project.color}
          workspaceId={workspace!.id}
          issueLabelIds={editingIssueLabelIds}
        />
      )}
    </div>
  )
}
