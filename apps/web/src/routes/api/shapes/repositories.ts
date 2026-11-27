import { createFileRoute } from "@tanstack/react-router"
import { andClauses, buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-2171 (sidebar-tree.json `teamItems`): the team's repositories, the
// 34th shape, member-only by `team_id`. The allowlist keeps the server's
// columns behind it: `provider_config` (the GitHub installation),
// `remote_url`, `ci_config`, `pr_sequence`, and `archived_at` (archived
// rows drop out by the static where, like boards). Checked against the
// schema by `shape-columns.test.ts`.
export const REPOSITORY_COLUMNS = [
  `id`,
  `team_id`,
  `provider`,
  `full_name`,
  `slug`,
  `description`,
  `sync`,
  `default_branch`,
  `default_branch_override`,
  `private`,
  `inaccessible_at`,
  `shared_by_user_id`,
  `sort_order`,
  `created_at`,
  `updated_at`,
] as const

export const Route = createFileRoute(`/api/shapes/repositories`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `repositories`,
        columns: [...REPOSITORY_COLUMNS],
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return andClauses(buildWhereClause(`team_id`, teamIds), `"archived_at" IS NULL`)
        },
      }),
    },
  },
})
