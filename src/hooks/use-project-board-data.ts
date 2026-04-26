import { useMemo } from "react"
import { and, eq } from "@tanstack/react-db"
import { useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
  projectCollection,
} from "@/lib/collections"
import {
  useWorkspaceBySlug,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import type { IssueFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
  getEditingIssue,
  getEditingIssueLabelIds,
} from "@/lib/project-board"
import type { Issue, IssueLabel, Label, Project } from "@/db/schema"

export function useProjectBoardData({
  editingIssueId,
  filters,
  projectSlug,
  workspaceSlug,
}: {
  editingIssueId: string | null
  filters: IssueFilters
  projectSlug: string
  workspaceSlug: string
}) {
  const workspace = useWorkspaceBySlug(workspaceSlug)

  const { data: projects } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ projects: projectCollection })
            .where(({ projects }) =>
              and(
                eq(projects.workspaceId, workspace.id),
                eq(projects.slug, projectSlug)
              )
            )
        : undefined,
    [projectSlug, workspace?.id]
  )

  const project = (projects?.[0] ?? null) as Project | null

  const { data: issues } = useLiveQuery(
    (query) =>
      project
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.projectId, project.id))
            .orderBy(({ issues }) => issues.createdAt)
        : undefined,
    [project?.id]
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
      project ? query.from({ issueLabels: issueLabelCollection }) : undefined,
    [project?.id]
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
      editingIssue: getEditingIssue(issueList, editingIssueId),
      editingIssueLabelIds: getEditingIssueLabelIds(
        issueLabelList,
        editingIssueId
      ),
      issueLabelMap,
      labelList,
      project,
      users,
      userMap,
      visibleGroups: buildVisibleIssueGroups(filteredIssues, filters.statuses),
      workspace,
    }
  }, [
    editingIssueId,
    filters,
    issueLabelList,
    issueList,
    labelList,
    project,
    userMap,
    users,
    workspace,
  ])
}
