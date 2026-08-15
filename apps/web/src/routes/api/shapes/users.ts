import { createFileRoute } from "@tanstack/react-router"
import {
  buildArrayOverlapClause,
  buildWhereClause,
  getUserTeamIds,
  orClauses,
  sqlStringLiteral,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/users`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `users`,
        // Server-pinned allowlist: exactly the 6 columns every client stores.
        // Keeps web-only/server-only columns (email_verified, is_admin,
        // creem_customer_id, had_trial, onboarding_completed_at) OUT of sync —
        // native schemas don't have them, and a partial update touching one
        // used to abort the batch before the offset saved and crash-loop the
        // sync engine. isAdmin comes from the session, never this shape. The
        // team_ids scoping mirror below is deliberately excluded too — a shape
        // may FILTER on a column its allowlist drops (Electric evaluates
        // `where` server-side; device_worktrees precedent).
        columns: [`id`, `name`, `email`, `image`, `created_at`, `updated_at`],
        // The users shape syncs FULL rows (including email), so its scope is
        // membership-only: co-members of teams the caller has joined, plus
        // themself. Scoped via the trigger-maintained users.team_ids mirror
        // (REV-37, sync_user_team_ids) instead of enumerating every readable
        // co-member id — that id list grew with instance size until the
        // where clause tripped Electric's ~10KB request-line limit (414) and
        // permanently killed the shape on every client. This clause is
        // bounded by the CALLER's own team count; co-members joining/leaving
        // arrive as incremental move-in/move-out deltas, and the identity
        // rotates only on the caller's OWN membership changes (REV2-5
        // stance). Anonymous: impossible-match sentinel.
        getWhere: async (userId) => {
          if (!userId) return buildWhereClause(`id`, [])
          const self = `"id" = ${sqlStringLiteral(userId)}`
          const teamIds = await getUserTeamIds(userId)
          if (teamIds.length === 0) return self
          return orClauses(self, buildArrayOverlapClause(`team_ids`, teamIds))
        },
      }),
    },
  },
})
