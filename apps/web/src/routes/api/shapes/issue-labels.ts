import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/issue-labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_labels`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          const scope = await getPublicProjectScope()
          return buildWhereClause(`project_id`, scope.projectIds)
        },
      }),
    },
  },
})
