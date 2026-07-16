import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getUserProjectIds,
  orClauses,
  sqlStringLiteral,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Per-user inbox feed. Streams only the requesting user's notification rows;
// never anonymous. The where clause is server-computed from the bearer token.
//
// Rows are additionally scoped to non-trashed projects the user can still
// reach via the trigger-denormalized project_id (REV-109): notifications of
// a soft-deleted project hide for the 48h trash window along with the
// project itself (and return on restore). The IS NULL arm is defensive — an
// issue-less notification carries no project identity and must never be
// silently dropped.
//
// `emailed_at` (the hourly digest sweep's server-side claim stamp) and
// `project_id` (trash-scoping bookkeeping, filtered on above) are
// deliberately excluded via the columns allowlist — neither is inbox state,
// and pinning them out keeps client row payloads unchanged.
export const Route = createFileRoute(`/api/shapes/notifications`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `notifications`,
        requireAuth: true,
        columns: [
          `id`,
          `user_id`,
          `issue_id`,
          `type`,
          `title`,
          `body`,
          `read_at`,
          `pushed_at`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          if (!userId) return null
          const projectIds = await getUserProjectIds(userId)
          return andClauses(
            `"user_id" = ${sqlStringLiteral(userId)}`,
            orClauses(
              `"project_id" IS NULL`,
              buildWhereClause(`project_id`, projectIds)
            )
          )
        },
      }),
    },
  },
})
