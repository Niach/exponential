import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableUserIdsInWorkspaces,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/users`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `users`,
        // The users shape syncs FULL rows (including email), so its scope is
        // membership-only: co-members of workspaces the caller has joined.
        // Public-workspace viewers who aren't members get no user rows —
        // see getReadableUserIdsInWorkspaces for the rationale.
        getWhere: async (userId) => {
          const sharedUserIds = await getReadableUserIdsInWorkspaces(userId)
          return buildWhereClause(`id`, sharedUserIds)
        },
      }),
    },
  },
})
