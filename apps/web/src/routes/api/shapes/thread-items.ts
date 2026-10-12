import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1284 — a thread's children (the 23rd shape): n issues + m runs. Scoped
// like every issue child (REV2-5, EXP-500): team + the static
// `board_deleted_at`/`board_archived_at IS NULL` suffixes, so an issue on a
// trashed or archived board drops out, and so does a run item: it mirrors
// its SESSION's board context (a batch/chat/action session's is NULL and
// always syncs). The two mirrors are server-only — filtered on, never
// streamed.
export const THREAD_ITEM_COLUMNS = [
  `id`,
  `thread_id`,
  `kind`,
  `issue_id`,
  `session_id`,
  `team_id`,
  `board_id`,
  `added_by`,
  `added_by_session_id`,
  `source`,
  `sort_order`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/thread-items`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `thread_items`,
        columns: THREAD_ITEM_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash/archive-aware. Anonymous:
          // impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
