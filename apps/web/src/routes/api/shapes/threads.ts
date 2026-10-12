import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { andClauses } from "@/lib/auth/shape-where"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1284 — the thread shells (the 22nd shape). Team-scoped and
// member-visible: `team_id IN (member teams)` plus the STATIC
// `has_visible_items = true` (review T3): a thread whose items all sit on
// trashed or archived boards (its ITEMS carry the board mirrors; the
// `refresh_thread_visibility` trigger keeps the flag) leaves sync with them,
// Guide included. The flag is filtered on, never streamed, and the identity
// still rotates only on membership changes. `results` = the thread's ONE
// Guide. Server-pinned allowlist like every proxy (REV-49).
export const THREAD_COLUMNS = [
  `id`,
  `team_id`,
  `title`,
  `created_by`,
  `results`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/threads`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `threads`,
        columns: THREAD_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return andClauses(
              buildWhereClause(`team_id`, teamIds),
              `"has_visible_items" = true`
            )
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
