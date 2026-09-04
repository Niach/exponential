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
// a batch row's Merge shortcut to its OWN PR), `started_reason` for EXP-530, `automation_id` for EXP-583
// (the Automations run history + per-row last run), `device_id` for
// EXP-549/550 (the hosting machine's steer deviceId — clients join the synced
// devices row for the renamed label and its online-ness), and
// `summary`/`ended_by`/`resumed_from_id` for EXP-637 (the agent's
// own close-out via `exponential_sessions_end`, who ended the run, and the
// run a Resume continues; EXP-686 dropped the self-reported `outcome` —
// "Running" or the summary is the whole story), plus `agent` for EXP-484
// (which agent CLI runs the
// session, so clients can name it and pair it with the host device's usage
// windows), and `pr_url`/`pr_number`/`pr_state` for EXP-734 (the chore PR an
// action or chat run opened with no issue to link — every client's Merge
// shortcut and Reviews queue key on the run; issue/batch rows still read the
// issue) — each a ONE-TIME shape-identity rotation (benign: small table,
// full resync; land in one deploy).
// `merged_own_pr` stays OUT: server-only like `host_user_id` (nothing on a
// client acts on it; only the merge-driven end paths read it), and so does
// `acked_at` (EXP-701: the device's pickup ack — read by orchestrating
// agents via the MCP session tools, not by synced clients).
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
  `automation_id`,
  `user_id`,
  `device_label`,
  `device_id`,
  `agent`,
  `status`,
  `branch`,
  `pr_url`,
  `pr_number`,
  `pr_state`,
  `summary`,
  `ended_by`,
  `resumed_from_id`,
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
