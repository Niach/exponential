import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getMemberTeamIds } from "@/lib/team-membership"
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
  // EXP-319 — PR automation targets: NULL status_id = builtin default,
  // *_automation=false = "do nothing". Synced so clients can render the
  // automation setting/hint.
  `pr_opened_status_id`,
  `pr_opened_automation`,
  `pr_merged_status_id`,
  `pr_merged_automation`,
  // EXP-711 — merge-ends-sessions toggle (default true).
  `end_sessions_on_merge`,
  // EXP-1105 — yolo mode (auto-merge + hidden Reviews, default false).
  `yolo_mode`,
  // EXP-1709 — the merge train's mode (auto | approval), replaces yolo_mode.
  `merge_mode`,
  // EXP-630 — the estimate scale (`none` = off).
  `estimation_type`,
  // VAPP-201 — the team time zone app hosts read dates in (NULL = the device's).
  `time_zone`,
  // #EXP-1730 phase A: the team's lead Assistant.
  `lead_assistant_id`,
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
          // #EXP-1720: joined teams only: a personal team never syncs (it
          // would show in every switcher, shipped clients included).
          const teamIds = userId ? await getMemberTeamIds(userId) : []
          return buildWhereClause(`id`, teamIds)
        },
      }),
    },
  },
})
