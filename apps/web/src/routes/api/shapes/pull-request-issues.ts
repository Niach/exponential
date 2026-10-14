import { createFileRoute } from "@tanstack/react-router"
import {
  buildTeamScopedChildWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — which issues a PR closes. Team-scoped with the issue's static
// trash/archive predicates; the board mirrors stay server-only.
export const PULL_REQUEST_ISSUE_COLUMNS = [
  `id`,
  `pull_request_id`,
  `issue_id`,
  `team_id`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/pull-request-issues`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `pull_request_issues`,
        columns: PULL_REQUEST_ISSUE_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildTeamScopedChildWhere(teamIds)
        },
      }),
    },
  },
})
