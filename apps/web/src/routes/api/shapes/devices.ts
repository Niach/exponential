import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildArrayOverlapClause,
  getUserTeamIds,
  orClauses,
  sqlStringLiteral,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-481: the 17th synced shape — the per-user device registry. Full row
// syncs; nothing on it is server-secret. `user_id` is deliberately IN the
// allowlist (unlike pure scoping mirrors): clients split "mine" vs "shared"
// on it and resolve the owner's display name — always possible because a
// sharing owner is by definition a member of the sharing team, hence inside
// the users shape. `launch_defaults(_updated_at)`, `agents`, `caps`,
// `unauthed_agents` sync so pickers and the device-settings view work from
// persisted data even while the machine is offline. EXP-622: `is_default`
// rides along so every client's picker can prefill the owner's default
// machine — it is the OWNER's preference, so clients honour it only on rows
// whose `user_id` is theirs. EXP-484: `agent_accounts`/`agent_usage`(+`_at`)
// carry the machine's read-only per-agent sign-in and usage status — a
// ONE-TIME shape-identity rotation, and never a convergence trigger (the
// desktop watches `launch_defaults_updated_at` alone).
const DEVICE_COLUMNS = [
  `id`,
  `user_id`,
  `device_id`,
  `label`,
  `kind`,
  `platform`,
  `version`,
  `agents`,
  `caps`,
  `unauthed_agents`,
  // EXP-749: the ACP-ready subset of `agents` (NULL = assume all).
  `acp_agents`,
  `launch_defaults`,
  `launch_defaults_updated_at`,
  `agent_accounts`,
  `agent_usage`,
  `agent_usage_at`,
  `active_sessions`,
  `last_seen_at`,
  // FEED-33: uuid[] — every team the server is shared with (`{}` = private).
  `shared_team_ids`,
  `is_default`,
  `update_requested_at`,
  `created_at`,
  `updated_at`,
]

// Own rows always; teammates' rows only when shared with a common team AND
// server-kind (the static literal arm keeps a desktop share — impossible by
// router rule, but never trust one layer — out of every teammate's shape).
// Identity: `user_id = me` is static, the team id list is sorted — the shape
// rotates ONLY on the caller's team-membership changes (REV2-5 legitimacy
// class, same as the teams shape). Individual share/unshare moves rows
// in/out incrementally via the column, never a where-clause rewrite.
// FEED-33: the share is a uuid[] (several teams), so the arm is an array
// overlap (`&&`, the users-shape `team_ids` precedent) instead of an IN.
export const Route = createFileRoute(`/api/shapes/devices`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `devices`,
        requireAuth: true,
        columns: DEVICE_COLUMNS,
        getWhere: async (userId) => {
          if (!userId) return null
          const teamIds = await getUserTeamIds(userId)
          return orClauses(
            `"user_id" = ${sqlStringLiteral(userId)}`,
            andClauses(
              buildArrayOverlapClause(`shared_team_ids`, teamIds),
              `"kind" = 'server'`
            )
          )
        },
      }),
    },
  },
})
