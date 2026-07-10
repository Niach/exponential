import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). These are exactly
// the columns every native client stores. Pinning keeps server-only columns
// (e.g. `comp_tier`) off the wire — an unallowlisted synced column bricks
// native sync loops.
const WORKSPACE_COLUMNS = [
  `id`,
  `name`,
  `slug`,
  `icon_url`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/workspaces`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workspaces`,
        columns: WORKSPACE_COLUMNS,
        getWhere: async (userId) => {
          const workspaceIds = await getReadableWorkspaceIds(userId)
          return buildWhereClause(`id`, workspaceIds)
        },
      }),
    },
  },
})
