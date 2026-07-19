import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `labels`,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return buildWhereClause(`team_id`, teamIds)
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
