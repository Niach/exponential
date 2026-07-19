import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserBoardIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Subscription rows, board-scoped (board_id is denormalized from
// issue→board by a trigger and never null here) so a trashed board's
// subscriptions drop out of sync for the 48h trash window along with the
// board itself. Clients use these to render the per-issue subscribe toggle
// with live state.
//
// `email` (widget reporter PII, masterplan §6.5 "Reporter PII stays
// owner-only") is deliberately excluded via the columns allowlist — no client
// reads subscriber emails from sync; the server-side notification fan-out
// reads them straight from the DB.
//
// Anonymous viewers get NOTHING (the subscribe toggle is member-only).
export const Route = createFileRoute(`/api/shapes/issue-subscribers`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_subscribers`,
        columns: [
          `id`,
          `issue_id`,
          `user_id`,
          `team_id`,
          `source`,
          `unsubscribed`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          const boardIds = userId ? await getUserBoardIds(userId) : []
          return buildWhereClause(`board_id`, boardIds)
        },
      }),
    },
  },
})
