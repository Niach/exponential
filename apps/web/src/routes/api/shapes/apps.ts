import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// VAPP-110: the team's APPS. Member-only, team-scoped, `deleted_at` is the
// trash mirror (clients hide trashed rows). The HEAD version's `package`
// rides along: the editor's live preview repaints from the synced row on
// every upsert (the package is capped by contract `apps.packageMaxBytes`).
export const APP_COLUMNS = [
  `id`,
  `team_id`,
  `slug`,
  `name`,
  `icon`,
  `color`,
  `description`,
  `package`,
  `head_version`,
  `created_by`,
  `deleted_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/apps`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `apps`,
        columns: APP_COLUMNS,
        getWhere: async (userId) => {
          if (userId) {
            const teamIds = await getUserTeamIds(userId)
            return buildWhereClause(`team_id`, teamIds)
          }
          return buildWhereClause(`id`, [])
        },
      }),
    },
  },
})
