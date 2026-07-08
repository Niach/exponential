import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/comments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `comments`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          // Anonymous: comments of public feedback boards that opted into
          // showing them. Project-scoped (project_id is trigger-denormalized)
          // — workspace scoping would leak sibling projects of the host
          // workspace.
          const scope = await getPublicProjectScope()
          return buildWhereClause(`project_id`, scope.commentProjectIds)
        },
      }),
    },
  },
})
