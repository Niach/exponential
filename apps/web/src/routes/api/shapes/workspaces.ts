import { createFileRoute } from "@tanstack/react-router"
import {
  getUserWorkspaceIds,
  buildWhereClause,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/workspaces`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workspaces`,
        getWhere: async (userId) => {
          const workspaceIds = await getUserWorkspaceIds(userId)
          return buildWhereClause(`id`, workspaceIds)
        },
      }),
    },
  },
})
