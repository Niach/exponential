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
  // #EXP-1730 phase A: the runner defaults as one object (trigger-kept).
  `defaults`,
  // #EXP-1656: the skills every run of the agent starts with.
  `skill_ids`,
  // #EXP-1793: the team MCP servers every run of the agent starts with.
  `mcp_server_ids`,
  `owner_id`,
  `sort_order`,
  // S-3: assistant | agent. An Assistant LISTS for its owner only (the
  // clients filter on `visibleAgent`), but every member keeps the row: the
  // issue actor, an existing assignee and a shared session tree name it.
  `role`,
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
