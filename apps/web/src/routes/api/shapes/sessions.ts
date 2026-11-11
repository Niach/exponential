import { createFileRoute } from "@tanstack/react-router"
import {
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291/NEW-1/NEW-2 — the session tree (chats, agent conversations
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
  `computer_use`,
  `ultracode`,
  // EXP-1583: the daemon reads it at launch.
  `subagent_model`,
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
  // #FEED-177: a private turn's rotation flag (the holders rotate).
  `seal_rotate_epoch`,
  `sealed_title`,
  // #EXP-1730 phase A: the list digest (mirrored from conversations).
  `digest`,
  // #EXP-1720: the Assistant conversation root's Teams pick (null = all).
  `scope_team_ids`,
  // VAPP-229: the app that started it ("via <app>", via-app.json; NOT
  // `app_id`, an editor session's app).
  `via_app_id`,
  // #EXP-1731: non-null = a CLI subagent row the device writes (subagents.json).
  `subagent_key`,
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
