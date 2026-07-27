import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-314 — the 16th synced shape: per-team issue statuses. Team-scoped like
// labels (statuses aren't board children, so no trash predicate). The column
// allowlist is pinned even though every column syncs today, so a future
// server-only column can't reach the wire by accident.
const ISSUE_STATUS_COLUMNS = [
  `id`,
  `team_id`,
  `category`,
  `name`,
  `color`,
  `sort_order`,
  `builtin_key`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/issue-statuses`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_statuses`,
        columns: ISSUE_STATUS_COLUMNS,
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
