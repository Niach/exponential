import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). Excludes the
// REV2-5 scoping columns `team_id` + `board_deleted_at` — server-only
// bookkeeping the where clause filters on (Electric evaluates `where`
// server-side, so a shape may filter on a column its allowlist excludes);
// native schemas don't carry them.
const ISSUE_COLUMNS = [
  `id`,
  `board_id`,
  `number`,
  `identifier`,
  `title`,
  `description`,
  `status`,
  `priority`,
  `assignee_id`,
  `creator_id`,
  `source`,
  `due_date`,
  `due_time`,
  `end_time`,
  `sort_order`,
  `completed_at`,
  `archived_at`,
  `duplicate_of_id`,
  `pr_url`,
  `pr_number`,
  `pr_state`,
  `branch`,
  `pr_merged_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issues`,
        columns: ISSUE_COLUMNS,
        getWhere: async (userId) => {
          // Members: team-scoped + trash-aware via the static
          // board_deleted_at predicate (REV2-5 — stable across board
          // create/trash). Anonymous: the empty team list yields the
          // impossible-match sentinel.
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
