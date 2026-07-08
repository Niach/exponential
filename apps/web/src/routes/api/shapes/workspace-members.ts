import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Membership rosters sync only to members. Anonymous public-board viewers get
// NOTHING — a public board must not expose who runs the workspace (user ids +
// roles), tighter than the old public-workspace behavior.
export const Route = createFileRoute(`/api/shapes/workspace-members`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workspace_members`,
        getWhere: async (userId) => {
          const workspaceIds = userId ? await getUserWorkspaceIds(userId) : []
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
