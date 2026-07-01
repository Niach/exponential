import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Live "coding now" rows, workspace-scoped (workspace_id is denormalized from
// issue→project by a trigger). Clients use these to render the coding-session
// badge + Watch/Steer button on issues with a running desktop session.
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        getWhere: async (userId) => {
          const workspaceIds = await getReadableWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
