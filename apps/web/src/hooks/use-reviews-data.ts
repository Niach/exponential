import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import {
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import type { Issue, Project, Workspace } from "@/db/schema"

export interface ReviewGroup {
  project: Project
  issues: Issue[]
}

// Cross-project review queue: every issue in the workspace with an open pull
// request, grouped by project (project sortOrder, issues newest-first). Pure
// client work over the already-synced issues shape — prState arrives on every
// issue row, and the collections' snakeCamelMapper makes the camelCase filter
// match the Postgres pr_state column.
export function useReviewsData(workspace: Workspace | null | undefined) {
  const projects = useWorkspaceProjects(workspace?.id)
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

    return {
      groups,
      count: list.length,
      // A workspace with no projects skips the query and can never deliver a
      // snapshot — treat it as ready-empty instead of loading forever.
      isLoading: !isReady && projects.length > 0,
      userMap,
    }
  }, [issues, isReady, projects, userMap])
}
