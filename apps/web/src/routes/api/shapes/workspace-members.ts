import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/workspace-members`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workspace_members`,
        getWhere: async (userId) => {
          const workspaceIds = await getReadableWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
