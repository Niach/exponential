import { createFileRoute } from "@tanstack/react-router"
import { sqlStringLiteral } from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-878: the caller's issue drafts — what the create-issue dialog keeps
// when it is closed with content in it, on every client. Streams only the
// requesting user's rows; never anonymous. Like `pins`, the where clause is
// fully STATIC per user (`user_id = me`, no membership id lists), so the
// shape identity never rotates — not even on team join/leave.
//
// Deliberately NOT team- or trash-scoped: clients render only the drafts of
// the active team (or, on mobile My Work, of every team) whose BOARD they can
// resolve from the boards shape, which carries the membership and trash
// predicates already. A draft on a trashed board simply has no board until it
// restores; the purge cascade removes it. Every column is synced — the
// draft's attachments are server-only rows (`attachments.draft_id`, excluded
// from the attachments shape) fetched via `issueDrafts.listAttachments`.
export const Route = createFileRoute(`/api/shapes/issue-drafts`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_drafts`,
        requireAuth: true,
        columns: [
          `id`,
          `user_id`,
          `team_id`,
          `board_id`,
          `title`,
          `description`,
          `status_id`,
          `priority`,
          `assignee_id`,
          `label_ids`,
          `due_date`,
          `created_at`,
          `updated_at`,
        ],
        getWhere: async (userId) => {
          if (!userId) return null
          return `"user_id" = ${sqlStringLiteral(userId)}`
        },
      }),
    },
  },
})
