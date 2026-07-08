import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getPublicLabelIds,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `labels`,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            return buildWhereClause(`workspace_id`, workspaceIds)
          }
          // Anonymous: only labels actually used on a public board's issues —
          // a label-ID list, NOT workspace scoping, which would leak the host
          // workspace's whole label taxonomy (private workspaces can hold
          // sensitive label names). This is the one data-driven anonymous
          // clause (it rotates when a label is first used / last removed on a
          // public project); anonymous is web-only and the web collection
          // layer recovers from must-refetch. Never reuse for authed shapes.
          const labelIds = await getPublicLabelIds()
          return buildWhereClause(`id`, labelIds)
        },
      }),
    },
  },
})
