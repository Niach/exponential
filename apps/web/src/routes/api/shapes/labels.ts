import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `labels`,
        // Server-pinned allowlist (REV-49): currently the full table, pinned
        // anyway so a future server-only column ships BEHIND it instead of
        // silently reaching (and bricking) every native client's sync loop.
        columns: [
          `id`,
          `team_id`,
          `name`,
          `color`,
          `sort_order`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return buildWhereClause(`team_id`, teamIds)
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
