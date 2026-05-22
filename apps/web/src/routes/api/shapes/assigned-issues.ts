import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableProjectIds,
  sqlStringLiteral,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Shape proxy used by the agent companion daemon. Streams only the issues
// assigned to the requesting user, scoped to projects they can read. The
// where clause is server-computed from the bearer token; clients cannot
// supply a where parameter.
export const Route = createFileRoute(`/api/shapes/assigned-issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issues`,
        requireAuth: true,
        getWhere: async (userId) => {
          if (!userId) return null
          const projectIds = await getReadableProjectIds(userId)
          const projectScope = buildWhereClause(`project_id`, projectIds)
          return `("assignee_id" = ${sqlStringLiteral(userId)}) AND (${projectScope})`
        },
      }),
    },
  },
})
