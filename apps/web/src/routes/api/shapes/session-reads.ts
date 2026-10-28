import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1323: the caller's read marks of sessions (`sessions.markRead`), the
// Threads list's bold rows + nav dot. Like `pins`: fully STATIC per user
// (`user_id = me`), never team- or trash-scoped; a row renders only beside
// a synced session, which carries the visibility rule already.
export const SESSION_READ_COLUMNS = [
  `id`,
  `user_id`,
  `session_id`,
  `read_at`,
  `archived_at`,
  `story_seen_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/session-reads`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `session_reads`,
        requireAuth: true,
        columns: SESSION_READ_COLUMNS,
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
