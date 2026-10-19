import { createFileRoute } from "@tanstack/react-router"
import {
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — the session tree (threads, agents and their sub-sessions). ONE
// visibility rule: `shared OR user_id = me` inside the member's teams. A
// device picks up its `queued` rows from this shape (the DB is the bus).
// SERVER-ONLY, behind the allowlist: `host_user_id`, `mcp_server_ids`,
// `work_launch`, `handoff` and an agent's `instructions` (tRPC
// `sessions.getAgent`).
export const SESSION_COLUMNS = [
  `id`,
  `team_id`,
  `parent_id`,
  `root_id`,
  `depth`,
  `kind`,
  `user_id`,
  `device_id`,
  `cli`,
  `account`,
  `model`,
  `effort`,
  `plan_mode`,
  `computer_use`,
  `ultracode`,
  `dispatch`,
  `repository_id`,
  `branch`,
  // EXP-1252: a pending move's target device (the `handoff` jsonb stays
  // server-only).
  `move_to_device_id`,
  // EXP-1349: the target restored the move (the source's cleanup gate).
  `handoff_restored_at`,
  `status`,
  `busy`,
  `needs_input`,
  `blocked`,
  `started_at`,
  `last_turn_at`,
  `ended_at`,
  `shared`,
  `title`,
  // EXP-1323: the live status line + a `/btw` aside child.
  `caption`,
  `aside`,
  `guide`,
  `action_id`,
  `action_name`,
  `action_inputs`,
  `spawned_by_session_id`,
  `name`,
  `icon`,
  `color`,
  `triggers`,
  `sort_order`,
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
