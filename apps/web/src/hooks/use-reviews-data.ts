import { useCallback, useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import {
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import { trpc } from "@/lib/trpc-client"
import type { OpenPull } from "@/lib/integrations/github-pr"
import type { Issue, Project, Workspace } from "@/db/schema"

export interface ReviewGroup {
  project: Project
  issues: Issue[]
}

export interface ExternalPullGroup {
  repositoryId: string
  fullName: string
  pulls: OpenPull[]
}

// Cross-project review queue: every issue in the workspace with an open pull
// request, grouped by project (project sortOrder, issues newest-first). Pure
// client work over the already-synced issues shape — prState arrives on every
// issue row, and the collections' snakeCamelMapper makes the camelCase filter
// match the Postgres pr_state column.
export function useReviewsData(workspace: Workspace | null | undefined) {
  const projects = useWorkspaceProjects(workspace?.id)
  const workspaceId = workspace?.id
  const projectIds = useMemo(
    () => projects.map((project) => project.id),
    [projects]
  )

  const { data: issues, isReady } = useLiveQuery(
    (query) =>
      projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.projectId, projectIds),
                eq(issues.prState, `open`)
              )
            )
        : undefined,
    [projectIds.join(`,`)]
  )

  const { userMap } = useWorkspaceUsers(workspace?.id)

  // Open PRs with no issue link, fetched live from GitHub through the server
  // (they have no synced row to live-query). Failures degrade to an empty
  // list — the issue-linked queue still renders.
  const [externalGroups, setExternalGroups] = useState<ExternalPullGroup[]>([])
  const [externalLoading, setExternalLoading] = useState(false)
  useEffect(() => {
    if (!workspaceId) return
    let cancelled = false
    setExternalLoading(true)
    trpc.repositories.openPulls
      .query({ workspaceId })
      .then((result) => {
        if (cancelled) return
        setExternalGroups(result.repos.filter((repo) => repo.pulls.length > 0))
      })
      .catch(() => {
        if (!cancelled) setExternalGroups([])
      })
      .finally(() => {
        if (!cancelled) setExternalLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [workspaceId])

  // External PRs have no Electric echo — a successful merge removes the row
  // locally.
  const removeExternalPull = useCallback(
    (repositoryId: string, prNumber: number) => {
      setExternalGroups((groups) =>
        groups
          .map((group) =>
            group.repositoryId === repositoryId
              ? {
                  ...group,
                  pulls: group.pulls.filter((pull) => pull.number !== prNumber),
                }
              : group
          )
          .filter((group) => group.pulls.length > 0)
      )
    },
    []
  )

  return useMemo(() => {
    const list = (issues ?? []) as Issue[]
    const byProject = new Map<string, Issue[]>()
    for (const issue of list) {
      const bucket = byProject.get(issue.projectId)
      if (bucket) {
        bucket.push(issue)
      } else {
        byProject.set(issue.projectId, [issue])
      }
    }

    const groups: ReviewGroup[] = []
    // `projects` is already ordered by sortOrder.
    for (const project of projects) {
      const bucket = byProject.get(project.id)
      if (!bucket) continue
      bucket.sort(
        (a, b) =>
          new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      )
      groups.push({ project, issues: bucket })
    }

    const externalCount = externalGroups.reduce(
      (sum, group) => sum + group.pulls.length,
      0
    )

    return {
      groups,
      externalGroups,
      count: list.length + externalCount,
      // A workspace with no projects skips the query and can never deliver a
      // snapshot — treat it as ready-empty instead of loading forever. The
      // external fetch has its own flag so the synced queue renders without
      // waiting on GitHub.
      isLoading: !isReady && projects.length > 0,
      externalLoading,
      userMap,
      removeExternalPull,
    }
  }, [
    issues,
    isReady,
    projects,
    userMap,
    externalGroups,
    externalLoading,
    removeExternalPull,
  ])
}
