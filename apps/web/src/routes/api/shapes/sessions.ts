import { createFileRoute } from "@tanstack/react-router"
import {
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291/NEW-1/NEW-2 — the session tree (threads, agent conversations
// and their sub-sessions). ONE visibility rule: `shared OR user_id = me`
// inside the member's teams. A device picks up its `queued` rows from this
// shape (the DB is the bus). SERVER-ONLY, behind the allowlist:
// `mcp_server_ids` and `wake_on`. NEW-2 adds `agent_id`.
export const SESSION_COLUMNS = [
  `id`,
  `team_id`,
  `parent_id`,
  `root_id`,
  `depth`,
  `agent_id`,
  `app_id`,
  `user_id`,
  `device_id`,
  `cli`,
  `account`,
  `model`,
  `effort`,
  `plan_mode`,
  `computer_use`,
  `ultracode`,
  `repository_id`,
  `branch`,
  `status`,
  `busy`,
  `background`,
  `needs_input`,
  `blocked`,
  `started_at`,
  `last_turn_at`,
  `ended_at`,
  `shared`,
  `title`,
  `guide`,
  // #EXP-1383: when the device last fetched the brief (`sessions.brief`).
  `agent_instructions_at`,
  // #EXP-1383: the agent whose session started this root.
  `started_by_agent_id`,
  // EXP-1458: the tree key (roots) and the sealed title (sealed-content.json).
  `seal_public_key`,
  `seal_epoch`,
  `sealed_title`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `sessions`,
        columns: SESSION_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildSessionVisibleWhere(teamIds, userId, `user_id`)
        },
      }),
    },
  },
})
