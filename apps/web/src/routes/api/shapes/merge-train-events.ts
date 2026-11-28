import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-2214 (ci-page.json MERGE QUEUE TAB): the merge queue's history
// (Landed, Sent back), member-only by `team_id`. Every column is display data.
export const MERGE_TRAIN_EVENT_COLUMNS = [
  `id`,
  `team_id`,
  `repository_id`,
  `pull_request_id`,
  `merge_train_id`,
  `kind`,
  `base`,
  `attempt`,
  `reason`,
  `checks`,
  `ci_job_id`,
  `job_url`,
  `files`,
  `sha`,
  `by_user_id`,
  `created_at`,
]

export const Route = createFileRoute(`/api/shapes/merge-train-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `merge_train_events`,
        columns: MERGE_TRAIN_EVENT_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
