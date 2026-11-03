import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #EXP-1656: the team's SKILLS (Settings › Skills, the + menu pick, the
// agent page). Server-pinned allowlist EXCLUDES `body`: the ≤64KB SKILL.md
// never rides sync (tRPC `skills.get` on demand; the running device gets it
// from `skills.resolveForLaunch`).
export const SKILL_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `description`,
  `enabled_by_default`,
  `created_by_id`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/skills`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `skills`,
        columns: SKILL_COLUMNS,
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
