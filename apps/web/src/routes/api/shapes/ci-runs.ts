import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-1617: eCode's CI runs, member-only by `team_id`. Every column is
// display data (the run page, the repo's Runs tab).
export const CI_RUN_COLUMNS = [
  `id`,
  `team_id`,
  `repository_id`,
  `number`,
  `name`,
  `sha`,
  `ref`,
  `trigger`,
  `pull_request_id`,
  `state`,
  `error`,
  `rerun_of`,
  `created_by_user_id`,
  `started_at`,
  `completed_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/ci-runs`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `ci_runs`,
        columns: CI_RUN_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
