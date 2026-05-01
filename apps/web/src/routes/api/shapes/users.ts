import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserIdsInWorkspaces,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/users`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `users`,
        getWhere: async (userId) => {
          const sharedUserIds = await getUserIdsInWorkspaces(userId)
          return buildWhereClause(`id`, sharedUserIds)
        },
      }),
    },
  },
})
