import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableBoardIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issues`,
        getWhere: async (userId) => {
          // Members: their boards. Anonymous: nothing (empty id list yields
          // the impossible-match sentinel).
          const boardIds = await getReadableBoardIds(userId)
          return buildWhereClause(`board_id`, boardIds)
        },
      }),
    },
  },
})
