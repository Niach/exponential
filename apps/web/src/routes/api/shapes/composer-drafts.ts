import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// #FEED-219 (fixture `composer-draft.json`): the caller's composer drafts —
// the text every comment, reply, session, agent, app and New chat composer
// keeps per target. Like `issue-drafts` and `pins`, the where clause is the
// fully STATIC `user_id = me`: never anonymous, never team- or
// trash-scoped (a composer only reads the row of the target it shows).
/** The synced columns. Checked against the schema by
 *  `shape-columns.test.ts`. */
export const COMPOSER_DRAFT_COLUMNS = [
  `id`,
  `user_id`,
  `team_id`,
  `key`,
  `text`,
  `created_at`,
  `updated_at`,
] as const

export const Route = createFileRoute(`/api/shapes/composer-drafts`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `composer_drafts`,
        requireAuth: true,
        columns: [...COMPOSER_DRAFT_COLUMNS],
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
