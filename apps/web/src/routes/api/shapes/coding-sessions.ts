import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Live "coding now" rows. Members: workspace-scoped (workspace_id is
// denormalized from issue→project by a trigger). Anonymous: only public
// feedback boards whose publicShowCoding is 'badge' or 'live' — powers the
// public "coding now" badge (deviceLabel IS the badge content; user identity
// stays anonymized client-side because the users shape never syncs).
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          const scope = await getPublicProjectScope()
          return buildWhereClause(`project_id`, scope.codingProjectIds)
        },
      }),
    },
  },
})
