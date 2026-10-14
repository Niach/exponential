import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — a thread's issues (always on the ROOT). Visible like the
// thread (`shared` / `owner_id` mirrors) and hidden with the issue's board
// (static trash/archive predicates). Mirrors stay out of the allowlist.
export const SESSION_ISSUE_COLUMNS = [
  `id`,
  `session_id`,
  `issue_id`,
  `team_id`,
  `source`,
  `added_by`,
  `added_by_session_id`,
  `sort_order`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/session-issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `session_issues`,
        columns: SESSION_ISSUE_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return andClauses(
            buildSessionVisibleWhere(teamIds, userId, `owner_id`),
            `"board_deleted_at" IS NULL`,
            `"board_archived_at" IS NULL`
          )
        },
      }),
    },
  },
})
