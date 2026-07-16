import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicProjectScope,
  getUserProjectIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/comments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `comments`,
        getWhere: async (userId) => {
          if (userId) {
            // Members: project-scoped so a trashed project's comments drop out
            // of sync for the 48h trash window along with the project itself
            // (project_id is trigger-denormalized and never null here).
            const projectIds = await getUserProjectIds(userId)
            return buildWhereClause(`project_id`, projectIds)
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
