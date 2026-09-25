import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1082 §3: the engine's event log, one short line per decision outcome,
// trimmed to the newest `contract.workflow.eventsMax` per workflow by
// `workflows.appendEvent`. Team-scoped like `workflow_nodes` (`team_id` is
// denormalized onto the row for exactly this clause). Every column is
// client-relevant: `WorkflowEventList` ×4 renders kind + message + time and
// links the node and the session.
export const WORKFLOW_EVENT_COLUMNS = [
  `id`,
  `workflow_id`,
  `team_id`,
  `node_id`,
  `session_id`,
  `at`,
  `kind`,
  `message`,
]

export const Route = createFileRoute(`/api/shapes/workflow-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workflow_events`,
        columns: WORKFLOW_EVENT_COLUMNS,
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
