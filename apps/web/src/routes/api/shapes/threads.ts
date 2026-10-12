import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1284 — the thread shells (the 22nd shape). Team-scoped and
// member-visible: `team_id IN (member teams)`, no board and no trash/archive
// predicate (a thread spans boards; its issue ITEMS carry the mirrors), so
// the identity rotates only on membership changes. `results` = the thread's
// ONE Guide. Server-pinned allowlist like every proxy (REV-49).
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
            return buildWhereClause(`team_id`, teamIds)
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
