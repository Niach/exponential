import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1610 — CI on every PR of the member's teams: one row per check at the
// PR's head commit (the checks list, the Merge pill). Team-visible like
// `pull_requests`. Every column syncs but `queued_flagged_at` (the
// watchdog's once-only flag, server-only).
export const PULL_REQUEST_CHECK_COLUMNS = [
  `id`,
  `team_id`,
  `pull_request_id`,
  `head_sha`,
  `name`,
  `status`,
  `conclusion`,
  `required`,
  `started_at`,
  `completed_at`,
  `details_url`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/pull-request-checks`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `pull_request_checks`,
        columns: PULL_REQUEST_CHECK_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
