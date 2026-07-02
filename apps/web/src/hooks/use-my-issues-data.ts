import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
} from "@/lib/collections"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import type { IssueFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
} from "@/lib/project-board"
import type { Issue, IssueLabel, Label, Project } from "@/db/schema"

// Cross-project "My Issues" board data: every issue assigned to the current
// user across all projects in the workspace, reusing the project-board
// grouping/filter machinery (mirrors use-project-board-data, minus the single
// project scope). Pure client work over the already-synced issues shape.
export function useMyIssuesData({
  filters,
  userId,
  workspaceSlug,
}: {
  filters: IssueFilters
  userId: string | undefined
  workspaceSlug: string
}) {
  // Const binding so TS narrowing survives into the live-query closure.
  const assignee = userId
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const projectIds = useMemo(
    () => projects.map((project) => project.id),
    [projects]
  )
  const projectMap = useMemo(
    () => new Map<string, Project>(projects.map((p) => [p.id, p])),
    [projects]
  )

  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      assignee && projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.projectId, projectIds),
                eq(issues.assigneeId, assignee)
              )
            )
            .orderBy(({ issues }) => issues.createdAt)
        : undefined,
    [assignee, projectIds.join(`,`)]
  )

  const { data: labels } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )

  const { data: issueLabels } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) =>
              eq(issueLabels.workspaceId, workspace.id)
            )
        : undefined,
    [workspace?.id]
  )

  const { userMap, users } = useWorkspaceUsers(workspace?.id)

  const issueList = (issues ?? []) as Issue[]
  const labelList = (labels ?? []) as Label[]
  const issueLabelList = (issueLabels ?? []) as IssueLabel[]

  return useMemo(() => {
    const issueLabelIdsMap = buildIssueLabelIdsMap(issueLabelList)
    const issueLabelMap = buildIssueLabelMap(issueLabelList, labelList)
    const filteredIssues = buildFilteredIssues(
      issueList,
      issueLabelIdsMap,
      filters
    )

    return {
      issueLabelMap,
      // The issues query is skipped until the session user + projects are
      // known; a workspace with no projects can never deliver a snapshot, so
      // treat it as ready-empty instead of loading forever.
      issuesReady: issuesReady || projectMap.size === 0,
      labelList,
      projectMap,
      totalIssueCount: issueList.length,
      users,
      userMap,
      visibleGroups: buildVisibleIssueGroups(filteredIssues, filters.statuses),
      workspace,
    }
  }, [
    filters,
    issueLabelList,
    issueList,
    issuesReady,
    labelList,
    projectMap,
    userMap,
    users,
    workspace,
  ])
}
