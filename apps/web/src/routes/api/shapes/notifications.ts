import { createFileRoute } from "@tanstack/react-router"
import { andClauses, sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Per-user inbox feed. Streams only the requesting user's notification rows;
// never anonymous. The where clause is server-computed from the bearer token
// and — REV2-5 — fully STATIC per user: it embeds no membership id lists, so
// this shape's identity never rotates (not even on team join/leave).
//
// Trash-awareness rides the trigger-maintained board_deleted_at mirror
// (REV-109 semantics, REV2-5 mechanism): notifications of a soft-deleted
// board hide for the 48h trash window along with the board itself (and
// return on restore). Issue-less rows (helpdesk support_reply) carry a NULL
// board_deleted_at and always sync — nothing issue-less is ever silently
// dropped.
//
// Deliberately NOT membership-scoped: rows are written exclusively for this
// user by the server-side fan-out, which is itself membership-filtered at
// delivery time (lib/integrations/notifications.ts — an ex-member never
// receives NEW rows). Notifications already delivered before a member was
// removed stay in their inbox, like received email.
//
// `emailed_at` (the hourly digest sweep's server-side claim stamp),
// `board_id`, and `board_deleted_at` (trash-scoping bookkeeping, filtered on
// above) are deliberately excluded via the columns allowlist — none is inbox
// state. `team_id` IS synced: issue-less rows (helpdesk support_reply) carry
// it so clients can route the notification to the right team's Support
// inbox; issue-anchored rows leave it NULL (their team comes from the
// issue).
export const Route = createFileRoute(`/api/shapes/notifications`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `notifications`,
        requireAuth: true,
        columns: [
          `id`,
          `user_id`,
          `issue_id`,
          `team_id`,
          `type`,
          `title`,
          `body`,
          `read_at`,
          `pushed_at`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          if (!userId) return null
          return andClauses(
            `"user_id" = ${sqlStringLiteral(userId)}`,
            `"board_deleted_at" IS NULL`
          )
        },
      }),
    },
  },
})
