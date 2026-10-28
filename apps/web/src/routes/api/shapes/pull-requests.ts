import { createFileRoute } from "@tanstack/react-router"
import { buildWhereClause, getUserTeamIds } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — every PR of the member's teams (the PR state that left
// `issues`). Team-visible: an issue's PR badge is team data, a private
// thread's PR included (GitHub shows it anyway). Every column syncs but
// FEED-160's `github_updated_at` (the webhook's draft guard, server-only).
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
