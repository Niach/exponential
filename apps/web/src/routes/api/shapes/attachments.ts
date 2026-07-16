import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getPublicProjectScope,
  getUserProjectIds,
  orClauses,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/attachments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `attachments`,
        getWhere: async (userId) => {
          if (userId) {
            // Members: project-scoped so a trashed project's attachments drop
            // out of sync for the 48h trash window along with the project
            // itself (project_id is trigger-denormalized and never null here).
            const projectIds = await getUserProjectIds(userId)
            return buildWhereClause(`project_id`, projectIds)
          }
          // Anonymous: issue/description attachments of every public board;
          // comment attachments only where comments are public. (The byte
          // route applies the same predicate — this shape is metadata only.)
          const scope = await getPublicProjectScope()
          return orClauses(
            andClauses(
              buildWhereClause(`project_id`, scope.projectIds),
              `"comment_id" IS NULL`
            ),
            buildWhereClause(`project_id`, scope.commentProjectIds)
          )
        },
      }),
    },
  },
})
