import { createFileRoute } from "@tanstack/react-router"
import {
  getUserWorkspaceIds,
  buildWhereClause,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Pending-invite rows, workspace-scoped, member-visible (they power the
// "Pending" list in workspace settings on all four clients).
//
// The bearer `token` is deliberately excluded via the columns allowlist — an
// invite token grants membership at the invite's role to WHOEVER presents it
// (accept is not recipient-bound, so a synced owner-role token would let any
// member escalate to owner via a second account). Members must never sync it;
// owners receive it exactly once, from `workspaceInvites.create`. The static
// array keeps the columns param byte-identical per request (shape-identity
// stability); the one-time rotation when this allowlist ships is expected —
// clients 409 → refetch the (tiny) table.
export const Route = createFileRoute(`/api/shapes/workspace-invites`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `workspace_invites`,
        columns: [
          `id`,
          `workspace_id`,
          `invited_by_id`,
          `role`,
          `accepted_at`,
          `expires_at`,
          `created_at`,
          `updated_at`,
        ],
        requireAuth: true,
        getWhere: async (userId) => {
          if (!userId) return null
          const workspaceIds = await getUserWorkspaceIds(userId)
          return buildWhereClause(`workspace_id`, workspaceIds)
        },
      }),
    },
  },
})
