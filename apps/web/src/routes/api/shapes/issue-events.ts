import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the `board_deleted_at` trash
// mirror (REV2-5) and the `board_archived_at` archive mirror (EXP-500), both
// server-only (the where clause filters on them).
const ISSUE_EVENT_COLUMNS = [
  `id`,
  `issue_id`,
  `team_id`,
  `board_id`,
  `actor_user_id`,
  `type`,
  `payload`,
  `created_at`,
  `updated_at`,
]

// Activity-log timeline events. Members: team-scoped + trash-aware (REV2-5)
// — a trashed board's events still drop out of sync for the 48h trash window
// via the static board_deleted_at predicate, without the per-user board-id
// list that rotated the shape identity on every board create/trash.
// Anonymous callers sync nothing.
export const Route = createFileRoute(`/api/shapes/issue-events`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_events`,
        columns: ISSUE_EVENT_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
