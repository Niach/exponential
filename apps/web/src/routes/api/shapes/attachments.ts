import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the `board_deleted_at` trash
// mirror (REV2-5) and the `board_archived_at` archive mirror (EXP-500), both
// server-only (the where clause filters on them).
const ATTACHMENT_COLUMNS = [
  `id`,
  `team_id`,
  `issue_id`,
  `board_id`,
  `comment_id`,
  `uploader_id`,
  `filename`,
  `content_type`,
  `size_bytes`,
  `storage_key`,
  `url`,
  `width`,
  `height`,
  // EXP-824: inline media. Clients render a player for `video/*` rows with
  // the duration chip and `?poster=1` when `poster_storage_key` is set.
  `duration_ms`,
  `poster_storage_key`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/attachments`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `attachments`,
        columns: ATTACHMENT_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash-aware (REV2-5) — a trashed board's
          // attachments still drop out of sync for the 48h trash window via
          // the static board_deleted_at predicate. Anonymous:
          // impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
