import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-1617: eCode's CI jobs, member-only by `team_id`. Server-only and
// outside this list: `spec` (scripts, env, secret names), the job token's
// hash, the dispatch/beat stamps, the queued flag and the log chunk index.
// The log itself is never synced (`ci.log` reads it on demand).
export const CI_JOB_COLUMNS = [
  `id`,
  `team_id`,
  `run_id`,
  `key`,
  `check`,
  `position`,
  `runs_on`,
  `needs`,
  `matrix`,
  `timeout_min`,
  `status`,
  `conclusion`,
  `reason`,
  `runner_device_id`,
  `steps`,
  `log_key`,
  `log_bytes`,
  `queued_at`,
  `started_at`,
  `completed_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/ci-jobs`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `ci_jobs`,
        columns: CI_JOB_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
