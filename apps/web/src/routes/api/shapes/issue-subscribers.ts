import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Subscription rows, team-scoped + trash-aware via the static
// board_deleted_at predicate (REV2-5) so a trashed board's subscriptions
// drop out of sync for the 48h trash window without a per-user board-id
// where clause. Clients use these to render the per-issue subscribe toggle
// with live state.
//
// `email` (widget reporter PII, masterplan §6.5 "Reporter PII stays
// owner-only") is deliberately excluded via the columns allowlist — no client
// reads subscriber emails from sync; the server-side notification fan-out
// reads them straight from the DB. `board_deleted_at` (the REV2-5 trash
// mirror) is excluded the same way.
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
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
