import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { andClauses } from "@/lib/auth/shape-where"
import { createShapeRouteHandler } from "@/lib/shape-route"
import { appRecordClauses, appShapeRules } from "@/lib/apps/records-shape"

// VAPP-122: an app's RECORDS (its own data). Member-only, team-scoped; the
// two trash predicates are STATIC literals (the record's own `deleted_at`,
// the trigger-kept `app_deleted_at` mirror of its app), so a trashed record
// or a trashed app's records never reach a client and the shape identity
// stays byte-stable. The mirror column itself stays behind the allowlist.
// eApps R3: a member at app access none syncs none of the app's rows, and
// a kind with `read: author` / `owners` syncs only what record-policy.json
// lets them read (lib/apps/records-shape.ts).
// VAPP-159: the web loads it ON DEMAND, one app's subset at a time
// (app-records.json SYNC); the natives keep the whole shape.
export const APP_RECORD_COLUMNS = [
  `id`,
  `app_id`,
  `team_id`,
  `kind`,
  `data`,
  `created_by`,
  // VAPP-187: whose record it is + the last writer (ownership).
  `author_id`,
  `updated_by`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/app-records`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `app_records`,
        columns: APP_RECORD_COLUMNS,
        subsets: true,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            // eApps R3: no hidden app's rows, and a private kind's only where the policy lets the member read.
            const rules = appRecordClauses(userId, await appShapeRules(userId, teamIds))
            return andClauses(buildWhereClause(`team_id`, teamIds), `"deleted_at" IS NULL`, `"app_deleted_at" IS NULL`, ...rules)
          }
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
