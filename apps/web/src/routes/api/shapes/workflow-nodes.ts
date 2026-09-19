import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-981: the nodes of every workflow of the member's teams. `team_id` is
// denormalized onto the row for exactly this clause (an id-list of workflows
// would rotate the shape identity on every create). The full row is
// client-relevant: `wave`/`lane`/`on_cycle` ARE the server-computed layout.
export const WORKFLOW_NODE_COLUMNS = [
  `id`,
  `workflow_id`,
  `team_id`,
  `issue_id`,
  `member_issue_ids`,
  `kind`,
  `state`,
  `risk`,
  `wave`,
  `lane`,
  `on_cycle`,
  `session_id`,
  `attempt`,
  `base_branch`,
  `approved_at`,
  `checkpoint_at`,
  `after_node_ids`,
  `review_round`,
  `review`,
  `note`,
  `budget`,
  `touches`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/workflow-nodes`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workflow_nodes`,
        columns: WORKFLOW_NODE_COLUMNS,
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
