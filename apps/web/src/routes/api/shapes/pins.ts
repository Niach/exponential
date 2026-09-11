import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-778: the caller's personal pins — the sidebar's "Pinned" group on every
// client (issues, coding sessions, actions). Streams only the requesting
// user's rows; never anonymous. The where clause is fully STATIC per user
// (`user_id = me`, no membership id lists), so the shape identity never
// rotates — not even on team join/leave.
//
// Deliberately NOT team- or trash-scoped: the row is a pointer, and clients
// render only the pins of the active team whose TARGET they can resolve from
// the other shapes (issues / coding_sessions / actions), which carry the
// membership and trash predicates already. A pin on a trashed board's issue
// simply has no target until the board restores, and the purge cascade
// removes it. Every column is synced — nothing here is server-only.
export const Route = createFileRoute(`/api/shapes/pins`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `pins`,
        requireAuth: true,
        columns: [
          `id`,
          `user_id`,
          `team_id`,
          `kind`,
          `issue_id`,
          `session_id`,
          `action_id`,
          `sort_order`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
