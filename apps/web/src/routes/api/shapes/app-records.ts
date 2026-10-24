import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { andClauses } from "@/lib/auth/shape-where"
import { createShapeRouteHandler } from "@/lib/shape-route"

// VAPP-122: an app's RECORDS (its own data). Member-only, team-scoped; the
// two trash predicates are STATIC literals (the record's own `deleted_at`,
// the trigger-kept `app_deleted_at` mirror of its app), so a trashed record
// or a trashed app's records never reach a client and the shape identity
// stays byte-stable. The mirror column itself stays behind the allowlist.
export const APP_RECORD_COLUMNS = [
  `id`,
  `app_id`,
  `team_id`,
  `kind`,
  `data`,
  `created_by`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/app-records`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `app_records`,
        columns: APP_RECORD_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return andClauses(buildWhereClause(`team_id`, teamIds), `"deleted_at" IS NULL`, `"app_deleted_at" IS NULL`)
          }
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
