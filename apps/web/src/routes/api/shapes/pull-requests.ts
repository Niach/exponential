import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — every PR of the member's teams (the PR state that left
// `issues`). Team-visible: an issue's PR badge is team data, a private
// chat's PR included (GitHub shows it anyway). Every column syncs but
// FEED-160's `provider_updated_at` (the webhook's draft guard, server-only;
// its one-release shim `github_updated_at` too),
// `not_run` and EXP-1709's `merge_ready_at` / `merge_approved_at`.
export const PULL_REQUEST_COLUMNS = [
  `id`,
  `team_id`,
  `root_session_id`,
  `session_id`,
  `repository_id`,
  `number`,
  `url`,
  `title`,
  `head`,
  `base`,
  `base_pull_request_id`,
  `state`,
  `merged_at`,
  // EXP-1610: the aggregate CI state (contract prChecksState).
  `checks_state`,
  // EXP-1709 (merge-train.json STATE): the train's state of the PR, what it
  // still waits for, bounces, the hold reason, the 24 h clock, Merge now,
  // the queue position and the testing run (`Testing with N`). Server-only:
  // `merge_ready_at`, `merge_approved_at`.
  `merge_state`,
  `merge_wait`,
  `merge_attempts`,
  `merge_hold`,
  `merge_queued_since`,
  `merge_now`,
  `merge_position`,
  `merge_train_id`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/pull-requests`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `pull_requests`,
        columns: PULL_REQUEST_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildWhereClause(`team_id`, teamIds)
        },
      }),
    },
  },
})
