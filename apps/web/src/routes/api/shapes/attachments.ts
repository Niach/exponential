import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserProjectIds,
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
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`project_id`, [])
        },
      }),
    },
  },
})
