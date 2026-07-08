import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
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
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
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
