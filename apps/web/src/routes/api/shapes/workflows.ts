import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-981: workflows are team-scoped like `actions` (a workflow spans boards,
// so the board trash rules do NOT apply). The runner device's engine reads
// its own rows (`device_id = me`) off this shape — there is no server
// scheduler. Server-pinned allowlist: `creator_id` stays behind it.
export const WORKFLOW_COLUMNS = [
  `id`,
  `team_id`,
  `repository_id`,
  `name`,
  `status`,
  `device_id`,
  `launch`,
  `gate`,
  `start_on`,
  `integration_branch`,
  `final_pr_url`,
  `final_pr_number`,
  `final_pr_state`,
  `decisions`,
  `metrics`,
  `started_at`,
  `ended_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/workflows`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workflows`,
        columns: WORKFLOW_COLUMNS,
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
