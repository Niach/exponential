import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). `deleted_at` is
// always NULL inside the shape (the where excludes non-null) but keeping it
// satisfies the web selectBoardSchema. Pinning prevents any future
// server-only board column from leaking to native clients — `archived_at`
// (EXP-500) is deliberately NOT here: it is server-only bookkeeping the where
// clause filters on (Electric evaluates `where` server-side). Syncing it and
// asking every client to filter is exactly what the first archiving attempt
// did before it was deleted for leaking (REV2-103).
const BOARD_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `slug`,
  `prefix`,
  `color`,
  `icon`,
  `repository_id`,
  `default_branch`,
  `sort_order`,
  `deleted_at`,
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
            // Trashed AND archived boards drop out of the members' shape. Both
            // suffixes are static literals → byte-stable shape identity.
            return andClauses(
              buildWhereClause(`team_id`, teamIds),
              `"deleted_at" IS NULL`,
              `"archived_at" IS NULL`
            )
          }
          // Anonymous callers sync nothing (impossible-match sentinel).
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
