import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `labels`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
