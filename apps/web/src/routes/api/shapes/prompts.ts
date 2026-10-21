import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// NEW-2: the team's saved PROMPTS (the composer's `/` menu, Settings →
// Prompts). Server-pinned allowlist EXCLUDES `body`: the ≤64KB markdown
// never rides sync (tRPC `prompts.get` / MCP `prompts_get` on demand).
export const PROMPT_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `icon`,
  `sort_order`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/prompts`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `prompts`,
        columns: PROMPT_COLUMNS,
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
