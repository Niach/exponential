import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-583: automations are their own entity (split out of actions.trigger).
// Team-scoped like `actions`; the bound device's host selects its own rows
// (device_id = me AND enabled) off this shape — there is no server scheduler.
// Server-pinned allowlist: every column is client-relevant today, so this is
// the full row; a future server-only column goes BEHIND it.
const AUTOMATION_COLUMNS = [
  `id`,
  `team_id`,
  `action_id`,
  `device_id`,
  `enabled`,
  `trigger`,
  `agent`,
  `model`,
  `effort`,
  `sort_order`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/automations`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `automations`,
        columns: AUTOMATION_COLUMNS,
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
