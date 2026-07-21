import { createFileRoute } from "@tanstack/react-router"
import {
  buildWhereClause,
  getReadableUserIdsInTeams,
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
        // sync engine. isAdmin comes from the session, never this shape.
        columns: [
          `id`,
          `name`,
          `email`,
          `image`,
          `created_at`,
          `updated_at`,
        ],
        // The users shape syncs FULL rows (including email), so its scope is
        // membership-only: co-members of teams the caller has joined.
        // Public-team viewers who aren't members get no user rows —
        // see getReadableUserIdsInTeams for the rationale.
        getWhere: async (userId) => {
          const sharedUserIds = await getReadableUserIdsInTeams(userId)
          return buildWhereClause(`id`, sharedUserIds)
        },
      }),
    },
  },
})
