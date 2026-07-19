import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserProjectIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Activity-log timeline events. Members: project-scoped (project_id is
// denormalized from issue→project by a trigger and never null here) so a
// trashed project's events drop out of sync for the 48h trash window along
// with the project itself. Anonymous callers sync nothing.
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
          return buildWhereClause(`project_id`, [])
        },
      }),
    },
  },
})
