import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// NEW-2: the team's event stream (listed on the Agents page; agent triggers
// listen on a row's `name`). Team-scoped, member-visible, pruned after
// contract `events.retentionDays`.
export const EVENT_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `payload`,
  `emitted_by`,
  `session_id`,
  `user_id`,
  `created_at`,
]

export const Route = createFileRoute(`/api/shapes/events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `events`,
        columns: EVENT_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return buildWhereClause(`team_id`, teamIds)
          }
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
