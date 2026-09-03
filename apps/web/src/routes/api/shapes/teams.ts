import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist (clients cannot widen it). These are exactly
// the columns every native client stores. Pinning keeps server-only columns
// (e.g. `comp_tier`) off the wire — an unallowlisted synced column bricks
// native sync loops.
const TEAM_COLUMNS = [
  `id`,
  `name`,
  `slug`,
  `icon_url`,
  `helpdesk_enabled`,
  // EXP-319 — PR automation targets: NULL status_id = builtin default,
  // *_automation=false = "do nothing". Synced so clients can render the
  // automation setting/hint.
  `pr_opened_status_id`,
  `pr_opened_automation`,
  `pr_merged_status_id`,
  `pr_merged_automation`,
  // EXP-711 — merge-ends-sessions toggle (default true).
  `end_sessions_on_merge`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/teams`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `teams`,
        columns: TEAM_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = await getReadableTeamIds(userId)
          return buildWhereClause(`id`, teamIds)
        },
      }),
    },
  },
})
