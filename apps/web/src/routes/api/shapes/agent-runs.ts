import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// The agent's current run per issue (plan/question text + run bookkeeping),
// workspace-scoped (workspace_id is denormalized from issue→project by a trigger
// so this filter stays stable). Mirrors the access of agentPlan.getState: any
// workspace member or public-workspace viewer may read it.
export const Route = createFileRoute(`/api/shapes/agent-runs`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `agent_runs`,
        getWhere: async (userId) => {
          const workspaceIds = await getReadableWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
