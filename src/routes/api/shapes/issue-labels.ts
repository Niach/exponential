import { createFileRoute } from "@tanstack/react-router"
import {
  getUserWorkspaceIds,
  buildWhereClause,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/issue-labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_labels`,
        getWhere: async (userId) => {
          const workspaceIds = await getUserWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
