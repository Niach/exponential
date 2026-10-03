import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the `board_deleted_at` trash
// mirror (REV2-5) and the `board_archived_at` archive mirror (EXP-500), both
// server-only (the where clause filters on them), and the SLOP-4
// `email_delivery_id` audit stamp. EXP-741 adds the reply parent and the
// user|mcp|reporter source; SLOP-4 the team|reporter audience (and a
// NULLABLE author_id for reporter rows); every client's local schema
// carries all of them.
const COMMENT_COLUMNS = [
  `id`,
  `issue_id`,
  `team_id`,
  `board_id`,
  `author_id`,
  `parent_id`,
  `source`,
  `audience`,
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
