import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — EXCLUDES `body` (EXP-268): the ≤64KB
// markdown prompt never rides sync. Clients list actions from this shape and
// fetch the body via tRPC `actions.get` on demand. EXP-1291: actions are
// SAVED PROMPTS (no repository, description, triggers or MCP list).
const ACTION_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `icon`,
  `inputs`,
  // EXP-825: the composer hint.
  `prompt_placeholder`,
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
