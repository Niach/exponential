import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// NEW-2: the team's agent DEFINITIONS (Settings → Agents, the Agents page,
// the composer's agent pick). Team-scoped, member-visible. SERVER-ONLY,
// behind the allowlist: `instructions` (tRPC `agents.get`) and the
// trigger bookkeeping `trigger_state`.
export const AGENT_COLUMNS = [
  `id`,
  `team_id`,
  `name`,
  `icon`,
  `color`,
  `triggers`,
  `device_id`,
  `cli`,
  `account`,
  `model`,
  `owner_id`,
  `sort_order`,
  // #EXP-1383: when the instructions last changed (chats.json `agentHint`).
  `instructions_updated_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/agents`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `agents`,
        columns: AGENT_COLUMNS,
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
