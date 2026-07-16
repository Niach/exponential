import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildTextInClause,
  buildWhereClause,
  getPublicProjectScope,
  getUserProjectIds,
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

// Activity-log timeline events. Members: project-scoped (project_id is
// denormalized from issue→project by a trigger and never null here) so a
// trashed project's events drop out of sync for the 48h trash window along
// with the project itself.
// Anonymous: project-scoped to public boards that opted into showing activity.
export const Route = createFileRoute(`/api/shapes/issue-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_events`,
        getWhere: async (userId) => {
          if (userId) {
            const projectIds = await getUserProjectIds(userId)
            return buildWhereClause(`project_id`, projectIds)
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
