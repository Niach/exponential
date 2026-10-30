import { createFileRoute } from "@tanstack/react-router"
import {
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// EXP-1291 — the Conversation (people, the agent's turn replies, cards) of
// every visible session. A device also delivers queued person messages off
// it (`consumed_at IS NULL`). The visibility mirrors stay server-only.
export const MESSAGE_COLUMNS = [
  `id`,
  // NEW-3: the conversation the row lives in (NEW-8: its only key).
  `conversation_id`,
  `root_id`,
  `team_id`,
  `author_kind`,
  `author_id`,
  // EXP-1323: the session that posted it (a child's report or question).
  `author_session_id`,
  `body`,
  `card`,
  // EXP-1458: the attachments a (possibly sealed) message carries.
  `attachment_ids`,
  `answered_at`,
  `consumed_at`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/messages`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `messages`,
        columns: MESSAGE_COLUMNS,
        // #EXP-1380: the web loads a chat's messages when it opens.
        subsets: true,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildSessionVisibleWhere(teamIds, userId, `owner_id`)
        },
      }),
    },
  },
})
