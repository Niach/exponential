import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-1832 (fixture `sidebar-folders.json`): the caller's own sidebar layout
// (folders + the order of their boards, agents and apps). Like `pins` and the
// drafts, the where clause is the fully STATIC `user_id = me`: never
// anonymous, never team- or trash-scoped (a row only places a target the
// client already resolves from the member-only boards / agents / apps
// shapes; one that never resolves is skipped).
/** The synced columns. Checked against the schema by
 *  `shape-columns.test.ts`. */
export const SIDEBAR_LAYOUT_COLUMNS = [
  `id`,
  `user_id`,
  `team_id`,
  `section`,
  `kind`,
  `target_id`,
  `parent_id`,
  `name`,
  `sort_order`,
  `created_at`,
  `updated_at`,
] as const

export const Route = createFileRoute(`/api/shapes/sidebar-layout`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `sidebar_layout`,
        requireAuth: true,
        columns: [...SIDEBAR_LAYOUT_COLUMNS],
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
