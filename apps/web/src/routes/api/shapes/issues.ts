import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableProjectIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issues`,
        getWhere: async (userId) => {
          const projectIds = await getReadableProjectIds(userId)
          return buildWhereClause(`project_id`, projectIds)
        },
      }),
    },
  },
})
