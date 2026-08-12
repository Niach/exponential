import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getUserTeamIds,
  orClauses,
  sqlStringLiteral,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-481: the 18th synced shape — per-device worktree inventory, reported by
// the device (devices.reportWorktrees). Powers resume offers, the
// device-settings worktree list, and prune UX even while the device is
// offline. The allowlist EXCLUDES the trigger-maintained scoping mirrors
// (`user_id`, `shared_team_id` — populate_device_worktree_owner /
// propagate_device_shared_team); Electric evaluates the where clause
// server-side. No `kind` arm needed here: the trigger's CASE guarantees only
// server devices ever carry a shared_team_id mirror. Identity rotates ONLY
// on team-membership changes — never a device-id list (the forbidden
// pattern; exactly what the denormalized mirrors buy).
const DEVICE_WORKTREE_COLUMNS = [
  `id`,
  `device_row_id`,
  `repo_full_name`,
  `branch`,
  `issue_identifier`,
  `agents`,
  `dirty`,
  `busy`,
  `reported_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/device-worktrees`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `device_worktrees`,
        requireAuth: true,
        columns: DEVICE_WORKTREE_COLUMNS,
        getWhere: async (userId) => {
          if (!userId) return null
          const teamIds = await getUserTeamIds(userId)
          return orClauses(
            `"user_id" = ${sqlStringLiteral(userId)}`,
            buildWhereClause(`shared_team_id`, teamIds)
          )
        },
      }),
    },
  },
})
