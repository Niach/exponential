import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Per-user inbox feed. Streams only the requesting user's notification rows;
// never anonymous. The where clause is server-computed from the bearer token.
export const Route = createFileRoute(`/api/shapes/notifications`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `notifications`,
        requireAuth: true,
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
