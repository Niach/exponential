import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the REV2-5 `board_deleted_at`
// trash mirror (server-only; the where clause filters on it).
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
