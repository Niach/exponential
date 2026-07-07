import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Per-user inbox feed. Streams only the requesting user's notification rows;
// never anonymous. The where clause is server-computed from the bearer token.
//
// `emailed_at` (the hourly digest sweep's server-side claim stamp) is
// deliberately excluded via the columns allowlist — it's delivery
// bookkeeping, not inbox state, and pinning it out keeps client row payloads
// unchanged across the digest rollout.
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
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
