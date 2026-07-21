import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the REV2-5 `board_deleted_at`
// trash mirror (server-only; the where clause filters on it).
const COMMENT_COLUMNS = [
  `id`,
  `issue_id`,
  `team_id`,
  `board_id`,
  `author_id`,
  `body`,
  `edited_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/comments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `comments`,
        columns: COMMENT_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash-aware (REV2-5) — a trashed board's
          // comments still drop out of sync for the 48h trash window via the
          // static board_deleted_at predicate, without the per-user board-id
          // list that rotated the shape identity on every board
          // create/trash. Anonymous: impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
