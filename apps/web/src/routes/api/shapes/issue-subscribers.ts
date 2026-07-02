import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Subscription rows, workspace-scoped (workspace_id is denormalized from
// issue→project by a trigger). Clients use these to render the per-issue
// subscribe toggle with live state.
//
// `email` (widget reporter PII, masterplan §6.5 "Reporter PII stays
// owner-only") is deliberately excluded via the columns allowlist: this shape
// is readable by anonymous visitors of public workspaces, and no client reads
// subscriber emails from sync — the server-side notification fan-out reads
// them straight from the DB.
export const Route = createFileRoute(`/api/shapes/issue-subscribers`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_subscribers`,
        columns: [
          `id`,
          `issue_id`,
          `user_id`,
          `workspace_id`,
          `source`,
          `unsubscribed`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          const workspaceIds = await getReadableWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
