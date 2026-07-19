import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). `is_protected` is
// the client grey-out signal and MUST sync; `deleted_at` is always NULL inside
// the shape (the where excludes non-null) but keeping it satisfies the web
// selectBoardSchema. Pinning prevents any future server-only board column
// from leaking to native clients.
const BOARD_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `slug`,
  `prefix`,
  `color`,
  `icon`,
  `repository_id`,
  `sort_order`,
  `archived_at`,
  `deleted_at`,
  `is_protected`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/boards`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `boards`,
        columns: BOARD_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            // Trashed boards drop out of the members' shape. The suffix is a
            // static literal → byte-stable shape identity.
            return andClauses(
              buildWhereClause(`team_id`, teamIds),
              `"deleted_at" IS NULL`
            )
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
