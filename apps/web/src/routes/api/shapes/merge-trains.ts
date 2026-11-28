import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-2214 (ci-page.json MERGE QUEUE TAB): the merge queue's runs,
// member-only by `team_id`: what is being tested, on which base commit, the
// attempt and the bisect halves still to go. `heads`, `run_ref`,
// `conflicts` and `error` stay server-only.
export const MERGE_TRAIN_COLUMNS = [
  `id`,
  `team_id`,
  `repository_id`,
  `base`,
  `base_sha`,
  `units`,
  `state`,
  `attempt`,
  `bisect_of`,
  `pending`,
  `untested`,
  `tested_sha`,
  `finished_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/merge-trains`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `merge_trains`,
        columns: MERGE_TRAIN_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
