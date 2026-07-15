import { useMemo } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  projectCollection,
} from "@/lib/collections"
import {
  useWorkspaceBySlug,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import { useProjectBoardData } from "@/hooks/use-project-board-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import {
  issueFiltersFromSearch,
  parseIssueFilterSearch,
  type IssueFilterSearch,
} from "@/lib/filters"
import { findIssuePosition } from "@/lib/project-board"
import type { Issue, IssueLabel, Project } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"

export const Route = createFileRoute(
  `/t/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`
)({
  // No route-level auth guard: the parent `/t/$workspaceSlug` layout route
  // (route.tsx) already gates access with public-workspace-aware logic —
  // anonymous visitors of a PUBLIC workspace pass through, while non-public /
  // inaccessible workspaces are redirected to login or 404'd there. Mirroring
  // the sibling project-board route, which likewise carries no beforeLoad, this
  // lets signed-out visitors open the read-only detail page (masterplan §4.3,
  // L29). The view renders read-only via `permissions.canMutateIssue` (false
  // when unauthenticated) and the comment/timeline UI is hidden for anonymous.
  //
  // Optional ?status/priority/labels mirror the board route's filter params —
  // navigating from a filtered board carries them here so the header's
  // prev/next switcher walks the board's exact filtered+sorted sequence, and
  // the project breadcrumb links back to the same filtered view. All params
  // are optional: links from my-issues / inbox / search arrive bare and fall
  // back to the unfiltered board ordering.
  validateSearch: (search: Record<string, unknown>): IssueFilterSearch =>
    parseIssueFilterSearch(search),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { workspaceSlug, projectSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()
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
    [workspace?.id, projectSlug]
  )
  const project = (projects?.[0] ?? null) as Project | null

  const { data: issues } = useLiveQuery(
    (query) =>
      project
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                eq(issues.projectId, project.id),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [project?.id, issueIdentifier]
  )
  const issue = (issues?.[0] ?? null) as Issue | null

  const { data: issueLabels } = useLiveQuery(
    (query) =>
      issue
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) => eq(issueLabels.issueId, issue.id))
        : undefined,
    [issue?.id]
  )
  const issueLabelIds = ((issueLabels ?? []) as IssueLabel[]).map(
    (row) => row.labelId
  )

  // Same pipeline the board renders from (buildFilteredIssues →
  // buildVisibleIssueGroups over locally-synced rows — cheap), so the
  // switcher's ordering can never drift from the list the user came from.
  const filters = useMemo(
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )
  const { visibleGroups } = useProjectBoardData({
    filters,
    projectSlug,
    workspaceSlug,
  })
  const position = issue ? findIssuePosition(visibleGroups, issue.id) : null
  const switcher = position
    ? {
        index: position.index,
        total: position.total,
        prevIdentifier: position.prev?.identifier ?? null,
        nextIdentifier: position.next?.identifier ?? null,
      }
    : null

  const { users } = useWorkspaceUsers(workspace?.id)
  const permissions = useWorkspacePermissions(workspace)

  if (!workspace || !project) {
    return (
      <div className="text-muted-foreground text-sm p-6">Loading…</div>
    )
  }

  if (!issue) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Issue <span className="font-mono">{issueIdentifier}</span> not found in
          this project.
        </div>
        <Link
          to="/t/$workspaceSlug/projects/$projectSlug"
          params={{ workspaceSlug, projectSlug }}
          className="text-foreground underline-offset-2 hover:underline"
        >
          ← Back to project
        </Link>
      </div>
    )
  }

  return (
    <IssueDetailView
      issue={issue}
      issueLabelIds={issueLabelIds}
      users={users}
      project={project}
      workspaceSlug={workspaceSlug}
      workspaceId={workspace.id}
      readOnly={!permissions.canMutateIssue(issue)}
      filterSearch={search}
      position={switcher}
    />
  )
}
