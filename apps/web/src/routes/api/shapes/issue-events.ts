import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildTextInClause,
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Event types anonymous public-board viewers may sync. POSITIVE allowlist:
// pr_opened/pr_merged are excluded because their jsonb payloads carry
// prUrl/prNumber/branch — repo identity that must not leak when a feedback
// board is backed by a private repo. Any newly added event type stays hidden
// from anonymous viewers until deliberately added here.
const ANONYMOUS_EVENT_TYPES = [
  `status_changed`,
  `assignee_changed`,
  `label_added`,
  `label_removed`,
]

// Activity-log timeline events. Members: workspace-scoped (workspace_id is
// denormalized from issue→project by a trigger so the filter stays stable).
// Anonymous: project-scoped to public boards that opted into showing activity.
export const Route = createFileRoute(`/api/shapes/issue-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_events`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          const scope = await getPublicProjectScope()
          return andClauses(
            buildWhereClause(`project_id`, scope.activityProjectIds),
            buildTextInClause(`type`, ANONYMOUS_EVENT_TYPES)
          )
        },
      }),
    },
  },
})
