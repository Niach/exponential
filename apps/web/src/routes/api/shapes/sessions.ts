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
// `mcp_server_ids` and `wake_on`. The LEGACY read shims (`dispatch`,
// `caption`, `aside`, `move_to_device_id`, `handoff_restored_at`,
// `spawned_by_session_id`, `action_*`, and the agent facets `kind`/`name`/
// `icon`/`color`/`triggers`/`sort_order`, mirrored on a conversation
// session) stay listed for the iOS store build and the ≤0.14.74 devices
// (NEW-8 drops them). NEW-2 adds `agent_id`.
export const SESSION_COLUMNS = [
  `id`,
  `team_id`,
  `parent_id`,
  `root_id`,
  `depth`,
  `kind`,
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
