import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — EXCLUDES `body` (EXP-268): the ≤64KB
// markdown prompt never rides sync. Clients list actions from this shape and
// fetch the body via tRPC `actions.get` on demand (editors on open, the
// desktop right before a run).
// `icon` was appended for EXP-273 — a ONE-TIME shape-identity rotation
// (benign: small table, full resync; land in one deploy). Old native builds
// drop unknown columns safely (verified: iOS filters to its SQLite schema,
// Android ignoreUnknownKeys + partial-plan filter, desktop serde non-strict),
// so this needs no CLIENT_MIN_VERSION bump — same shape as the coding-sessions
// `action_id`/`action_name` addition. EXP-530's `trigger` column was dropped
// again in EXP-583 — automations are their own shape now.
const ACTION_COLUMNS = [
  `id`,
  `team_id`,
  `repository_id`,
  `name`,
  `description`,
  `icon`,
  `inputs`,
  `sort_order`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/actions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `actions`,
        columns: ACTION_COLUMNS,
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
