import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the REV2-5 `board_deleted_at`
// trash mirror (server-only; the where clause filters on it). issue_labels
// has no timestamps; the composite PK columns are both included.
const ISSUE_LABEL_COLUMNS = [`issue_id`, `label_id`, `team_id`, `board_id`]

export const Route = createFileRoute(`/api/shapes/issue-labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_labels`,
        columns: ISSUE_LABEL_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash-aware (REV2-5) — a trashed board's
          // label links still drop out of sync for the 48h trash window via
          // the static board_deleted_at predicate. Anonymous:
          // impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
