import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-736 — the 20th shape. Server-pinned column allowlist: excludes the
// `board_deleted_at` trash mirror (REV2-5) and the `board_archived_at`
// archive mirror (EXP-500), both server-only (the where clause filters on
// them). Rows are scoped by their SOURCE issue's board, so a client queries
// `issue_id = me OR related_issue_id = me` and hides rows whose far side is
// not synced.
const ISSUE_RELATION_COLUMNS = [
  `id`,
  `issue_id`,
  `related_issue_id`,
  `type`,
  `source`,
  `team_id`,
  `board_id`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/issue-relations`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_relations`,
        columns: ISSUE_RELATION_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash/archive-aware (REV2-5, EXP-500) —
          // the identity rotates only on team-membership changes. Anonymous:
          // impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
