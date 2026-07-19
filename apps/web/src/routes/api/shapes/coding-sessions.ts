import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getUserProjectIds,
  getUserWorkspaceIds,
  orClauses,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Live "coding now" rows. MEMBER-ONLY: anonymous callers get NOTHING (the
// anonymous branch's empty id list yields the impossible-match sentinel —
// zero rows, no 401).
//
// Members sync sessions in their workspaces that are either batch-scoped
// (project_id NULL — a batch run spans projects and carries no project
// identity) or belong to a non-trashed project. Issue-scoped sessions of a
// trashed project therefore hide for the 48h trash window along with the
// project itself. workspace_id is trigger-denormalized, as is project_id
// except on batch rows where it is deliberately null.
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        getWhere: async (userId) => {
          // Anonymous clause stays byte-identical via this early return.
          if (!userId) return buildWhereClause(`workspace_id`, [])
          const workspaceIds = await getUserWorkspaceIds(userId)
          const projectIds = await getUserProjectIds(userId)
          return andClauses(
            buildWhereClause(`workspace_id`, workspaceIds),
            orClauses(
              `"project_id" IS NULL`,
              buildWhereClause(`project_id`, projectIds)
            )
          )
        },
      }),
    },
  },
})
