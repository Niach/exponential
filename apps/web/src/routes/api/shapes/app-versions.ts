import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// VAPP-110: an app's VERSIONS (the editor's Versions list). The allowlist
// EXCLUDES `package`: an old version's package loads on demand (tRPC
// `apps.version` / MCP `apps get {version}`); the head's rides on `apps`.
export const APP_VERSION_COLUMNS = [
  `id`,
  `app_id`,
  `team_id`,
  `number`,
  `note`,
  `session_id`,
  `created_by`,
  `created_at`,
]

export const Route = createFileRoute(`/api/shapes/app-versions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `app_versions`,
        columns: APP_VERSION_COLUMNS,
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
