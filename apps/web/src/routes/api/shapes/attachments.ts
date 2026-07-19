import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserBoardIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/attachments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `attachments`,
        getWhere: async (userId) => {
          if (userId) {
            // Members: board-scoped so a trashed board's attachments drop
            // out of sync for the 48h trash window along with the board
            // itself (board_id is trigger-denormalized and never null here).
            const boardIds = await getUserBoardIds(userId)
            return buildWhereClause(`board_id`, boardIds)
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`board_id`, [])
        },
      }),
    },
  },
})
