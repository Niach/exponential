import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getPublicProjectScope,
  getUserWorkspaceIds,
} from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). `is_protected` is
// the client grey-out signal and MUST sync; `deleted_at` is always NULL inside
// the shape (the where excludes non-null) but keeping it satisfies the web
// selectProjectSchema. Pinning prevents any future server-only project column
// from leaking to native clients.
const PROJECT_COLUMNS = [
  `id`,
  `workspace_id`,
  `name`,
  `slug`,
  `prefix`,
  `color`,
  `is_public`,
  `icon`,
  `public_show_comments`,
  `public_show_activity`,
  `repository_id`,
  `sort_order`,
  `archived_at`,
  `deleted_at`,
  `is_protected`,
  `helpdesk_enabled`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/projects`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `projects`,
        columns: PROJECT_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const workspaceIds = await getUserWorkspaceIds(userId)
            // Trashed projects drop out of the members' shape. The suffix is a
            // static literal → byte-stable shape identity.
            return andClauses(
              buildWhereClause(`workspace_id`, workspaceIds),
              `"deleted_at" IS NULL`
            )
          }
          // Anonymous: only the public feedback-board projects themselves —
          // never sibling projects of the host workspace. The scope already
          // excludes trashed projects (getPublicProjectScope).
          const scope = await getPublicProjectScope()
          return buildWhereClause(`id`, scope.projectIds)
        },
      }),
    },
  },
})
