import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Membership rosters sync only to members. Anonymous callers get NOTHING
// (the impossible-match sentinel) — like every shape since EXP-180.
export const Route = createFileRoute(`/api/shapes/team-members`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `team_members`,
        // Server-pinned allowlist (REV-49): currently the full table, pinned
        // anyway so a future server-only column (e.g. invite provenance)
        // ships BEHIND it instead of silently reaching every client.
        columns: [
          `id`,
          `team_id`,
          `user_id`,
          `role`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
