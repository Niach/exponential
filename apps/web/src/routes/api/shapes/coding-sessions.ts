import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Live "coding now" rows, workspace-scoped (workspace_id is denormalized from
// issue→project by a trigger). MEMBER-ONLY since EXP-90: anonymous
// feedback-board viewers get NOTHING (the empty id list yields the
// impossible-match sentinel — zero rows, no 401, same posture as releases).
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        getWhere: async (userId) => {
          const workspaceIds = userId ? await getUserWorkspaceIds(userId) : []
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
