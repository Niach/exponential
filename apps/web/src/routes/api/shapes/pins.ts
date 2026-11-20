import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-2027 compat sentinel: the Pins feature (EXP-778) and its `pins` table
// are gone, but iOS, Android and desktop builds from before the removal still
// subscribe to this shape. Like the SLOP-3 workflow sentinels it answers an
// EMPTY shape (an impossible match on `teams`) instead of a 404, so an old
// build's sync never stalls; its pin rows simply vanish. Delete once every
// CLIENT_MIN_VERSION_* floor is past the first release without pins.
export const Route = createFileRoute(`/api/shapes/pins`)({
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
