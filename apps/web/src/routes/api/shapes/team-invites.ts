import { createFileRoute } from "@tanstack/react-router"
import {
  getUserTeamIds,
  buildWhereClause,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Pending-invite rows, team-scoped, member-visible (they power the
// "Pending" list in team settings on all four clients).
//
// The bearer `token` is deliberately excluded via the columns allowlist — an
// invite token grants membership at the invite's role to WHOEVER presents it
// (accept is not recipient-bound, so a synced owner-role token would let any
// member escalate to owner via a second account). Members must never sync it;
// owners receive it exactly once, from `teamInvites.create`. The static
// array keeps the columns param byte-identical per request (shape-identity
// stability); the one-time rotation when this allowlist ships is expected —
// clients 409 → refetch the (tiny) table.
export const Route = createFileRoute(`/api/shapes/team-invites`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `team_invites`,
        columns: [
          `id`,
          `team_id`,
          `invited_by_id`,
          `role`,
          `accepted_at`,
          `expires_at`,
          `created_at`,
          `updated_at`,
        ],
        requireAuth: true,
        getWhere: async (userId) => {
          if (!userId) return null
          const teamIds = await getUserTeamIds(userId)
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
