import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// SLOP-3 compat sentinel: iOS <= 0.14.50, Android <= 0.14.51 and desktop/CLI <= 0.14.59 still subscribe.
// Delete once CLIENT_MIN_VERSION_IOS > 0.14.50, _ANDROID > 0.14.51 and _DESKTOP/_CLI > 0.14.59 (SLOP-14).
export const Route = createFileRoute(`/api/shapes/workflows`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `teams`,
        columns: [`id`],
        getWhere: async () => buildWhereClause(`id`, []),
      }),
    },
  },
})
