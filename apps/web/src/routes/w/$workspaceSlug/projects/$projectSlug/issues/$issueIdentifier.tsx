import { createFileRoute, Link, redirect } from "@tanstack/react-router"
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
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import type { Issue, IssueLabel, Project } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"

export const Route = createFileRoute(
  `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`
)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { workspaceSlug, projectSlug, issueIdentifier } = Route.useParams()
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
          to="/w/$workspaceSlug/projects/$projectSlug"
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
      restrictModeration={!permissions.isModerator && workspace.isPublic}
    />
  )
}
