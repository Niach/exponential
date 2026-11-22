import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// A shape this server no longer serves: an older build still subscribes to
// it, so it answers an EMPTY shape (an impossible match on `teams`) rather
// than a 404 that would stall that build's sync.
export const Route = createFileRoute(`/api/shapes/$`)({
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
