import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/projects`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `projects`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          // Anonymous: only the public feedback-board projects themselves —
          // never sibling projects of the host workspace.
          const scope = await getPublicProjectScope()
          return buildWhereClause(`id`, scope.projectIds)
        },
      }),
    },
  },
})
