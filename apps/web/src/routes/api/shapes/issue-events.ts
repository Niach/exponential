import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserBoardIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Activity-log timeline events. Members: board-scoped (board_id is
// denormalized from issue→board by a trigger and never null here) so a
// trashed board's events drop out of sync for the 48h trash window along
// with the board itself. Anonymous callers sync nothing.
export const Route = createFileRoute(`/api/shapes/issue-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_events`,
        getWhere: async (userId) => {
          if (userId) {
            const boardIds = await getUserBoardIds(userId)
            return buildWhereClause(`board_id`, boardIds)
          }
          return buildWhereClause(`board_id`, [])
        },
      }),
    },
  },
})
