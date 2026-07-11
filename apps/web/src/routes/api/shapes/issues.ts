import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableProjectIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Anonymous viewers of public feedback boards must not learn the backing
// repo/branch names (a feedback board may be backed by a PRIVATE repo):
// pr_url, pr_number and branch are excluded from their columns allowlist.
// pr_state + pr_merged_at stay so the board can show a "shipped" signal
// without exposing where the code lives. Members sync full rows (no allowlist).
// `release_id` is intentionally absent: releases are member-only (their shape
// never syncs anonymously), so the FK would be a dangling internal id — do not
// "helpfully" add it here.
const ANONYMOUS_COLUMNS = [
  `id`,
  `project_id`,
  `number`,
  `identifier`,
  `title`,
  `description`,
  `status`,
  `priority`,
  `assignee_id`,
  `creator_id`,
  `due_date`,
  `due_time`,
  `end_time`,
  `sort_order`,
  `completed_at`,
  `archived_at`,
  `recurrence_interval`,
  `recurrence_unit`,
  `duplicate_of_id`,
  `pr_state`,
  `pr_merged_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issues`,
        columns: (userId) => (userId ? undefined : ANONYMOUS_COLUMNS),
        getWhere: async (userId) => {
          // Members: their projects. Anonymous: public feedback-board projects.
          const projectIds = await getReadableProjectIds(userId)
          return buildWhereClause(`project_id`, projectIds)
        },
      }),
    },
  },
})
