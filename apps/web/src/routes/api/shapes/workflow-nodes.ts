import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// old iOS (<floor) still subscribes; delete with SLOP-14
export const Route = createFileRoute(`/api/shapes/workflow-nodes`)({
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
