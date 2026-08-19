import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Server-pinned column allowlist — excludes the `board_deleted_at` trash
// mirror (REV2-5) and the `board_archived_at` archive mirror (EXP-500), both
// server-only (the where clause filters on them).
// `action_id`/`action_name` were appended for EXP-253, `branch` for EXP-545
// (the batch↔PR linkage: stamped by the MCP pr_open batch flip so clients tie
// a batch row's Merge shortcut to its OWN PR), `started_reason` for EXP-530
// (the "Automated" badge + Automations run history), `device_id` for
// EXP-549/550 (the hosting machine's steer deviceId — clients join the synced
// devices row for the renamed label and its online-ness) — each a ONE-TIME
// shape-identity rotation (benign: small table, full resync; land in one
// deploy).
// Old native builds drop unknown columns safely (verified: iOS filters to
// its SQLite schema, Android ignoreUnknownKeys + partial-plan filter,
// desktop serde non-strict).
const CODING_SESSION_COLUMNS = [
  `id`,
  `issue_id`,
  `team_id`,
  `board_id`,
  `action_id`,
  `action_name`,
  `started_reason`,
  `user_id`,
  `device_label`,
  `device_id`,
  `status`,
  `branch`,
  `needs_input`,
  `started_at`,
  `ended_at`,
  `created_at`,
  `updated_at`,
]

// Live "coding now" rows. MEMBER-ONLY: anonymous callers get NOTHING (the
// empty team list yields the impossible-match sentinel — zero rows, no 401).
//
// Members sync their teams' sessions minus those of trashed boards (REV2-5:
// the static board_deleted_at predicate — issue-scoped sessions of a trashed
// board hide for the 48h trash window along with the board itself).
// Batch-scoped rows (issue_id + board_id NULL — a batch run spans boards)
// keep a NULL board_deleted_at and therefore always sync; the old explicit
// `board_id IS NULL` OR-arm is subsumed.
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        columns: CODING_SESSION_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
